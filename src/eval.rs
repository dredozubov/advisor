use crate::edgar::{
    self,
    filing::{self, CompanyFilings},
    query::Query,
};
use anyhow::Result;
use chrono::NaiveDate;
use langchain_rust::{
    chain::{Chain, LLMChainBuilder},
    fmt_message, fmt_template,
    llm::{OpenAI, OpenAIConfig},
    message_formatter,
    prompt::HumanMessagePromptTemplate,
    prompt_args,
    schemas::Message,
    template_fstring,
};

pub async fn eval(
    input: &str,
    http_client: &reqwest::Client,
    llm: &OpenAI<OpenAIConfig>,
    _thread_id: &mut Option<String>,
) -> Result<String> {
    // Step 1: Extract date ranges and report types using Anthropic LLM
    let query_json = extract_query_params(llm, input).await?;
    println!("{}", query_json);

    // // Step 2: Construct Query and fetch data
    let query = Query::from_json(&query_json)?;
    // Fetch relevant filings based on the query
    for ticker in &query.tickers {
        log::info!("Fetching filings for ticker: {}", ticker);
        let filings = filing::fetch_matching_filings(http_client, &query).await?;

        // Process the fetched filings (you can modify this as needed)
        for filing in &filings {
            log::info!("Fetched filing: {:?}", filing);

            // Parse and save each fetched filing
            for filing in &filings {
                let company_name = ticker;
                let filing_type_with_date =
                    format!("{}_{}", filing.report_type, filing.filing_date);
                let output_file = format!(
                    "edgar_data/parsed/{}/{}.txt",
                    company_name, filing_type_with_date
                );

                // Ensure the parsed directory exists
                let parsed_dir = std::path::Path::new("edgar_data/parsed");
                if !parsed_dir.exists() {
                    log::debug!("Creating parsed directory: {:?}", parsed_dir);
                    if let Err(e) = std::fs::create_dir_all(parsed_dir) {
                        log::error!("Failed to create parsed directory: {}", e);
                        continue;
                    }
                }

                // Check if the output file already exists
                let output_path = std::path::Path::new(&output_file);
                if output_path.exists() {
                    log::info!("Output file already exists for filing: {}", output_file);
                } else {
                    log::debug!("Parsing filing: {}", filing.primary_document);
                    match filing::extract_complete_submission_filing(
                        &filing.primary_document,
                        Some(output_path),
                    ) {
                        Ok(_) => {
                            log::info!("Parsed and saved filing to {}", output_file);
                            log::debug!("Filing content: {:?}", filing);
                        }
                        Err(e) => log::error!("Failed to parse filing: {}", e),
                    }
                }
            }
        }
    }
    Ok("OK".to_string())
}

async fn extract_query_params(llm: &OpenAI<OpenAIConfig>, input: &str) -> Result<String> {
    println!("Starting extract_query_params with input: {}", input);
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
    
    Use reasonable defaults for missing values if they are missing. Do not format the response as markdown, provide only JSON string. If user asks for the latest report or latest quarterly report assume a date range from 'today - 90 days' and 'today'.
    
    Construct it from the user input:
    {input}"#, *edgar::report::REPORT_TYPES
    )
    .to_string();

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

async fn fetch_filings(
    query: &Query,
    client: &reqwest::Client,
    llm: &OpenAI<OpenAIConfig>,
) -> Result<Vec<filing::CompanyFilings>> {
    // Get all tickers data to find CIKs
    let tickers = edgar::tickers::fetch_tickers().await?;

    // Process each ticker in parallel using futures
    let filing_futures: Vec<_> = query
        .tickers
        .iter()
        .filter_map(|ticker| {
            // Find matching ticker data
            let ticker_data = tickers.iter().find(|(t, _, _)| t.as_str() == ticker);

            match ticker_data {
                Some(data) => {
                    // Get CIK from ticker data and clone it for the async closure
                    let cik = data.2.to_string();
                    let client = client.clone();
                    // Create future for fetching filings
                    Some(async move { filing::get_company_filings(&client, &cik, Some(10)).await })
                }
                None => {
                    log::warn!("Ticker not found: {}", ticker);
                    None
                }
            }
        })
        .collect();

    // Wait for all futures to complete
    let results = futures::future::join_all(filing_futures).await;

    // Collect successful results
    let mut filings = Vec::new();
    for result in results {
        match result {
            Ok(filing) => filings.push(filing),
            Err(e) => log::error!("Error fetching filings: {}", e),
        }
    }

    if filings.is_empty() {
        Err(anyhow::anyhow!(
            "No valid filings found for any provided tickers"
        ))
    } else {
        Ok(filings)
    }
}

// fn tokenize_filings(filings: &[filing::Filing]) -> Result<String> {
//     let tokenizer = Tokenizer::from_pretrained("gpt2", None)?;

//     let mut tokenized_data = String::new();
//     for filing in filings {
//         let tokens = tokenizer.encode(filing.content(), false)?;
//         tokenized_data.push_str(&tokens.get_tokens().join(" "));
//         tokenized_data.push('\n');
//     }

//     Ok(tokenized_data)
// }

// async fn get_llm_response(
//     client: &Client,
//     input: &str,
//     thread_id: &mut Option<String>,
// ) -> Result<String> {
//     let messages = {
//         vec![Message {
//             role: Role::User,
//             content: vec![ContentBlock::Text {
//                 text: input.to_string(),
//             }],
//         }]
//     };

//     let response = client.create_message(Arc::new(messages)).await?;

//     if thread_id.is_none() {
//         *thread_id = Some(response.id.clone());
//     }

//     Ok(response.completion)
// }
