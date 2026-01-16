//! Go workspace parsing (go.work, go.mod).

/// Parse workspace members from go.mod (no workspace, returns empty).
pub fn parse_workspace(_content: &str) -> Vec<String> {
    // go.mod doesn't define workspaces, go.work does
    vec![]
}

/// Parse workspace members from go.work.
pub fn parse_gowork(content: &str) -> Vec<String> {
    let mut members = vec![];
    let mut in_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == ")" {
            in_block = false;
        } else if trimmed.starts_with("use (") || trimmed == "use (" {
            in_block = true;
        } else if let Some(rest) = trimmed.strip_prefix("use ") {
            // Single use: use ./cmd/foo
            let path = rest.trim().trim_start_matches("./");
            if !path.is_empty() {
                members.push(path.to_string());
            }
        } else if in_block && trimmed.starts_with("./") {
            let path = trimmed.trim_start_matches("./");
            members.push(path.to_string());
        }
    }

    members
}

/// Parse module name from go.mod.
pub fn parse_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("module ") {
            return Some(trimmed.strip_prefix("module ")?.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gowork_single() {
        let content = "go 1.21\nuse ./cmd/server";
        assert_eq!(parse_gowork(content), vec!["cmd/server"]);
    }

    #[test]
    fn test_gowork_multi() {
        let content = r#"
go 1.21

use (
    ./cmd/server
    ./pkg/lib
)
"#;
        assert_eq!(parse_gowork(content), vec!["cmd/server", "pkg/lib"]);
    }

    #[test]
    fn test_go_mod_name() {
        let content = "module github.com/acme/myapp\n\ngo 1.21";
        assert_eq!(
            parse_name(content),
            Some("github.com/acme/myapp".to_string())
        );
    }
}
