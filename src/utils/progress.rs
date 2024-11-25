use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;

pub struct ProgressTracker {
    pub multi_progress: MultiProgress,
    pub task_bars: HashMap<String, ProgressBar>, // Map task IDs to progress bars
}

impl ProgressTracker {
    pub fn new(tasks: &[crate::fetch::FetchTask]) -> Self {
        let multi = MultiProgress::new();
        let mut task_bars = HashMap::new();

        for (i, task) in tasks.iter().enumerate() {
            let task_id = format!("task_{}", i);
            let bar = multi.add(ProgressBar::new(100));
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );

            let desc = match task {
                crate::fetch::FetchTask::EdgarFiling { filing, .. } => {
                    format!("Filing {} {}", filing.report_type, filing.accession_number)
                }
                crate::fetch::FetchTask::EarningsTranscript { ticker, quarter, year, .. } => {
                    format!("{} Q{} {}", ticker, quarter, year)
                }
            };
            bar.set_message(desc);
            task_bars.insert(task_id, bar);
        }

        Self {
            multi_progress: multi,
            task_bars,
        }
    }

    pub fn get_bar(&self, task_id: &str) -> Option<&ProgressBar> {
        self.task_bars.get(task_id)
    }

    pub fn as_multi_progress(&self) -> &MultiProgress {
        &self.multi_progress
    }
}
