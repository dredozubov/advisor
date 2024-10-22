use crate::edgar::tickers::TICKER_DATA;
use once_cell::sync::Lazy;
use radixdb::RadixTree;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::validate::Validator;
use rustyline::{Context, Result, Helper};
use std::borrow::Cow;

static TICKER_TREE: Lazy<RadixTree> = Lazy::new(|| {
    let mut tree = RadixTree::default();
    for (ticker, _) in TICKER_DATA.iter() {
        tree.insert(ticker, ticker.to_string());
    }
    tree
});

pub struct ReplHelper;

impl ReplHelper {
    pub fn new() -> Self {
        ReplHelper
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_uppercase();
            let candidates: Vec<Pair> = TICKER_TREE
                .scan_prefix(prefix)
                .map(|(i, val)| Pair {
                    display: String::from_utf8_lossy(i.as_ref()).into_owned(),
                    replacement: String::from_utf8_lossy(val.as_ref()).into_owned(),
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

impl Helper for ReplHelper {}
