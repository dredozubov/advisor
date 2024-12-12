use crate::memory::ConversationManager;
use crate::{edgar::tickers::fetch_tickers, memory::ConversationChainManager};
use anyhow::Result as AnyhowResult;
use crossterm::{
    event, execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use langchain_rust::llm::{OpenAI, OpenAIConfig};
use once_cell::sync::Lazy;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::FileHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{
    Cmd, CompletionType, ConditionalEventHandler, Config as RustylineConfig, Context, EditMode,
    Editor, Event, EventContext, EventHandler, Helper, KeyEvent, RepeatCount, Result,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::io::{stdout, Write};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

static HELP_LABEL: &str = "Select a conversation (↑/↓ to navigate, Enter to select, DEL/Ctrl+D to delete, Esc/Ctrl+[ to cancel):\n\n";

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
        // Find the word being completed
        let (word_start, word) = find_word_at_pos(line, pos);

        // Check if we're completing a ticker (starts with @)
        if let Some(ticker_part) = word.strip_prefix('@') {
            let prefix = ticker_part.to_uppercase();

            // Generate completion candidates
            let candidates: Vec<Pair> = self
                .ticker_map
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, (ticker, company, _))| {
                    let display = format!("{} ({})", ticker.as_str(), company);
                    let replacement = format!("@{}", ticker.as_str());
                    Pair {
                        display,
                        replacement,
                    }
                })
                .collect();

            return Ok((word_start, candidates));
        }

        Ok((pos, vec![]))
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let mut highlighted = String::new();
        let mut last_pos = 0;

        // Find all ticker symbols and highlight them
        for (start, part) in line.match_indices('@') {
            // Add unhighlighted text before the ticker
            highlighted.push_str(&line[last_pos..start]);

            // Find the end of the ticker symbol
            let end = start
                + part.len()
                + line[start + part.len()..]
                    .find(|c: char| !c.is_alphanumeric() && c != '-')
                    .unwrap_or(line[start + part.len()..].len());

            let ticker = &line[start..end];

            // Check if it's a valid ticker
            if let Some(ticker_str) = ticker.strip_prefix('@') {
                let is_valid = self
                    .ticker_map
                    .values()
                    .any(|(t, _, _)| t.as_str() == ticker_str.to_uppercase());

                // Use different colors for valid/invalid tickers
                if is_valid {
                    highlighted.push_str("\x1b[32m"); // Green for valid
                } else {
                    highlighted.push_str("\x1b[31m"); // Red for invalid
                }
            }

            highlighted.push_str(ticker);
            highlighted.push_str("\x1b[0m"); // Reset color

            last_pos = end;
        }

        // Add remaining text
        highlighted.push_str(&line[last_pos..]);

        // Highlight current word at cursor if it's a command
        if pos < line.len() && line.starts_with('/') {
            let word_end = line[pos..].find(' ').map_or(line.len(), |i| i + pos);
            highlighted.insert_str(pos, "\x1b[36m"); // Cyan for commands
            highlighted.insert_str(word_end, "\x1b[0m");
        }

        Cow::Owned(highlighted)
    }

    fn highlight_char(
        &self,
        line: &str,
        pos: usize,
        _forced: rustyline::highlight::CmdKind,
    ) -> bool {
        // Highlight characters in tickers and commands
        let word = find_word_at_pos(line, pos).1;
        word.starts_with('@') || word.starts_with('/')
    }
}

impl Validator for ReplHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> Result<ValidationResult> {
        let input = ctx.input();

        // Check for empty input
        if input.trim().is_empty() {
            return Ok(ValidationResult::Invalid(Some(
                "Input cannot be empty".to_string(),
            )));
        }

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
                "Please include at least one ticker symbol (e.g. @AAPL)".to_string(),
            )));
        }

        Ok(ValidationResult::Valid(None))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        // Don't show hints for empty lines
        if line.trim().is_empty() {
            return Some(" Type @<ticker> to reference a company...".into());
        }

        // Find current word
        let word = find_word_at_pos(line, pos).1;

        // Show hint for partial ticker
        if let Some(partial) = word.strip_prefix('@') {
            if !partial.is_empty() {
                // Find first matching ticker
                if let Some((_, (ticker, company, _))) = self
                    .ticker_map
                    .iter()
                    .find(|(key, _)| key.starts_with(&partial.to_uppercase()))
                {
                    return Some(format!(" → {} ({})", ticker.as_str(), company));
                }
            }
        }

        // Show command hints
        if line.starts_with('/') {
            return Some(match line {
                "/d" => " → /delete <conversation_id>".into(),
                "/l" => " → /list".into(),
                "/h" => " → /help".into(),
                "/q" => " → /quit".into(),
                _ => "".into(),
            });
        }

        None
    }
}

