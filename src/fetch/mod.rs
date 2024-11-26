use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use langchain_rust::vectorstore::VectorStore;
use reqwest::Client;
use serde::{Deserialize, Serialize}; 
use sqlx::{Pool, Postgres};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FetchTask {
    EdgarFiling {
        cik: String,
        filing: crate::edgar::filing::Filing,
        output_path: PathBuf,
        #[serde(skip)]
        progress_bar: Option<ProgressBar>,
    },
    EarningsTranscript {
        ticker: String,
        quarter: i32,
        year: i32,
        output_path: PathBuf,
        #[serde(skip)]
        progress_bar: Option<ProgressBar>,
    },
}

impl FetchTask {
    pub fn progress_bar(&self) -> Option<&ProgressBar> {
        match self {
            FetchTask::EdgarFiling { progress_bar, .. } => progress_bar.as_ref(),
            FetchTask::EarningsTranscript { progress_bar, .. } => progress_bar.as_ref(),
        }
    }

    pub fn new_edgar_filing(
        multi: &MultiProgress,
        cik: String,
        filing: crate::edgar::filing::Filing,
        output_path: PathBuf,
    ) -> Self {
        let desc = format!("Filing {} {}", filing.report_type, filing.accession_number);
        let pb = multi.add(ProgressBar::new(100));
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        pb.set_message(desc);

        FetchTask::EdgarFiling {
            cik,
            filing,
            output_path,
            progress_bar: Some(pb),
        }
    }

    pub fn new_earnings_transcript(
        multi: &MultiProgress,
        ticker: String,
        quarter: i32,
        year: i32,
        output_path: PathBuf,
    ) -> Self {
        let desc = format!("Earnings {} Q{} {}", ticker, quarter, year);
        let pb = multi.add(ProgressBar::new(100));
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
            )
            .unwrap()
            .progress_chars("##-"),
        );
        pb.set_message(desc);

        FetchTask::EarningsTranscript {
            ticker,
            quarter,
            year,
            output_path,
            progress_bar: Some(pb),
        }
    }
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
    pub async fn execute(
        &self,
        client: &Client,
        store: &dyn VectorStore,
        pg_pool: &Pool<Postgres>,
    ) -> Result<FetchResult> {
        let progress_bar = self.progress_bar();
        if let Some(pb) = progress_bar {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}")
                    .unwrap()
                    .progress_chars("#>-")
            );
            pb.set_message("Starting task...");
            pb.set_position(0);
        }
        match self {
            FetchTask::EdgarFiling {
                cik,
                filing,
                output_path: _,
                progress_bar,
            } => {
                if let Some(pb) = progress_bar {
                    pb.set_message("Downloading filing...");
                    pb.set_position(30);
                }

                // Download the filing
                match crate::edgar::filing::fetch_filing_document(client, cik, filing).await {
                    Ok(path) => {
                        if let Some(pb) = progress_bar {
                            pb.set_message("Parsing and storing...");
                            pb.set_position(60);
                        }

                        // Parse and store the filing immediately after download
                        match crate::edgar::filing::extract_complete_submission_filing(
                            &path,
                            filing.report_type.clone(),
                            store,
                            pg_pool,
                            None, // We're using the task's progress bar instead
                        )
                        .await
                        {
                            Ok(_) => {
                                if let Some(pb) = progress_bar {
                                    pb.set_position(100);
                                    pb.finish_with_message("✓ Complete");
                                }
                                Ok(FetchResult {
                                    task: self.clone(),
                                    status: FetchStatus::Success,
                                    output_path: Some(PathBuf::from(path)),
                                    error: None,
                                })
                            }
                            Err(e) => {
                                if let Some(pb) = progress_bar {
                                    pb.finish_with_message("✗ Parse failed");
                                }
                                Ok(FetchResult {
                                    task: self.clone(),
                                    status: FetchStatus::Failed,
                                    output_path: Some(PathBuf::from(path)),
                                    error: Some(e.to_string()),
                                })
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(pb) = progress_bar {
                            pb.finish_with_message("✗ Download failed");
                        }
                        Ok(FetchResult {
                            task: self.clone(),
                            status: FetchStatus::Failed,
                            output_path: None,
                            error: Some(e.to_string()),
                        })
                    }
                }
            }
            FetchTask::EarningsTranscript {
                ticker,
                quarter,
                year,
                output_path: _,
                progress_bar,
            } => {
                if let Some(pb) = progress_bar {
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template(
                                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}",
                            )
                            .unwrap()
                            .progress_chars("#>-"),
                    );
                    pb.set_message("Fetching transcript...");
                }
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
    multi_progress: Option<Arc<MultiProgress>>,
    store: Arc<dyn VectorStore>,
    pg_pool: Pool<Postgres>,
}

impl FetchManager {
    pub fn new(
        multi_progress: Option<Arc<MultiProgress>>,
        store: Arc<dyn VectorStore>,
        pg_pool: Pool<Postgres>,
    ) -> Self {
        Self {
            client: Client::new(),
            multi_progress,
            store,
            pg_pool,
        }
    }

    pub async fn execute_tasks(&self, tasks: Vec<FetchTask>) -> Result<Vec<FetchResult>> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut handles = Vec::new();

        // Launch tasks concurrently
        for task in tasks {
            let tx = tx.clone();
            let client = self.client.clone();
            let store = self.store.clone();
            let pg_pool = self.pg_pool.clone();

            if let Some(pb) = task.progress_bar() {
                pb.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}")
                    .unwrap()
                    .progress_chars("#>-"));
                pb.set_message("Preparing task...");
            }

            let handle = tokio::spawn(async move {
                let result = task.execute(&client, &*store, &pg_pool).await;
                if let Ok(fetch_result) = &result {
                    tx.send(Ok(fetch_result.clone())).await.expect("Channel send failed");
                } else {
                    tx.send(result.clone()).await.expect("Channel send failed");
                }
                result
            });
            handles.push(handle);
        }

        drop(tx);

        // Collect results while progress bars update independently
        let mut results = Vec::with_capacity(handles.len());
        while let Some(result) = rx.recv().await {
            results.push(result?);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let _ = handle.await?;
        }

        // Clear progress bars when done
        if let Some(mp) = &self.multi_progress {
            mp.clear().unwrap();
        }

        Ok(results)
    }
}
