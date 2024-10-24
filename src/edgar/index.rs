use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use csv::WriterBuilder;
use futures::future::join_all;
use once_cell::sync::Lazy;
use reqwest::Client;
use sled::{Db, IVec};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tokio::task;
use url::Url;

pub const EDGAR_FULL_MASTER_URL: &str = "https://www.sec.gov/Archives/edgar/full-index/master.idx";
pub const EDGAR_ARCHIVES_URL: &str = "https://www.sec.gov/Archives/";
pub const INDEX_FILES: &[&str] = &["master.idx", "form.idx", "company.idx"];
pub const USER_AGENT: &str = "Example@example.com";
pub const FULL_INDEX_DATA_DIR: &str = "edgar_data/";

static DB_PATH: Lazy<PathBuf> =
    Lazy::new(|| get_full_index_data_dir().join("merged_idx_files.sled"));

pub fn get_edgar_full_master_url() -> Url {
    Url::parse(EDGAR_FULL_MASTER_URL).expect("Invalid EDGAR master URL")
}

pub fn get_edgar_archives_url() -> Url {
    Url::parse(EDGAR_ARCHIVES_URL).expect("Invalid EDGAR archives URL")
}

pub fn get_full_index_data_dir() -> PathBuf {
    PathBuf::from(FULL_INDEX_DATA_DIR)
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
        .header(reqwest::header::USER_AGENT, USER_AGENT)
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
        .header(reqwest::header::USER_AGENT, USER_AGENT)
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

async fn process_quarter_data(client: &Client, year: &str, qtr: &str) -> Result<()> {
    for file in INDEX_FILES {
        let filepath = get_full_index_data_dir().join(&year).join(&qtr).join(file);
        let _csv_filepath = filepath.with_extension("csv");

        if let Some(parent) = filepath.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create directory: {:?}", parent))?;
        }

        let url = get_edgar_archives_url()
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
            super::utils::fetch_and_save(client, &url, &filepath, USER_AGENT).await?;
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

fn store_date_range(db: &Db, start_date: NaiveDate, end_date: NaiveDate) -> Result<()> {
    db.insert("index_start_date", start_date.to_string().as_bytes())?;
    db.insert("index_end_date", end_date.to_string().as_bytes())?;
    Ok(())
}

fn get_date_range(db: &Db) -> Result<Option<(NaiveDate, NaiveDate)>> {
    let start = db.get("index_start_date")?;
    let end = db.get("index_end_date")?;

    match (start, end) {
        (Some(start), Some(end)) => {
            let start_str = String::from_utf8(start.to_vec())?;
            let end_str = String::from_utf8(end.to_vec())?;
            let start_date = NaiveDate::parse_from_str(&start_str, "%Y-%m-%d")?;
            let end_date = NaiveDate::parse_from_str(&end_str, "%Y-%m-%d")?;
            Ok(Some((start_date, end_date)))
        }
        _ => Ok(None),
    }
}

pub async fn update_full_index_feed(
    index_start_date: NaiveDate,
    index_end_date: NaiveDate,
) -> Result<()> {
    log::debug!("Opening sled database for update check at {:?}", &*DB_PATH);
    let db = sled::open(&*DB_PATH)?;
    log::debug!("Successfully opened sled database");

    // Check if we need to update
    log::debug!("Checking if update is needed");
    let should_update = if !DB_PATH.exists() {
        log::debug!("No index database found at {:?}", &*DB_PATH);
        false // No database, don't try to update
    } else if let Some((stored_start, stored_end)) = get_date_range(&db)? {
        println!(
            "DEBUG: Found stored date range: {} to {}",
            stored_start, stored_end
        );
        let needs_update = index_start_date < stored_start || index_end_date > stored_end;
        log::debug!("Update needed: {}", needs_update);
        needs_update
    } else {
        log::debug!("No stored date range found in existing database");
        true // Database exists but no date range stored, need to update
    };

    if should_update {
        update_index_feed(index_start_date, index_end_date).await?;
    } else {
        println!("Index is up to date for the requested date range");
    }

    Ok(())
}

async fn update_index_feed(index_start_date: NaiveDate, index_end_date: NaiveDate) -> Result<()> {
    fs::create_dir_all(get_full_index_data_dir())
        .context("Failed to create full index data directory")?;

    let dates_quarters =
        generate_folder_names_years_quarters(index_start_date, index_end_date).await;
    let latest_full_index_master = get_full_index_data_dir().join("master.idx");

    let client = Client::new();

    // Open sled database
    let db_path = get_full_index_data_dir().join("merged_idx_files.sled");
    let db = sled::open(db_path)?;

    // Check and update master.idx if necessary
    let should_update = if latest_full_index_master.exists() {
        let local_modified = fs::metadata(&latest_full_index_master)?.modified()?;
        let local_modified: DateTime<Utc> = local_modified.into();

        let remote_modified =
            check_remote_file_modified(&client, &get_edgar_full_master_url()).await?;

        remote_modified > local_modified
    } else {
        true
    };

    if should_update {
        println!("Updating master.idx file...");
        fetch_and_save(
            &client,
            &get_edgar_full_master_url(),
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
            let year = year.to_string();
            let qtr = qtr.to_string();
            let task = task::spawn(async move { process_quarter_data(&client, &year, &qtr).await });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            result??;
        }
    }

    println!("\n\n\tCompleted Index Update\n\n\t");

    // Create or open sled database
    log::debug!("Creating/Opening sled database at {:?}", &*DB_PATH);
    if DB_PATH.exists() {
        log::debug!("Removing old sled database at {:?}", &*DB_PATH);
        fs::remove_dir_all(&*DB_PATH)?;
    }
    let db = sled::open(&*DB_PATH)?;
    log::debug!("Successfully created new sled database");

    // Store the date range in the database
    log::debug!("Storing new date range in database");
    store_date_range(&db, index_start_date, index_end_date)?;
    log::debug!("Successfully stored date range");

    // Flush the database to ensure all data is written
    log::debug!("Flushing database");
    println!(
        "DEBUG: Date range stored: {} to {}",
        index_start_date, index_end_date
    );
    db.flush()?;
    log::debug!("Successfully flushed database");

    println!("\n\n\tCompleted Merging IDX files\n\n\t");

    Ok(())
}
