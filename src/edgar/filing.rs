use anyhow::Result;
use chardet::detect;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use html_escape::decode_html_entities;
use log::{error, info};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::io::{BufReader, Read};
use std::path::Path;
use url::Url;
use uuencode::uudecode;

// Hardcoded values
const FILING_DATA_DIR: &str = "/path/to/filing/data";
const EDGAR_ARCHIVES_URL: &str = "https://www.sec.gov/Archives/";
const USER_AGENT: &str = "ask-edgar@ask-edgar.com";
