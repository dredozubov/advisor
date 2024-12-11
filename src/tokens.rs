use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokenizers::Tokenizer;

static TOKENIZER: Lazy<Tokenizer> = Lazy::new(|| {
    Tokenizer::from_pretrained("bert-base-uncased", None)
        .expect("Failed to load tokenizer")
});

#[derive(Debug)]
pub struct TokenUsage {
    max_tokens: usize,
    max_input_tokens: usize,
    current_tokens: AtomicUsize,
}

impl TokenUsage {
    pub fn new(max_tokens: usize, max_input_tokens: usize) -> Self {
        Self {
            max_tokens,
            max_input_tokens,
            current_tokens: AtomicUsize::new(0),
        }
    }

    pub fn count_tokens(text: &str) -> usize {
        TOKENIZER
            .encode(text, false)
            .expect("Failed to encode text")
            .get_tokens()
            .len()
    }

    pub fn update_current_tokens(&self, text: &str) {
        let count = Self::count_tokens(text);
        self.current_tokens.store(count, Ordering::SeqCst);
    }

    pub fn get_current_tokens(&self) -> usize {
        self.current_tokens.load(Ordering::SeqCst)
    }

    pub fn get_max_input_tokens(&self) -> usize {
        self.max_input_tokens
    }

    pub fn format_prompt(&self, summary: &str) -> String {
        format!(
            "{} [{}/{}]> ",
            summary,
            self.get_current_tokens(),
            self.max_input_tokens
        )
    }
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self::new(16000, 12000) // Default values for GPT-3.5-turbo
    }
}
