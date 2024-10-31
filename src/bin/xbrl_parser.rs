use advisor::edgar::filing;
use anyhow::Result;
use std::path::Path;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "xbrl_parser", about = "Parse XBRL files from SEC EDGAR")]
struct Opt {
    /// Input file to parse
    #[structopt(parse(from_os_str))]
    input: std::path::PathBuf,

    /// Output directory for parsed results
    #[structopt(parse(from_os_str), default_value = "parsed_output")]
    output_dir: std::path::PathBuf,
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
            // Create output directory if specified and it doesn't exist
            if !opt.output_dir.exists() {
                std::fs::create_dir_all(&opt.output_dir)?;
            }

            // Generate output filename based on input filename
            let input_filename = opt.input.file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("output");
            let output_path = opt.output_dir.join(format!("{}.json", input_filename));

            // Save the result to a file if output directory was specified
            if opt.output_dir.to_str().unwrap() != "parsed_output" {
                std::fs::write(
                    &output_path,
                    serde_json::to_string_pretty(&result)?,
                )?;
                println!("Results saved to: {:?}", output_path);
            }

            // Pretty print the JSON result to stdout
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("Error parsing XBRL file: {}", e);
            std::process::exit(1);
        }
    }
}
