use crate::edgar::tickers::TICKER_DATA;
use once_cell::sync::Lazy;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper, Result};
use std::borrow::Cow;
use std::collections::HashMap;

static TICKER_MAP: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for (ticker, _) in TICKER_DATA.iter() {
        map.insert(ticker.to_lowercase(), ticker.to_string());
    }
    map
});

fn print_all_tickers() {
    println!("Available tickers for auto-completion:");
    for (ticker, _) in TICKER_DATA.iter() {
        println!("  {}", ticker);
    }
    println!("Total number of tickers: {}", TICKER_DATA.len());
}

pub struct ReplHelper;

impl ReplHelper {
    pub fn new() -> Self {
        print_all_tickers();
        ReplHelper
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_lowercase();
            
            let candidates: Vec<Pair> = TICKER_MAP
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(_, val)| Pair {
                    display: format!("{} ({})", val, TICKER_DATA.get(val).unwrap_or(&"Unknown")),
                    replacement: val.clone(),
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
                highlighted.push(c);
            } else if in_ticker && !c.is_alphanumeric() {
                in_ticker = false;
                highlighted.push_str("\x1b[0m"); // Reset color
                highlighted.push(c);
            } else if in_ticker {
                highlighted.push_str("\x1b[32m"); // Green color for tickers
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

impl Validator for ReplHelper {}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        // For now, we'll return None (no hints)
        // You can implement more sophisticated hinting logic here in the future
        None
    }
}

impl Helper for ReplHelper {}
