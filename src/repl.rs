use crate::edgar::{self, tickers};
use once_cell::sync::Lazy;
use radixdb::RadixTree;
use rustyline::completion::{Completer, Pair};
use rustyline::Context;
use rustyline::Result;
use rustyline_derive::{Completer, Helper, Highlighter, Hinter, Validator};

static CANDIDATES: Lazy<Vec<String>> = Lazy::new(|| {
    let mut dict = RadixTree::default();
    tickers::TICKER_DATA
        .iter()
        .map(|(t, _)| t.to_string())
        .collect();
    dict
});

pub struct TickerCompleter;

impl Completer for TickerCompleter {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_uppercase();
            let candidates_list = CANDIDATES;
            let candidates: Vec<Pair> = candidates_list
                .iter()
                .filter(|(ticker, _)| ticker.starts_with(prefix))
                .map(|(ticker, _)| Pair {
                    display: ticker.to_string(),
                    replacement: ticker.to_string(),
                })
                .collect();
            Ok((at_pos + 1, candidates))
        } else {
            Ok((pos, vec![]))
        }
    }
}

pub struct HistoryAndTickerHinter;

#[derive(Completer, Hinter, Helper, Validator, Highlighter)]
pub struct ReplHelper {
    #[rustyline(Completer)]
    pub completer: TickerCompleter,
    #[rustyline(TickerHinter)]
    pub hinter: HistoryAndTickerHinter,
    #[rustyline(Validator)]
    pub validator: (),
}
