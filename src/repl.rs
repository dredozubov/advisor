use crate::edgar::tickers::{TickerData, load_tickers};
use anyhow::Result as AnyhowResult;
use once_cell::sync::Lazy;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{Validator, ValidationContext, ValidationResult};
use rustyline::{Context, Helper, Result};
use std::borrow::Cow;
use std::collections::HashMap;

static TICKER_DATA: Lazy<AnyhowResult<Vec<TickerData>>> = Lazy::new(|| {
    load_tickers()
});

static TICKER_MAP: Lazy<HashMap<String, (String, String, String)>> = Lazy::new(|| {
    let mut map = HashMap::new();
    if let Ok(tickers) = TICKER_DATA.as_ref() {
        for (ticker, company, cik) in tickers {
            map.insert(ticker.to_lowercase(), (ticker.clone(), company.clone(), cik.clone()));
        }
    }
    map
});

fn print_all_tickers() {
    if let Ok(tickers) = TICKER_DATA.as_ref() {
        println!("Available tickers for auto-completion:");
        for (ticker, company, _) in tickers {
            println!("  {} - {}", ticker, company);
        }
        println!("Total number of tickers: {}", tickers.len());
    } else {
        println!("Failed to load tickers data.");
    }
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
                .map(|(_, (ticker, company, _))| {
                    Pair {
                        display: format!("{} ({})", ticker, company),
                        replacement: ticker.clone(),
                    }
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
        
        for word in words {
            if word.starts_with('@') {
                let ticker = &word[1..].to_uppercase(); // Remove the '@' prefix and convert to uppercase
                if !TICKER_MAP.values().any(|(t, _, _)| t == ticker) {
                    return Ok(ValidationResult::Invalid(Some(format!("Invalid ticker: {}", ticker))));
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
