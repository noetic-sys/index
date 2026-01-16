//! Search command - find code within indexed packages.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalSearch};

#[derive(Args)]
pub struct SearchCmd {
    /// Natural language query
    pub query: String,

    /// Package to search within
    #[arg(short, long)]
    pub package: Option<String>,

    /// Filter to specific version
    #[arg(short = 'V', long)]
    pub version: Option<String>,

    /// Filter to registry (npm, crates, pypi)
    #[arg(short, long)]
    pub registry: Option<String>,

    /// Include full code (not just snippets)
    #[arg(short = 'c', long)]
    pub code: bool,

    /// Max results
    #[arg(short, long, default_value = "10")]
    pub limit: u32,
}

impl SearchCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir()
            .context("No .index directory found. Run `idx init` first.")?;

        let start = std::time::Instant::now();
        let search = LocalSearch::new(&index_dir).await?;

        let results = search
            .search(
                &self.query,
                self.package.as_deref(),
                self.registry.as_deref(),
                self.version.as_deref(),
                self.limit as usize,
            )
            .await?;

        let elapsed = start.elapsed().as_millis();

        println!("Found {} results in {}ms\n", results.len(), elapsed);

        for (i, r) in results.iter().enumerate() {
            println!(
                "{}. {} `{}` in {}:{}@{} (score: {:.2})",
                i + 1,
                r.chunk_type,
                r.name,
                r.registry,
                r.package,
                r.version,
                r.score
            );
            println!("   {} L{}-{}", r.file_path, r.start_line, r.end_line);

            if let Some(ref sig) = r.signature {
                println!("   {}", sig);
            }

            if self.code {
                if let Ok(code) = search.get_code(&r.storage_key).await {
                    println!("   ---");
                    for line in code.lines() {
                        println!("   {}", line);
                    }
                    println!("   ---");
                }
            } else {
                let snippet: String = r
                    .snippet
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("\n   ");
                println!("   {}", snippet);
            }
            println!();
        }

        Ok(())
    }
}
