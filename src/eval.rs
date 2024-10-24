use std::sync::Arc;

use crate::edgar::{
    filing,
    index::{update_full_index_feed, Config},
    query::Query,
};
use anyhow::Result;
use llm_chain::{
    chains::{map_reduce, sequential},
    executor, options, parameters, prompt,
    step::Step,
    traits::Executor,
};
use tokenizers::Tokenizer;

pub async fn eval<E: Executor>(
    input: &str,
    config: &Config,
    llm_client: &E,
    http_client: &reqwest::Client,
    thread_id: &mut Option<String>,
) -> Result<String> {
    // Step 1: Extract date ranges and report types using Anthropic LLM
    let query_json = extract_query_params(llm_client, input).await?;
    println!("{}", query_json);

    // // Step 2: Construct Query and fetch data
    let query = Query::from_json(&query_json)?;
    let response = fetch_filings(&query, config, http_client, llm_client).await?;

    Ok(response)
}

async fn extract_query_params<E: Executor>(client: &E, input: &str) -> Result<String> {
    println!("Starting extract_query_params with input: {}", input);

    // Create a chain of steps for parameter extraction
    let chain = sequential::Chain::new(vec![
        // First step: Extract raw parameters
        Step::for_prompt_template(prompt!(
            "You are an AI assistant that extracts query parameters from user input.",
            "Extract the following parameters from the input text:\n- Company tickers\n- Date ranges\n- Report types\n\nInput: {{input}}"
        )),
        // Second step: Format as JSON
        Step::for_prompt_template(prompt!(
            "You are an AI assistant that formats extracted parameters as JSON.",
            r#"Format these parameters as a JSON object with fields:
            - 'tickers': array of strings
            - 'start_date': ISO date (YYYY-MM-DD)
            - 'end_date': ISO date (YYYY-MM-DD)
            - 'report_types': array of strings
            
            Use reasonable defaults for missing values.
            
            Parameters to format:
            {{text}}"#
        ))
    ]);

    // Run the chain
    let params = parameters!("input" => input);
    let response = chain.run(params, client).await?;
    let result = response.to_immediate().await?.as_content();
    
    println!("Extracted parameters: {}", result);
    Ok(result)
}

async fn fetch_filings(
    query: &Query,
    config: &Config,
    client: &reqwest::Client,
    executor: &impl Executor,
) -> Result<String> {
    // Update index if necessary
    crate::edgar::index::update_full_index_feed(config).await?;

    // Create map-reduce chain for processing filings
    let map_prompt = Step::for_prompt_template(prompt!(
        "You are a financial document analyzer specialized in SEC filings. Focus on key metrics, material changes, and risk factors.",
        r#"Analyze this SEC filing and extract key information in bullet points:
        - Financial metrics and changes
        - Material business updates
        - Risk factors and concerns
        - Notable disclosures
        
        Filing text:
        {{text}}"#
    ));

    let reduce_prompt = Step::for_prompt_template(prompt!(
        "You are a financial report summarizer specialized in creating executive summaries.",
        r#"Create a comprehensive summary of these filing analyses. Structure it as:
        1. Key Financial Highlights
        2. Business Updates
        3. Risk Assessment
        4. Notable Disclosures
        
        Analyses to combine:
        {{text}}"#
    ));

    let chain = map_reduce::Chain::new(map_prompt, reduce_prompt);

    // Fetch and process filings
    let mut filing_params = Vec::new();
    for ticker in &query.tickers {
        let filing = filing::process_filing(client, &[("ticker", ticker.as_str())]).await?;
        filing_params.push(parameters!(filing.content()));
    }

    // Run map-reduce chain
    let result = chain.run(filing_params, parameters!(), executor).await?;
    Ok(result.to_immediate().await?.as_content())
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
