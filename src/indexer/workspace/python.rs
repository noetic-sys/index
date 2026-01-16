//! Python workspace parsing (Poetry, PDM, Hatch).

use toml::Value;

/// Parse workspace members from pyproject.toml.
pub fn parse_workspace(content: &str) -> Vec<String> {
    let toml: Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    // Poetry: [tool.poetry.packages] or workspace plugin
    // PDM: [tool.pdm.dev-dependencies] with workspace
    // Hatch: [tool.hatch.envs]
    // For now, check common patterns

    // Poetry workspace (via poetry-monorepo-plugin)
    if let Some(members) = toml
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("packages"))
        .and_then(|p| p.as_array())
    {
        return members
            .iter()
            .filter_map(|v| v.get("include").and_then(|i| i.as_str()).map(String::from))
            .collect();
    }

    vec![]
}

/// Parse package name from pyproject.toml.
pub fn parse_name(content: &str) -> Option<String> {
    let toml: Value = content.parse().ok()?;

    // PEP 621: [project] name
    if let Some(name) = toml
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        return Some(name.to_string());
    }

    // Poetry: [tool.poetry] name
    if let Some(name) = toml
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        return Some(name.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pep621_name() {
        let content = r#"
[project]
name = "my-package"
version = "1.0.0"
"#;
        assert_eq!(parse_name(content), Some("my-package".to_string()));
    }

    #[test]
    fn test_poetry_name() {
        let content = r#"
[tool.poetry]
name = "my-poetry-pkg"
version = "1.0.0"
"#;
        assert_eq!(parse_name(content), Some("my-poetry-pkg".to_string()));
    }
}
