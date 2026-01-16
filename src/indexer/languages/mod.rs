mod typescript;
mod python;
mod rust_lang;
mod go;
mod java;
mod markdown;

use super::error::IndexerError;
use super::language::{Language, LanguageParser};

pub use typescript::TypeScriptParser;
pub use python::PythonParser;
pub use rust_lang::RustParser;
pub use go::GoParser;
pub use java::JavaParser;
pub use markdown::MarkdownParser;

/// Get a parser for the given language.
pub fn get_parser(language: Language) -> Result<Box<dyn LanguageParser>, IndexerError> {
    match language {
        Language::TypeScript => Ok(Box::new(TypeScriptParser::new()?)),
        Language::JavaScript => Ok(Box::new(TypeScriptParser::new_javascript()?)),
        Language::Python => Ok(Box::new(PythonParser::new()?)),
        Language::Rust => Ok(Box::new(RustParser::new()?)),
        Language::Go => Ok(Box::new(GoParser::new()?)),
        Language::Java => Ok(Box::new(JavaParser::new()?)),
        Language::Markdown => Ok(Box::new(MarkdownParser::new()?)),
    }
}

/// Get a parser for the given file path based on extension.
pub fn get_parser_for_file(file_path: &str) -> Result<Box<dyn LanguageParser>, IndexerError> {
    let language = Language::from_path(file_path)
        .ok_or_else(|| IndexerError::UnsupportedLanguage(file_path.to_string()))?;
    get_parser(language)
}
