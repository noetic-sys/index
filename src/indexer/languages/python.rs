use crate::types::{ChunkType, Visibility};
use tree_sitter::{Node, Parser};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for Python using tree-sitter.
///
/// Extracts:
/// - Functions (def)
/// - Async functions (async def)
/// - Classes
/// - Docstrings (first string literal in function/class body)
pub struct PythonParser {
    _marker: (), // Placeholder for any state
}

impl PythonParser {
    pub fn new() -> Result<Self, IndexerError> {
        Ok(Self { _marker: () })
    }

    fn create_parser() -> Result<Parser, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
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
            .ok_or_else(|| IndexerError::ParseError("failed to parse Python source".into()))?;

        let mut chunks = Vec::new();
        self.visit_node(tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn visit_node(&self, node: Node, source: &str, file_path: &str, chunks: &mut Vec<CodeChunk>) {
        match node.kind() {
            "function_definition" => {
                if let Some(chunk) = self.extract_function(node, source, file_path) {
                    chunks.push(chunk);
                }
            }
            "class_definition" => {
                if let Some(chunk) = self.extract_class(node, source, file_path) {
                    chunks.push(chunk);
                }
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
        let docstring = self.extract_docstring(node, source);
        let signature = self.extract_function_signature(node, source);
        let visibility = self.detect_visibility(&name);

        // Check if async
        let is_async = node.child(0).map(|n| n.kind() == "async").unwrap_or(false);
        let chunk_type = if is_async {
            ChunkType::Function // Could add AsyncFunction if needed
        } else {
            ChunkType::Function
        };

        ChunkBuilder::new()
            .chunk_type(chunk_type)
            .visibility(visibility)
            .name(name)
            .signature(signature.unwrap_or_else(|| code.lines().next().unwrap_or("").to_string()))
            .code(code)
            .documentation(docstring.unwrap_or_default())
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
        let docstring = self.extract_docstring(node, source);
        let signature = code.lines().next().map(|s| s.to_string());
        let visibility = self.detect_visibility(&name);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Class)
            .visibility(visibility)
            .name(name)
            .signature(signature.unwrap_or_default())
            .code(code)
            .documentation(docstring.unwrap_or_default())
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_docstring(&self, node: Node, source: &str) -> Option<String> {
        // In Python, docstring is the first expression_statement in the body
        // that contains a string literal
        let body = node.child_by_field_name("body")?;

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "expression_statement" {
                let mut expr_cursor = child.walk();
                for expr_child in child.children(&mut expr_cursor) {
                    if expr_child.kind() == "string" {
                        let text = expr_child.utf8_text(source.as_bytes()).ok()?;
                        return Some(self.clean_docstring(text));
                    }
                }
            }
            // Stop if we hit a non-docstring statement
            if child.kind() != "expression_statement" && child.kind() != "comment" {
                break;
            }
        }
        None
    }

    fn extract_function_signature(&self, node: Node, source: &str) -> Option<String> {
        // Get def name(params) -> return_type:
        let start = node.start_byte();

        // Find the colon that ends the signature
        let body = node.child_by_field_name("body")?;
        let end = body.start_byte();

        source.get(start..end).map(|s| s.trim().to_string())
    }

    fn get_child_text(&self, node: Node, field: &str, source: &str) -> Option<String> {
        let child = node.child_by_field_name(field)?;
        child
            .utf8_text(source.as_bytes())
            .ok()
            .map(|s| s.to_string())
    }

    fn clean_docstring(&self, docstring: &str) -> String {
        // Remove quotes (""" or ''')
        let content = docstring
            .trim()
            .strip_prefix("\"\"\"")
            .or_else(|| docstring.strip_prefix("'''"))
            .unwrap_or(docstring)
            .strip_suffix("\"\"\"")
            .or_else(|| docstring.strip_suffix("'''"))
            .unwrap_or(docstring);

        // Clean up indentation
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return String::new();
        }

        // Find minimum indentation (excluding first line and empty lines)
        let min_indent = lines
            .iter()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        // Remove common indentation
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 || line.trim().is_empty() {
                    line.trim().to_string()
                } else if line.len() >= min_indent {
                    line[min_indent..].to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    /// Detect visibility from Python naming conventions.
    ///
    /// Python visibility rules:
    /// - `__dunder__` = Public (special methods like __init__)
    /// - `__name` (no trailing __) = Private (name mangling)
    /// - `_name` = Private (convention)
    /// - No prefix = Public
    fn detect_visibility(&self, name: &str) -> Visibility {
        // __dunder__ methods are public (special methods)
        if name.starts_with("__") && name.ends_with("__") {
            return Visibility::Public;
        }

        // __name (name mangling) is private
        if name.starts_with("__") {
            return Visibility::Private;
        }

        // _name is private by convention
        if name.starts_with('_') {
            return Visibility::Private;
        }

        Visibility::Public
    }
}

impl LanguageParser for PythonParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        self.extract_chunks(source, file_path)
    }

    fn language(&self) -> Language {
        Language::Python
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_function_with_docstring() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def add(a: int, b: int) -> int:
    """Add two numbers together.

    Args:
        a: First number
        b: Second number

    Returns:
        The sum of a and b
    """
    return a + b
"#;
        let chunks = parser.parse(source, "math.py").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "add");
        assert_eq!(chunks[0].chunk_type, ChunkType::Function);
        let doc = chunks[0].documentation.as_ref().unwrap();
        assert!(doc.contains("Add two numbers together"));
        assert!(doc.contains("Args:"));
    }

