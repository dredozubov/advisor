use crate::document::{DocType, Metadata};
use crate::earnings;
use crate::edgar::{self, filing};
use crate::query::Query;
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use futures::StreamExt;
use langchain_rust::chain::ConversationalChain;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{chain::Chain, prompt_args};
use qdrant_client::Qdrant;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

async fn process_documents(
    query: &Query,
    http_client: &reqwest::Client,
    store: &dyn VectorStore,
    qdrant_client: &Qdrant,
) -> Result<()> {
    // Process EDGAR filings if requested
    if query.parameters.get("filings").is_some() {
        log::debug!("Filings data is requested");
        match query.to_edgar_query() {
            Ok(edgar_query) => {
                for ticker in &edgar_query.tickers {
                    log::info!("Fetching EDGAR filings for ticker: {}", ticker);
                    let filings = filing::fetch_matching_filings(http_client, &edgar_query).await?;
                    process_edgar_filings(filings, store, qdrant_client).await?;
                }
            }
            Err(e) => {
                log::error!("Failed to create EDGAR query: {}", e);
            }
        }
    }

    // Process earnings data if requested
    if let Ok(earnings_query) = query.to_earnings_query() {
        log::info!(
            "Fetching earnings data for ticker: {}",
            earnings_query.ticker
        );
        let transcripts = earnings::fetch_transcripts(
            http_client,
            &earnings_query.ticker,
            earnings_query.start_date,
            earnings_query.end_date,
        )
        .await?;
        process_earnings_transcripts(transcripts, store, qdrant_client).await?;
    }

    Ok(())
}

