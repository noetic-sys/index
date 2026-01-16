//! Local configuration management.
//!
//! Config is stored at `~/.config/idx/config.toml` and contains:
//! - OpenAI API key for embeddings

use std::path::PathBuf;

use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

const CONFIG_DIR: &str = "idx";
const CONFIG_FILE: &str = "config.toml";

/// Local configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    /// OpenAI API key for embeddings.
    #[serde(default)]
    pub openai_api_key: Option<String>,

    /// Base URL for OpenAI-compatible API (default: https://api.openai.com).
    #[serde(default = "default_openai_base_url")]
    pub openai_base_url: String,

    /// Model to use for embeddings (default: text-embedding-3-small).
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
}

fn default_openai_base_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            openai_api_key: None,
            openai_base_url: default_openai_base_url(),
            embedding_model: default_embedding_model(),
        }
    }
}

impl LocalConfig {
    /// Load config from the default location.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path).context("Failed to read config file")?;

        toml::from_str(&content).context("Failed to parse config file")
    }

    /// Save config to the default location.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;

        std::fs::write(&path, content).context("Failed to write config file")
    }

    /// Get the OpenAI API key as a SecretString.
    pub fn openai_api_key_secret(&self) -> Option<SecretString> {
        self.openai_api_key.clone().map(SecretString::from)
    }

    /// Check if the config has a valid OpenAI API key.
    pub fn has_openai_key(&self) -> bool {
        self.openai_api_key
            .as_ref()
            .map(|k| !k.is_empty())
            .unwrap_or(false)
    }

    /// Set the OpenAI API key.
    pub fn set_openai_key(&mut self, key: String) {
        self.openai_api_key = Some(key);
    }

    /// Get the config file path.
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;

        Ok(config_dir.join(CONFIG_DIR).join(CONFIG_FILE))
    }

    /// Get the config directory path.
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;

        Ok(config_dir.join(CONFIG_DIR))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LocalConfig::default();
        assert!(config.openai_api_key.is_none());
        assert_eq!(config.openai_base_url, "https://api.openai.com");
        assert_eq!(config.embedding_model, "text-embedding-3-small");
    }

    #[test]
    fn test_has_openai_key() {
        let mut config = LocalConfig::default();
        assert!(!config.has_openai_key());

        config.set_openai_key("sk-test123".to_string());
        assert!(config.has_openai_key());
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut config = LocalConfig::default();
        config.set_openai_key("sk-test".to_string());

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: LocalConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.openai_api_key, config.openai_api_key);
        assert_eq!(parsed.openai_base_url, config.openai_base_url);
    }
}
