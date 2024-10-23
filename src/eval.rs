use std::sync::Arc;

use crate::edgar::{
    filing,
    index::{update_full_index_feed, Config},
    query::Query,
};
use llm_chain::{executor, parameters, prompt, chains::{Chain, LLMChain}, traits::Executor};
use llm_chain_openai::chatgpt::Executor as ChatGPT;
use anyhow::Result;
use tokenizers::Tokenizer;

pub async fn eval(
    input: &str,
    config: &Config,
    llm_client: &ChatGPT,
    http_client: &reqwest::Client,
    thread_id: &mut Option<String>,
) -> Result<String> {
    // Step 1: Extract date ranges and report types using Anthropic LLM
    let query_json = extract_query_params(llm_client, input).await?;
    println!("{}", query_json);

    // // Step 2: Construct Query and fetch data
    let query = Query::from_json(&query_json)?;
    let filings = fetch_filings(&query, config, http_client).await?;

    // // Step 3: Tokenize fetched data
    // let tokenized_data = tokenize_filings(&filings)?;

    // // Step 4: Augment user input with tokenized data and get LLM response
    // let augmented_input = format!("{}\n\nContext:\n{}", input, tokenized_data);
    // let response = get_llm_response(llm_client, &augmented_input, thread_id).await?;

    // // Step 5: Update prompt
    // if let Some(id) = thread_id {
    //     println!("Thread ID: {}", id);
    // }
    let response = "".to_string();

    Ok(response)
}

async fn extract_query_params(client: &Client, input: &str) -> Result<String> {
    println!("Starting extract_query_params with input: {}", input);

    let user_message = format!("You are an AI assistant that extracts query parameters from user input. \
                             Return a JSON object with 'tickers', 'start_date', 'end_date', and 
    'report_types' fields. \
                             Use ISO date format (YYYY-MM-DD) for dates. Infer reasonable defaults if 
    information is missing. Extract query parameters from: {}", input);
    println!("Formatted user message: {}", user_message);

    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text { text: user_message }],
    }];
    println!("Created messages vector");

    let complete_request = CompleteRequestBuilder::default()
        .prompt(format!("{HUMAN_PROMPT}{{&user_message}}{AI_PROMPT}"))
        .model("claude-3-haiku-20240307".to_string())
        .max_tokens_to_sample(1000_usize)
        .stream(false)
        .stop_sequences(vec![HUMAN_PROMPT.to_string()])
        .build()?;
    println!("Built complete request");

    println!("Sending request to Anthropic API...");
    let response = client.complete(complete_request).await?;
    println!(
        "Received response from Anthropic API: {}",
        response.completion
    );

    Ok(response.completion)
}

async fn fetch_filings(
    query: &Query,
    config: &Config,
    client: &reqwest::Client,
) -> Result<Vec<filing::Filing>> {
    // Update index if necessary
    crate::edgar::index::update_full_index_feed(config).await?;

    // Fetch filings based on the query
    let mut filings = Vec::new();
    for ticker in &query.tickers {
        let filing = filing::process_filing(client, &[("ticker", ticker.as_str())]).await?;
        filings.push(filing);
    }

    Ok(filings)
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
