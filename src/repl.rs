use crate::edgar::tickers::fetch_tickers;
use crate::memory::ConversationManager;
use anyhow::Result as AnyhowResult;
use crossterm::{
    event, execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use once_cell::sync::Lazy;
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
use std::io::{stdout, Write};
use std::sync::Arc;
use uuid::Uuid;

type TickerMap = Arc<HashMap<String, (crate::edgar::tickers::Ticker, String, String)>>;

static HISTORY_PATH: Lazy<String> = Lazy::new(|| {
    let home_dir = env::var("HOME").expect("HOME environment variable not set");
    format!("{}/.advisor.history", home_dir)
});

#[derive(Clone)]
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
        let ticker_map = Arc::new(map);
        // print_all_tickers(&ticker_map).await;
        Ok(ReplHelper { ticker_map })
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        if let Some(at_pos) = line[..pos].rfind('@') {
            let prefix = &line[at_pos + 1..pos].to_lowercase();

            let ticker_map = &self.ticker_map;
            let candidates: Vec<Pair> = ticker_map
                .iter()
                .filter(|(key, _)| key.to_lowercase().starts_with(prefix))
                .map(|(_, (ticker, company, _))| Pair {
                    display: format!("{} ({})", ticker.as_str(), company),
                    replacement: ticker.as_str().to_string(),
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
            } else {
                highlighted.push(c);
            }
        }
        if in_ticker {
            highlighted.push_str("\x1b[0m"); // Reset color at the end if needed
        }
        Cow::Owned(highlighted)
    }

    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _forced: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }
}

impl Validator for ReplHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> Result<ValidationResult> {
        let input = ctx.input();
        
        // Skip validation for commands
        if input.starts_with('/') {
            return Ok(ValidationResult::Valid(None));
        }

        let words: Vec<&str> = input.split_whitespace().collect();
        let mut found_valid_ticker = false;

