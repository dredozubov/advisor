// Empty - removing this file as progress bars are now owned by FetchTask
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)] 
pub struct ProgressTracker {
    pub(crate) multi_progress: Option<Arc<MultiProgress>>,
    progress_bar: Option<ProgressBar>,
}

impl ProgressTracker {
    pub fn new(multi_progress: Option<&Arc<MultiProgress>>) -> Self {
        let progress_bar = multi_progress.map(|mp| {
            let pb = mp.add(ProgressBar::new(100));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg:>50}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        });
        Self {
            multi_progress: multi_progress.cloned(),
            progress_bar,
        }
    }

    pub fn update_message(&self, message: &str) {
        if let Some(pb) = &self.progress_bar {
            pb.set_message(message.to_string());
        }
    }

    pub fn update_progress(&self, position: u64) {
        if let Some(pb) = &self.progress_bar {
            pb.set_position(position);
        }
    }

    pub fn start_progress(&self, total: u64, initial_message: &str) {
        if let Some(pb) = &self.progress_bar {
            pb.reset();
            pb.set_length(total);
            pb.set_message(initial_message.to_string());
            pb.set_position(0);
        }
    }

    pub fn finish(&self) {
        if let Some(pb) = &self.progress_bar {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/blue}] {msg:>50}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb.set_message("Complete");
            pb.finish();
        }
    }

    pub fn increment(&self, delta: u64) {
        if let Some(pb) = &self.progress_bar {
            pb.inc(delta);
        }
    }
}
