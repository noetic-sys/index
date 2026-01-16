//! Workspace detection for monorepos.

mod analyze;
mod cargo;
mod go;
mod jvm;
mod npm;
mod patterns;
mod python;
mod types;

pub use analyze::analyze_repo;
pub use patterns::should_skip_dir;
pub use types::DetectedPackage;
