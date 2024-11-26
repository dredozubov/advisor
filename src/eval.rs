use crate::edgar::{self, filing};
use crate::memory::{Conversation, ConversationManager, MessageRole};
use crate::query::Query;
use crate::{earnings, ProgressTracker};
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar};
use itertools::Itertools;
use langchain_rust::chain::ConversationalChain;
use langchain_rust::vectorstore::pgvector::Store;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{chain::Chain, prompt_args};
use sqlx::{Pool, Postgres};
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

async fn process_documents(
    query: &Query,
    http_client: &reqwest::Client,
    store: Arc<Store>,
    pg_pool: &Pool<Postgres>,
    progress: Option<&Arc<MultiProgress>>,
) -> Result<()> {
    let progress_bar = progress.map(|mp| mp.add(ProgressBar::new(100)));
    let progress_tracker = ProgressTracker::new(progress_bar.as_ref());

    // Process EDGAR filings if requested
    if query.parameters.get("filings").is_some() {
        log::debug!("Filings data is requested");
        match query.to_edgar_query() {
            Ok(edgar_query) => {
                for ticker in &edgar_query.tickers {
                    log::debug!(
                        "Fetching EDGAR filings ({}) for ticker: {} in date range {} to {}",
                        edgar_query
                            .report_types
                            .iter()
                            .map(|rt| rt.to_string())
                            .join(", "),
                        ticker,
                        edgar_query.start_date,
                        edgar_query.end_date
                    );
                    let filings = filing::fetch_matching_filings(
                        http_client,
                        &edgar_query,
                        Some(&progress_tracker),
                    )
                    .await?;
                    process_edgar_filings(
                        filings,
                        Arc::clone(&store),
                        pg_pool.clone(),
                        Some(&progress_tracker),
                    )
                    .await?;
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
        process_earnings_transcripts(transcripts, store, pg_pool.clone(), Some(&progress_tracker))
            .await?;
    }

    Ok(())
}

async fn build_context(
    query: &Query,
    input: &str,
    conversation: &Conversation,
    store: Arc<Store>,
) -> Result<String> {
    log::debug!("Building context for query: {:?}", query);

    // Get document context
    let doc_context = build_document_context(query, input, Arc::clone(&store)).await?;

    // Combine with conversation context
    let full_context = format!(
        "Conversation Summary: {}\n\
         Focus Companies: {}\n\n\
         Document Context:\n{}",
        conversation.summary,
        conversation.tickers.join(", "),
        doc_context
    );

    Ok(full_context)
}

async fn build_document_context(query: &Query, input: &str, store: Arc<Store>) -> Result<String> {
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
        log::info!("Using filter for similarity search: {}", filter);
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
                MAX_TOKENS / 500, // Rough estimate of docs that will fit. TODO: fix me
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

    Ok(context)
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

async fn generate_query(
    chain: &ConversationalChain,
    input: &str,
    conversation: &Conversation,
) -> Result<(Query, String)> {
    let context = format!(
        "Current conversation context: {}\nFocus companies: {}\n\nUser question: {}",
        conversation.summary,
        conversation.tickers.join(", "),
        input
    );

    let summary = get_conversation_summary(chain, &context).await?;
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
        "Based on the conversation context and available documents, answer this question: {}\n\n\
         Context:\n{}\n\n\
         If you don't know the answer, just say that you don't know, don't try to make up an answer. \
         Use the conversation history to provide more relevant and contextual answers. \
         Helpful answer:\n",
        input,
        context
    );
    log::trace!("Prompt: {}", prompt);

    // Return streaming response
    let prompt_args = prompt_args![
        "input" => [
            "You are a helpful financial analyst assistant. Provide clear, quantitative, \
             and informative answers based on the conversation context and provided documents. \
             Maintain continuity with previous discussion points when relevant.",
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
    conversation: &Conversation,
    http_client: &reqwest::Client,
    stream_chain: &ConversationalChain,
    query_chain: &ConversationalChain,
    store: Arc<Store>,
    pg_pool: &Pool<Postgres>,
    conversation_manager: Arc<ConversationManager>,
) -> Result<(
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
    String,
)> {
    // Store user message
    conversation_manager
        .add_message(
            &conversation.id,
            MessageRole::User,
            input.to_string(),
            serde_json::json!({
                "type": "question"
            }),
        )
        .await?;

    // Generate response
    let (query, summary) = generate_query(query_chain, input, conversation).await?;

    let multi_progress = if std::io::stdout().is_terminal() {
        let mp = MultiProgress::new();
        mp.set_move_cursor(true);
        Some(Arc::new(mp))
    } else {
        None
    };

    let store = Arc::new(store);
    process_documents(
        &query,
        http_client,
        Arc::clone(&store),
        pg_pool,
        multi_progress.as_ref(),
    )
    .await?;
    let context = build_context(&query, input, conversation, Arc::clone(&store)).await?;
    let stream = generate_response(stream_chain, input, &context).await?;

    // Create a new stream for collecting the complete response
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    // Create a new stream that forwards chunks and collects them
    let stream = Box::pin(futures::stream::unfold(
        (stream, tx.clone()),
        |(mut stream, tx)| async move {
            if let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        let _ = tx.send(chunk.clone()).await;
                        Some((Ok(chunk), (stream, tx)))
                    }
                    Err(e) => Some((Err(e), (stream, tx))),
                }
            } else {
                None
            }
        },
    ));

    // Spawn task to collect and store complete response
    let conversation_id = conversation.id;
    let query = query.clone();
    let summary = summary.clone();
    let conversation_manager = Arc::clone(&conversation_manager);

    let summary_clone = summary.clone();

    tokio::spawn(async move {
        let mut response_content = String::new();
        while let Some(chunk) = rx.recv().await {
            response_content.push_str(&chunk);
        }
        let _ = conversation_manager
            .add_message(
                &conversation_id,
                MessageRole::Assistant,
                response_content,
                serde_json::json!({
                    "type": "answer",
                    "query": query,
                    "summary": summary_clone
                }),
            )
            .await;
    });

    Ok((stream, summary))
}

async fn process_edgar_filings(
    filings: HashMap<String, filing::Filing>,
    store: Arc<Store>,
    pg_pool: Pool<Postgres>,
    progress_tracker: Option<&ProgressTracker>,
) -> Result<()> {
    let mut success_count = 0;
    let mut error_count = 0;
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(), anyhow::Error>>(100);

    // Launch tasks concurrently
    for (filepath, filing) in filings {
        let tx = tx.clone();
        let store = store.clone();
        let pg_pool = pg_pool.clone();
        if let Some(tracker) = progress_tracker {
            tracker.start_progress(
                100,
                &format!(
                    "Processing filing: {} {}",
                    filing.report_type, filing.accession_number
                ),
            );
        }

        let handle = tokio::spawn(async move {
            match filing::extract_complete_submission_filing(
                &filepath,
                filing.report_type,
                store,
                &pg_pool,
                progress_tracker.map(|v| Arc::new(v)),
            )
            .await
            {
                Ok(()) => {
                    let _ = tx.send(Ok(())).await;
                    Ok(())
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    let err = Err(e);
                    let _ = tx.send(Err(anyhow::anyhow!("{}", err_msg))).await;
                    err
                }
            }
        });
        handles.push(handle);
    }

    // Drop the sender to signal no more messages will be sent
    drop(tx);

    // Collect and process results
    while let Some(result) = rx.recv().await {
        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                error_count += 1;
                log::error!("Error processing filing: {}", e);
            }
        }
    }

    // Wait for all tasks
    for handle in handles {
        handle.await??;
    }

    log::info!(
        "Processed {} filings: {} successful, {} failed",
        success_count + error_count,
        success_count,
        error_count
    );

    Ok(())
}

