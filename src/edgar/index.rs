use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use csv::WriterBuilder;
use futures::future::join_all;
use reqwest::Client;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tokio::task;
use url::Url;

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
        if fields.len() == 5 && !fields[0].contains("---") {
            let date = NaiveDate::parse_from_str(fields[3], "%Y-%m-%d")?;
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

async fn fetch_and_save(
    client: &Client,
    url: &Url,
    filepath: &Path,
    user_agent: &str,
) -> Result<()> {
    let response = client
        .get(url.as_str())
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .await?;
    let content = response.bytes().await?;
    let mut file = File::create(filepath)?;
    file.write_all(&content)?;
    Ok(())
}

async fn check_remote_file_modified(
    client: &Client,
    url: &Url,
    user_agent: &str,
) -> Result<DateTime<Utc>> {
    let response = client
        .head(url.as_str())
        .header(reqwest::header::USER_AGENT, user_agent)
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

            let remote_modified =
                check_remote_file_modified(client, &url, &config.user_agent).await?;

            remote_modified > local_modified
        } else {
            true
        };

        if should_update {
            println!("Updating file: {}", filepath.display());
            fetch_and_save(client, &url, &filepath, &config.user_agent).await?;
            println!("\n\n\tConverting idx to csv\n\n");
            convert_idx_to_csv(&filepath)?;
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

    // Check and update master.idx if necessary
    let should_update = if latest_full_index_master.exists() {
        let local_modified = fs::metadata(&latest_full_index_master)?.modified()?;
        let local_modified: DateTime<Utc> = local_modified.into();

        let remote_modified =
            check_remote_file_modified(&client, &config.edgar_full_master_url, &config.user_agent)
                .await?;

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
            &config.user_agent,
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
            let task = task::spawn(async move {
                process_quarter_data(&client, &config, &year, &qtr).await
            });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            result??;
        }
    }

    println!("\n\n\tCompleted Index Update\n\n\t");

    Ok(())
}
