use crate::types::{ChunkType, Visibility};

/// A code chunk extracted from source code.
///
/// Contains the code itself plus any associated documentation
/// (docstrings, comments, JSDoc, etc.).
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// Type of code construct (function, class, etc.)
    pub chunk_type: ChunkType,

    /// Visibility/access level (public, private, etc.)
    pub visibility: Visibility,

    /// Name of the symbol (function name, class name, etc.)
    pub name: String,

    /// Full signature if applicable (e.g., function signature with params)
    pub signature: Option<String>,

    /// The actual code
    pub code: String,

    /// Associated documentation (docstring, JSDoc, doc comment)
    pub documentation: Option<String>,

    /// File path within the package
    pub file_path: String,

    /// Start line (1-indexed)
    pub start_line: u32,

    /// End line (1-indexed)
    pub end_line: u32,

    /// Start byte offset
    pub start_byte: usize,

    /// End byte offset
    pub end_byte: usize,
}

impl CodeChunk {
    /// Create text for embedding: combines documentation + signature + code snippet.
    ///
    /// For semantic search, we want the embedding to capture:
    /// - What the code does (from docs)
    /// - The API surface (from signature)
    /// - The implementation (from code)
    pub fn embedding_text(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref doc) = self.documentation {
            parts.push(doc.as_str());
        }

        if let Some(ref sig) = self.signature {
            parts.push(sig.as_str());
        }

        // Include a preview of the code (not the whole thing for large chunks)
        let code_preview = if self.code.len() > 1000 {
            &self.code[..self.code.floor_char_boundary(1000)]
        } else {
            &self.code
        };
        parts.push(code_preview);

        parts.join("\n\n")
    }

    /// Create a snippet for display in search results.
    /// Prioritizes signature + first few lines of code.
    pub fn snippet(&self, max_len: usize) -> String {
        let mut result = String::new();

        if let Some(ref sig) = self.signature {
            result.push_str(sig);
            result.push('\n');
        }

        let remaining = max_len.saturating_sub(result.len());
        if remaining > 0 {
            let code_snippet: String = self.code.chars().take(remaining).collect();
            result.push_str(&code_snippet);
        }

        result
    }
}

/// Builder for creating CodeChunks during parsing.
#[derive(Debug, Default)]
pub struct ChunkBuilder {
    chunk_type: Option<ChunkType>,
    visibility: Visibility,
    name: Option<String>,
    signature: Option<String>,
    code: Option<String>,
    documentation: Option<String>,
    file_path: Option<String>,
    start_line: Option<u32>,
    end_line: Option<u32>,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
}

impl ChunkBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chunk_type(mut self, t: ChunkType) -> Self {
        self.chunk_type = Some(t);
        self
    }

    pub fn visibility(mut self, v: Visibility) -> Self {
        self.visibility = v;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    pub fn code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn documentation(mut self, doc: impl Into<String>) -> Self {
        let doc = doc.into();
        if !doc.is_empty() {
            self.documentation = Some(doc);
        }
        self
    }

    pub fn file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    pub fn location(
        mut self,
        start_line: u32,
        end_line: u32,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        self.start_line = Some(start_line);
        self.end_line = Some(end_line);
        self.start_byte = Some(start_byte);
        self.end_byte = Some(end_byte);
        self
    }

    pub fn build(self) -> Option<CodeChunk> {
        Some(CodeChunk {
            chunk_type: self.chunk_type?,
            visibility: self.visibility,
            name: self.name?,
            signature: self.signature,
            code: self.code?,
            documentation: self.documentation,
            file_path: self.file_path?,
            start_line: self.start_line?,
            end_line: self.end_line?,
            start_byte: self.start_byte?,
            end_byte: self.end_byte?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_builder() {
        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .visibility(Visibility::Public)
            .name("fetchData")
            .signature("async function fetchData(url: string): Promise<Response>")
            .code("async function fetchData(url: string): Promise<Response> {\n  return fetch(url);\n}")
            .documentation("Fetches data from the given URL.")
            .file_path("src/api.ts")
            .location(10, 13, 100, 200)
            .build()
            .unwrap();

        assert_eq!(chunk.name, "fetchData");
        assert_eq!(chunk.chunk_type, ChunkType::Function);
        assert_eq!(chunk.visibility, Visibility::Public);
        assert!(chunk.documentation.is_some());
    }

    #[test]
    fn test_chunk_builder_default_visibility() {
        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .name("helper")
            .code("fn helper() {}")
            .file_path("test.rs")
            .location(1, 1, 0, 10)
            .build()
            .unwrap();

        // Default visibility is Public
        assert_eq!(chunk.visibility, Visibility::Public);
    }

    #[test]
    fn test_chunk_builder_private_visibility() {
        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .visibility(Visibility::Private)
            .name("_helper")
            .code("def _helper(): pass")
            .file_path("test.py")
            .location(1, 1, 0, 10)
            .build()
            .unwrap();

        assert_eq!(chunk.visibility, Visibility::Private);
    }

    #[test]
    fn test_embedding_text_includes_docs() {
        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .name("test")
            .signature("fn test()")
            .code("fn test() { }")
            .documentation("This is a test function.")
            .file_path("test.rs")
            .location(1, 1, 0, 10)
            .build()
            .unwrap();

        let text = chunk.embedding_text();
        assert!(text.contains("This is a test function"));
        assert!(text.contains("fn test()"));
    }

    #[test]
    fn test_snippet_respects_max_len() {
        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .name("long")
            .code("x".repeat(1000))
            .file_path("test.rs")
            .location(1, 1, 0, 1000)
            .build()
            .unwrap();

        let snippet = chunk.snippet(100);
        assert!(snippet.len() <= 100);
    }

    #[test]
    fn test_embedding_text_utf8_boundary() {
        // Create code with multi-byte UTF-8 characters near the 1000 byte boundary
        // '─' is 3 bytes (E2 94 80), so we need to test truncation doesn't panic
        let mut code = "a".repeat(998);
        code.push_str("─────"); // Each ─ is 3 bytes

        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .name("test")
            .code(code)
            .file_path("test.rs")
            .location(1, 1, 0, 100)
            .build()
            .unwrap();

        // Should not panic
        let text = chunk.embedding_text();
        assert!(!text.is_empty());
    }

    #[test]
    fn test_embedding_text_truncates_long_code() {
        let code = "x".repeat(2000);

        let chunk = ChunkBuilder::new()
            .chunk_type(ChunkType::Function)
            .name("test")
            .code(code)
            .file_path("test.rs")
            .location(1, 1, 0, 2000)
            .build()
            .unwrap();

        let text = chunk.embedding_text();
        // Should be truncated to ~1000 bytes, not the full 2000
        assert!(text.len() < 1500);
    }
}
