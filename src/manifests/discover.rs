//! Manifest discovery for monorepos.
//!
//! Walks the directory tree to find all manifest files, skipping build/cache directories.
//! Optionally reads `.idx.toml` for explicit root configuration.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Directories to skip during manifest discovery.
/// Unlike indexer patterns, we DON'T skip tests/examples - those can have their own manifests.
const SKIP_DIRS: &[&str] = &[
    // Dependencies
    "node_modules",
    "vendor",
    // Build artifacts
    "target",
    "dist",
    "build",
    ".build",
    ".next",
    ".nuxt",
    ".output",
    "out",
    // Caches
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".cache",
    ".parcel-cache",
    ".turbo",
    "coverage",
    ".nyc_output",
    // Virtual envs
    ".venv",
    "venv",
    // VCS
    ".git",
    ".svn",
    ".hg",
    // IDE
    ".idea",
    ".vscode",
    // Our own index
    ".index",
];

/// Manifest files we look for.
const MANIFEST_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
];

/// Configuration from `.idx.toml`.
#[derive(Debug, Default)]
pub struct DiscoveryConfig {
    /// Explicit roots to scan (if set, disables auto-discovery).
    pub roots: Option<Vec<PathBuf>>,
    /// Additional directories to exclude during discovery.
    pub exclude: Vec<String>,
}

impl DiscoveryConfig {
    /// Load config from `.idx.toml` in the given directory.
    pub fn load(dir: &Path) -> Result<Self> {
        let config_path = dir.join(".idx.toml");
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let toml: toml::Value = content.parse()?;

        let roots = toml.get("roots").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| dir.join(s))
                .collect()
        });

        let exclude = toml
            .get("exclude")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self { roots, exclude })
    }
}

/// Discover all directories containing manifest files.
///
/// Returns a list of unique directories that contain at least one manifest file.
/// If `.idx.toml` specifies explicit roots, uses those instead of auto-discovery.
pub fn discover_manifest_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let config = DiscoveryConfig::load(root)?;

    // If explicit roots are configured, use them
    if let Some(roots) = config.roots {
        let valid_roots: Vec<_> = roots
            .into_iter()
            .filter(|p| p.exists() && has_manifest(p))
            .collect();
        return Ok(valid_roots);
    }

    // Auto-discover
    let mut manifest_dirs = HashSet::new();
    let extra_excludes: HashSet<_> = config.exclude.iter().map(|s| s.as_str()).collect();

    discover_recursive(root, &mut manifest_dirs, &extra_excludes)?;

    let mut dirs: Vec<_> = manifest_dirs.into_iter().collect();
    dirs.sort();
    Ok(dirs)
}

fn discover_recursive(
    dir: &Path,
    found: &mut HashSet<PathBuf>,
    extra_excludes: &HashSet<&str>,
) -> Result<()> {
    // Check if this directory has any manifest files
    if has_manifest(dir) {
        found.insert(dir.to_path_buf());
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // Skip unreadable directories
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // Skip junk directories
        if should_skip(name, extra_excludes) {
            continue;
        }

        discover_recursive(&path, found, extra_excludes)?;
    }

    Ok(())
}

fn should_skip(name: &str, extra_excludes: &HashSet<&str>) -> bool {
    // Check standard excludes
    if SKIP_DIRS.contains(&name) {
        return true;
    }

    // Check user-configured excludes
    if extra_excludes.contains(name) {
        return true;
    }

    false
}

fn has_manifest(dir: &Path) -> bool {
    MANIFEST_FILES.iter().any(|&f| dir.join(f).exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discover_monorepo() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create monorepo structure
        fs::create_dir_all(root.join("frontend")).unwrap();
        fs::write(root.join("frontend/package.json"), "{}").unwrap();

        fs::create_dir_all(root.join("backend")).unwrap();
        fs::write(
            root.join("backend/Cargo.toml"),
            "[package]\nname = \"backend\"",
        )
        .unwrap();

        fs::create_dir_all(root.join("services/api")).unwrap();
        fs::write(root.join("services/api/go.mod"), "module api").unwrap();

        // Create some junk that should be skipped
        fs::create_dir_all(root.join("frontend/node_modules/foo")).unwrap();
        fs::write(root.join("frontend/node_modules/foo/package.json"), "{}").unwrap();

        let dirs = discover_manifest_dirs(root).unwrap();

        assert_eq!(dirs.len(), 3);
        assert!(dirs.iter().any(|p| p.ends_with("frontend")));
        assert!(dirs.iter().any(|p| p.ends_with("backend")));
        assert!(dirs.iter().any(|p| p.ends_with("api")));

        // Should NOT include node_modules
        assert!(
            !dirs
                .iter()
                .any(|p| p.to_string_lossy().contains("node_modules"))
        );
    }

    #[test]
    fn test_discover_with_exclude_config() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create dirs
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("app/package.json"), "{}").unwrap();

        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/Cargo.toml"), "[package]\nname = \"lib\"").unwrap();

        fs::create_dir_all(root.join("experiments")).unwrap();
        fs::write(root.join("experiments/package.json"), "{}").unwrap();

        // Config excludes experiments
        fs::write(root.join(".idx.toml"), "exclude = [\"experiments\"]").unwrap();

        let dirs = discover_manifest_dirs(root).unwrap();

        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().any(|p| p.ends_with("app")));
        assert!(dirs.iter().any(|p| p.ends_with("lib")));
        assert!(!dirs.iter().any(|p| p.ends_with("experiments")));
    }

    #[test]
    fn test_discover_explicit_roots() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create dirs
        fs::create_dir_all(root.join("frontend")).unwrap();
        fs::write(root.join("frontend/package.json"), "{}").unwrap();

        fs::create_dir_all(root.join("backend")).unwrap();
        fs::write(
            root.join("backend/Cargo.toml"),
            "[package]\nname = \"backend\"",
        )
        .unwrap();

        fs::create_dir_all(root.join("other")).unwrap();
        fs::write(root.join("other/go.mod"), "module other").unwrap();

        // Config specifies only frontend and backend
        fs::write(
            root.join(".idx.toml"),
            "roots = [\"frontend\", \"backend\"]",
        )
        .unwrap();

        let dirs = discover_manifest_dirs(root).unwrap();

        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().any(|p| p.ends_with("frontend")));
        assert!(dirs.iter().any(|p| p.ends_with("backend")));
        // other should NOT be included even though it has a manifest
        assert!(!dirs.iter().any(|p| p.ends_with("other")));
    }

    #[test]
    fn test_single_project_at_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Single project at root
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();

        let dirs = discover_manifest_dirs(root).unwrap();

        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], root);
    }

    #[test]
    fn test_includes_test_dirs_with_manifests() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Main app
        fs::write(root.join("package.json"), "{}").unwrap();

        // Test package with its own manifest (should be included!)
        fs::create_dir_all(root.join("tests/e2e")).unwrap();
        fs::write(root.join("tests/e2e/package.json"), "{}").unwrap();

        let dirs = discover_manifest_dirs(root).unwrap();

        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().any(|p| p == root));
        assert!(dirs.iter().any(|p| p.ends_with("e2e")));
    }
}