async fn build_context(
    query: &Query,
    input: &str,
    store: &dyn VectorStore,
) -> Result<(String, String)> {
    log::debug!("Building context for query: {:?}", query);

    // 1. Get all documents specified by the query
    let mut required_docs = Vec::new();

    if let Some(filings) = query.parameters.get("filings") {
        let start_date = filings
            .get("start_date")
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("Missing start_date"))?;
        let end_date = filings
            .get("end_date")
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("Missing end_date"))?;

        if let Some(types) = filings.get("report_types").and_then(|t| t.as_array()) {
            let filing_types: Vec<&str> = types.iter().filter_map(|t| t.as_str()).collect();
            let filter = serde_json::json!({
                "must": [
                    {
                        "key": "type",
                        "match": { "value": "edgar_filing" }
                    },
                    {
                        "key": "filing_type",
                        "match": { "any": filing_types }
                    },
                    {
                        "key": "filing_date",
                        "range": {
                            "gte": start_date,
                            "lte": end_date
                        }
                    }
                ]
            })
            .to_string();
            log::info!("Using filter for similarity search: {}", filter);
            let docs = store
                .similarity_search(
                    &filter,
                    50,
                    &langchain_rust::vectorstore::VecStoreOptions::default(),
                )
                .await
                .map_err(|e| anyhow!("Failed to retrieve documents from vector store: {}", e))?;
            required_docs.extend(docs);
        }
    }

    if let Some(earnings) = query.parameters.get("earnings") {
        let start_date = earnings
            .get("start_date")
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("Missing earnings start_date"))?;
        let end_date = earnings
            .get("end_date")
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("Missing earnings end_date"))?;

        let filter = serde_json::json!({
            "must": [
                {
                    "key": "type",
                    "match": { "value": "earnings_transcript" }
                },
                {
                    "key": "date",
                    "range": {
                        "gte": start_date,
                        "lte": end_date
                    }
                }
            ]
        })
        .to_string();
        log::debug!("Using filter for similarity search: {}", filter);
        let docs = store
            .similarity_search(
                &filter,
                50,
                &langchain_rust::vectorstore::VecStoreOptions::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to search documents: {}", e))?;
        required_docs.extend(docs);
    }

    log::debug!(
        "Query-based search returned {} documents",
        required_docs.len()
    );
    if required_docs.is_empty() {
        return Err(anyhow!("No relevant documents found in vector store"));
    }

    // 2. Calculate total tokens
    const MAX_TOKENS: usize = 12000; // Adjust based on your model
    let total_tokens: usize = required_docs
        .iter()
        .map(|doc| doc.page_content.split_whitespace().count() * 4) // Estimate 4 tokens per word
        .sum();

    log::info!(
        "Retrieved {} documents with approximately {} tokens",
        required_docs.len(),
        total_tokens
    );

    // 3. If we're over the token limit, use similarity search to reduce content
    let final_docs = if total_tokens > MAX_TOKENS {
        log::info!(
            "Token count ({}) exceeds limit ({}), using similarity search to reduce content",
            total_tokens,
            MAX_TOKENS
        );

        store
            .similarity_search(
                input,
                MAX_TOKENS / 500, // Rough estimate of docs that will fit
                &langchain_rust::vectorstore::VecStoreOptions::default(),
            )
            .await
            .map_err(|e| anyhow!("Failed to search documents: {}", e))?
    } else {
        required_docs
    };

    // Compile metadata summary
    let mut filing_types = std::collections::HashSet::new();
    let mut companies = std::collections::HashSet::new();
    let mut date_range = (None, None);

    for doc in &final_docs {
        if let Some(doc_type) = doc.metadata.get("type").and_then(|v| v.as_str()) {
            match doc_type {
                "edgar_filing" => {
                    if let Some(filing_type) =
                        doc.metadata.get("filing_type").and_then(|v| v.as_str())
                    {
                        filing_types.insert(filing_type.to_string());
                    }
                    if let Some(cik) = doc.metadata.get("cik").and_then(|v| v.as_str()) {
                        companies.insert(cik.to_string());
                    }
                }
                "earnings_transcript" => {
                    if let Some(symbol) = doc.metadata.get("symbol").and_then(|v| v.as_str()) {
                        companies.insert(symbol.to_string());
                    }
                }
                _ => {}
            }
        }

        // Track date range across all documents
        if let Some(date) = doc
            .metadata
            .get("filing_date")
            .or_else(|| doc.metadata.get("date"))
            .and_then(|v| v.as_str())
        {
            if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                match date_range {
                    (None, None) => date_range = (Some(parsed_date), Some(parsed_date)),
                    (Some(start), Some(end)) => {
                        if parsed_date < start {
                            date_range.0 = Some(parsed_date);
                        }
                        if parsed_date > end {
                            date_range.1 = Some(parsed_date);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let summary = build_metadata_summary(filing_types, companies, date_range, &final_docs);

    // Format documents for LLM context
    let context = final_docs
        .iter()
        .map(|doc| {
            log::debug!(
                "Document (score: {:.3}):\nMetadata: {:?}\nContent: {}",
                doc.score,
                doc.metadata,
                doc.page_content
            );
            format!("Document (score: {:.3}): {}", doc.score, doc.page_content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    log::info!(
        "=== Complete LLM Context ===\n{}\n=== End Context ===",
        summary
    );

    Ok((summary, context))
}

fn build_metadata_summary(
    filing_types: HashSet<String>,
    companies: HashSet<String>,
    date_range: (Option<NaiveDate>, Option<NaiveDate>),
    similar_docs: &Vec<langchain_rust::schemas::Document>,
) -> String {
    // Build metadata summary
    let mut summary = String::new();
    summary.push_str("Context includes:\n");
    if !filing_types.is_empty() {
        summary.push_str(&format!(
            "- Filing types: {}\n",
            filing_types.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !companies.is_empty() {
        summary.push_str(&format!(
            "- Companies: {}\n",
            companies.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if let (Some(start), Some(end)) = date_range {
        summary.push_str(&format!("- Date range: {} to {}\n", start, end));
    }

    // Group documents by source and count chunks
    let mut doc_summaries = std::collections::HashMap::new();
    for doc in similar_docs {
        let key = match (
            doc.metadata.get("doc_type").and_then(|v| v.as_str()),
            doc.metadata.get("filing_type").and_then(|v| v.as_str()),
            doc.metadata.get("quarter").and_then(|v| v.as_u64()),
            doc.metadata.get("year").and_then(|v| v.as_u64()),
            doc.metadata.get("total_chunks").and_then(|v| v.as_u64()),
        ) {
            (Some("filing"), Some(filing_type), _, _, Some(total)) => {
                format!("SEC {} Filing ({} chunks)", filing_type, total)
            }
            (Some("earnings_transcript"), _, Some(quarter), Some(year), Some(total)) => {
                format!("Q{} {} Earnings Call ({} chunks)", quarter, year, total)
            }
            _ => {
                log::debug!("Unknown document type in metadata: {:?}", doc.metadata);
                "Unknown Document Type".to_string()
            }
        };

        let entry = doc_summaries.entry(key).or_insert((0, 0));
        entry.0 += 1; // Increment chunk count
        if let Some(total) = doc.metadata.get("total_chunks").and_then(|v| v.as_i64()) {
            entry.1 = total as usize; // Update total chunks
        }
    }

    summary.push_str("\nDocuments retrieved:\n");
    for (doc_type, (chunks_found, total_chunks)) in doc_summaries {
        summary.push_str(&format!(
            "- {}: {} of {} chunks\n",
            doc_type, chunks_found, total_chunks
        ));
    }

    summary
}

async fn generate_query(chain: &ConversationalChain, input: &str) -> Result<(Query, String)> {
    let summary = get_conversation_summary(chain, input).await?;
    log::info!("Summary done: {}", summary);

    let query = extract_query_params(chain, input).await?;
    log::info!("Query params done: {:?}", query);

    Ok((query, summary))
}

async fn generate_response(
    chain: &ConversationalChain,
    input: &str,
    context: &str,
) -> Result<
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
> {
    log::info!("generate_response::input: {}", input);
    // Create prompt with context
    let prompt = format!(
        "Based on the following SEC filings and financial documents, answer this question: {}\n\nContext:\n{}\n\n If you don't know the answer, just say that you don't know, don't try to make up an answer. Helpful answer:\n",
        input,
        context
    );
    log::trace!("Prompt: {}", prompt);

    // Return streaming response
    let prompt_args = prompt_args![
        "input" => [
            "You are a helpful financial analyst assistant. Provide clear, quantitative, and informative answers for the analyst based on the provided context.",
            &prompt
        ]
    ];

    let stream = chain.stream(prompt_args).await?;
    log::info!("LLM stream started successfully");

    Ok(Box::pin(stream.map(|r| {
        r.map(|s| s.content)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    })))
}

pub async fn eval(
    input: &str,
    http_client: &reqwest::Client,
    stream_chain: &ConversationalChain,
    query_chain: &ConversationalChain,
    store: &dyn VectorStore,
    qdrant_client: &Qdrant,
) -> Result<(
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
    String,
)> {
    // 1. Generate query and get summary
    let (query, summary) = generate_query(query_chain, input).await?;

    // 2. Process documents based on query
    process_documents(&query, http_client, store, qdrant_client).await?;

    // 3. Build context from processed documents
    let (_summary, context) = build_context(&query, input, store).await?;

    // 4. Generate streaming response
    let stream = generate_response(stream_chain, input, &context).await?;

    Ok((stream, summary))
}

async fn process_edgar_filings(
    filings: HashMap<String, filing::Filing>,
    store: &(dyn VectorStore + Send + Sync),
    qdrant_client: &Qdrant,
) -> Result<()> {
    for (input_file, filing) in &filings {
        log::info!("Processing filing ({:?}): {:?}", input_file, filing);
        let company_name = &filing.accession_number;
        let filing_type_with_date = format!("{}_{}", filing.report_type, filing.filing_date);
        let output_file_path = format!(
            "data/edgar/parsed/{}/{}",
            company_name, filing_type_with_date
        );

        let output_path = std::path::Path::new(&output_file_path);
        if output_path.exists() {
            log::info!(
                "Output file already exists for filing: {}",
                output_file_path
            );
            continue;
        }

        log::debug!(
            "Parsing filing: {} with output path: {:?}",
            input_file,
            output_path.parent().unwrap()
        );

        log::info!("INTO EXTRACT_COMPLETE_SUBMISSION_FILING");
        filing::extract_complete_submission_filing(
            input_file,
            filing.report_type.clone(),
            store,
            qdrant_client,
        )
        .await?;
    }
    Ok(())
}

async fn process_earnings_transcripts(
    transcripts: Vec<(earnings::Transcript, PathBuf)>,
    store: &(dyn VectorStore + Send + Sync),
    qdrant_client: &Qdrant,
) -> Result<()> {
    for (transcript, filepath) in transcripts {
        log::info!(
            "Processing transcript for {} on {}",
            transcript.symbol,
            transcript.date
        );

        // Create document for vector storage
        let metadata = Metadata::MetaEarningsTranscript {
            doc_type: DocType::EarningTranscript,
            filepath: filepath.clone(),
            symbol: transcript.symbol.clone(),
            year: transcript.year as usize,
            quarter: transcript.quarter as usize,
            chunk_index: 0,  // Set appropriately
            total_chunks: 1, // Set appropriately
        };

        // Store the transcript using the chunking utility with caching
        log::info!("Storing earnings transcript in vector store");
        crate::document::store_chunked_document(transcript.content, metadata, store, qdrant_client)
            .await?;
        log::info!(
            "Added earnings transcript to vector store: {} Q{}",
            transcript.symbol,
            transcript.quarter
        );
    }
    Ok(())
}

async fn get_conversation_summary(chain: &ConversationalChain, input: &str) -> Result<String> {
    let summary_task = format!(
        "Provide a 2-3 word summary of this query, mentioning any ticker symbols if present. Examples:\n\
         Input: Show me Apple's revenue breakdown for Q1 2024 -> AAPL Revenue\n\
         Input: What were the key risks mentioned in the latest 10-K of TSLA? -> TSLA Risk Factors\n\
         Input: Compare Microsoft and Google cloud revenue growth -> MSFT GOOGL comparison\n\n\
         Query to summarize: {}", 
        input
    );

    match chain.invoke(prompt_args! {"input" => summary_task}).await {
        Ok(result) => Ok(result.trim().to_string()),
        Err(e) => Err(anyhow!("Error getting summary: {:?}", e)),
    }
}

async fn extract_query_params(chain: &ConversationalChain, input: &str) -> Result<Query> {
    log::debug!("Starting extract_query_params with input: {}", input);
    let now = chrono::Local::now();
    let _today_year = now.format("%Y");
    let _today_month = now.format("%M");
    let _today_day = now.format("%d");
    let task = format!(
        r#"Extract query parameters from the input text to build a comprehensive financial analysis query.
    
    Format the parameters as a JSON object with these fields:
    - 'tickers': array of company ticker symbols
    - 'parameters': object containing query parameters:
        - 'filings': optional object for SEC filings:
            - 'start_date': ISO date (YYYY-MM-DD)
            - 'end_date': ISO date (YYYY-MM-DD) 
            - 'report_types': array of SEC filing types:
                - Required reports (10-K, 10-Q)
                - Management discussion (8-K items 2.02, 7.01, 8.01)
                - Strategic changes (8-K items 1.01, 1.02, 2.01)
                - Guidance & projections (8-K item 7.01)
                - Proxy statements (DEF 14A)
                Possible values are: {}

    Example:
    {{"tickers": ["AAPL"], "parameters": {{"filings": {{"start_date": "2024-01-01", "end_date": "2024-03-31", "report_types": ["10-K", "10-Q", "8-K"]}}, "earnings": {{"start_date": "2024-01-01", "end_date": "2024-03-31"}} }} }}

    Infer which data sources to query based on the user's question:
    - Include 'filings' for questions about financial reports, SEC filings, corporate actions
    - Include 'earnings' for questions about earnings calls, management commentary, guidance
    - Include both when the question spans multiple areas
    
    Use these defaults if values are missing:
    - Latest report: date range from 'today - 90 days' to 'today'
    - Latest quarterly report: include both 10-Q and relevant 8-K filings
    - Earnings analysis: automatically include earnings call transcripts
    
    Current date is: {}.
    Return only a json document, as it's meant to be parsed by the software. No markdown formatting is allowed. No JSON formatting is allowed including pretty-printing and newlines.
    
    Parse this user input:
    {input}"#, *edgar::report::REPORT_TYPES, now.format("%Y-%m-%d")
    )
    .to_string();

    log::debug!("Task: {task}");

    // We can also guide it's response with a prompt template. Prompt templates are used to convert raw user input to a better input to the LLM.
    match chain.invoke(prompt_args! {"input" => task.clone()}).await {
        Ok(result) => {
            log::debug!("Result: {:?}", result);
            let query: Query = match serde_json::from_str(&result) {
                Ok(query) => query,
                Err(e) => {
                    return Err(anyhow!("LLM returned a malformed query, halting: {}", e));
                }
            };
            log::debug!("Parsed generated query: {:?}", query);
            Ok(query)
        }
        Err(e) => Err(anyhow!("Error invoking LLMChain: {:?}", e)),
    }
}