async fn get_conversation_summary(chain: &ConversationalChain, input: &str) -> Result<String> {
    let summary_task = format!(
        "Provide a 2-3 word summary of thiass query, mentioning any ticker symbols if present. Examples:\n\
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
    - 'is_adr': boolean indicating if the security is an ADR (American Depositary Receipt)
    - 'parameters': object containing query parameters:
        - 'filings': optional object for SEC filings:
            - 'start_date': ISO date (YYYY-MM-DD)
            - 'end_date': ISO date (YYYY-MM-DD) 
            - 'report_types': array of SEC filing types:
                - Required reports (10-K, 10-Q for US stocks, 20-F, 6-K, 8-K for ADRs)
                - Management discussion (8-K items 2.02, 7.01, 8.01)
                - Strategic changes (8-K items 1.01, 1.02, 2.01)
                - Guidance & projections (8-K item 7.01)
                - Proxy statements (DEF 14A)
                Possible values are: {} etc, use appropriate EDGAR report types even if not mentioned here.

    Examples:
    {{"tickers": ["AAPL"], "is_adr": false, "parameters": {{"filings": {{"start_date": "2024-01-01", "end_date": "2024-03-31", "report_types": ["10-K", "10-Q", "8-K"]}}, "earnings": {{"start_date": "2024-01-01", "end_date": "2024-03-31"}} }} }}
    {{"tickers": ["BABA"], "is_adr": true, "parameters": {{"filings": {{"start_date": "2024-01-01", "end_date": "2024-03-31", "report_types": ["20-F", "6-K"]}}, "earnings": {{"start_date": "2024-01-01", "end_date": "2024-03-31"}} }} }}

    Infer which data sources to query based on the user's question:
    - Use the ticker and the company name to establish if it's a US stock or ADRs
    - Include 'filings' for questions about financial reports, SEC filings, corporate actions
    - Include 'earnings' for questions about earnings calls, management commentary, guidance
    - Include both when the question spans multiple areas
    
    Use these defaults if values are missing:
    - Latest report: date range from 'today - 90 days' to 'today'
    - Latest quarterly report: include 10-Q, 8-F for US stocks. 20-F, 40-F, 6-K filings for ADRs. If no sure ask for both.
    - Yearly reports include: 10-K for US stocks, and 20-K for ADRs. If not sure ask for both.
    - Earnings analysis: automatically include earnings call transcripts and quarterly reports (10-Q for US stocks, 6-K for ADRs) or yearly reports(10-K for US stocks, 20-F, 40-F for ADRs) depending on the contex timeline.
    
    Current date is: {}.
    Return only a json document, as it's meant to be parsed by the software. No markdown formatting is allowed. No JSON formatting is allowed including pretty-printing and newlines.
    
    Parse this user input:
    {input}"#, *edgar::report::REPORT_TYPES, now.format("%Y-%m-%d")
    )
    .to_string();

    log::info!("Task: {task}");

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

async fn process_earnings_transcripts(
    transcripts: Vec<(earnings::Transcript, PathBuf)>,
    store: Arc<Store>,
    pg_pool: Pool<Postgres>,
    progress_tracker: Option<&ProgressTracker>,
) -> Result<()> {
    // Create tasks with progress bars
    let mut success_count = 0;
    let mut error_count = 0;
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(), anyhow::Error>>(100);

    // Launch tasks concurrently
    for (transcript, filepath) in transcripts {
        let tx = tx.clone();
        let store = store.clone();
        let pg_pool = pg_pool.clone();
        if let Some(tracker) = progress_tracker {
            tracker.start_progress(
                100,
                &format!(
                    "Processing transcript: {} Q{} {}",
                    transcript.symbol, transcript.quarter, transcript.year
                ),
            );
        }

        let handle = tokio::spawn(async move {
            // Store the transcript
            let metadata = crate::document::Metadata::MetaEarningsTranscript {
                doc_type: crate::document::DocType::EarningTranscript,
                filepath: filepath.clone(),
                symbol: transcript.symbol.clone(),
                year: transcript.year as usize,
                quarter: transcript.quarter as usize,
                chunk_index: 0,
                total_chunks: 1,
            };

            let content = transcript.content.clone();
            crate::document::store_chunked_document(content, metadata, store, &pg_pool, None)
                .await?;

            let _ = tx.send(Ok(())).await;
            Ok::<_, anyhow::Error>(())
        });
        handles.push(handle);
    }

    // Drop sender to signal no more messages
    drop(tx);

    // Collect results from channel
    while let Some(result) = rx.recv().await {
        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                error_count += 1;
                log::error!("Error processing transcript: {}", e);
            }
        }
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await??;
    }

    log::info!(
        "Processed {} transcripts: {} successful, {} failed",
        success_count + error_count,
        success_count,
        error_count
    );

    Ok(())
}
