use anyhow::Result;
use std::fs;

// Base data directory
pub const DATA_DIR: &str = "data";

// EDGAR specific directories
pub const EDGAR_DIR: &str = "data/edgar";
pub const EDGAR_FILINGS_DIR: &str = "data/edgar/filings";
pub const EDGAR_PARSED_DIR: &str = "data/edgar/parsed";

// Earnings specific directories
pub const EARNINGS_DIR: &str = "data/earnings";

pub fn ensure_dir(path: &str) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn ensure_data_dirs() -> Result<()> {
    ensure_dir(DATA_DIR)?;
    Ok(())
}

pub fn ensure_edgar_dirs() -> Result<()> {
    ensure_data_dirs()?;
    ensure_dir(EDGAR_DIR)?;
    ensure_dir(EDGAR_FILINGS_DIR)?;
    ensure_dir(EDGAR_PARSED_DIR)?;
    Ok(())
}

pub fn ensure_earnings_dirs() -> Result<()> {
    ensure_data_dirs()?;
    ensure_dir(EARNINGS_DIR)?;
    Ok(())
}
