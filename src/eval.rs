use std::fmt;

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
    let response = fetch_filings(&query, http_client, llm).await?;

    Ok("".to_string())
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
    let filing_futures: Vec<_> = query.tickers.iter()
        .filter_map(|ticker| {
            // Find matching ticker data
            let ticker_data = tickers.iter()
                .find(|(t, _, _)| t.as_str() == ticker);
            
            match ticker_data {
                Some(data) => {
                    // Get CIK from ticker data
                    let cik = data.2.clone();
                    // Create future for fetching filings
                    Some(filing::get_company_filings(client, &cik, Some(10)))
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
        Err(anyhow::anyhow!("No valid filings found for any provided tickers"))
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
