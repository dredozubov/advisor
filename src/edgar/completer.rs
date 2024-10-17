use rustyline::completion::{Completer, Pair};
use rustyline::Context;
use rustyline::Result;
use crate::edgar::tickers::TICKERS;

pub struct TickerCompleter;

impl Completer for TickerCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_uppercase();
            let candidates: Vec<Pair> = TICKERS
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
