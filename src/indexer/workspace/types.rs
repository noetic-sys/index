//! Workspace detection types.

use crate::types::Registry;

/// A package detected in a repository.
#[derive(Debug, Clone)]
pub struct DetectedPackage {
    pub registry: Registry,
    pub name: Option<String>,
    pub root_path: String,
}
