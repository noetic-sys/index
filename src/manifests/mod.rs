//! Manifest file parsing for dependency extraction.

mod cargo;
mod discover;
mod go;
mod maven;
mod npm;
mod python;

pub use cargo::parse_cargo_deps;
pub use discover::discover_manifest_dirs;
pub use go::parse_go_deps;
pub use maven::parse_maven_deps;
pub use npm::parse_npm_deps;
pub use python::parse_python_deps;

/// A dependency extracted from a manifest file.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub registry: String,
    pub name: String,
    pub version: String,
}

/// Collect all dependencies from manifests found under `path`.
///
/// Discovers manifest roots (handles monorepos), parses all supported registries,
/// and deduplicates by (registry, name), keeping the first occurrence.
pub fn collect_manifest_deps(path: &std::path::Path) -> anyhow::Result<Vec<Dependency>> {
    use std::collections::HashMap;

    let manifest_dirs = discover_manifest_dirs(path)?;
    let mut all_deps: Vec<Dependency> = Vec::new();

    for dir in &manifest_dirs {
        if let Ok(deps) = parse_npm_deps(dir) {
            all_deps.extend(deps);
        }
        if let Ok(deps) = parse_cargo_deps(dir) {
            all_deps.extend(deps);
        }
        if let Ok(deps) = parse_python_deps(dir) {
            all_deps.extend(deps);
        }
        if let Ok(deps) = parse_maven_deps(dir) {
            all_deps.extend(deps);
        }
        if let Ok(deps) = parse_go_deps(dir) {
            all_deps.extend(deps);
        }
    }

    // Dedupe by (registry, name) — keep first occurrence
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for (i, dep) in all_deps.iter().enumerate() {
        seen.entry((dep.registry.clone(), dep.name.clone()))
            .or_insert(i);
    }

    let mut indices: Vec<_> = seen.into_values().collect();
    indices.sort();

    Ok(indices.into_iter().map(|i| all_deps[i].clone()).collect())
}

