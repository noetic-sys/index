//! Markdown parser for extracting code blocks from documentation files.

use crate::types::{ChunkType, Visibility};
use tree_sitter::{Node, Parser};

use crate::indexer::chunk::{ChunkBuilder, CodeChunk};
use crate::indexer::error::IndexerError;
use crate::indexer::language::{Language, LanguageParser};

/// Parser for Markdown files (README.md, docs/).
///
/// Uses tree-sitter to extract fenced code blocks with surrounding context.
pub struct MarkdownParser {
    _marker: (),
}

impl MarkdownParser {
    pub fn new() -> Result<Self, IndexerError> {
        Ok(Self { _marker: () })
    }

    fn create_parser() -> Result<Parser, IndexerError> {
        let mut parser = Parser::new();
        let language = tree_sitter_md::LANGUAGE;
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
            .ok_or_else(|| IndexerError::ParseError("failed to parse Markdown".into()))?;

        let mut chunks = Vec::new();
        let mut chunk_index = 0;
        self.visit_node(
            tree.root_node(),
            source,
            file_path,
            &mut chunks,
            &mut chunk_index,
        );
        Ok(chunks)
    }

    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
        chunk_index: &mut usize,
    ) {
        // Look for fenced code blocks
        if node.kind() == "fenced_code_block" {
            if let Some(chunk) = self.extract_code_block(node, source, file_path, *chunk_index) {
                chunks.push(chunk);
                *chunk_index += 1;
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, chunks, chunk_index);
        }
    }

    fn extract_code_block(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        chunk_index: usize,
    ) -> Option<CodeChunk> {
        // Get the language info string (e.g., "rust", "python")
        let lang_hint = node
            .child_by_field_name("info_string")
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind() == "info_string" || c.kind() == "language")
            })
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        // Get the code content
        let code = node
            .child_by_field_name("code")
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind() == "code_fence_content" || c.kind() == "code")
            })
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Skip empty code blocks
        if code.trim().is_empty() {
            return None;
        }

        // Extract context from preceding siblings
        let context = self.extract_context(node, source);
        let name = self.generate_name(&context, &lang_hint, chunk_index);

        ChunkBuilder::new()
            .chunk_type(ChunkType::Documentation)
            .visibility(Visibility::Public)
            .name(name)
            .signature(if !lang_hint.is_empty() {
                format!("```{}", lang_hint)
            } else {
                "```".to_string()
            })
            .code(code)
            .documentation(context)
            .file_path(file_path)
            .location(
                node.start_position().row as u32 + 1,
                node.end_position().row as u32 + 1,
                node.start_byte(),
                node.end_byte(),
            )
            .build()
    }

    fn extract_context(&self, node: Node, source: &str) -> String {
        let mut context_parts = Vec::new();

        if let Some(parent) = node.parent() {
            let mut cursor = parent.walk();
            let siblings: Vec<_> = parent.children(&mut cursor).collect();

            if let Some(pos) = siblings.iter().position(|n| n.id() == node.id()) {
                for i in (0..pos).rev() {
                    let sibling = siblings[i];
                    match sibling.kind() {
                        "atx_heading" | "setext_heading" | "heading" => {
                            if let Ok(text) = sibling.utf8_text(source.as_bytes()) {
                                context_parts.insert(0, text.trim().to_string());
                                break;
                            }
                        }
                        "paragraph" => {
                            if let Ok(text) = sibling.utf8_text(source.as_bytes()) {
                                context_parts.insert(0, text.trim().to_string());
                            }
                            if context_parts.len() >= 3 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        context_parts.join("\n\n")
    }

    fn generate_name(&self, context: &str, lang_hint: &str, index: usize) -> String {
        if let Some(line) = context.lines().next() {
            let cleaned = line.trim_start_matches('#').trim();
            if !cleaned.is_empty() {
                return slugify(cleaned);
            }
        }

        if !lang_hint.is_empty() {
            format!("{}_{}", lang_hint, index)
        } else {
            format!("code_block_{}", index)
        }
    }
}

impl LanguageParser for MarkdownParser {
    fn parse(&self, source: &str, file_path: &str) -> Result<Vec<CodeChunk>, IndexerError> {
        self.extract_chunks(source, file_path)
    }

    fn language(&self) -> Language {
        Language::Markdown
    }
}

/// Convert a string to a slug-like identifier.
fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_code_block() {
        let parser = MarkdownParser::new().unwrap();
        let source = r#"# Getting Started

Here's how to use the library:

```rust
fn main() {
    println!("Hello, world!");
}
```
"#;

        let chunks = parser.parse(source, "README.md").unwrap();
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].chunk_type, ChunkType::Documentation);
        assert!(chunks[0].code.contains("println!"));
    }

    #[test]
    fn test_parse_multiple_code_blocks() {
        let parser = MarkdownParser::new().unwrap();
        let source = r#"# Examples

## Basic Usage

```javascript
const x = 1;
```

## Advanced Usage

```typescript
const y: number = 2;
```
"#;

        let chunks = parser.parse(source, "README.md").unwrap();
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Getting Started"), "getting_started");
        assert_eq!(slugify("How to use API"), "how_to_use_api");
    }
}