        for word in words {
            if word.starts_with('@') {
                let ticker_with_punctuation = word.strip_prefix("@").unwrap(); // Remove the '@' prefix
                let ticker = ticker_with_punctuation
                    .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '-');
                let ticker = ticker.to_uppercase();
                if !&self
                    .ticker_map
                    .values()
                    .any(|(t, _, _)| t.as_str() == ticker)
                {
                    return Ok(ValidationResult::Invalid(Some(format!(
                        "Invalid ticker: {}",
                        ticker
                    ))));
                }
                found_valid_ticker = true;
            }
        }

        // For non-command input, require at least one valid ticker
        if !found_valid_ticker {
            return Ok(ValidationResult::Invalid(Some(
                "Please include at least one ticker symbol (e.g. @AAPL)".to_string()
            )));
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

pub async fn handle_list_command(
    conversation_manager: &mut ConversationManager,
    rl: &mut Editor<ReplHelper, FileHistory>,
) -> anyhow::Result<String> {
    let mut conversations = conversation_manager.list_conversations().await?;
    if conversations.is_empty() {
        return Ok("No conversations found.".to_string());
    }

    // Start with the last conversation selected
    let mut selection = conversations.len() - 1;
    // Enable raw mode and clear screen
    crossterm::terminal::enable_raw_mode()?;
    execute!(
        stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0),
        Print("Select a conversation (↑/↓ to navigate, Enter to select, DEL/Ctrl+D to delete, Esc/Ctrl+L to cancel):\n\n")
    )?;

    loop {
        // Always start from a known position
        execute!(stdout(), crossterm::cursor::MoveTo(0, 2))?;

        // Draw all conversations
        for (i, conv) in conversations.iter().enumerate() {
            let helper = rl.helper().as_ref().unwrap().clone();
            let ticker_map = &helper.ticker_map;

            // Get company names for all tickers
            let ticker_info: Vec<String> = conv
                .tickers
                .iter()
                .map(|ticker| {
                    if let Some((_, company_name, _)) = ticker_map.get(ticker) {
                        format!("{}: {}", ticker, company_name)
                    } else {
                        ticker.clone()
                    }
                })
                .collect();

            let summary = format!("{} ({})", conv.summary, ticker_info.join(", "));
            let date = conv
                .updated_at
                .format(
                    &time::format_description::parse("[year]-[month]-[day] [hour]:[minute]")
                        .unwrap(),
                )
                .unwrap();

            // Truncate summary if too long, leaving space for date
            let max_summary_width = 50;
            let truncated_summary = if summary.len() > max_summary_width {
                format!("{}...", &summary[..max_summary_width - 3])
            } else {
                summary
            };

            // Format with summary left-aligned and date right-aligned
            let line = format!(
                "{:<width$} {: >19}",
                truncated_summary,
                date,
                width = max_summary_width
            );

            execute!(
                stdout(),
                crossterm::cursor::MoveToColumn(0),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
            )?;

            if i == selection {
                execute!(
                    stdout(),
                    SetForegroundColor(Color::Green),
                    Print(format!("→ {}", line)),
                    ResetColor,
                    Print("\n")
                )?;
            } else {
                execute!(stdout(), Print(format!("  {}\n", line)))?;
            }
        }

        stdout().flush()?;

        // Read a single keypress
        match event::read()? {
            event::Event::Key(key) => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) && key.code == event::KeyCode::Char('t') {
                    // Create new conversation
                    let conv_id = conversation_manager.create_conversation("New conversation".to_string(), vec![]).await?;
                    conversation_manager.switch_conversation(&conv_id).await?;
                    
                    // Disable raw mode and clear screen before returning
                    crossterm::terminal::disable_raw_mode()?;
                    execute!(
                        stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0)
                    )?;
                    return Ok("Started new conversation. Please enter your first question with at least one valid ticker symbol (e.g. @AAPL)".to_string());
                }

                match key.code {
                    event::KeyCode::Up | event::KeyCode::Char('p') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        if selection > 0 {
                            selection -= 1;
                        }
                    }
                    event::KeyCode::Down | event::KeyCode::Char('n') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        if selection < conversations.len() - 1 {
                            selection += 1;
                        }
                    }
                    event::KeyCode::Up => {
                        if selection > 0 {
                            selection -= 1;
                        }
                    }
                    event::KeyCode::Down => {
                        if selection < conversations.len() - 1 {
                            selection += 1;
                        }
                    }
                    event::KeyCode::Enter => {
                        let selected = &conversations[selection];
                        conversation_manager
                            .switch_conversation(&selected.id)
                            .await?;
                        // Disable raw mode and clear screen before returning
                        crossterm::terminal::disable_raw_mode()?;
                        execute!(
                            stdout(),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                            crossterm::cursor::MoveTo(0, 0)
                        )?;
                        return Ok(format!("Switched to conversation: {}", selected.id));
                    }
                    event::KeyCode::Char('l') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        // Disable raw mode and clear screen before returning
                        crossterm::terminal::disable_raw_mode()?;
                        execute!(
                            stdout(),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                            crossterm::cursor::MoveTo(0, 0)
                        )?;
                        return Ok("Toggle list view".to_string());
                    }
                    event::KeyCode::Delete | event::KeyCode::Backspace | event::KeyCode::Char('d') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        let selected = &conversations[selection];
                        conversation_manager.delete_conversation(&selected.id).await?;
                        
                        // Clear screen before refreshing list
                        execute!(
                            stdout(),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                            crossterm::cursor::MoveTo(0, 0),
                            Print("Select a conversation (↑/↓ to navigate, Enter to select, DEL/Ctrl+D to delete, Esc/Ctrl+L to cancel):\n\n")
                        )?;

                        // Refresh conversations list
                        let new_conversations = conversation_manager.list_conversations().await?;
                        if new_conversations.is_empty() {
                            // If no conversations left, exit menu
                            crossterm::terminal::disable_raw_mode()?;
                            execute!(
                                stdout(),
                                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                                crossterm::cursor::MoveTo(0, 0)
                            )?;
                            return Ok("All conversations deleted".to_string());
                        }
                        
                        // Update local list and adjust selection if needed
                        conversations = new_conversations;
                        if selection >= conversations.len() {
                            selection = conversations.len() - 1;
                        }
                    }
                    event::KeyCode::Esc => {
                        // Disable raw mode before returning
                        crossterm::terminal::disable_raw_mode()?;
                        return Ok("Cancelled selection".to_string());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

pub async fn handle_delete_command(
    line: &str,
    conversation_manager: &mut ConversationManager,
) -> anyhow::Result<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 2 {
        return Ok("Usage: /delete <conversation_id>".to_string());
    }

    let id =
        Uuid::parse_str(parts[1]).map_err(|_| anyhow::anyhow!("Invalid conversation ID format"))?;

    conversation_manager.delete_conversation(&id).await?;
    Ok(format!("Conversation {} deleted", id))
}

pub async fn create_editor() -> Result<EditorWithHistory> {
    log::debug!("Creating rustyline editor configuration");
    let rustyline_config = RustylineConfig::builder()
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();

    log::debug!("Creating editor with config");
    let mut rl = Editor::<ReplHelper, FileHistory>::with_config(rustyline_config)?;

    log::debug!("Loading editor history");
    if rl.load_history(&**HISTORY_PATH).is_err() {
        log::debug!("No previous history file found");
    } else {
        log::debug!("History loaded successfully");
    }

    log::debug!("Creating ReplHelper");
    let helper = ReplHelper::new().await.map_err(|e| {
        log::error!("Failed to create ReplHelper: {}", e);
        ReadlineError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    log::debug!("Setting helper for editor");
    rl.set_helper(Some(helper));

    // Wrap the editor in a custom type that adds history entries
    log::debug!("Creating EditorWithHistory wrapper");
    Ok(EditorWithHistory::new(rl))
}

pub struct EditorWithHistory {
    inner: Editor<ReplHelper, FileHistory>,
}

impl EditorWithHistory {
    fn new(editor: Editor<ReplHelper, FileHistory>) -> Self {
        EditorWithHistory { inner: editor }
    }

    pub fn readline(&mut self, prompt: &str) -> Result<String> {
        let line = self.inner.readline(prompt)?;
        let _ = self.inner.add_history_entry(line.as_str());
        Ok(line)
    }
}

impl std::ops::Deref for EditorWithHistory {
    type Target = Editor<ReplHelper, FileHistory>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for EditorWithHistory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub fn save_history(rl: &mut Editor<ReplHelper, FileHistory>) -> Result<()> {
    rl.save_history(&**HISTORY_PATH)
}
