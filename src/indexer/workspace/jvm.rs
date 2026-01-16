//! JVM workspace parsing (Maven, Gradle).

use quick_xml::Reader;
use quick_xml::events::Event;

/// Parse workspace members from pom.xml (Maven modules).
pub fn parse_workspace(content: &str) -> Vec<String> {
    let mut reader = Reader::from_str(content);
    let mut members = vec![];
    let mut in_modules = false;
    let mut in_module = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.local_name();
                if name.as_ref() == b"modules" {
                    in_modules = true;
                } else if in_modules && name.as_ref() == b"module" {
                    in_module = true;
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                if name.as_ref() == b"modules" {
                    in_modules = false;
                } else if name.as_ref() == b"module" {
                    in_module = false;
                }
            }
            Ok(Event::Text(e)) if in_module => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim();
                    if !text.is_empty() {
                        members.push(text.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    members
}

/// Parse artifact ID from pom.xml.
pub fn parse_name(content: &str) -> Option<String> {
    let mut reader = Reader::from_str(content);
    let mut in_artifact_id = false;
    let mut depth = 0;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                // Only get artifactId at depth 2 (direct child of project)
                if depth == 2 && e.local_name().as_ref() == b"artifactId" {
                    in_artifact_id = true;
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                in_artifact_id = false;
            }
            Ok(Event::Text(e)) if in_artifact_id => {
                if let Ok(text) = e.unescape() {
                    let text = text.trim();
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_maven_modules() {
        let content = r#"
<project>
    <modules>
        <module>core</module>
        <module>api</module>
    </modules>
</project>
"#;
        assert_eq!(parse_workspace(content), vec!["core", "api"]);
    }

    #[test]
    fn test_maven_artifact_id() {
        let content = r#"
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>1.0.0</version>
</project>
"#;
        assert_eq!(parse_name(content), Some("my-app".to_string()));
    }
}
