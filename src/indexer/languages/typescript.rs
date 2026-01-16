use crate::types::{ChunkType, Visibility};
use tree_sitter::{Node, Parser, Tree};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for TypeScript and JavaScript using tree-sitter.
///
/// Extracts:
/// - Functions (function declarations, arrow functions, methods)
/// - Classes
/// - Interfaces/Types (TypeScript)
/// - JSDoc comments associated with declarations
pub struct TypeScriptParser {
    parser: Parser,
    is_typescript: bool,
}

impl TypeScriptParser {
    pub fn new() -> Result<Self, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::TreeSitter(e.to_string()))?;

        Ok(Self {
            parser,
            is_typescript: true,
        })
    }

    pub fn new_javascript() -> Result<Self, IndexerError> {
        let mut parser = Parser::new();
        // tree-sitter-typescript includes TSX which handles JS as well
        let language = tree_sitter_typescript::LANGUAGE_TSX;
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::TreeSitter(e.to_string()))?;

        Ok(Self {
            parser,
            is_typescript: false,
        })
    }

    fn parse_tree(&mut self, source: &str) -> Result<Tree, IndexerError> {
        self.parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::ParseError("failed to parse source".into()))
    }

    fn extract_chunks(&self, tree: &Tree, source: &str, file_path: &str) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();
        let root = tree.root_node();

        self.visit_node(root, source, file_path, &mut chunks, None);

        chunks
    }

    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
        preceding_comment: Option<&str>,
    ) {
        let kind = node.kind();

        // Check if this node produces a chunk
        let chunk = match kind {
            "function_declaration" => {
                self.extract_function(node, source, file_path, preceding_comment)
            }
            "method_definition" => self.extract_method(node, source, file_path, preceding_comment),
            "class_declaration" => self.extract_class(node, source, file_path, preceding_comment),
            "interface_declaration" if self.is_typescript => {
                self.extract_interface(node, source, file_path, preceding_comment)
            }
            "type_alias_declaration" if self.is_typescript => {
                self.extract_type_alias(node, source, file_path, preceding_comment)
            }
            "lexical_declaration" => {
                // Could be arrow function: const foo = () => {}
                self.extract_arrow_function(node, source, file_path, preceding_comment)
            }
            _ => None,
        };

        if let Some(c) = chunk {
            chunks.push(c);
        }

        // Find preceding comment for next sibling
        let comment = if kind == "comment" {
            Some(node.utf8_text(source.as_bytes()).unwrap_or(""))
        } else {
            None
        };

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, chunks, comment);
        }
    }

    fn extract_function(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let signature = self.extract_function_signature(node, source);
        let doc = preceding_comment
            .map(|c| self.clean_jsdoc(c))
            .or_else(|| self.find_leading_comment(node, source));
        let visibility = self.detect_visibility(node, &name, source);

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

    fn extract_method(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let signature = self.extract_function_signature(node, source);
        let doc = preceding_comment
            .map(|c| self.clean_jsdoc(c))
            .or_else(|| self.find_leading_comment(node, source));
        let visibility = self.detect_visibility(node, &name, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Method)
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

    fn extract_class(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = preceding_comment
            .map(|c| self.clean_jsdoc(c))
            .or_else(|| self.find_leading_comment(node, source));
        let visibility = self.detect_visibility(node, &name, source);

        // Extract class signature (first line usually has extends/implements)
        let signature = code.lines().next().map(|s| s.to_string());

        ChunkBuilder::new()
            .chunk_type(ChunkType::Class)
            .visibility(visibility)
            .name(name)
            .signature(signature.unwrap_or_default())
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

    fn extract_interface(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = preceding_comment
            .map(|c| self.clean_jsdoc(c))
            .or_else(|| self.find_leading_comment(node, source));
        let visibility = self.detect_visibility(node, &name, source);

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

    fn extract_type_alias(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        let name = self.get_child_text(node, "name", source)?;
        let code = node.utf8_text(source.as_bytes()).ok()?;
        let doc = preceding_comment
            .map(|c| self.clean_jsdoc(c))
            .or_else(|| self.find_leading_comment(node, source));
        let visibility = self.detect_visibility(node, &name, source);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Type)
            .visibility(visibility)
            .name(name)
            .signature(code.to_string())
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

    fn extract_arrow_function(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        preceding_comment: Option<&str>,
    ) -> Option<CodeChunk> {
        // lexical_declaration -> variable_declarator -> arrow_function
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let name_node = child.child_by_field_name("name")?;
                let name = name_node.utf8_text(source.as_bytes()).ok()?;

                let value_node = child.child_by_field_name("value")?;
                if value_node.kind() == "arrow_function" {
                    let code = node.utf8_text(source.as_bytes()).ok()?;
                    let doc = preceding_comment
                        .map(|c| self.clean_jsdoc(c))
                        .or_else(|| self.find_leading_comment(node, source));
                    let visibility = self.detect_visibility(node, name, source);

                    return ChunkBuilder::new()
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
                        .build();
                }
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

    fn extract_function_signature(&self, node: Node, source: &str) -> Option<String> {
        // Get everything up to and including the parameter list
        let _name = node.child_by_field_name("name")?;
        let params = node.child_by_field_name("parameters")?;

        let start = node.start_byte();
        let end = params.end_byte();

        // Include return type if present
        let end = if let Some(return_type) = node.child_by_field_name("return_type") {
            return_type.end_byte()
        } else {
            end
        };

        source.get(start..end).map(|s| s.to_string())
    }

    fn find_leading_comment(&self, node: Node, source: &str) -> Option<String> {
        // Look for comment node immediately before this node
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            match sibling.kind() {
                "comment" => {
                    let text = sibling.utf8_text(source.as_bytes()).ok()?;
                    return Some(self.clean_jsdoc(text));
                }
                _ if sibling.end_position().row + 1 < node.start_position().row => {
                    // Gap too large, not associated
                    return None;
                }
                _ => {}
            }
            prev = sibling.prev_sibling();
        }
        None
    }

    fn clean_jsdoc(&self, comment: &str) -> String {
        // Remove /** */ and * prefixes
        let trimmed = comment.trim();
        let without_start = trimmed
            .strip_prefix("/**")
            .or_else(|| trimmed.strip_prefix("/*"))
            .unwrap_or(trimmed);
        let without_end = without_start.strip_suffix("*/").unwrap_or(without_start);

        without_end
            .lines()
            .map(|line| line.trim().strip_prefix("*").unwrap_or(line).trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Detect visibility for a node.
    ///
    /// TypeScript/JavaScript visibility rules:
    /// - `export` keyword = Public
    /// - `#privateName` = Private (ES private fields)
    /// - `_prefixedName` = Private (convention)
    /// - No export = Internal (module-private)
    fn detect_visibility(&self, node: Node, name: &str, source: &str) -> Visibility {
        // Check for # prefix (ES private)
        if name.starts_with('#') {
            return Visibility::Private;
        }

        // Check for _ prefix (convention)
        if name.starts_with('_') && !name.starts_with("__") {
            return Visibility::Private;
        }

        // Check if parent is export_statement
        if let Some(parent) = node.parent() {
            if parent.kind() == "export_statement" {
                return Visibility::Public;
            }
            // Check grandparent for `export default`
            if let Some(grandparent) = parent.parent()
                && grandparent.kind() == "export_statement"
            {
                return Visibility::Public;
            }
        }

        // Check if there's an export keyword in the node text
        let node_text = node.utf8_text(source.as_bytes()).unwrap_or("");
        if node_text.trim().starts_with("export ") {
            return Visibility::Public;
        }

        // Default: Internal (module-private, not exported)
        Visibility::Internal
    }
}

impl LanguageParser for TypeScriptParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        // Need to clone parser for mutability - tree-sitter requires &mut
        let mut parser = Parser::new();
        let language = if self.is_typescript {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT
        } else {
            tree_sitter_typescript::LANGUAGE_TSX // TSX handles JS as well
        };
        parser
            .set_language(&language.into())
            .map_err(|e| IndexerError::TreeSitter(e.to_string()))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IndexerError::ParseError("failed to parse source".into()))?;

        Ok(self.extract_chunks(&tree, source, file_path))
    }

    fn language(&self) -> Language {
        if self.is_typescript {
            Language::TypeScript
        } else {
            Language::JavaScript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
/**
 * Adds two numbers together.
 * @param a First number
 * @param b Second number
 * @returns The sum
 */
function add(a: number, b: number): number {
    return a + b;
}
"#;
        let chunks = parser.parse(source, "math.ts").unwrap();

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
    fn test_parse_class() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
/**
 * A simple calculator class.
 */
class Calculator {
    /**
     * Adds two numbers.
     */
    add(a: number, b: number): number {
        return a + b;
    }
}
"#;
        let chunks = parser.parse(source, "calc.ts").unwrap();

        // Should get class and method
        assert!(chunks.len() >= 1);
        let class_chunk = chunks.iter().find(|c| c.chunk_type == ChunkType::Class);
        assert!(class_chunk.is_some());
        assert_eq!(class_chunk.unwrap().name, "Calculator");
    }

    #[test]
    fn test_parse_arrow_function() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
/** Doubles a number */
const double = (x: number): number => x * 2;
"#;
        let chunks = parser.parse(source, "utils.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "double");
        assert_eq!(chunks[0].chunk_type, ChunkType::Function);
    }

    #[test]
    fn test_parse_interface() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
/**
 * User object shape.
 */
interface User {
    id: string;
    name: string;
    email: string;
}
"#;
        let chunks = parser.parse(source, "types.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "User");
        assert_eq!(chunks[0].chunk_type, ChunkType::Interface);
    }

    #[test]
    fn test_parse_type_alias() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
/** User ID type */
type UserId = string;
"#;
        let chunks = parser.parse(source, "types.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "UserId");
        assert_eq!(chunks[0].chunk_type, ChunkType::Type);
    }

    #[test]
    fn test_clean_jsdoc() {
        let parser = TypeScriptParser::new().unwrap();

        let input = "/**\n * This is a doc.\n * @param x Input\n */";
        let cleaned = parser.clean_jsdoc(input);

        assert!(cleaned.contains("This is a doc"));
        assert!(cleaned.contains("@param x Input"));
        assert!(!cleaned.contains("/**"));
        assert!(!cleaned.contains("*/"));
    }

    #[test]
    fn test_javascript_mode() {
        let parser = TypeScriptParser::new_javascript().unwrap();
        let source = r#"
/**
 * Greets a person.
 */
function greet(name) {
    return "Hello, " + name;
}
"#;
        let chunks = parser.parse(source, "greet.js").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "greet");
        assert_eq!(parser.language(), Language::JavaScript);
    }

    #[test]
    fn test_visibility_exported_function() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
export function publicFunc(): void {}
"#;
        let chunks = parser.parse(source, "test.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_visibility_non_exported_function() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
function internalFunc(): void {}
"#;
        let chunks = parser.parse(source, "test.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Internal);
    }

    #[test]
    fn test_visibility_underscore_prefix() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
function _privateHelper(): void {}
"#;
        let chunks = parser.parse(source, "test.ts").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_exported_class() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
export class PublicClass {
    publicMethod(): void {}
    _privateMethod(): void {}
}
"#;
        let chunks = parser.parse(source, "test.ts").unwrap();

        let class_chunk = chunks.iter().find(|c| c.name == "PublicClass").unwrap();
        assert_eq!(class_chunk.visibility, Visibility::Public);

        let private_method = chunks.iter().find(|c| c.name == "_privateMethod").unwrap();
        assert_eq!(private_method.visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_exported_interface() {
        let parser = TypeScriptParser::new().unwrap();
        let source = r#"
export interface PublicInterface {
    field: string;
}

interface InternalInterface {
    field: string;
}
"#;
        let chunks = parser.parse(source, "test.ts").unwrap();

        let public_if = chunks.iter().find(|c| c.name == "PublicInterface").unwrap();
        assert_eq!(public_if.visibility, Visibility::Public);

        let internal_if = chunks
            .iter()
            .find(|c| c.name == "InternalInterface")
            .unwrap();
        assert_eq!(internal_if.visibility, Visibility::Internal);
    }
}
