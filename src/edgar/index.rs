use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use csv::WriterBuilder;
use futures::future::join_all;
use once_cell::sync::Lazy;
use reqwest::Client;
use sled::Db;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tokio::task;
use url::Url;

static USER_AGENT: Lazy<String> = Lazy::new(|| {
    Config::load()
        .map(|config| config.user_agent)
        .unwrap_or_else(|_| "software@example.com".to_string())
});

#[derive(Clone)]
#[derive(Clone)]
pub struct Config {
    pub index_start_date: NaiveDate,
    pub index_end_date: NaiveDate,
    pub full_index_data_dir: PathBuf,
    pub edgar_full_master_url: Url,
    pub edgar_archives_url: Url,
    pub index_files: Vec<String>,
    pub user_agent: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        // This is a placeholder implementation. You should replace this with your actual
        // configuration loading logic, e.g., reading from a file or environment variables.
        Ok(Config {
            index_start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
            index_end_date: Utc::now().date_naive(),
            full_index_data_dir: PathBuf::from("edgar_data"),
            edgar_full_master_url: Url::parse(
                "https://www.sec.gov/Archives/edgar/full-index/master.idx",
            )?,
            edgar_archives_url: Url::parse("https://www.sec.gov/Archives/")?,
            index_files: vec!["master.idx".to_string()],
            user_agent: "software@example.com".to_string(),
        })
    }
}

async fn generate_folder_names_years_quarters(
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current_date = start_date;
    while current_date <= end_date {
        let year = current_date.year().to_string();
        let quarter = ((current_date.month() - 1) / 3 + 1).to_string();
        result.push((year, format!("QTR{}", quarter)));
        current_date = current_date + chrono::Duration::days(90);
    }
    result
}

fn convert_idx_to_csv(filepath: &Path) -> Result<()> {
    let input_file = File::open(filepath)?;
    let reader = BufReader::new(input_file);
    let mut lines = reader.lines();

    // Skip the first 10 lines
    for _ in 0..10 {
        lines.next();
    }

    let output_path = filepath.with_extension("csv");
    let output_file = File::create(&output_path)?;
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .flexible(true) // Allow flexible number of fields
        .from_writer(output_file);

    writer.write_record(&[
        "CIK",
        "Company Name",
        "Form Type",
        "Date Filed",
        "Filename",
        "published",
    ])?;

    let mut records = Vec::new();

    for line in lines {
        let line = line?;
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() >= 5 && !fields[0].contains("---") {
            let date = NaiveDate::parse_from_str(fields[3], "%Y-%m-%d").unwrap_or_default();
            records.push((
                fields[0].to_string(),
                fields[1].to_string(),
                fields[2].to_string(),
                date,
                fields[4].to_string(),
            ));
        }
    }

    // Sort by date in descending order
    records.sort_by(|a, b| b.3.cmp(&a.3));

    for (cik, company, form_type, date, filename) in records {
        writer.write_record(&[
            &cik,
            &company,
            &form_type,
            &date.to_string(),
            &filename,
            &date.to_string(),
        ])?;
    }

    writer.flush()?;
    Ok(())
}

async fn fetch_and_save(client: &Client, url: &Url, filepath: &Path) -> Result<()> {
    let response = client
        .get(url.as_str())
        .header(reqwest::header::USER_AGENT, USER_AGENT.as_str())
        .send()
        .await?;
    let content = response.bytes().await?;
    let mut file = File::create(filepath)?;
    file.write_all(&content)?;
    Ok(())
}

async fn check_remote_file_modified(client: &Client, url: &Url) -> Result<DateTime<Utc>> {
    let response = client
        .head(url.as_str())
        .header(reqwest::header::USER_AGENT, USER_AGENT.as_str())
        .send()
        .await?;

    let last_modified = response
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .context("Last-Modified header not found")?
        .to_str()?;

    let last_modified = DateTime::parse_from_rfc2822(last_modified)?;
    Ok(last_modified.with_timezone(&Utc))
}

async fn process_quarter_data(
    client: &Client,
    config: &Config,
    year: &str,
    qtr: &str,
    _db: &Db,
) -> Result<()> {
    for file in &config.index_files {
        let filepath = config.full_index_data_dir.join(&year).join(&qtr).join(file);
        let _csv_filepath = filepath.with_extension("csv");

        if let Some(parent) = filepath.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create directory: {:?}", parent))?;
        }

        let url = config
            .edgar_archives_url
            .join(&format!("edgar/full-index/{}/{}/{}", year, qtr, file))?;

        let should_update = if filepath.exists() {
            let local_modified = fs::metadata(&filepath)?.modified()?;
            let local_modified: DateTime<Utc> = local_modified.into();

            let remote_modified = check_remote_file_modified(client, &url).await?;

            remote_modified > local_modified
        } else {
            true
        };

        if should_update {
            println!("Updating file: {}", filepath.display());
            super::utils::fetch_and_save(client, &url, &filepath, &USER_AGENT).await?;
            println!("\n\n\tConverting idx to csv\n\n");
            convert_idx_to_csv(&filepath)?;

            // Update sled database with the new data
            println!("\n\n\tUpdating edgar database\n\n");
        } else {
            println!("File is up to date: {}", filepath.display());
        }
    }

    Ok(())
}

pub async fn update_full_index_feed(config: &Config) -> Result<()> {
    fs::create_dir_all(&config.full_index_data_dir)
        .context("Failed to create full index data directory")?;

    let dates_quarters =
        generate_folder_names_years_quarters(config.index_start_date, config.index_end_date).await;
    let latest_full_index_master = config.full_index_data_dir.join("master.idx");

    let client = Client::new();

    // Open sled database
    let db_path = config.full_index_data_dir.join("merged_idx_files.sled");
    let db = sled::open(db_path)?;

    // Check and update master.idx if necessary
    let should_update = if latest_full_index_master.exists() {
        let local_modified = fs::metadata(&latest_full_index_master)?.modified()?;
        let local_modified: DateTime<Utc> = local_modified.into();

        let remote_modified =
            check_remote_file_modified(&client, &config.edgar_full_master_url).await?;

        remote_modified > local_modified
    } else {
        true
    };

    if should_update {
        println!("Updating master.idx file...");
        fetch_and_save(
            &client,
            &config.edgar_full_master_url,
            &latest_full_index_master,
        )
        .await?;
        convert_idx_to_csv(&latest_full_index_master)?;
    } else {
        println!("master.idx is up to date, using local version.");
    }

    // Process quarters in batches of 8
    for chunk in dates_quarters.chunks(8) {
        let chunk_vec: Vec<_> = chunk.to_vec();
        let mut tasks = Vec::new();

        for (year, qtr) in chunk_vec {
            let client = client.clone();
            let config = config.clone();
            let year = year.to_string();
            let qtr = qtr.to_string();
            let db = db.clone();
            let task = task::spawn(async move {
                process_quarter_data(&client, &config, &year, &qtr, &db).await
            });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            result??;
        }
    }

    println!("\n\n\tCompleted Index Update\n\n\t");

    // Flush the database to ensure all data is written
    db.flush()?;

    println!("\n\n\tCompleted Merging IDX files\n\n\t");

    Ok(())
}
