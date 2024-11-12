use crate::earnings;
use crate::edgar::{self, filing};
use crate::query::Query;
use anyhow::{anyhow, Result};
use futures::StreamExt;
use langchain_rust::chain::ConversationalChain;
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{chain::Chain, prompt_args};
use serde_json::Value;
use std::collections::HashMap;

pub async fn eval(
    input: &str,
    http_client: &reqwest::Client,
    chain: &ConversationalChain,
    store: &dyn VectorStore,
) -> Result<(
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
    String,
)> {
    if let Err(e) = extract_query_params(chain, input).await {
        log::error!("Failure to create an EDGAR query: {e}");
        return Err(anyhow!("Failed to create query: {}", e));
    }

    let query_json = extract_query_params(chain, input).await?;
    log::debug!("Extracted query: {}", query_json);

    // Parse into our new high-level Query type
    log::debug!("Parsing query JSON: {}", query_json);
    let base_query: Query = serde_json::from_str(&query_json)?;
    log::debug!("Parsed base query: {:?}", base_query);

    let mut has_processed_data = false;

    // Process EDGAR filings if requested
    log::debug!("Checking if filings data is requested");
    if let Some(filings) = base_query.parameters.get("filings") {
        log::debug!("Filings data is requested");
        match base_query.to_edgar_query() {
            Ok(edgar_query) => {
                for ticker in &edgar_query.tickers {
                    log::info!("Fetching EDGAR filings for ticker: {}", ticker);
                    let filings = filing::fetch_matching_filings(http_client, &edgar_query).await?;
                    process_edgar_filings(filings, store).await?;
                    has_processed_data = true;
                }
            }
            Err(e) => {
                log::error!("Failed to create EDGAR query: {}", e);
            }
        }

        // Process filings if it's a Value object
        if let Some(filings_obj) = filings.as_object() {
            for (_, filing) in filings_obj {
                if let Some(filing_obj) = filing.as_object() {
                    let metadata: HashMap<String, Value> = [
                        (
                            "type".to_string(),
                            Value::String("edgar_filing".to_string()),
                        ),
                        (
                            "report_type".to_string(),
                            filing_obj
                                .get("report_type")
                                .cloned()
                                .unwrap_or(Value::String("unknown".to_string())),
                        ),
                        (
                            "filing_date".to_string(),
                            filing_obj
                                .get("filing_date")
                                .cloned()
                                .unwrap_or(Value::String("unknown".to_string())),
                        ),
                        (
                            "accession_number".to_string(),
                            filing_obj
                                .get("accession_number")
                                .cloned()
                                .unwrap_or(Value::String("unknown".to_string())),
                        ),
                    ]
                    .into_iter()
                    .collect();

                    log::info!("Storing filing in vector store");
                    crate::document::store_chunked_document_with_cache(
                        serde_json::to_string_pretty(&filing)?,
                        metadata,
                        "data/edgar/parsed",
                        &format!(
                            "{}_{}",
                            filing_obj
                                .get("report_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown"),
                            filing_obj
                                .get("filing_date")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        ),
                        store,
                    )
                    .await?;
                    log::info!(
                        "Filing added to vector store: {}_{}",
                        filing_obj
                            .get("report_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown"),
                        filing_obj
                            .get("filing_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                    );
                }
            }
        }
    }

    // Process earnings data if requested
    log::debug!("Checking if earnings data is requested");

    if let Some(earnings) = base_query.parameters.get("earnings") {
        log::debug!("Earnings data is requested");
        let _start_date = earnings
            .get("start_date")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("start_date missing or invalid"))?;
        let _end_date = earnings
            .get("end_date")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("end_date missing or invalid"))?;

        if let Err(e) = base_query.to_earnings_query() {
            log::error!("Failed to create earnings query: {}", e);
        } else {
            let earnings_query = base_query.to_earnings_query()?;
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
            process_earnings_transcripts(transcripts, store).await?;

            // Perform similarity search
            log::debug!("Performing similarity search for input: {}", input);
            let similar_docs = store
                .similarity_search(
                    input,
                    50,
                    &langchain_rust::vectorstore::VecStoreOptions::default(),
                )
                .await
                .map_err(|e| anyhow!("Failed to perform similarity search: {}", e))?;

            log::debug!(
                "Similarity search returned {} documents",
                similar_docs.len()
            );
            if similar_docs.is_empty() {
                log::warn!(
                    "No similar documents found in vector store for input: {}",
                    input
                );
            }

            log::debug!("Similar search returned: {:?}", similar_docs);

            // Format documents for LLM context
            let context = similar_docs
                .iter()
                .map(|doc| format!("Document (score: {:.3}): {}", doc.score, doc.page_content))
                .collect::<Vec<_>>()
                .join("\n\n");

            log::debug!("LLM context: {:?}", context);

            // Create prompt with context
            let prompt = format!(
                "Based on the following earnings call transcripts and financial documents, answer this question: {}\n\nContext:\n{}",
                input,
                context
            );

            // Return streaming response
            let prompt_args = prompt_args![
                "input" => [
                    "You are a helpful financial analyst assistant. Provide clear, concise answers based on the provided context.",
                    &prompt
                ]
            ];

            log::debug!("Sending prompt to LLM: {:?}", prompt_args);
            let stream = chain.stream(prompt_args).await?;
            log::debug!("LLM stream started successfully");
            // Get conversation summary
            let summary = get_conversation_summary(chain, input).await?;

            return Ok((
                Box::pin(stream.map(|r| {
                    r.map(|s| s.content)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                })),
                summary,
            ));
        }
    }

    if !has_processed_data {
        return Err(anyhow!("No data was processed"));
    }

    // Perform similarity search
    log::debug!("Performing similarity search for input: {}", input);
    let similar_docs = store
        .similarity_search(
            input,
            5, // Get top 5 most similar documents
            &langchain_rust::vectorstore::VecStoreOptions::default(),
        )
        .await
        .map_err(|e| anyhow!("Failed to perform similarity search: {}", e))?;

    log::debug!(
        "Similarity search returned {} documents",
        similar_docs.len()
    );
    if similar_docs.is_empty() {
        return Err(anyhow!("No similar documents found in vector store"));
    }

    // Format documents for LLM context
    log::info!("Documents found for context:");
    let context = similar_docs
        .iter()
        .map(|doc| {
            log::info!(
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
        context
    );

    // Create prompt with context
    let prompt = format!(
        "Based on the following SEC filings and financial documents, answer this question: {}\n\nContext:\n{}",
        input,
        context
    );

    // Return streaming response
    let prompt_args = prompt_args![
        "input" => [
            "You are a helpful financial analyst assistant. Provide clear, concise answers based on the provided context.",
            &prompt
        ]
    ];

    let stream = chain.stream(prompt_args).await?;
    log::debug!("LLM stream started successfully");
    // Get conversation summary
    let summary = get_conversation_summary(chain, input).await?;

    Ok((
        Box::pin(stream.map(|r| {
            r.map(|s| s.content)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })),
        summary,
    ))
}

async fn process_edgar_filings(
    filings: HashMap<String, filing::Filing>,
    store: &(dyn VectorStore + Send + Sync),
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

        match filing::extract_complete_submission_filing(input_file, store).await {
            Ok(parsed) => {
                if !output_path.exists() {
                    std::fs::create_dir_all(output_path.parent().unwrap())?;
                }
                let output_file = output_path.with_extension("json");
                std::fs::write(&output_file, serde_json::to_string_pretty(&parsed)?)?;
                log::info!("Saved parsed results to: {:?}", output_file);
            }
            Err(e) => log::error!("Failed to parse filing: {}", e),
        }
    }
    Ok(())
}

async fn process_earnings_transcripts(
    transcripts: Vec<earnings::Transcript>,
    store: &(dyn VectorStore + Send + Sync),
) -> Result<()> {
    for transcript in transcripts {
        log::info!(
            "Processing transcript for {} on {}",
            transcript.symbol,
            transcript.date
        );

        // Create document for vector storage
        let metadata_json = serde_json::json!({
            "symbol": transcript.symbol,
            "quarter": transcript.quarter,
            "year": transcript.year,
            "date": transcript.date,
            "type": "earnings_transcript"
        });

        let metadata: HashMap<String, Value> = metadata_json
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        // Define cache directory and filename
        let cache_dir = format!("data/earnings/parsed/{}", transcript.symbol);
        let cache_filename = format!("{}_Q{}", transcript.year, transcript.quarter);

        // Store the transcript using the chunking utility with caching
        crate::document::store_chunked_document_with_cache(
            transcript.content.clone(),
            metadata.clone(),
            &cache_dir,
            &cache_filename,
            store,
        )
        .await?;

        log::info!(
            "Added earnings transcript to vector store: {} Q{}",
            transcript.symbol,
            transcript.quarter
        );

        // Store the transcript using the chunking utility with caching
        log::info!("Storing earnings transcript in vector store");
        crate::document::store_chunked_document_with_cache(
            transcript.content,
            metadata,
            &cache_dir,
            &cache_filename,
            store,
        )
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

async fn extract_query_params(chain: &ConversationalChain, input: &str) -> Result<String> {
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
            Ok(result)
        }
        Err(e) => panic!("Error invoking LLMChain: {:?}", e),
    }
}
