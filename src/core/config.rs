use anyhow::{anyhow, Result};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AdvisorConfig {
    pub openai_key: String,
    pub database_url: String,
    pub user_agent: String,
    pub data_dir: PathBuf,
}

impl AdvisorConfig {
    pub fn from_env() -> Result<Self> {
        let openai_key = std::env::var("OPENAI_KEY")
            .map_err(|_| anyhow!("OPENAI_KEY environment variable not set"))?;

        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow!("DATABASE_URL environment variable not set"))?;

        let user_agent = std::env::var("USER_AGENT")
            .unwrap_or_else(|_| "software@example.com".to_string());

        let data_dir = PathBuf::from(
            std::env::var("ADVISOR_DATA_DIR").unwrap_or_else(|_| "data".to_string())
        );

        Ok(Self {
            openai_key,
            database_url,
            user_agent,
            data_dir,
        })
    }
}
