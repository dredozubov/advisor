use std::fmt;

use crate::edgar::{self, filing, query::Query};
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
    
    Use reasonable defaults for missing values if they are missing. Do not format the response as markdown, provide only JSON string.
    
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
) -> Result<String> {
    use edgar::index::{
        self, get_edgar_archives_url, get_edgar_full_master_url, get_full_index_data_dir,
    };

    // Update index if necessary
    index::update_full_index_feed(query.start_date, query.end_date).await?;

    // TODO: Implement filing retrieval logic
    Ok("Index checked/updated successfully".to_string())
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
