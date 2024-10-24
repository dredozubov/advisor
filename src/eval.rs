use crate::edgar::{filing, index::Config, query::Query};
use anyhow::Result;
use llm_chain::{
    chains::{sequential, Chain},
    parameters, prompt,
    step::Step,
    traits::Executor,
};
use llm_chain_openai::chatgpt::Executor as ChatGPT;

pub async fn eval(
    input: &str,
    config: &Config,
    http_client: &reqwest::Client,
    llm: &ChatGPT,
    _thread_id: &mut Option<String>,
) -> Result<String> {
    // Step 1: Extract date ranges and report types using Anthropic LLM
    let query_json = extract_query_params(llm, input).await?;
    println!("{}", query_json);

    // // Step 2: Construct Query and fetch data
    let query = Query::from_json(&query_json)?;
    let response = fetch_filings(&query, config, http_client, llm).await?;

    Ok(response)
}

async fn extract_query_params(llm: &ChatGPT, input: &str) -> Result<String> {
    println!("Starting extract_query_params with input: {}", input);

    // Create prompt templates
    let extract_prompt = PromptTemplate::new(
        "Extract the following parameters from the input text:\n- Company tickers\n- Date ranges\n- Report types\n\nInput: {input}".to_string(),
        vec!["input".to_string()]
    );

    let format_prompt = PromptTemplate::new(
        r#"Format these parameters as a JSON object with fields:
        - 'tickers': array of strings
        - 'start_date': ISO date (YYYY-MM-DD)
        - 'end_date': ISO date (YYYY-MM-DD)
        - 'report_types': array of strings
        
        Use reasonable defaults for missing values.
        
        Parameters to format:
        {text}"#.to_string(),
        vec!["text".to_string()]
    );

    // Create sequential chain
    let chain = SequentialChain::new(vec![
        Box::new(extract_prompt),
        Box::new(format_prompt),
    ]);

    // Run chain with system context
    let messages = Messages::new()
        .with_system("You are an AI assistant that extracts and formats query parameters.")
        .with_user(input);
    
    let result = chain.run(messages, llm).await?;

    println!("Extracted parameters: {}", result);
    Ok(result)
}

async fn fetch_filings(
    query: &Query,
    config: &Config,
    client: &reqwest::Client,
    llm: &ChatGPT,
) -> Result<String> {
    // Update index if necessary
    crate::edgar::index::update_full_index_feed(config).await?;

    // Create prompt for analyzing filings
    let analyze_prompt = PromptTemplate::new(
        "Analyze this SEC filing and extract key information:\n{content}".to_string(),
        vec!["content".to_string()]
    );

    let mut results = Vec::new();
    for ticker in &query.tickers {
        let filing = filing::process_filing(client, &[("ticker", ticker.as_str())]).await?;
        
        // Run analysis on each filing
        let messages = Messages::new()
            .with_system("You are a financial analyst specialized in SEC filings.")
            .with_user(&filing.content());
        
        let analysis = analyze_prompt.format(messages).await?;
        results.push(analysis);
    }

    // Combine results
    Ok(results.join("\n\n"))
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
