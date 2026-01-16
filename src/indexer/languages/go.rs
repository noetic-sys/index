use tree_sitter::{Node, Parser};
use crate::types::{ChunkType, Visibility};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for Go using tree-sitter.
///
/// Go visibility is based on name casing:
/// - Capitalized = exported (Public)
/// - lowercase = unexported (Internal)
pub struct GoParser {
    _marker: (),
}

impl GoParser {
    pub fn new() -> Result<Self, IndexerError> {
        Ok(Self { _marker: () })
    }

    fn create_parser() -> Result<Parser, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_go::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::TreeSitter(e.to_string()))?;
        Ok(parser)
    }

    fn extract_chunks(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        let mut parser = Self::create_parser()?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::ParseError("failed to parse Go source".into()))?;

        let mut chunks = Vec::new();
        self.visit_node(tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        match node.kind() {
            "function_declaration" => {
                if let Some(chunk) = self.extract_function(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "method_declaration" => {
                if let Some(chunk) = self.extract_method(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "type_declaration" => {
                self.extract_types(node, source, file_path, &mut *chunks);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, chunks);
        }
    }

    fn extract_function(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
        let visibility = self.detect_visibility(&name);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
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

    fn extract_method(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
        let visibility = self.detect_visibility(&name);

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

    fn extract_types(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                if let Some(chunk) = self.extract_type_spec(child, source, file_path) {
                    chunks.push(chunk);
                }
            }
        }
    }

    fn extract_type_spec(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node.parent()?, source);
        let visibility = self.detect_visibility(&name);

        // Determine if struct or interface
        let mut cursor = node.walk();
        let chunk_type = node.children(&mut cursor)
            .find(|c| c.kind() == "struct_type" || c.kind() == "interface_type")
            .map(|c| if c.kind() == "interface_type" { ChunkType::Interface } else { ChunkType::Type })
            .unwrap_or(ChunkType::Type);

        ChunkBuilder::new()
            .chunk_type(chunk_type)
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

    fn extract_doc_comment(&self, node: Node, source: &str) -> Option<String> {
        let mut comments = Vec::new();
        let mut prev = node.prev_sibling();

        while let Some(sibling) = prev {
            if sibling.kind() == "comment" {
                let text = sibling.utf8_text(source.as_bytes()).ok()?;
                comments.push(text.trim_start_matches("//").trim().to_string());
            } else {
                break;
            }
            prev = sibling.prev_sibling();
        }

        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }

    fn get_child_text(&self, node: Node, field: &str, source: &str) -> Option<String> {
        let child = node.child_by_field_name(field)?;
        child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string())
    }

    /// Go visibility: Capitalized = exported, lowercase = unexported
    fn detect_visibility(&self, name: &str) -> Visibility {
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            Visibility::Public
        } else {
            Visibility::Internal
        }
    }
}

impl LanguageParser for GoParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        self.extract_chunks(source, file_path)
    }

    fn language(&self) -> Language {
        Language::Go
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exported_function() {
        let parser = GoParser::new().unwrap();
        let source = r#"
// Add adds two numbers.
func Add(a, b int) int {
    return a + b
}
"#;
        let chunks = parser.parse(source, "math.go").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "Add");
        assert_eq!(chunks[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_parse_unexported_function() {
        let parser = GoParser::new().unwrap();
        let source = r#"
func helper() {}
"#;
        let chunks = parser.parse(source, "utils.go").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "helper");
        assert_eq!(chunks[0].visibility, Visibility::Internal);
    }

    #[test]
    fn test_visibility_detection() {
        let parser = GoParser::new().unwrap();

        assert_eq!(parser.detect_visibility("Exported"), Visibility::Public);
        assert_eq!(parser.detect_visibility("unexported"), Visibility::Internal);
    }
}
