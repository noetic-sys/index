//! Skip patterns for test/example/fixture directories.

const SKIP_DIRS: &[&str] = &[
    // Tests
    "test", "tests", "__tests__", "spec", "specs",
    // Examples
    "example", "examples", "demo", "demos", "sample", "samples",
    // Fixtures
    "fixture", "fixtures", "__fixtures__", "testdata", "mocks", "__mocks__",
    // Benchmarks
    "bench", "benches", "benchmark", "benchmarks",
    // E2E/Integration
    "e2e", "integration", "integration-tests",
    // Dependencies/build artifacts (avoid traversing these trees)
    "node_modules", "vendor", "target", "dist", "build", ".build",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache",
    ".git", ".svn", ".hg",
    ".venv", "venv", ".env", "env",
    "coverage", ".coverage", ".nyc_output",
];

/// Check if a path contains a test/example/fixture directory.
pub fn should_skip_dir(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }

    let path_lower = path.to_lowercase();

    for dir in SKIP_DIRS {
        if path_lower == *dir
            || path_lower.starts_with(&format!("{}/", dir))
            || path_lower.contains(&format!("/{}/", dir))
            || path_lower.ends_with(&format!("/{}", dir))
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_test_dirs() {
        assert!(should_skip_dir("tests"));
        assert!(should_skip_dir("packages/foo/tests"));
        assert!(should_skip_dir("__tests__"));
    }

    #[test]
    fn test_skip_example_dirs() {
        assert!(should_skip_dir("examples"));
        assert!(should_skip_dir("demo"));
    }

    #[test]
    fn test_allow_valid_dirs() {
        assert!(!should_skip_dir(""));
        assert!(!should_skip_dir("src"));
        assert!(!should_skip_dir("packages/core"));
    }

    #[test]
    fn test_skip_node_modules() {
        assert!(should_skip_dir("node_modules"));
        assert!(should_skip_dir("packages/foo/node_modules"));
        assert!(should_skip_dir("node_modules/@types/lodash"));
    }

    #[test]
    fn test_allow_deep_paths() {
        // Deep paths are fine as long as they're not in skip dirs
        assert!(!should_skip_dir("a/b/c/d"));
        assert!(!should_skip_dir("agents/qa/src/runner"));
        assert!(!should_skip_dir("libs/documents/src/parsers/pdf"));
    }
}
