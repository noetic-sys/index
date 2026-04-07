//! Search command - find code within indexed packages.

use anyhow::Result;
use clap::Args;

use crate::local::{self, LocalSearch};

/// High-level content category for filtering search results.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum ContentKind {
    /// Functions, methods, classes, interfaces, types, constants, modules
    Code,
    /// Usage examples
    Example,
    /// READMEs, changelogs, and other documentation
    Documentation,
}

impl ContentKind {
    /// Returns true if the given chunk_type string belongs to this kind.
    fn matches(&self, chunk_type: &str) -> bool {
        match self {
            ContentKind::Code => matches!(
                chunk_type,
                "function" | "method" | "class" | "interface" | "type" | "constant" | "module"
            ),
            ContentKind::Example => chunk_type == "example",
            ContentKind::Documentation => chunk_type == "documentation",
        }
    }
}

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

    /// Filter by content kind: code, example, documentation (repeatable)
    #[arg(short, long = "type", value_name = "KIND")]
    pub kinds: Vec<ContentKind>,

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

        // Fetch extra results to account for post-filtering by kind
        let fetch_limit = if self.kinds.is_empty() {
            self.limit as usize
        } else {
            (self.limit as usize * 4).max(40)
        };

        let mut results = search
            .search(
                &self.query,
                self.package.as_deref(),
                self.registry.as_deref(),
                self.version.as_deref(),
                project_id.as_deref(),
                fetch_limit,
            )
            .await?;

        // Apply kind filter
        if !self.kinds.is_empty() {
            results.retain(|r| self.kinds.iter().any(|k| k.matches(&r.chunk_type)));
            results.truncate(self.limit as usize);
        }

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
            } else if r.signature.is_none() {
                let snippet: String = r.snippet.lines().take(3).collect::<Vec<_>>().join("\n   ");
                println!("   {}", snippet);
            }
            println!();
        }

        Ok(())
    }
}
