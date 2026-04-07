//! Search command - find code within indexed packages.

use anyhow::Result;
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

    /// Search the entire global index instead of scoping to this project's dependencies
    #[arg(long)]
    pub global: bool,
}

impl SearchCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = local::get_index_dir();

        let project_id = if self.global {
            None
        } else {
            let cwd = std::env::current_dir()?;
            let id = local::project_id(&cwd);
            Some(id)
        };

        let start = std::time::Instant::now();
        let search = LocalSearch::new(&index_dir).await?;

        // Verify the project has been initialized if scoping
        if let Some(ref pid) = project_id {
            let deps = search.db().list_project_packages(pid).await?;
            if deps.is_empty() {
                anyhow::bail!(
                    "No packages indexed for this project. Run `idx init` first, or use --global."
                );
            }
        }

        let results = search
            .search(
                &self.query,
                self.package.as_deref(),
                self.registry.as_deref(),
                self.version.as_deref(),
                project_id.as_deref(),
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
                let snippet: String = r.snippet.lines().take(3).collect::<Vec<_>>().join("\n   ");
                println!("   {}", snippet);
            }
            println!();
        }

        Ok(())
    }
}
