use advisor::edgar::parsing;
use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "xbrl_parser", about = "Parse and clean EDGAR XBRL files")]
struct Opt {
    /// Input XBRL file path
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output format (json or text)
    #[structopt(short, long, default_value = "json")]
    format: String,

    /// Show facts only
    #[structopt(long)]
    facts_only: bool,

    /// Show sections only
    #[structopt(long)]
    sections_only: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();

    // Read and parse the file
    let content = std::fs::read_to_string(&opt.input)?;

    if opt.facts_only {
        // Extract and output facts only
        let facts = parsing::extract_facts(&content)?;
        match opt.format.as_str() {
            "json" => println!("{}", serde_json::to_string_pretty(&facts)?),
            "text" => {
                for fact in facts {
                    println!("Fact: {}", fact.name);
                    println!("Value: {}", fact.formatted_value);
                    println!("Unit: {:?}", fact.unit);
                    println!("Period: {:?}", fact.period);
                    println!("---");
                }
            }
            _ => return Err(anyhow::anyhow!("Unsupported output format")),
        }
    } else if opt.sections_only {
        // Parse and output sections only
        let doc = parsing::parse_filing(&opt.input)?;
        match opt.format.as_str() {
            "json" => println!("{}", serde_json::to_string_pretty(&doc.sections)?),
            "text" => {
                for section in doc.sections {
                    println!("Section: {}", section.title);
                    println!("Type: {:?}", section.section_type);
                    println!("Content:");
                    println!("{}", section.content);
                    println!("---");
                }
            }
            _ => return Err(anyhow::anyhow!("Unsupported output format")),
        }
    } else {
        // Parse and output complete document
        let doc = parsing::parse_filing(&opt.input)?;
        match opt.format.as_str() {
            "json" => println!("{}", serde_json::to_string_pretty(&doc)?),
            "text" => {
                println!("Filing Document");
                println!("Path: {:?}", doc.path);
                println!("\nSections:");
                for section in doc.sections {
                    println!("\nSection: {}", section.title);
                    println!("Type: {:?}", section.section_type);
                    println!("Content:");
                    println!("{}", section.content);
                }
                println!("\nFacts:");
                for fact in doc.facts {
                    println!("\nFact: {}", fact.name);
                    println!("Value: {}", fact.formatted_value);
                    println!("Unit: {:?}", fact.unit);
                    println!("Period: {:?}", fact.period);
                }
            }
            _ => return Err(anyhow::anyhow!("Unsupported output format")),
        }
    }

    Ok(())
}
