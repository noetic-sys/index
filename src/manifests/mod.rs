//! Manifest file parsing for dependency extraction.

mod cargo;
mod go;
mod maven;
mod npm;
mod python;

pub use cargo::parse_cargo_deps;
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
