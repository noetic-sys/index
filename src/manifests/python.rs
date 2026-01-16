//! Python manifest parsing (pyproject.toml, requirements.txt).

use std::path::Path;

use anyhow::{Context, Result};

use super::Dependency;

/// Parse Python dependencies from a directory.
pub fn parse_python_deps(dir: &Path) -> Result<Vec<Dependency>> {
    let mut deps = Vec::new();

    // Try pyproject.toml
    let pyproject_path = dir.join("pyproject.toml");
    if pyproject_path.exists() {
        deps.extend(parse_pyproject(&pyproject_path)?);
    }

    // Try requirements.txt (may have additional deps)
    let requirements_path = dir.join("requirements.txt");
    if requirements_path.exists() {
        deps.extend(parse_requirements(&requirements_path)?);
    }

    // Dedupe by name (prefer pyproject versions)
    let mut seen = std::collections::HashSet::new();
    deps.retain(|d| seen.insert(d.name.clone()));

    Ok(deps)
}

fn parse_pyproject(path: &Path) -> Result<Vec<Dependency>> {
    let content = std::fs::read_to_string(path).context("Failed to read pyproject.toml")?;
    let toml: toml::Value = content.parse().context("Failed to parse pyproject.toml")?;

    let mut deps = Vec::new();

    // PEP 621: [project.dependencies]
    if let Some(project_deps) = toml
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in project_deps {
            if let Some(s) = dep.as_str() {
                if let Some(d) = parse_pep508(s) {
                    deps.push(d);
                }
            }
        }
    }

    // Poetry: [tool.poetry.dependencies]
    if let Some(poetry_deps) = toml
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (name, value) in poetry_deps {
            if name == "python" {
                continue;
            }
            let version = match value {
                toml::Value::String(v) => Some(v.clone()),
                toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()).map(String::from),
                _ => None,
            };
            if let Some(v) = version.and_then(|v| clean_version(&v)) {
                deps.push(Dependency {
                    registry: "pypi".to_string(),
                    name: name.clone(),
                    version: v,
                });
            }
        }
    }

    Ok(deps)
}

fn parse_requirements(path: &Path) -> Result<Vec<Dependency>> {
    let content = std::fs::read_to_string(path).context("Failed to read requirements.txt")?;

    let deps = content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#') && !l.trim().starts_with('-'))
        .filter_map(parse_pep508)
        .collect();

    Ok(deps)
}

/// Parse PEP 508 dependency spec: "package==1.2.3" or "package>=1.2.3"
fn parse_pep508(spec: &str) -> Option<Dependency> {
    let spec = spec.split(';').next()?.trim(); // Remove env markers

    let (name, version) = if let Some(pos) = spec.find("==") {
        (spec[..pos].trim(), spec[pos + 2..].trim())
    } else if let Some(pos) = spec.find(">=") {
        (spec[..pos].trim(), spec[pos + 2..].trim())
    } else if let Some(pos) = spec.find("~=") {
        (spec[..pos].trim(), spec[pos + 2..].trim())
    } else {
        return None;
    };

    // Clean extras: "package[extra]" -> "package"
    let name = name.split('[').next()?.trim();

    Some(Dependency {
        registry: "pypi".to_string(),
        name: name.to_string(),
        version: clean_version(version)?,
    })
}

fn clean_version(version: &str) -> Option<String> {
    let v = version.trim().trim_start_matches('^').trim_start_matches('~').trim_start_matches('=');

    if v.contains(',') || v.contains(' ') || v.contains('*') {
        return None;
    }

    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pep508() {
        let dep = parse_pep508("requests==2.28.0").unwrap();
        assert_eq!(dep.name, "requests");
        assert_eq!(dep.version, "2.28.0");

        let dep = parse_pep508("numpy>=1.20.0").unwrap();
        assert_eq!(dep.name, "numpy");
        assert_eq!(dep.version, "1.20.0");

        let dep = parse_pep508("torch[cuda]>=2.0.0 ; sys_platform == 'linux'").unwrap();
        assert_eq!(dep.name, "torch");
        assert_eq!(dep.version, "2.0.0");
    }
}
