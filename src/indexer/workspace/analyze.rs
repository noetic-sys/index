//! Repository analysis - detects packages from files.

use std::collections::HashSet;

use crate::types::Registry;

use super::{cargo, go, jvm, npm, patterns, python, DetectedPackage};
use crate::indexer::Language;

/// Analyze a repository to detect packages.
pub fn analyze_repo(files: &[(String, String)]) -> Vec<DetectedPackage> {
    let manifests = collect_manifests(files);
    let workspace_members = detect_workspaces(files, &manifests);
    let has_workspace = !workspace_members.is_empty();

    let mut packages = Vec::new();

    for (path, content, registry) in manifests {
        let root_path = parent_dir(path);

        if patterns::should_skip_dir(&root_path) {
            continue;
        }

        if has_workspace && !is_member(&root_path, &workspace_members) {
            continue;
        }

        // Only include if there are source files for this registry
        if !has_source_files(&root_path, registry, files) {
            continue;
        }

        let name = parse_name(content, registry);
        packages.push(DetectedPackage { registry, name, root_path });
    }

    dedupe(&mut packages);
    packages
}

/// Check if a directory has source files matching the registry's languages.
fn has_source_files(root_path: &str, registry: Registry, files: &[(String, String)]) -> bool {
    let langs = Language::from_registry(registry);

    files.iter().any(|(path, _)| {
        let in_subtree = if root_path.is_empty() {
            true
        } else {
            path.starts_with(root_path)
        };

        in_subtree && Language::from_path(path)
            .map(|lang| langs.contains(&lang))
            .unwrap_or(false)
    })
}

fn collect_manifests<'a>(files: &'a [(String, String)]) -> Vec<(&'a str, &'a str, Registry)> {
    files
        .iter()
        .filter_map(|(path, content)| {
            let reg = manifest_registry(path)?;
            Some((path.as_str(), content.as_str(), reg))
        })
        .collect()
}

fn manifest_registry(path: &str) -> Option<Registry> {
    let name = path.rsplit('/').next()?;
    match name {
        "package.json" => Some(Registry::Npm),
        "Cargo.toml" => Some(Registry::Crates),
        "pyproject.toml" | "setup.py" => Some(Registry::Pypi),
        "go.mod" => Some(Registry::Go),
        "pom.xml" => Some(Registry::Maven),
        _ => None,
    }
}

fn detect_workspaces(files: &[(String, String)], manifests: &[(&str, &str, Registry)]) -> HashSet<String> {
    let mut members = HashSet::new();

    // Root manifests with workspace config
    for (path, content, registry) in manifests {
        if path.contains('/') {
            continue;
        }
        let m = match registry {
            Registry::Npm => npm::parse_workspace(content),
            Registry::Crates => cargo::parse_workspace(content),
            Registry::Pypi => python::parse_workspace(content),
            Registry::Go => go::parse_workspace(content),
            Registry::Maven => jvm::parse_workspace(content),
        };
        members.extend(m);
    }

    // Standalone workspace files
    for (path, content) in files {
        match path.as_str() {
            "pnpm-workspace.yaml" => members.extend(npm::parse_pnpm_workspace(content)),
            "lerna.json" => members.extend(npm::parse_lerna(content)),
            "go.work" => members.extend(go::parse_gowork(content)),
            _ => {}
        }
    }

    members
}

fn is_member(path: &str, members: &HashSet<String>) -> bool {
    if path.is_empty() {
        return members.contains(".");
    }
    if members.contains(path) {
        return true;
    }

    for pattern in members {
        if pattern.ends_with("/*") {
            let prefix = pattern.trim_end_matches("/*");
            if let Some(rest) = path.strip_prefix(prefix) {
                if rest.starts_with('/') && !rest[1..].contains('/') {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_name(content: &str, registry: Registry) -> Option<String> {
    match registry {
        Registry::Npm => npm::parse_name(content),
        Registry::Crates => cargo::parse_name(content),
        Registry::Pypi => python::parse_name(content),
        Registry::Go => go::parse_name(content),
        Registry::Maven => jvm::parse_name(content),
    }
}

fn parent_dir(path: &str) -> String {
    path.rfind('/').map(|i| path[..i].to_string()).unwrap_or_default()
}

fn dedupe(packages: &mut Vec<DetectedPackage>) {
    let mut seen: HashSet<(String, Registry)> = HashSet::new();
    packages.retain(|p| seen.insert((p.root_path.clone(), p.registry)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_npm_package() {
        let files = vec![
            ("package.json".to_string(), r#"{"name": "my-app"}"#.to_string()),
            ("src/index.ts".to_string(), "export {}".to_string()),
        ];

        let packages = analyze_repo(&files);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("my-app".to_string()));
        assert_eq!(packages[0].registry, Registry::Npm);
    }

    #[test]
    fn test_npm_workspace() {
        let files = vec![
            ("package.json".to_string(), r#"{"workspaces": ["packages/*"]}"#.to_string()),
            ("packages/core/package.json".to_string(), r#"{"name": "core"}"#.to_string()),
            ("packages/core/index.ts".to_string(), "export {}".to_string()),
            ("packages/utils/package.json".to_string(), r#"{"name": "utils"}"#.to_string()),
            ("packages/utils/index.ts".to_string(), "export {}".to_string()),
            ("tests/package.json".to_string(), r#"{"name": "tests"}"#.to_string()),
        ];

        let packages = analyze_repo(&files);
        assert_eq!(packages.len(), 2);
        assert!(packages.iter().any(|p| p.name == Some("core".to_string())));
        assert!(packages.iter().any(|p| p.name == Some("utils".to_string())));
    }

    #[test]
    fn test_skip_test_dirs() {
        let files = vec![
            ("package.json".to_string(), r#"{"name": "app"}"#.to_string()),
            ("src/index.ts".to_string(), "export {}".to_string()),
            ("tests/package.json".to_string(), r#"{"name": "test-pkg"}"#.to_string()),
        ];

        let packages = analyze_repo(&files);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, Some("app".to_string()));
    }

    #[test]
    fn test_mixed_manifest_prefers_one_with_source() {
        // Directory has both package.json and pyproject.toml
        // but only Python source files - should only detect pypi
        let files = vec![
            ("agents/chunking/package.json".to_string(), r#"{"name": "agent-chunking", "private": true}"#.to_string()),
            ("agents/chunking/pyproject.toml".to_string(), "[project]\nname = \"agent-chunking\"".to_string()),
            ("agents/chunking/src/main.py".to_string(), "print('hello')".to_string()),
        ];

        let packages = analyze_repo(&files);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].registry, Registry::Pypi);
        assert_eq!(packages[0].name, Some("agent-chunking".to_string()));
    }

    #[test]
    fn test_true_polyglot_keeps_both() {
        // Directory has both JS and Python source files - keep both
        let files = vec![
            ("lib/package.json".to_string(), r#"{"name": "my-lib"}"#.to_string()),
            ("lib/pyproject.toml".to_string(), "[project]\nname = \"my-lib\"".to_string()),
            ("lib/index.ts".to_string(), "export {}".to_string()),
            ("lib/main.py".to_string(), "print('hello')".to_string()),
        ];

        let packages = analyze_repo(&files);
        assert_eq!(packages.len(), 2);
    }
}
