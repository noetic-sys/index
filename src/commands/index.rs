//! Index command - trigger package indexing.

use std::str::FromStr;

use crate::types::Registry;
use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalIndexer};

#[derive(Args)]
pub struct IndexCmd {
    /// Package spec: registry:name@version (e.g., npm:axios@1.7.9)
    pub package: String,
}

impl IndexCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let (registry_str, name, version) = parse_package_spec(&self.package)?;

        let registry = Registry::from_str(&registry_str)
            .map_err(|e| anyhow::anyhow!("Unknown registry '{}': {}", registry_str, e))?;

        println!("Indexing {}:{}@{}...", registry_str, name, version);

        let indexer = LocalIndexer::new(&index_dir).await?;
        let result = indexer.index_package(registry, &name, &version).await?;

        if result.chunks_indexed > 0 {
            println!(
                "Indexed {} chunks from {} files",
                result.chunks_indexed, result.files_processed
            );
        } else {
            println!("Already indexed (skipped)");
        }

        Ok(())
    }
}

/// Parse package spec: registry:name@version
fn parse_package_spec(spec: &str) -> Result<(String, String, String)> {
    let (registry, rest) = spec
        .split_once(':')
        .context("Invalid format. Use: registry:name@version (e.g., npm:axios@1.7.9)")?;

    let (name, version) = rest
        .rsplit_once('@')
        .context("Invalid format. Use: registry:name@version (e.g., npm:axios@1.7.9)")?;

    Ok((registry.to_string(), name.to_string(), version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_spec() {
        let (reg, name, ver) = parse_package_spec("npm:axios@1.7.9").unwrap();
        assert_eq!(reg, "npm");
        assert_eq!(name, "axios");
        assert_eq!(ver, "1.7.9");
    }

    #[test]
    fn test_parse_package_spec_crates() {
        let (reg, name, ver) = parse_package_spec("crates:serde@1.0.228").unwrap();
        assert_eq!(reg, "crates");
        assert_eq!(name, "serde");
        assert_eq!(ver, "1.0.228");
    }

    #[test]
    fn test_parse_package_spec_scoped_npm() {
        let (reg, name, ver) = parse_package_spec("npm:@types/node@20.0.0").unwrap();
        assert_eq!(reg, "npm");
        assert_eq!(name, "@types/node");
        assert_eq!(ver, "20.0.0");
    }

    #[test]
    fn test_parse_package_spec_invalid() {
        assert!(parse_package_spec("invalid").is_err());
        assert!(parse_package_spec("npm:axios").is_err());
    }
}
