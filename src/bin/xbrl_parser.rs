use advisor::edgar::parsing::xbrl::parser::xml::{XBRLFiling, FactItem, FactTableRow, DimensionTableRow};
use anyhow::Result;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "xbrl_parser", about = "Parse EDGAR XBRL files using reference implementation")]
struct Opt {
    /// Input XBRL file path
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output format (json, facts, dimensions)
    #[structopt(short, long, default_value = "json")]
    format: String,

    /// Email for SEC API access
    #[structopt(short, long, default_value = "example@example.com")]
    email: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opt = Opt::from_args();

    let input_path = opt.input.to_string_lossy().to_string();
    let output_types = match opt.format.as_str() {
        "json" => vec!["json"],
        "facts" => vec!["facts"],
        "dimensions" => vec!["dimensions"],
        "all" => vec!["json", "facts", "dimensions"],
        _ => return Err(anyhow::anyhow!("Unsupported output format")),
    };

    // Parse using reference implementation
    let filing = XBRLFiling::new(input_path, opt.email, output_types);

    // Output based on format
    match opt.format.as_str() {
        "json" => {
            if let Some(facts) = filing.json {
                println!("{}", serde_json::to_string_pretty(&facts)?);
            }
        }
        "facts" => {
            if let Some(facts) = filing.facts {
                print_facts_table(&facts);
            }
        }
        "dimensions" => {
            if let Some(dimensions) = filing.dimensions {
                print_dimensions_table(&dimensions);
            }
        }
        "all" => {
            println!("JSON Facts:");
            if let Some(facts) = filing.json {
                println!("{}", serde_json::to_string_pretty(&facts)?);
            }
            println!("\nFacts Table:");
            if let Some(facts) = filing.facts {
                print_facts_table(&facts);
            }
            println!("\nDimensions Table:");
            if let Some(dimensions) = filing.dimensions {
                print_dimensions_table(&dimensions);
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn print_facts_table(facts: &[FactTableRow]) {
    for fact in facts {
        println!("Tag: {}", fact.tag);
        println!("Value: {}", fact.value);
        println!("Prefix: {}", fact.prefix);
        if let Some(ref unit) = fact.unit {
            println!("Unit: {}", unit);
        }
        if let Some(ref start) = fact.period_start {
            println!("Period Start: {}", start);
        }
        if let Some(ref end) = fact.period_end {
            println!("Period End: {}", end);
        }
        if let Some(ref point) = fact.point_in_time {
            println!("Point in Time: {}", point);
        }
        println!("Dimensions: {}", fact.num_dim);
        println!("---");
    }
}

fn print_dimensions_table(dimensions: &[DimensionTableRow]) {
    for dim in dimensions {
        println!("Context: {}", dim.context_ref);
        println!("Axis: {}:{}", dim.axis_prefix, dim.axis_tag);
        println!("Member: {}:{}", dim.member_prefix, dim.member_tag);
        println!("---");
    }
}
