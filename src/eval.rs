use crate::edgar::{self, filing, query::Query};
use anyhow::Result;
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

            // // Step 2: Construct Query and fetch data
            let query = Query::from_json(&query_json)?;
            // Fetch relevant filings based on the query
            for ticker in &query.tickers {
                log::info!("Fetching filings for ticker: {}", ticker);
                let filings = filing::fetch_matching_filings(http_client, &query).await?;

                // Process the fetched filings (you can modify this as needed)
                for (input_file, filing) in &filings {
                    log::info!("Fetched filing ({:?}): {:?}", input_file, filing);
                    let company_name = ticker;
                    let filing_type_with_date =
                        format!("{}_{}", filing.report_type, filing.filing_date);
                    let output_file_path = format!(
                        "edgar_data/parsed/{}/{}",
                        company_name, filing_type_with_date
                    );

                    // Check if the output file already exists
                    let output_path = std::path::Path::new(&output_file_path);
                    if output_path.exists() {
                        log::info!(
                            "Output file already exists for filing: {}",
                            output_file_path
                        );
                    } else {
                        log::debug!(
                            "Parsing filing: {} with output path: {:?}",
                            &input_file,
                            output_path.parent().unwrap()
                        );

                        match filing::extract_complete_submission_filing(input_file) {
                            Ok(parsed) => {
                                log::debug!("{:?}", parsed.keys());
                                log::debug!("Filing content: {:?}", filing);
                            }
                            Err(e) => log::error!("Failed to parse filing: {}", e),
                        }
                    }
                }
            }
        }
        Err(e) => {
            log::error!("Failure to create an EDGAR query: {e}")
        }
    }

    Ok("OK".to_string())
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