// Helper function to find word boundaries
fn find_word_at_pos(line: &str, pos: usize) -> (usize, &str) {
    let start = line[..pos]
        .rfind(|c: char| c.is_whitespace())
        .map_or(0, |i| i + 1);
    let end = line[pos..]
        .find(|c: char| c.is_whitespace())
        .map_or(line.len(), |i| i + pos);
    (start, &line[start..end])
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
        Print(HELP_LABEL)
    )?;

    loop {
        // Always start from a known position
        execute!(stdout(), crossterm::cursor::MoveTo(0, 2))?;

        // Draw all conversations
        for (i, conv) in conversations.iter().enumerate() {
            let helper = (*rl.helper().unwrap()).clone();
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
        if let event::Event::Key(key) = event::read()? {
            if key.modifiers.contains(event::KeyModifiers::CONTROL)
                && key.code == event::KeyCode::Char('t')
            {
                // Create new conversation
                let conv_id = conversation_manager
                    .create_conversation("New conversation".to_string(), vec![])
                    .await?;
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
                event::KeyCode::Up | event::KeyCode::Char('p')
                    if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                {
                    if selection > 0 {
                        selection = selection.saturating_sub(1);
                    }
                }
                event::KeyCode::Down | event::KeyCode::Char('n')
                    if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                {
                    if selection < conversations.len() - 1 {
                        selection += 1;
                    }
                }
                event::KeyCode::Up => {
                    if selection > 0 {
                        selection = selection.saturating_sub(1);
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
                event::KeyCode::Char('[') | event::KeyCode::Esc
                    if key.modifiers.contains(event::KeyModifiers::CONTROL)
                        || key.code == event::KeyCode::Esc =>
                {
                    // Disable raw mode and clear screen before returning
                    crossterm::terminal::disable_raw_mode()?;
                    execute!(
                        stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0)
                    )?;
                    return Ok("Toggle list view".to_string());
                }
                event::KeyCode::Delete | event::KeyCode::Backspace | event::KeyCode::Char('d')
                    if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                {
                    let selected = &conversations[selection];
                    conversation_manager
                        .delete_conversation(&selected.id)
                        .await?;

                    // Clear screen before refreshing list
                    execute!(
                        stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0),
                        Print(HELP_LABEL)
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

pub async fn create_editor(
    conversation_manager: ConversationManager,
    chain_manager: Arc<ConversationChainManager>,
    llm: OpenAI<OpenAIConfig>,
) -> Result<EditorWithHistory> {
    log::debug!("Creating rustyline editor configuration");
    let rustyline_config = RustylineConfig::builder()
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .auto_add_history(true)
        .completion_prompt_limit(100)
        .max_history_size(1000)?
        .history_ignore_space(true)
        .history_ignore_dups(true)?
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

    // Bind Ctrl+[ and ESC to list view handler
    let list_handler = Box::new(ListViewHandler);
    rl.bind_sequence(
        KeyEvent::ctrl('['),
        EventHandler::Conditional(list_handler.clone()),
    );
    rl.bind_sequence(
        KeyEvent::from('\x1b'),
        EventHandler::Conditional(list_handler.clone()),
    );

    // Bind Ctrl+T to new conversation handler
    let new_conv_handler = Box::new(AdvisorConversationHandler::new(
        Arc::new(RwLock::new(conversation_manager)),
        chain_manager,
        llm,
    ));
    rl.bind_sequence(
        KeyEvent::ctrl('t'),
        EventHandler::Conditional(new_conv_handler),
    );

    // Wrap the editor in a custom type that adds history entries
    log::debug!("Creating EditorWithHistory wrapper");
    Ok(EditorWithHistory::new(rl))
}

#[derive(Clone)]
struct ListViewHandler;

#[derive(Clone)]
pub struct AdvisorConversationHandler {
    conversation_manager: Arc<RwLock<ConversationManager>>,
    chain_manager: Arc<ConversationChainManager>,
    llm: OpenAI<OpenAIConfig>,
}

impl AdvisorConversationHandler {
    pub fn new(
        conversation_manager: Arc<RwLock<ConversationManager>>,
        chain_manager: Arc<ConversationChainManager>,
        llm: OpenAI<OpenAIConfig>,
    ) -> Self {
        Self {
            conversation_manager,
            chain_manager,
            llm,
        }
    }
}

impl ConditionalEventHandler for AdvisorConversationHandler {
    fn handle(&self, evt: &Event, _: RepeatCount, _: bool, _ctx: &EventContext) -> Option<Cmd> {
        if let Event::KeySeq(ref keys) = evt {
            if keys[0] == KeyEvent::ctrl('T') {
                let conversation_manager = self.conversation_manager.clone();
                let chain_manager = self.chain_manager.clone();
                let llm = self.llm.clone();

                // Create a channel for async communication
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);

                // Spawn the async work
                tokio::spawn({
                    let conversation_manager = conversation_manager.clone();
                    let chain_manager = chain_manager.clone();
                    let llm = llm.clone();
                    let tx = tx.clone();

                    async move {
                        let result = async {
                            let conv_id = conversation_manager
                                .write()
                                .await
                                .create_conversation("New conversation".to_string(), vec![])
                                .await?;

                            chain_manager.get_or_create_chain(&conv_id, llm).await?;

                            conversation_manager
                                .write()
                                .await
                                .switch_conversation(&conv_id)
                                .await?;

                            Ok::<_, anyhow::Error>(())
                        }
                        .await;

                        let _ = tx.send(result).await;
                    }
                });

                // Use try_recv to avoid blocking
                match rx.try_recv() {
                    Ok(Ok(_)) => {
                        print!("\r\n"); // Move to new line
                        println!("Started new conversation. Please enter your first question with at least one valid ticker symbol (e.g. @AAPL)");
                        stdout().flush().unwrap(); // Ensure output is flushed
                    }
                    _ => {
                        eprintln!("Error creating new conversation");
                    }
                }

                return Some(Cmd::ClearScreen);
            }
        }
        None
    }
}

impl ConditionalEventHandler for ListViewHandler {
    fn handle(&self, evt: &Event, _: RepeatCount, _: bool, _ctx: &EventContext) -> Option<Cmd> {
        if let Some(k) = evt.get(0) {
            if *k == KeyEvent::ctrl('[') || *k == KeyEvent::from('\x1b') {
                return Some(Cmd::AcceptLine);
            }
        }
        None
    }
}

pub struct EditorWithHistory {
    inner: Editor<ReplHelper, FileHistory>,
}

impl EditorWithHistory {
    fn new(editor: Editor<ReplHelper, FileHistory>) -> Self {
        EditorWithHistory { inner: editor }
    }

    pub fn readline(&mut self, prompt: &str) -> Result<String> {
        self.inner.readline(prompt)
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
