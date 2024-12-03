use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::io::{stdout, Write};

pub struct TerminalManager {
    pub raw_mode_enabled: bool,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}
impl TerminalManager {
    pub fn new() -> Self {
        Self {
            raw_mode_enabled: false,
        }
    }

    pub fn enable_raw_mode(&mut self) -> Result<()> {
        if !self.raw_mode_enabled {
            crossterm::terminal::enable_raw_mode()?;
            self.raw_mode_enabled = true;
        }
        Ok(())
    }

    pub fn disable_raw_mode(&mut self) -> Result<()> {
        if self.raw_mode_enabled {
            crossterm::terminal::disable_raw_mode()?;
            self.raw_mode_enabled = false;
        }
        Ok(())
    }

    pub fn clear_screen(&self) -> Result<()> {
        execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
        Ok(())
    }

    pub fn clear_line(&self) -> Result<()> {
        execute!(
            stdout(),
            MoveTo(0, crossterm::cursor::position()?.1),
            Clear(ClearType::CurrentLine)
        )?;
        Ok(())
    }

    pub fn print_highlighted(&self, text: &str, color: Color) -> Result<()> {
        execute!(stdout(), SetForegroundColor(color), Print(text), ResetColor)?;
        Ok(())
    }

    pub fn print_line(&self, text: &str) -> Result<()> {
        execute!(stdout(), Print(format!("{}\n", text)))?;
        Ok(())
    }

    pub fn move_to(&self, x: u16, y: u16) -> Result<()> {
        execute!(stdout(), MoveTo(x, y))?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        stdout().flush()?;
        Ok(())
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        let _ = self.disable_raw_mode();
    }
}
