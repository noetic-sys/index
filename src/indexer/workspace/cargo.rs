//! Cargo workspace parsing.

use toml::Value;

/// Parse workspace members from Cargo.toml.
pub fn parse_workspace(content: &str) -> Vec<String> {
    let toml: Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    toml.get("workspace")
        .and_then(|ws| ws.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse package name from Cargo.toml.
pub fn parse_name(content: &str) -> Option<String> {
    let toml: Value = content.parse().ok()?;
    toml.get("package")?.get("name")?.as_str().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_members() {
        let content = r#"
[workspace]
members = ["crates/*", "apps/*"]
"#;
        assert_eq!(parse_workspace(content), vec!["crates/*", "apps/*"]);
    }

    #[test]
    fn test_parse_name() {
        let content = r#"
[package]
name = "my-crate"
version = "0.1.0"
"#;
        assert_eq!(parse_name(content), Some("my-crate".to_string()));
    }
}
