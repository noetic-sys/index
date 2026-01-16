//! npm/pnpm/Lerna workspace parsing.

use serde::Deserialize;

#[derive(Deserialize)]
struct PackageJson {
    name: Option<String>,
    workspaces: Option<Workspaces>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Workspaces {
    Array(Vec<String>),
    Object { packages: Vec<String> },
}

#[derive(Deserialize)]
struct LernaJson {
    packages: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct PnpmWorkspace {
    packages: Option<Vec<String>>,
}

/// Parse workspace members from package.json.
pub fn parse_workspace(content: &str) -> Vec<String> {
    let pkg: PackageJson = match serde_json::from_str(content) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    match pkg.workspaces {
        Some(Workspaces::Array(arr)) => arr,
        Some(Workspaces::Object { packages }) => packages,
        None => vec![],
    }
}

/// Parse pnpm-workspace.yaml.
pub fn parse_pnpm_workspace(content: &str) -> Vec<String> {
    let ws: PnpmWorkspace = match serde_yaml::from_str(content) {
        Ok(w) => w,
        Err(_) => return vec![],
    };
    ws.packages.unwrap_or_default()
}

/// Parse lerna.json.
pub fn parse_lerna(content: &str) -> Vec<String> {
    let lerna: LernaJson = match serde_json::from_str(content) {
        Ok(l) => l,
        Err(_) => return vec![],
    };
    lerna.packages.unwrap_or_default()
}

/// Parse package name from package.json.
pub fn parse_name(content: &str) -> Option<String> {
    let pkg: PackageJson = serde_json::from_str(content).ok()?;
    pkg.name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_npm_workspaces() {
        let content = r#"{"workspaces": ["packages/*", "apps/*"]}"#;
        assert_eq!(parse_workspace(content), vec!["packages/*", "apps/*"]);
    }

    #[test]
    fn test_yarn_workspaces() {
        let content = r#"{"workspaces": {"packages": ["packages/*"]}}"#;
        assert_eq!(parse_workspace(content), vec!["packages/*"]);
    }

    #[test]
    fn test_pnpm_workspace() {
        let content = "packages:\n  - 'packages/*'\n  - 'apps/*'";
        assert_eq!(parse_pnpm_workspace(content), vec!["packages/*", "apps/*"]);
    }

    #[test]
    fn test_lerna() {
        let content = r#"{"packages": ["packages/*"]}"#;
        assert_eq!(parse_lerna(content), vec!["packages/*"]);
    }
}
