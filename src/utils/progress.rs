use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct ProgressTracker {
    pub multi_progress: MultiProgress,
    pub download: ProgressBar,
    pub parse: ProgressBar,
    pub store: ProgressBar,
}

impl ProgressTracker {
    pub fn new(total_tasks: usize) -> Self {
        let multi = MultiProgress::new();
        
        let download = multi.add(ProgressBar::new(total_tasks as u64));
        log::debug!("Initializing download progress bar");
        download.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} Downloads ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        
        let parse = multi.add(ProgressBar::new(total_tasks as u64));
        parse.set_style(ProgressStyle::default_bar()
            .template("{spinner:.yellow} [{elapsed_precise}] [{bar:40.yellow/blue}] {pos}/{len} Parsing ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        
        let store = multi.add(ProgressBar::new(total_tasks as u64));
        store.set_style(ProgressStyle::default_bar()
            .template("{spinner:.red} [{elapsed_precise}] [{bar:40.red/blue}] {pos}/{len} Storing ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));

        Self {
            multi_progress: multi,
            download,
            parse,
            store,
        }
    }

    pub fn as_multi_progress(&self) -> &MultiProgress {
        &self.multi_progress
    }
}
