//! Explain command - quick symbol lookup by name.

use anyhow::{Context, Result};
use clap::Args;

use crate::local::{self, LocalSearch};

#[derive(Args)]
pub struct ExplainCmd {
    /// Symbol name (e.g., "from_str" or "serde_json::from_str")
    pub symbol: String,

    /// Filter to specific package
    #[arg(short, long)]
    pub package: Option<String>,
}

impl ExplainCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir =
            local::get_index_dir().context("No .index directory found. Run `idx init` first.")?;

        let search = LocalSearch::new(&index_dir).await?;

        // Parse symbol: "serde_json::from_str" -> (Some("serde_json"), "from_str")
        let (pkg_hint, name) = parse_symbol(&self.symbol);
        let package = self.package.as_deref().or(pkg_hint);

        let results = search.find_by_name(name, package).await?;

        if results.is_empty() {
            println!("No symbol found matching `{}`", self.symbol);
            println!("\nTry: idx search \"{}\"", self.symbol);
            return Ok(());
        }

        // Show the first (best) match
        let r = &results[0];

        // Header
        println!(
            "{} `{}` from {}:{}@{}",
            r.chunk_type, r.name, r.registry, r.package, r.version
        );
        println!("{} L{}-{}", r.file_path, r.start_line, r.end_line);

        // Signature
        if let Some(ref sig) = r.signature {
            println!("\nSignature:");
            println!("  {}", sig.replace('\n', "\n  "));
        }

        // Docstring
        if let Some(ref doc) = r.docstring {
            println!("\nDocumentation:");
            for line in doc.lines().take(10) {
                println!("  {}", line);
            }
            if doc.lines().count() > 10 {
                println!("  ...");
            }
        }

        // Code
        println!("\nCode:");
        println!("  ----------------------------------------");
        if let Ok(code) = search.get_code(&r.storage_key).await {
            for line in code.lines() {
                println!("  {}", line);
            }
        } else {
            for line in r.snippet.lines() {
                println!("  {}", line);
            }
        }
        println!("  ----------------------------------------");

        // Other matches
        if results.len() > 1 {
            println!(
                "\n{} other matches found. Use `idx search \"{}\"` to see all.",
                results.len() - 1,
                name
            );
        }

        Ok(())
    }
}

/// Parse "serde_json::from_str" into (Some("serde_json"), "from_str")
fn parse_symbol(symbol: &str) -> (Option<&str>, &str) {
    if let Some(pos) = symbol.rfind("::") {
        let pkg = &symbol[..pos];
        let name = &symbol[pos + 2..];
        // Take root module as package hint
        let root = pkg.split("::").next().unwrap_or(pkg);
        (Some(root), name)
    } else if let Some(pos) = symbol.rfind('.') {
        // JS style: lodash.get
        (Some(&symbol[..pos]), &symbol[pos + 1..])
    } else {
        (None, symbol)
    }
}
