//! Config command - manage local configuration.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::local::LocalConfig;

#[derive(Args)]
pub struct ConfigCmd {
    #[command(subcommand)]
    pub command: ConfigSubCmd,
}

#[derive(Subcommand)]
pub enum ConfigSubCmd {
    /// Set the API key for embeddings
    SetKey(SetKeyCmd),

    /// Set the API base URL (default: https://api.openai.com)
    SetUrl(SetUrlCmd),

    /// Set the embedding model (default: text-embedding-3-small)
    SetModel(SetModelCmd),

    /// Show current configuration
    Show,
}

#[derive(Args)]
pub struct SetKeyCmd {
    /// API key (OpenAI or OpenRouter)
    pub key: String,
}

#[derive(Args)]
pub struct SetUrlCmd {
    /// API base URL (e.g., https://openrouter.ai/api)
    pub url: String,
}

#[derive(Args)]
pub struct SetModelCmd {
    /// Embedding model name (e.g., text-embedding-3-small, openai/text-embedding-3-small)
    pub model: String,
}

impl ConfigCmd {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            ConfigSubCmd::SetKey(cmd) => {
                let mut config = LocalConfig::load()?;
                config.set_openai_key(cmd.key.clone());
                config.save()?;
                println!("API key saved.");
            }
            ConfigSubCmd::SetUrl(cmd) => {
                let mut config = LocalConfig::load()?;
                config.openai_base_url = cmd.url.clone();
                config.save()?;
                println!("Base URL set to: {}", cmd.url);
            }
            ConfigSubCmd::SetModel(cmd) => {
                let mut config = LocalConfig::load()?;
                config.embedding_model = cmd.model.clone();
                config.save()?;
                println!("Embedding model set to: {}", cmd.model);
            }
            ConfigSubCmd::Show => {
                let config = LocalConfig::load()?;
                println!("Config: {}", LocalConfig::config_path()?.display());
                println!();
                println!(
                    "api_key:    {}",
                    if config.has_openai_key() {
                        "(set)"
                    } else {
                        "(not set)"
                    }
                );
                println!("base_url:   {}", config.openai_base_url);
                println!("model:      {}", config.embedding_model);
            }
        }
        Ok(())
    }
}
