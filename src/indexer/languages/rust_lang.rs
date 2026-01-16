use crate::types::{ChunkType, Visibility};
use tree_sitter::{Node, Parser};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for Rust using tree-sitter.
///
/// Extracts:
/// - Functions (fn)
/// - Methods (impl blocks)
/// - Structs
/// - Enums
/// - Traits
/// - Doc comments (///, //!, /** */)
pub struct RustParser {
    _marker: (),
}

impl RustParser {
    pub fn new() -> Result<Self, IndexerError> {
        Ok(Self { _marker: () })
    }

    fn create_parser() -> Result<Parser, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE;
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
            .ok_or_else(|| IndexerError::ParseError("failed to parse Rust source".into()))?;

        let mut chunks = Vec::new();
        self.visit_node(tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn visit_node(&self, node: Node, source: &str, file_path: &str, chunks: &mut Vec<CodeChunk>) {
        match node.kind() {
            "function_item" => {
                if let Some(chunk) = self.extract_function(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "struct_item" => {
                if let Some(chunk) = self.extract_struct(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "enum_item" => {
                if let Some(chunk) = self.extract_enum(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "trait_item" => {
                if let Some(chunk) = self.extract_trait(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "impl_item" => {
                // Extract methods from impl block
                self.extract_impl_methods(node, source, file_path, chunks);
            }
            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, chunks);
        }
    }

    fn extract_function(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
        let visibility = self.detect_visibility(node, source);
        let signature = self.extract_signature(node, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .visibility(visibility)
            .name(name)
            .signature(signature.unwrap_or_else(|| code.lines().next().unwrap_or("").to_string()))
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

    fn extract_struct(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
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

    fn extract_enum(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
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

    fn extract_trait(&self, node: Node, source: &str, file_path: &str) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = self.extract_doc_comment(node, source);
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

    fn extract_impl_methods(
        &self,
        impl_node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let mut cursor = impl_node.walk();
        for child in impl_node.children(&mut cursor) {
            if child.kind() == "declaration_list" {
                let mut inner_cursor = child.walk();
                for item in child.children(&mut inner_cursor) {
                    if item.kind() == "function_item"
                        && let Some(mut chunk) = self.extract_function(item, source, file_path) {
                            chunk.chunk_type = ChunkType::Method;
                            chunks.push(chunk);
                        }
                }
            }
        }
    }

    fn extract_doc_comment(&self, node: Node, source: &str) -> Option<String> {
        // Look for doc comments (///, //!, /** */) before this node
        let mut comments = Vec::new();
        let mut prev = node.prev_sibling();

        while let Some(sibling) = prev {
            let kind = sibling.kind();
            if kind == "line_comment" || kind == "block_comment" {
                let text = sibling.utf8_text(source.as_bytes()).ok()?;
                if text.starts_with("///") || text.starts_with("//!") || text.starts_with("/**") {
                    comments.push(self.clean_doc_comment(text));
                }
            } else if sibling.end_position().row + 1 < node.start_position().row {
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

    fn clean_doc_comment(&self, comment: &str) -> String {
        comment
            .trim()
            .strip_prefix("///")
            .or_else(|| comment.strip_prefix("//!"))
            .or_else(|| comment.strip_prefix("/**"))
            .unwrap_or(comment)
            .strip_suffix("*/")
            .unwrap_or(comment)
            .trim()
            .to_string()
    }

    fn extract_signature(&self, node: Node, source: &str) -> Option<String> {
        // Get everything up to the block
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block" {
                let end = child.start_byte();
                return source
                    .get(node.start_byte()..end)
                    .map(|s| s.trim().to_string());
            }
        }
        None
    }

    fn get_child_text(&self, node: Node, field: &str, source: &str) -> Option<String> {
        let child = node.child_by_field_name(field)?;
        child
            .utf8_text(source.as_bytes())
            .ok()
            .map(|s| s.to_string())
    }

    /// Detect visibility from Rust modifiers.
    ///
    /// Rust visibility rules:
    /// - `pub` = Public
    /// - `pub(crate)` = Internal
    /// - `pub(super)` = Protected
    /// - `pub(in path)` = Protected
    /// - No modifier = Private
    fn detect_visibility(&self, node: Node, source: &str) -> Visibility {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                if text.contains("pub(crate)") {
                    return Visibility::Internal;
                } else if text.contains("pub(super)") || text.contains("pub(in") {
                    return Visibility::Protected;
                } else if text.starts_with("pub") {
                    return Visibility::Public;
                }
            }
        }
        Visibility::Private
    }
}

impl LanguageParser for RustParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        self.extract_chunks(source, file_path)
    }

    fn language(&self) -> Language {
        Language::Rust
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let parser = RustParser::new().unwrap();
        let source = r#"
/// Adds two numbers together.
///
/// # Arguments
/// * `a` - First number
/// * `b` - Second number
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let chunks = parser.parse(source, "math.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "add");
        assert_eq!(chunks[0].chunk_type, ChunkType::Function);
        assert!(
            chunks[0]
                .documentation
                .as_ref()
                .unwrap()
                .contains("Adds two numbers")
        );
    }

    #[test]
    fn test_parse_struct() {
        let parser = RustParser::new().unwrap();
        let source = r#"
/// A point in 2D space.
pub struct Point {
    pub x: f64,
    pub y: f64,
}
"#;
        let chunks = parser.parse(source, "geometry.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "Point");
        assert_eq!(chunks[0].chunk_type, ChunkType::Type);
    }

    #[test]
    fn test_parse_trait() {
        let parser = RustParser::new().unwrap();
        let source = r#"
/// A drawable object.
pub trait Drawable {
    fn draw(&self);
}
"#;
        let chunks = parser.parse(source, "traits.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "Drawable");
        assert_eq!(chunks[0].chunk_type, ChunkType::Interface);
    }

    #[test]
    fn test_visibility_pub() {
        let parser = RustParser::new().unwrap();
        let source = r#"
pub fn public_fn() {}
"#;
        let chunks = parser.parse(source, "test.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_visibility_private() {
        let parser = RustParser::new().unwrap();
        let source = r#"
fn private_fn() {}
"#;
        let chunks = parser.parse(source, "test.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_pub_crate() {
        let parser = RustParser::new().unwrap();
        let source = r#"
pub(crate) fn internal_fn() {}
"#;
        let chunks = parser.parse(source, "test.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Internal);
    }

    #[test]
    fn test_visibility_pub_super() {
        let parser = RustParser::new().unwrap();
        let source = r#"
pub(super) fn protected_fn() {}
"#;
        let chunks = parser.parse(source, "test.rs").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Protected);
    }

    #[test]
    fn test_impl_methods() {
        let parser = RustParser::new().unwrap();
        let source = r#"
pub struct Foo;

impl Foo {
    pub fn new() -> Self {
        Foo
    }

    fn private_helper(&self) {}
}
"#;
        let chunks = parser.parse(source, "test.rs").unwrap();

        let new_method = chunks.iter().find(|c| c.name == "new").unwrap();
        assert_eq!(new_method.chunk_type, ChunkType::Method);
        assert_eq!(new_method.visibility, Visibility::Public);

        let helper = chunks.iter().find(|c| c.name == "private_helper").unwrap();
        assert_eq!(helper.visibility, Visibility::Private);
    }
}
