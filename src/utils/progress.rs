use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub fn create_progress_bar(multi: &MultiProgress, desc: &str) -> ProgressBar {
    let bar = multi.add(ProgressBar::new(100));
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap()
        .progress_chars("##-")
    );
    bar.set_message(desc.to_string());
    bar
}
