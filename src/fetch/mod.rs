use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
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
    pub async fn execute(
        &self,
        client: &Client,
        store: &dyn VectorStore,
        pg_pool: &Pool<Postgres>,
        progress: Option<&ProgressBar>,
    ) -> Result<FetchResult> {
        match self {
            FetchTask::EdgarFiling {
                cik,
                filing,
                output_path: _,
            } => {
                if let Some(pb) = progress {
                    pb.set_style(
                        ProgressStyle::default_bar()
                            .template(
                                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}",
                            )
                            .unwrap()
                            .progress_chars("#>-"),
                    );
                    pb.set_message("Downloading filing...");
                    pb.inc(25);
                    pb.tick();
                }

                // Download the filing
                match crate::edgar::filing::fetch_filing_document(client, cik, filing).await {
                    Ok(path) => {
                        if let Some(pb) = progress {
                            pb.set_message("Parsing and storing...");
                            pb.inc(25);
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
                                if let Some(pb) = progress {
                                    pb.inc(50);
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
                                if let Some(pb) = progress {
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
                        if let Some(pb) = progress {
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
            } => {
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
    progress_tracker: Option<Arc<crate::utils::progress::ProgressTracker>>,
    store: Arc<dyn VectorStore>,
    pg_pool: Pool<Postgres>,
}

impl FetchManager {
    pub fn new(
        _tasks: &[FetchTask],
        progress_tracker: Option<Arc<crate::utils::progress::ProgressTracker>>,
        store: Arc<dyn VectorStore>,
        pg_pool: Pool<Postgres>,
    ) -> Self {
        Self {
            client: Client::new(),
            progress_tracker,
            store,
            pg_pool,
        }
    }

    pub async fn execute_tasks(&self, tasks: Vec<FetchTask>) -> Result<Vec<FetchResult>> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut handles = Vec::new();

        for (i, task) in tasks.iter().enumerate() {
            let tx = tx.clone();
            let client = self.client.clone();
            let task_id = format!("task_{}", i);
            let progress = self
                .progress_tracker
                .as_ref()
                .and_then(|tracker| tracker.get_bar(&task_id))
                .cloned();
            let task = task.clone();

            let store = self.store.clone();
            let pg_pool = self.pg_pool.clone();
            let handle = tokio::spawn(async move {
                let result = task.execute(&client, &*store, &pg_pool, progress.as_ref()).await;
                if let Some(pb) = progress {
                    match &result {
                        Ok(fetch_result) => {
                            match fetch_result.status {
                                FetchStatus::Success => {
                                    pb.inc(50); // Increment progress
                                    pb.finish_with_message("✓");
                                    pb.finish_with_message("✓");
                                }
                                FetchStatus::Failed => {
                                    pb.finish_with_message("✗");
                                }
                                FetchStatus::Skipped => {
                                    pb.finish_with_message("-");
                                }
                            }
                        }
                        Err(_) => {
                            pb.finish_with_message("✗");
                        }
                    }
                }
                tx.send(result).await
            });
            handles.push(handle);
        }

        drop(tx);

        let mut results = Vec::with_capacity(tasks.len());
        while let Some(result) = rx.recv().await {
            if let Some(tracker) = self.progress_tracker.as_ref() {
                let mp = tracker.as_multi_progress();
                mp.println("Task completed").unwrap();
            }
            results.push(result?);
        }

        for handle in handles {
            let _ = handle.await?;
        }

        // All tasks complete
        Ok(results)
    }
}