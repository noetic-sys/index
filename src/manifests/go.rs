//! Go module parsing (go.mod).
//!
//! Only indexes DIRECT dependencies, not transitive.
//! Go modules use paths like github.com/user/repo.

use std::path::Path;

use anyhow::{Context, Result};

use super::Dependency;

/// Parse Go dependencies from a directory.
/// Returns DIRECT dependencies from go.mod.
pub fn parse_go_deps(dir: &Path) -> Result<Vec<Dependency>> {
    let mod_path = dir.join("go.mod");
    if !mod_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&mod_path).context("Failed to read go.mod")?;
    let deps = parse_go_mod(&content);

    Ok(deps)
}

/// Parse dependencies from go.mod content.
fn parse_go_mod(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut in_require_block = false;

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Handle require block
        if line == "require (" {
            in_require_block = true;
            continue;
        }

        if line == ")" {
            in_require_block = false;
            continue;
        }

        // Parse require statements
        if in_require_block {
            if let Some(dep) = parse_require_line(line) {
                deps.push(dep);
            }
        } else if line.starts_with("require ") {
            // Single-line require
            let rest = line.strip_prefix("require ").unwrap().trim();
            if let Some(dep) = parse_require_line(rest) {
                deps.push(dep);
            }
        }
    }

    deps
}

/// Parse a single require line like "github.com/gin-gonic/gin v1.9.1"
fn parse_require_line(line: &str) -> Option<Dependency> {
    let line = line.trim();

    // Skip indirect dependencies (comments with // indirect)
    if line.contains("// indirect") {
        return None;
    }

    // Split on whitespace
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let module = parts[0];
    let version = parts[1];

    // Clean version (remove v prefix for consistency, though Go uses it)
    let version = version.trim_start_matches('v');

    Some(Dependency {
        registry: "go".to_string(),
        name: module.to_string(),
        version: version.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_require_block() {
        let content = r#"
module example.com/myapp

go 1.21

require (
    github.com/gin-gonic/gin v1.9.1
    github.com/stretchr/testify v1.8.4
)
"#;
        let deps = parse_go_mod(content);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].name, "github.com/gin-gonic/gin");
        assert_eq!(deps[0].version, "1.9.1");
        assert_eq!(deps[1].name, "github.com/stretchr/testify");
    }

    #[test]
    fn test_parse_single_require() {
        let content = r#"
module example.com/myapp

go 1.21

require github.com/gin-gonic/gin v1.9.1
"#;
        let deps = parse_go_mod(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "github.com/gin-gonic/gin");
    }

    #[test]
    fn test_skip_indirect() {
        let content = r#"
module example.com/myapp

require (
    github.com/gin-gonic/gin v1.9.1
    github.com/indirect/dep v1.0.0 // indirect
)
"#;
        let deps = parse_go_mod(content);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "github.com/gin-gonic/gin");
    }

    #[test]
    fn test_mixed_requires() {
        let content = r#"
module example.com/myapp

require github.com/first/dep v1.0.0

require (
    github.com/second/dep v2.0.0
    github.com/third/dep v3.0.0
)

require github.com/fourth/dep v4.0.0
"#;
        let deps = parse_go_mod(content);
        assert_eq!(deps.len(), 4);
    }
}