    #[test]
    fn test_parse_class_with_docstring() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
class Calculator:
    """A simple calculator class.

    Provides basic arithmetic operations.
    """

    def add(self, a: int, b: int) -> int:
        """Add two numbers."""
        return a + b

    def subtract(self, a: int, b: int) -> int:
        """Subtract b from a."""
        return a - b
"#;
        let chunks = parser.parse(source, "calc.py").unwrap();

        // Should get class and two methods
        assert!(chunks.len() >= 1);

        let class_chunk = chunks.iter().find(|c| c.chunk_type == ChunkType::Class);
        assert!(class_chunk.is_some());
        let class_doc = class_chunk.unwrap().documentation.as_ref().unwrap();
        assert!(class_doc.contains("simple calculator class"));

        let add_chunk = chunks.iter().find(|c| c.name == "add");
        assert!(add_chunk.is_some());
    }

    #[test]
    fn test_parse_async_function() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
async def fetch_data(url: str) -> dict:
    """Fetch data from URL.

    Args:
        url: The URL to fetch

    Returns:
        Response data as dict
    """
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.json()
"#;
        let chunks = parser.parse(source, "api.py").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "fetch_data");
        assert!(
            chunks[0]
                .documentation
                .as_ref()
                .unwrap()
                .contains("Fetch data from URL")
        );
    }

    #[test]
    fn test_parse_function_without_docstring() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def multiply(a, b):
    return a * b
"#;
        let chunks = parser.parse(source, "math.py").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].name, "multiply");
        // No docstring
        assert!(
            chunks[0].documentation.is_none()
                || chunks[0].documentation.as_ref().unwrap().is_empty()
        );
    }

    #[test]
    fn test_clean_docstring() {
        let parser = PythonParser::new().unwrap();

        let input = r#""""This is a docstring.

    With multiple lines.
    And indentation.
""""#;
        let cleaned = parser.clean_docstring(input);

        assert!(cleaned.contains("This is a docstring"));
        assert!(cleaned.contains("With multiple lines"));
        assert!(!cleaned.contains("\"\"\""));
    }

    #[test]
    fn test_function_signature() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def complex_func(a: int, b: str = "default", *args, **kwargs) -> List[int]:
    """A complex function."""
    pass
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        assert_eq!(chunks.len(), 1);
        let sig = chunks[0].signature.as_ref().unwrap();
        assert!(sig.contains("complex_func"));
        assert!(sig.contains("a: int"));
        assert!(sig.contains("**kwargs"));
    }

    #[test]
    fn test_visibility_public_function() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def public_func():
    pass
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Public);
    }

    #[test]
    fn test_visibility_private_underscore() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
def _private_func():
    pass
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_dunder_is_public() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
class MyClass:
    def __init__(self):
        pass

    def __str__(self):
        return "MyClass"
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        let init = chunks.iter().find(|c| c.name == "__init__").unwrap();
        assert_eq!(init.visibility, Visibility::Public);

        let str_method = chunks.iter().find(|c| c.name == "__str__").unwrap();
        assert_eq!(str_method.visibility, Visibility::Public);
    }

    #[test]
    fn test_visibility_name_mangled_is_private() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
class MyClass:
    def __private_method(self):
        pass
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        let private = chunks
            .iter()
            .find(|c| c.name == "__private_method")
            .unwrap();
        assert_eq!(private.visibility, Visibility::Private);
    }

    #[test]
    fn test_visibility_class() {
        let parser = PythonParser::new().unwrap();
        let source = r#"
class PublicClass:
    pass

class _PrivateClass:
    pass
"#;
        let chunks = parser.parse(source, "test.py").unwrap();

        let public = chunks.iter().find(|c| c.name == "PublicClass").unwrap();
        assert_eq!(public.visibility, Visibility::Public);

        let private = chunks.iter().find(|c| c.name == "_PrivateClass").unwrap();
        assert_eq!(private.visibility, Visibility::Private);
    }
}
