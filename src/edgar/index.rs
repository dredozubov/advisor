use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use csv::{ReaderBuilder, WriterBuilder};
use reqwest::Client;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use url::Url;

struct Config {
    index_start_date: NaiveDate,
    index_end_date: NaiveDate,
    full_index_data_dir: PathBuf,
    edgar_full_master_url: Url,
    edgar_archives_url: Url,
    index_files: Vec<String>,
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

async fn convert_idx_to_csv(filepath: &Path) -> Result<()> {
    let input_file = File::open(filepath).await?;
    let reader = BufReader::new(input_file);
    let mut lines = reader.lines();

    // Skip the first 10 lines
    for _ in 0..10 {
        lines.next_line().await?;
    }

    let output_path = filepath.with_extension("csv");
    let output_file = File::create(&output_path).await?;
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

    while let Some(line) = lines.next_line().await? {
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

async fn merge_idx_files() -> Result<()> {
    // Implementation of merge_idx_files function
    Ok(())
}

async fn fetch_and_save(client: &Client, url: &Url, filepath: &Path) -> Result<()> {
    let response = client.get(url.as_str()).send().await?;
    let content = response.bytes().await?;
    let mut file = File::create(filepath).await?;
    file.write_all(&content).await?;
    Ok(())
}

async fn update_full_index_feed(config: &Config) -> Result<()> {
    let dates_quarters =
        generate_folder_names_years_quarters(config.index_start_date, config.index_end_date).await;
    let latest_full_index_master = config.full_index_data_dir.join("master.idx");

    if latest_full_index_master.exists() {
        fs::remove_file(&latest_full_index_master).await?;
    }

    let client = Client::new();
    fetch_and_save(
        &client,
        &config.edgar_full_master_url,
        &latest_full_index_master,
    )
    .await?;
    convert_idx_to_csv(&latest_full_index_master).await?;

    for (year, qtr) in dates_quarters {
        for file in &config.index_files {
            let filepath = config.full_index_data_dir.join(&year).join(&qtr).join(file);
            let csv_filepath = filepath.with_extension("csv");

            if filepath.exists() {
                fs::remove_file(&filepath).await?;
            }
            if csv_filepath.exists() {
                fs::remove_file(&csv_filepath).await?;
            }

            if let Some(parent) = filepath.parent() {
                fs::create_dir_all(parent).await?;
            }
            let url = config
                .edgar_archives_url
                .join(&format!("edgar/full-index/{}/{}/{}", year, qtr, file))?;
            fetch_and_save(&client, &url, &filepath).await?;

            println!("\n\n\tConverting idx to csv\n\n");
            convert_idx_to_csv(&filepath).await?;
        }
    }

    println!("\n\n\tMerging IDX files\n\n");
    merge_idx_files().await?;
    println!("\n\n\tCompleted Index Download\n\n\t");

    Ok(())
}
