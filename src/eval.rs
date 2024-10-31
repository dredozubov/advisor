use crate::query::Query;
use crate::edgar::{self, filing};
use crate::earnings;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use langchain_rust::{
    chain::{Chain, LLMChainBuilder},
    fmt_message,
    llm::{OpenAI, OpenAIConfig},
    message_formatter, prompt_args,
    schemas::Message,
};

pub async fn eval(
    input: &str,
    http_client: &reqwest::Client,
    llm: &OpenAI<OpenAIConfig>, // Use llm as it is needed in the function
    _thread_id: &mut Option<String>,
) -> Result<String> {
    match extract_query_params(llm, input).await {
        Ok(query_json) => {
            // Step 1: Extract date ranges and report types using Anthropic LLM
            println!("{:?}", query_json);

            // Parse into our new high-level Query type
            let base_query: Query = serde_json::from_str(&query_json)?;
            
            // Add EDGAR query if report types were specified
            let query = if let Some(report_types) = extract_report_types(&query_json)? {
                base_query.with_edgar_query(report_types)
            } else {
                base_query
            };

            // Process EDGAR filings if requested
            if let Some(edgar_query) = &query.edgar_query {
                for ticker in &edgar_query.tickers {
                    log::info!("Fetching EDGAR filings for ticker: {}", ticker);
                    let filings = filing::fetch_matching_filings(http_client, edgar_query).await?;
                    process_edgar_filings(filings)?;
                }
            }

            // Process earnings data if requested
            if let Some(earnings_query) = &query.earnings_query {
                log::info!("Fetching earnings data for ticker: {}", earnings_query.ticker);
                let transcripts = earnings::fetch_transcripts(
                    http_client,
                    &earnings_query.ticker,
                    earnings_query.start_date,
                    earnings_query.end_date,
                ).await?;
                process_earnings_transcripts(transcripts)?;
            }

            Ok("Query processed successfully".to_string())
        }
        Err(e) => {
            log::error!("Failure to create an EDGAR query: {e}");
            Err(anyhow!("Failed to create query: {}", e))
        }
    }

}

fn extract_report_types(query_json: &str) -> Result<Option<Vec<edgar::report::ReportType>>> {
    let v: serde_json::Value = serde_json::from_str(query_json)?;
    if let Some(types) = v.get("report_types") {
        let report_types = types
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("report_types is not an array"))?
            .iter()
            .map(|t| t.as_str().unwrap().parse())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(report_types))
    } else {
        Ok(None)
    }
}

fn process_edgar_filings(filings: HashMap<String, filing::Filing>) -> Result<()> {
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

        match filing::extract_complete_submission_filing(input_file) {
            Ok(parsed) => {
                if !output_path.exists() {
                    std::fs::create_dir_all(output_path.parent().unwrap())?;
                }
                let output_file = output_path.with_extension("json");
                std::fs::write(
                    &output_file,
                    serde_json::to_string_pretty(&parsed)?,
                )?;
                log::info!("Saved parsed results to: {:?}", output_file);
            }
            Err(e) => log::error!("Failed to parse filing: {}", e),
        }
    }
    Ok(())
}

fn process_earnings_transcripts(transcripts: Vec<earnings::Transcript>) -> Result<()> {
    for transcript in transcripts {
        log::info!("Processing transcript for {} on {}", transcript.ticker, transcript.date);
        // Add transcript processing logic here
    }
    Ok(())
}

async fn extract_query_params(llm: &OpenAI<OpenAIConfig>, input: &str) -> Result<String> {
    println!("Starting extract_query_params with input: {}", input);
    let now = chrono::Local::now();
    let _today_year = now.format("%Y");
    let _today_month = now.format("%M");
    let _today_day = now.format("%d");
    let task = format!(
        r#"Extract the following parameters from the input text:
    - Company tickers
    - Date ranges
    - Report types.
    Format these parameters as a JSON object with fields:
    - 'tickers': array of strings
    - 'start_date': ISO date (YYYY-MM-DD)
    - 'end_date': ISO date (YYYY-MM-DD)
    - 'report_types': array of strings, possible values are {}
    
    Use reasonable defaults for missing values if they are missing. Do not format the response as markdown, provide only JSON string. If user asks for the latest report or latest quarterly report assume a date range from 'today - 90 days' and 'today'. Current date is {}".
    
    Construct it from the user input:
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
            println!("Result: {:?}", result);
            Ok(result)
        }
        Err(e) => panic!("Error invoking LLMChain: {:?}", e),
    }
}
