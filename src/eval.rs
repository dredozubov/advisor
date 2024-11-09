use crate::earnings;
use crate::edgar::{self, filing};
use crate::query::Query;
use langchain_rust::vectorstore::VectorStore;
use anyhow::{anyhow, Result};
use langchain_rust::{
    chain::{Chain, LLMChainBuilder},
    fmt_message,
    llm::{OpenAI, OpenAIConfig},
    message_formatter, prompt_args,
    schemas::Message,
    language_models::llm::LLM,
};
use std::collections::HashMap;

pub async fn eval(
    input: &str,
    http_client: &reqwest::Client,
    llm: &OpenAI<OpenAIConfig>,
    _thread_id: &mut Option<String>,
    store: &langchain_rust::vectorstore::sqlite_vss::Store,
) -> Result<futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>> {
    match extract_query_params(llm, input).await {
        Ok(query_json) => {
            // Step 1: Extract date ranges and report types using Anthropic LLM
            println!("{:?}", query_json);

            // Parse into our new high-level Query type
            let base_query: Query = serde_json::from_str(&query_json)?;

            // Process EDGAR filings if requested
            if let Some(_filings) = base_query.parameters.get("filings") {
                if let Some(_filings) = base_query.parameters.get("filings") {
                    match base_query.to_edgar_query() {
                        Ok(edgar_query) => {
                            for ticker in &edgar_query.tickers {
                                log::info!("Fetching EDGAR filings for ticker: {}", ticker);
                                let filings =
                                    filing::fetch_matching_filings(http_client, &edgar_query)
                                        .await?;
                                process_edgar_filings(filings, store).await?;
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to create EDGAR query: {}", e);
                        }
                    }
                }
            }

            // Process earnings data if requested
            if let Some(_earnings) = base_query.parameters.get("earnings") {
                match base_query.to_earnings_query() {
                    Ok(earnings_query) => {
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
                        let similar_docs = store.similarity_search(
                            input,
                            5,  // Get top 5 most similar documents
                            &langchain_rust::vectorstore::VecStoreOptions::default()
                        ).await.map_err(|e| anyhow!("Failed to perform similarity search: {}", e))?;

                        // Format documents for LLM context
                        let context = similar_docs.iter()
                            .map(|doc| format!("Document (score: {:.3}): {}", doc.score, doc.page_content))
                            .collect::<Vec<_>>()
                            .join("\n\n");

                        // Create prompt with context
                        let prompt = format!(
                            "Based on the following earnings call transcripts and financial documents, answer this question: {}\n\nContext:\n{}",
                            input,
                            context
                        );

                        // Return streaming response
                        return Ok(llm.stream(vec![
                            langchain_rust::schemas::Message::new_system_message(
                                "You are a helpful financial analyst assistant. Provide clear, concise answers based on the provided context."
                            ),
                            langchain_rust::schemas::Message::new_human_message(&prompt)
                        ]).await?)
                    }
                    Err(e) => {
                        log::error!("Failed to create earnings query: {}", e);
                    }
                }
            }

            Err(anyhow!("No response generated"))
        }
        Err(e) => {
            log::error!("Failure to create an EDGAR query: {e}");
            Err(anyhow!("Failed to create query: {}", e))
        }
    }
}

async fn process_edgar_filings(
    filings: HashMap<String, filing::Filing>,
    store: &langchain_rust::vectorstore::sqlite_vss::Store,
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
    store: &langchain_rust::vectorstore::sqlite_vss::Store,
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
        
        let metadata = metadata_json.as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let doc = langchain_rust::schemas::Document {
            page_content: transcript.content,
            metadata: metadata,
            score: 0.0, // Default score since it's required
        };

        // Store the document in vector storage
        store.add_documents(&[doc], &langchain_rust::vectorstore::VecStoreOptions::default())
            .await
            .map_err(|e| anyhow!("Failed to store transcript in vector storage: {}", e))?;

        log::info!("Stored transcript in vector storage");
    }
    Ok(())
}

async fn extract_query_params(llm: &OpenAI<OpenAIConfig>, input: &str) -> Result<String> {
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

    log::info!("Task: {task}");

    // We can also guide it's response with a prompt template. Prompt templates are used to convert raw user input to a better input to the LLM.
    let prompt = message_formatter![
        fmt_message!(Message::new_system_message(
            "You are the parser assising human with turning natural language response into structured JSON."
        )),
        fmt_message!(Message::new_human_message(task))
    ];

    let chain = LLMChainBuilder::new()
        .prompt(prompt)
        .llm(llm.clone())
        .build()
        .unwrap();

    match chain.invoke(prompt_args! {}).await {
        Ok(result) => {
            log::debug!("Result: {:?}", result);
            Ok(result)
        }
        Err(e) => panic!("Error invoking LLMChain: {:?}", e),
    }
}
