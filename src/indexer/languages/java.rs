use crate::types::{ChunkType, Visibility};
use tree_sitter::{Node, Parser};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for Java using tree-sitter.
///
/// Java visibility uses explicit modifiers:
/// - public = Public
/// - protected = Protected
/// - (default/package) = Internal
/// - private = Private
pub struct JavaParser {
    _marker: (),
}

impl JavaParser {
    pub fn new() -> Result<Self, IndexerError> {
        Ok(Self { _marker: () })
    }

    fn create_parser() -> Result<Parser, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::TreeSitter(e.to_string()))?;
        Ok(parser)
    }

    fn extract_chunks(
        &self,
        source: &str,
        file_path: &str,
    ) -> Result<Vec<CodeChunk>, IndexerError> {
        let mut parser = Self::create_parser()?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::ParseError("failed to parse Java source".into()))?;

        let mut chunks = Vec::new();
        self.visit_node(tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn visit_node(&self, node: Node, source: &str, file_path: &str, chunks: &mut Vec<CodeChunk>) {
        match node.kind() {
            "method_declaration" => {
                if let Some(chunk) = self.extract_method(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "class_declaration" => {
                if let Some(chunk) = self.extract_class(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "interface_declaration" => {
                if let Some(chunk) = self.extract_interface(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "enum_declaration" => {
                if let Some(chunk) = self.extract_enum(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, chunks);
        }
    }

    fn extract_method(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_javadoc(node, source);
        let visibility = self.detect_visibility(node, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Method)
            .visibility(visibility)
            .name(name)
            .signature(code.lines().next().unwrap_or("").to_string())
            .code(code)
            .documentation(doc.unwrap_or_default())
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_class(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_javadoc(node, source);
        let visibility = self.detect_visibility(node, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Class)
            .visibility(visibility)
            .name(name)
            .signature(code.lines().next().unwrap_or("").to_string())
            .code(code)
            .documentation(doc.unwrap_or_default())
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_interface(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_javadoc(node, source);
        let visibility = self.detect_visibility(node, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Interface)
            .visibility(visibility)
            .name(name)
            .signature(code.lines().next().unwrap_or("").to_string())
            .code(code)
            .documentation(doc.unwrap_or_default())
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_enum(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_javadoc(node, source);
        let visibility = self.detect_visibility(node, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Type)
            .visibility(visibility)
            .name(name)
            .signature(code.lines().next().unwrap_or("").to_string())
            .code(code)
            .documentation(doc.unwrap_or_default())
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_javadoc(&self, node: Node, source: &str) -> Option<String> {
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            if sibling.kind() == "block_comment" {
                let text = sibling.utf8_text(source.as_bytes()).ok()?;
                if text.starts_with("/**") {
                    return Some(self.clean_javadoc(text));
                }
            } else if sibling.kind() != "line_comment"
                && sibling.kind() != "marker_annotation"
                && sibling.kind() != "annotation"
            {
                break;
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn clean_javadoc(&self, comment: &str) -> String {
        comment
            .trim()
            .strip_prefix("/**")
            .unwrap_or(comment)
            .strip_suffix("*/")
            .unwrap_or(comment)
            .lines()
            .map(|line| line.trim().strip_prefix("*").unwrap_or(line).trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_child_text(&self, node: Node, field: &str, source: &str) -> Option<String> {
        let child = node.child_by_field_name(field)?;
        child
            .utf8_text(source.as_bytes())
            .ok()
            .map(|s| s.to_string())
    }

    /// Detect visibility from Java modifiers.
    fn detect_visibility(&self, node: Node, source: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                if text.contains("public") {
                    return Visibility::Public;
                } else if text.contains("protected") {
                    return Visibility::Protected;
                } else if text.contains("private") {
                    return Visibility::Private;
                }
            }
        }
        // Default (package-private) in Java
        Visibility::Internal
    }
}

impl LanguageParser for JavaParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        self.extract_chunks(source, file_path)
    }

    fn language(&self) -> Language {
        Language::Java
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_public_method() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Calculator {
    /**
     * Adds two numbers.
     * @param a first number
     * @param b second number
     * @return the sum
     */
    public int add(int a, int b) {
        return a + b;
    }
}
"#;
        let chunks = parser.parse(source, "Calculator.java").unwrap();

        let method = chunks.iter().find(|c| c.name == "add").unwrap();
        assert_eq!(method.visibility, Visibility::Public);
        assert!(
            method
                .documentation
                .as_ref()
                .unwrap()
                .contains("Adds two numbers")
        );
    }

    #[test]
    fn test_visibility_private() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Foo {
    private void helper() {}
}
"#;
        let chunks = parser.parse(source, "Foo.java").unwrap();

        let method = chunks.iter().find(|c| c.name == "helper").unwrap();
        assert_eq!(method.visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_protected() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
public class Base {
    protected void onInit() {}
}
"#;
        let chunks = parser.parse(source, "Base.java").unwrap();

        let method = chunks.iter().find(|c| c.name == "onInit").unwrap();
        assert_eq!(method.visibility, Visibility::Protected);
    }

    #[test]
    fn test_visibility_package_private() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
class Internal {
    void process() {}
}
"#;
        let chunks = parser.parse(source, "Internal.java").unwrap();

        let class = chunks.iter().find(|c| c.name == "Internal").unwrap();
        assert_eq!(class.visibility, Visibility::Internal);
    }

    #[test]
    fn test_parse_interface() {
        let parser = JavaParser::new().unwrap();
        let source = r#"
/**
 * Service interface.
 */
public interface Service {
    void execute();
}
"#;
        let chunks = parser.parse(source, "Service.java").unwrap();

        let iface = chunks.iter().find(|c| c.name == "Service").unwrap();
        assert_eq!(iface.chunk_type, ChunkType::Interface);
        assert_eq!(iface.visibility, Visibility::Public);
    }
}
