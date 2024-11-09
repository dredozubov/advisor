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

    // Ensure data directory exists
    std::fs::create_dir_all("data")?;
    
    // Initialize SQLite vector storage with absolute path
    let db_path = std::env::current_dir()?.join("data").join("vectors.db");
    let connection_url = format!("sqlite://{}", db_path.display());
    
    let store = langchain_rust::vectorstore::sqlite_vss::StoreBuilder::new()
        .embedder(embedder)
        .connection_url(&connection_url)
        .table("documents")
        .vector_dimensions(1536)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create SQLite store: {}", e))?;

    match filing::extract_complete_submission_filing(opt.input.to_str().unwrap(), &store).await {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to extract filing: {}", e))
    }
}
