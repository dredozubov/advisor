use advisor::edgar::filing;
use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "xbrl_parser", about = "Parse XBRL files from SEC EDGAR")]
struct Opt {
    /// Input file to parse
    #[structopt(parse(from_os_str))]
    input: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();

    // Ensure input file exists
    if !opt.input.exists() {
        eprintln!("Input file does not exist: {:?}", opt.input);
        std::process::exit(1);
    }

    // Parse the XBRL file
    match filing::extract_complete_submission_filing(opt.input.to_str().unwrap()) {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error parsing XBRL file: {}", e);
            std::process::exit(1);
        }
    }
}
use advisor::edgar::filing;
use anyhow::Result;
use langchain_rust::embedding::openai::OpenAiEmbedder;
use langchain_rust::llm::OpenAIConfig;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "xbrl_parser", about = "Parse XBRL filings")]
struct Opt {
    /// Input file to parse
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let opt = Opt::from_args();
    let openai_key = std::env::var("OPENAI_KEY").expect("OPENAI_KEY environment variable must be set");

    // Initialize OpenAI embedder
    let embedder = OpenAiEmbedder::default()
        .with_config(OpenAIConfig::default().with_api_key(openai_key));

    // Initialize SQLite vector storage
    let store = langchain_rust::vectorstore::sqlite_vss::StoreBuilder::new()
        .embedder(embedder)
        .connection_url("sqlite://data/vectors.db")
        .table("documents")
        .vector_dimensions(1536)
        .build()
        .await?;

    match filing::extract_complete_submission_filing(opt.input.to_str().unwrap(), &store).await {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}
