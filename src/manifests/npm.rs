//! npm manifest parsing (package.json, package-lock.json).
//!
//! Only indexes DIRECT dependencies, not transitive.
//! Uses pinned versions from package-lock.json if available, otherwise cleans version ranges.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::Dependency;

/// Parse npm dependencies from a directory.
/// Only returns DIRECT dependencies with resolved versions.
pub fn parse_npm_deps(dir: &Path) -> Result<Vec<Dependency>> {
    let pkg_path = dir.join("package.json");
    if !pkg_path.exists() {
        return Ok(vec![]);
    }

    // Get direct deps from package.json
    let direct_deps = parse_package_json(&pkg_path)?;

    if direct_deps.is_empty() {
        return Ok(vec![]);
    }

    // Build version map from package-lock.json (if exists)
    let lock_path = dir.join("package-lock.json");
    let lock_versions = if lock_path.exists() {
        build_lock_version_map(&lock_path).unwrap_or_default()
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
                registry: "npm".to_string(),
                name,
                version: v,
            });
        }
    }

    Ok(deps)
}

#[derive(Deserialize)]
struct PackageJson {
    dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    dev_dependencies: Option<HashMap<String, String>>,
}

/// Parse direct dependencies (name -> version spec) from package.json.
fn parse_package_json(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).context("Failed to read package.json")?;
    let pkg: PackageJson = serde_json::from_str(&content).context("Failed to parse package.json")?;

    let mut deps = HashMap::new();

    for map in [pkg.dependencies, pkg.dev_dependencies].into_iter().flatten() {
        for (name, version) in map {
            // Skip git/file/url deps
            if version.starts_with("git") || version.starts_with("file:")
                || version.starts_with("http") || version.contains("github:") {
                continue;
            }
            deps.insert(name, version);
        }
    }

    Ok(deps)
}

#[derive(Deserialize)]
struct PackageLock {
    packages: Option<HashMap<String, PackageLockEntry>>,
}

#[derive(Deserialize)]
struct PackageLockEntry {
    version: Option<String>,
}

/// Build a name -> version map from package-lock.json (top-level packages only).
fn build_lock_version_map(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).context("Failed to read package-lock.json")?;
    let lock: PackageLock = serde_json::from_str(&content).context("Failed to parse package-lock.json")?;

    let mut map = HashMap::new();

    if let Some(packages) = lock.packages {
        for (key, entry) in packages {
            // Skip root
            if key.is_empty() || key == "." {
                continue;
            }

            // Keys: "node_modules/axios" or "node_modules/@types/node"
            let name = key.strip_prefix("node_modules/").unwrap_or(&key);

            // Skip nested deps (transitive)
            if name.contains("node_modules/") {
                continue;
            }

            if let Some(version) = entry.version {
                map.insert(name.to_string(), version);
            }
        }
    }

    Ok(map)
}

fn clean_version(version: &str) -> Option<String> {
    let v = version
        .trim()
        .trim_start_matches('^')
        .trim_start_matches('~')
        .trim_start_matches('=')
        .trim_start_matches('v');

    // Skip ranges, urls, git refs
    if v.contains(' ') || v.contains("||") || v.contains("http") || v.contains("git")
        || v.contains("file:") || v.starts_with('>') || v.starts_with('<') || v.starts_with('*')
    {
        return None;
    }

    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_version() {
        assert_eq!(clean_version("^1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(clean_version("~1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(clean_version(">=1.0.0 <2.0.0"), None);
        assert_eq!(clean_version("*"), None);
    }
}
