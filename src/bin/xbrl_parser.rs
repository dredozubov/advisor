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
