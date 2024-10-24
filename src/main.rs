use chrono::NaiveDate;
use claude_api_interaction::edgar::index::Config;
use claude_api_interaction::eval;
use claude_api_interaction::repl;
use langchain_rust::llm::OpenAIConfig;
use langchain_rust::{
    chain::{Chain, LLMChainBuilder},
    fmt_message, fmt_placeholder, fmt_template,
    language_models::llm::LLM,
    llm::openai::{OpenAI, OpenAIModel},
    message_formatter,
    prompt::HumanMessagePromptTemplate,
    prompt_args,
    schemas::messages::Message,
    template_fstring,
};
use rustyline::error::ReadlineError;
use std::env;
use std::error::Error;
use std::path::PathBuf;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the logger.
    env_logger::init();

    // Initialize ChatGPT executor with API key from environment
    let openai_key = env::var("OPENAI_KEY").expect("OPENAI_KEY environment variable must be set");
    let open_ai = OpenAI::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key))
        .with_model(OpenAIModel::Gpt4oMini.to_string());
    // Create a Config instance
    let config = Config {
        index_start_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        index_end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
        full_index_data_dir: PathBuf::from("edgar_data/"),
        edgar_full_master_url: Url::parse(
            "https://www.sec.gov/Archives/edgar/full-index/master.idx",
        )?,
        edgar_archives_url: Url::parse("https://www.sec.gov/Archives/")?,
        index_files: vec![
            "master.idx".to_string(),
            "form.idx".to_string(),
            "company.idx".to_string(),
        ],
        user_agent: "Example@example.com".to_string(),
    };

    // Create a rustyline Editor
    let mut rl = repl::create_editor().await?;

    // You can also include timeouts and other settings
    let http_client = reqwest::Client::builder()
        .user_agent("software@example.com")
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create client");

    println!("Enter 'quit' to exit");
    let mut thread_id = None;
    loop {
        let prompt = thread_id
            .as_ref()
            .map_or(">> ".to_string(), |id| format!("{}> ", id));
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.eq_ignore_ascii_case("quit") {
                    break;
                }

                // Process the input using the eval function
                match eval::eval(input, &config, &http_client, &open_ai, &mut thread_id).await {
                    Ok(result) => println!("{}", result),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    println!("Goodbye!");

    // Save history
    repl::save_history(&mut rl)?;

    Ok(())
}
