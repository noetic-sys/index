//! Clean command - remove .index directory.

use anyhow::Result;
use clap::Args;

use crate::local;

#[derive(Args)]
pub struct CleanCmd {
    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

impl CleanCmd {
    pub async fn run(&self) -> Result<()> {
        let index_dir = match local::get_index_dir() {
            Some(dir) => dir,
            None => {
                println!("No .index directory found.");
                return Ok(());
            }
        };

        if !self.yes {
            println!("This will delete: {}", index_dir.display());
            print!("Continue? [y/N] ");
            std::io::Write::flush(&mut std::io::stdout())?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Aborted.");
                return Ok(());
            }
        }

        std::fs::remove_dir_all(&index_dir)?;
        println!("Removed {}", index_dir.display());

        Ok(())
    }
}
