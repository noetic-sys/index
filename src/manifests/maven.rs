//! Maven manifest parsing (pom.xml).
//!
//! Only indexes DIRECT dependencies, not transitive.
//! Maven coordinates are groupId:artifactId:version.

use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;

use super::Dependency;

/// Parse Maven dependencies from a directory.
/// Returns DIRECT dependencies from pom.xml.
pub fn parse_maven_deps(dir: &Path) -> Result<Vec<Dependency>> {
    let pom_path = dir.join("pom.xml");
    if !pom_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&pom_path).context("Failed to read pom.xml")?;

    // Parse properties for version variable resolution
    let properties = parse_properties(&content);

    // Parse dependencies
    let deps = parse_dependencies(&content, &properties);

    Ok(deps)
}

/// Parse <properties> section for variable resolution.
fn parse_properties(content: &str) -> std::collections::HashMap<String, String> {
    let mut props = std::collections::HashMap::new();
    let mut reader = Reader::from_str(content);
    let mut in_properties = false;
    let mut current_prop: Option<String> = None;
    let mut depth = 0;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if depth == 2 && name == "properties" {
                    in_properties = true;
                } else if in_properties && depth == 3 {
                    current_prop = Some(name);
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "properties" {
                    in_properties = false;
                }
                current_prop = None;
                depth -= 1;
            }
            Ok(Event::Text(e)) if current_prop.is_some() => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim();
                    if !text.is_empty()
                        && let Some(ref prop) = current_prop {
                            props.insert(prop.clone(), text.to_string());
                        }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    props
}

/// Parse <dependencies> section.
fn parse_dependencies(
    content: &str,
    properties: &std::collections::HashMap<String, String>,
) -> Vec<Dependency> {
    let mut deps = Vec::new();
    let mut reader = Reader::from_str(content);
    let mut in_dependencies = false;
    let mut in_dependency = false;
    let mut in_dependency_mgmt = false;
    let mut current_field: Option<String> = None;
    let mut group_id: Option<String> = None;
    let mut artifact_id: Option<String> = None;
    let mut version: Option<String> = None;
    let mut scope: Option<String> = None;
    let mut depth = 0;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();

                // Skip dependencyManagement section
                if name == "dependencyManagement" {
                    in_dependency_mgmt = true;
                }

                if !in_dependency_mgmt && depth == 2 && name == "dependencies" {
                    in_dependencies = true;
                } else if in_dependencies && name == "dependency" {
                    in_dependency = true;
                    group_id = None;
                    artifact_id = None;
                    version = None;
                    scope = None;
                } else if in_dependency {
                    current_field = Some(name);
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();

                if name == "dependencyManagement" {
                    in_dependency_mgmt = false;
                }

                if name == "dependencies" {
                    in_dependencies = false;
                } else if name == "dependency" && in_dependency {
                    // Only include compile/runtime scope (or unspecified = compile)
                    let include = match scope.as_deref() {
                        None | Some("compile") | Some("runtime") => true,
                        Some("test") | Some("provided") | Some("system") => false,
                        _ => true,
                    };

                    if include
                        && let (Some(g), Some(a), Some(v)) = (&group_id, &artifact_id, &version) {
                            // Resolve property references like ${guava.version}
                            let resolved_version = resolve_property(v, properties);

                            deps.push(Dependency {
                                registry: "maven".to_string(),
                                name: format!("{}:{}", g, a),
                                version: resolved_version,
                            });
                        }
                    in_dependency = false;
                }
                current_field = None;
                depth -= 1;
            }
            Ok(Event::Text(e)) if current_field.is_some() => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        match current_field.as_deref() {
                            Some("groupId") => group_id = Some(text),
                            Some("artifactId") => artifact_id = Some(text),
                            Some("version") => version = Some(text),
                            Some("scope") => scope = Some(text),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    deps
}

/// Resolve ${property} references in version strings.
fn resolve_property(
    version: &str,
    properties: &std::collections::HashMap<String, String>,
) -> String {
    if version.starts_with("${") && version.ends_with("}") {
        let prop_name = &version[2..version.len() - 1];
        if let Some(resolved) = properties.get(prop_name) {
            return resolved.clone();
        }
    }
    version.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_pom() {
        let content = r#"
<project>
    <dependencies>
        <dependency>
            <groupId>com.google.guava</groupId>
            <artifactId>guava</artifactId>
            <version>33.0.0-jre</version>
        </dependency>
    </dependencies>
</project>
"#;
        let deps = parse_dependencies(content, &std::collections::HashMap::new());
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "com.google.guava:guava");
        assert_eq!(deps[0].version, "33.0.0-jre");
    }

    #[test]
    fn test_skip_test_scope() {
        let content = r#"
<project>
    <dependencies>
        <dependency>
            <groupId>junit</groupId>
            <artifactId>junit</artifactId>
            <version>4.13.2</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>com.google.guava</groupId>
            <artifactId>guava</artifactId>
            <version>33.0.0-jre</version>
        </dependency>
    </dependencies>
</project>
"#;
        let deps = parse_dependencies(content, &std::collections::HashMap::new());
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "com.google.guava:guava");
    }

    #[test]
    fn test_property_resolution() {
        let content = r#"
<project>
    <properties>
        <guava.version>33.0.0-jre</guava.version>
    </properties>
    <dependencies>
        <dependency>
            <groupId>com.google.guava</groupId>
            <artifactId>guava</artifactId>
            <version>${guava.version}</version>
        </dependency>
    </dependencies>
</project>
"#;
        let props = parse_properties(content);
        let deps = parse_dependencies(content, &props);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].version, "33.0.0-jre");
    }

    #[test]
    fn test_skip_dependency_management() {
        let content = r#"
<project>
    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>org.springframework</groupId>
                <artifactId>spring-core</artifactId>
                <version>6.0.0</version>
            </dependency>
        </dependencies>
    </dependencyManagement>
    <dependencies>
        <dependency>
            <groupId>com.google.guava</groupId>
            <artifactId>guava</artifactId>
            <version>33.0.0-jre</version>
        </dependency>
    </dependencies>
</project>
"#;
        let deps = parse_dependencies(content, &std::collections::HashMap::new());
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "com.google.guava:guava");
    }
}
