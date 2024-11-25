use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use reqwest::Client;
use tokio::sync::mpsc;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FetchTask {
    EdgarFiling {
        cik: String,
        filing: crate::edgar::filing::Filing,
        output_path: PathBuf,
    },
    EarningsTranscript {
        ticker: String,
        quarter: i32,
        year: i32,
        output_path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub task: FetchTask,
    pub status: FetchStatus,
    pub output_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FetchStatus {
    Success,
    Failed,
    Skipped,
}

impl FetchTask {
    pub async fn execute(&self, client: &Client) -> Result<FetchResult> {
        match self {
            FetchTask::EdgarFiling { cik, filing, output_path } => {
                match crate::edgar::filing::fetch_filing_document(client, cik, filing).await {
                    Ok(path) => Ok(FetchResult {
                        task: self.clone(),
                        status: FetchStatus::Success,
                        output_path: Some(PathBuf::from(path)),
                        error: None,
                    }),
                    Err(e) => Ok(FetchResult {
                        task: self.clone(),
                        status: FetchStatus::Failed,
                        output_path: None,
                        error: Some(e.to_string()),
                    }),
                }
            },
            FetchTask::EarningsTranscript { ticker, quarter, year, output_path } => {
                let date = chrono::NaiveDate::from_ymd_opt(*year, (quarter * 3) as u32, 1)
                    .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
                
                match crate::earnings::fetch_transcript(client, ticker, date).await {
                    Ok((_, path)) => Ok(FetchResult {
                        task: self.clone(),
                        status: FetchStatus::Success,
                        output_path: Some(path),
                        error: None,
                    }),
                    Err(e) => Ok(FetchResult {
                        task: self.clone(),
                        status: FetchStatus::Failed,
                        output_path: None,
                        error: Some(e.to_string()),
                    }),
                }
            }
        }
    }
}

pub struct FetchProgress {
    pub total: usize,
    pub completed: usize,
    pub successful: usize,
    pub failed: usize,
    pub current_task: Option<FetchTask>,
}

pub struct FetchManager {
    client: Client,
    progress_bar: ProgressBar,
}

impl FetchManager {
    pub fn new(total_tasks: usize) -> Self {
        let progress_bar = ProgressBar::new(total_tasks as u64);
        progress_bar.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        
        Self {
            client: Client::new(),
            progress_bar,
        }
    }

    pub async fn execute_tasks(&self, tasks: Vec<FetchTask>) -> Result<Vec<FetchResult>> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut handles = Vec::new();
        let total_tasks = tasks.len();

        for task in tasks {
            let tx = tx.clone();
            let client = self.client.clone();
            let pb = self.progress_bar.clone();

            let handle = tokio::spawn(async move {
                let result = task.execute(&client).await;
                match &result {
                    Ok(fetch_result) => {
                        pb.inc(1);
                        match fetch_result.status {
                            FetchStatus::Success => pb.set_message("✓"),
                            FetchStatus::Failed => pb.set_message("✗"),
                            FetchStatus::Skipped => pb.set_message("-"),
                        }
                    }
                    Err(_) => {
                        pb.inc(1);
                        pb.set_message("✗");
                    }
                }
                tx.send(result).await
            });
            handles.push(handle);
        }

        drop(tx);

        let mut results = Vec::with_capacity(total_tasks);
        while let Some(result) = rx.recv().await {
            results.push(result?);
        }

        for handle in handles {
            handle.await?;
        }

        self.progress_bar.finish_with_message("Download complete");
        Ok(results)
    }
}
