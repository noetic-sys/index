//! Cargo manifest parsing (Cargo.toml, Cargo.lock).
//!
//! Only indexes DIRECT dependencies, not transitive.
//! Uses pinned versions from Cargo.lock if available, otherwise cleans version ranges.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use super::Dependency;

/// Parse Cargo dependencies from a directory.
/// Only returns DIRECT dependencies with resolved versions.
pub fn parse_cargo_deps(dir: &Path) -> Result<Vec<Dependency>> {
    let toml_path = dir.join("Cargo.toml");
    if !toml_path.exists() {
        return Ok(vec![]);
    }

    // Get direct deps with version specs from Cargo.toml (and workspace members)
    let direct_deps = collect_direct_deps(dir)?;

    if direct_deps.is_empty() {
        return Ok(vec![]);
    }

    // Build version map from Cargo.lock (if exists)
    let lock_path = dir.join("Cargo.lock");
    let lock_versions = if lock_path.exists() {
        build_version_map(&lock_path).unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Resolve versions: prefer lockfile, fall back to cleaned manifest version
    let mut deps = Vec::new();
    for (name, manifest_version) in direct_deps {
        let version = lock_versions
            .get(&name)
            .cloned()
            .or_else(|| clean_version(&manifest_version));

        if let Some(v) = version {
            deps.push(Dependency {
                registry: "crates".to_string(),
                name,
                version: v,
            });
        }
    }

    Ok(deps)
}

/// Collect direct dependencies (name -> version spec) from Cargo.toml and workspace members.
fn collect_direct_deps(dir: &Path) -> Result<HashMap<String, String>> {
    let mut all_deps = HashMap::new();

    let toml_path = dir.join("Cargo.toml");
    let content = std::fs::read_to_string(&toml_path).context("Failed to read Cargo.toml")?;
    let toml: toml::Value = content.parse().context("Failed to parse Cargo.toml")?;

    // Get workspace.dependencies first (these are the canonical versions)
    let workspace_versions: HashMap<String, String> = toml
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(name, value)| {
                    let version = extract_version(value)?;
                    Some((name.clone(), version))
                })
                .collect()
        })
        .unwrap_or_default();

    // Get deps from root Cargo.toml
    all_deps.extend(extract_deps(&toml, &workspace_versions));

    // Check for workspace members
    if let Some(workspace) = toml.get("workspace")
        && let Some(members) = workspace.get("members").and_then(|m| m.as_array())
    {
        for member in members {
            if let Some(member_pattern) = member.as_str() {
                // Expand glob patterns like "crates/*"
                let pattern = dir.join(member_pattern).join("Cargo.toml");
                let pattern_str = pattern.to_string_lossy();

                if let Ok(paths) = glob::glob(&pattern_str) {
                    for entry in paths.flatten() {
                        if let Ok(content) = std::fs::read_to_string(&entry)
                            && let Ok(toml) = content.parse::<toml::Value>()
                        {
                            all_deps.extend(extract_deps(&toml, &workspace_versions));
                        }
                    }
                }
            }
        }
    }

    Ok(all_deps)
}

/// Extract dependencies (name -> version) from a Cargo.toml.
fn extract_deps(
    toml: &toml::Value,
    workspace_versions: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut deps = HashMap::new();

    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(d) = toml.get(section).and_then(|v| v.as_table()) {
            for (name, value) in d {
                // Skip path/git deps
                if let toml::Value::Table(t) = value {
                    if t.contains_key("path") || t.contains_key("git") {
                        continue;
                    }
                    // Check for workspace = true
                    if t.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                        if let Some(v) = workspace_versions.get(name) {
                            deps.insert(name.clone(), v.clone());
                        }
                        continue;
                    }
                }

                if let Some(version) = extract_version(value) {
                    deps.insert(name.clone(), version);
                }
            }
        }
    }

    deps
}

/// Extract version string from a dependency value.
fn extract_version(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(v) => Some(v.clone()),
        toml::Value::Table(t) => {
            // Skip path/git deps
            if t.contains_key("path") || t.contains_key("git") {
                return None;
            }
            t.get("version").and_then(|v| v.as_str()).map(String::from)
        }
        _ => None,
    }
}

/// Build a name -> version map from Cargo.lock.
fn build_version_map(path: &Path) -> Result<HashMap<String, String>> {
    let lockfile = cargo_lock::Lockfile::load(path).context("Failed to parse Cargo.lock")?;

    let map = lockfile
        .packages
        .into_iter()
        .map(|pkg| (pkg.name.as_str().to_string(), pkg.version.to_string()))
        .collect();

    Ok(map)
}

/// Clean a version range to a usable version.
/// "^1.2.3" -> "1.2.3", "~1.0" -> "1.0", etc.
fn clean_version(version: &str) -> Option<String> {
    let v = version
        .trim()
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches('=')
        .trim_start_matches('>')
        .trim_start_matches('<');

    // Skip complex ranges we can't resolve
    if v.is_empty() || v.contains(',') || v.contains(' ') || v.contains('*') {
        return None;
    }

    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_version() {
        assert_eq!(clean_version("^1.2"), Some("1.2".to_string()));
        assert_eq!(clean_version("1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(clean_version("~1.0.0"), Some("1.0.0".to_string()));
        assert_eq!(clean_version(">=1.0, <2.0"), None);
        assert_eq!(clean_version("*"), None);
    }
}
