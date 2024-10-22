use crate::edgar::tickers::fetch_tickers;
use anyhow::Result as AnyhowResult;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::FileHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{
    CompletionType, Config as RustylineConfig, Context, EditMode, Editor, Helper, Result,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::RwLock;

type TickerMap = Arc<RwLock<HashMap<String, (crate::edgar::tickers::Ticker, String, String)>>>;

async fn print_all_tickers(ticker_map: &TickerMap) {
    let tickers = ticker_map.read().await;
    println!("Available tickers for auto-completion:");
    for (ticker, (_, company, _)) in tickers.iter() {
        println!("  {} - {}", ticker, company);
    }
    println!("Total number of tickers: {}", tickers.len());
}

pub struct ReplHelper {
    ticker_map: TickerMap,
}

impl ReplHelper {
    pub async fn new() -> AnyhowResult<Self> {
        let tickers = fetch_tickers().await?;
        let mut map = HashMap::new();
        for (ticker, company, cik) in tickers {
            map.insert(
                ticker.to_string(),
                (ticker.clone(), company.clone(), cik.clone()),
            );
        }
        Ok(ReplHelper {
            ticker_map: Arc::new(RwLock::new(map)),
        })
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_lowercase();

            let ticker_map = futures::executor::block_on(self.ticker_map.read());
            let candidates: Vec<Pair> = ticker_map
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(_, (ticker, company, _))| Pair {
                    display: format!("{} ({})", ticker, company),
                    replacement: ticker.to_string(),
                })
                .collect();

            Ok((at_pos + 1, candidates))
        } else {
            Ok((pos, vec![]))
        }
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let mut highlighted = String::new();
        let mut in_ticker = false;
        for (_i, c) in line.char_indices() {
            if c == '@' {
                in_ticker = true;
                highlighted.push_str("\x1b[32m"); // Green color for tickers
                highlighted.push(c);
            } else if in_ticker && !c.is_alphanumeric() {
                in_ticker = false;
                highlighted.push_str("\x1b[0m"); // Reset color
                highlighted.push(c);
            } else if in_ticker {
                highlighted.push(c);
            } else {
                highlighted.push(c);
            }
        }
        if in_ticker {
            highlighted.push_str("\x1b[0m"); // Reset color at the end if needed
        }
        Cow::Owned(highlighted)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl Validator for ReplHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> Result<ValidationResult> {
        let input = ctx.input();
        let words: Vec<&str> = input.split_whitespace().collect();

        let ticker_map = futures::executor::block_on(self.ticker_map.read());
        for word in words {
            if word.starts_with('@') {
                let ticker = &word[1..].to_uppercase(); // Remove the '@' prefix and convert to uppercase
                if !ticker_map
                    .values()
                    .any(|(t, _, _)| &t.to_string() == ticker)
                {
                    return Ok(ValidationResult::Invalid(Some(format!(
                        "Invalid ticker: {}",
                        ticker
                    ))));
                }
            }
        }

        Ok(ValidationResult::Valid(None))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        // For now, we'll return None (no hints)
        // You can implement more sophisticated hinting logic here in the future
        None
    }
}

impl Helper for ReplHelper {}

pub async fn create_editor() -> Result<Editor<ReplHelper, FileHistory>> {
    let rustyline_config = RustylineConfig::builder()
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();

    let home_dir = env::var("HOME").expect("HOME environment variable not set");
    let history_path = format!("{}/.ask-edgar.history", home_dir);

    let mut rl = Editor::<ReplHelper, FileHistory>::with_config(rustyline_config)?;

    if rl.load_history(&history_path).is_err() {
        println!("No previous history.");
    }

    let helper = ReplHelper::new().await.map_err(|e| {
        ReadlineError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;
    rl.set_helper(Some(helper));

    Ok(rl)
}

pub fn save_history(rl: &mut Editor<ReplHelper, FileHistory>) -> Result<()> {
    let home_dir = env::var("HOME").expect("HOME environment variable not set");
    let history_path = format!("{}/.ask-edgar.history", home_dir);
    rl.save_history(&history_path)
}
