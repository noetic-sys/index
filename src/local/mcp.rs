//! Local MCP server implementation.

use std::str::FromStr;
use std::sync::Arc;

use crate::types::Registry;
use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::io::stdio,
};
use serde::Deserialize;

use super::indexer::LocalIndexer;
use super::search::LocalSearch;

/// Local MCP Server for Code Intelligence.
pub struct LocalMcpServer {
    search: LocalSearch,
    indexer: Arc<LocalIndexer>,
    tool_router: ToolRouter<LocalMcpServer>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchCodeInput {
    /// The search query - describe what code you're looking for
    pub query: String,
    /// Package name (optional)
    #[serde(default)]
    pub package: Option<String>,
    /// Filter to specific registry (npm, pypi, crates)
    #[serde(default)]
    pub registry: Option<String>,
    /// Filter to specific version (or "latest")
    #[serde(default)]
    pub version: Option<String>,
    /// Include full code in results (not just snippets)
    #[serde(default)]
    pub include_code: bool,
    /// Maximum results to return (default: 10)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    10
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListPackagesInput {
    /// Filter to specific registry (npm, pypi, crates)
    #[serde(default)]
    pub registry: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexPackageInput {
    /// Registry name (npm, pypi, crates)
    pub registry: String,
    /// Package name
    pub package: String,
    /// Package version
    pub version: String,
}

#[tool_router]
impl LocalMcpServer {
    /// Create a new local MCP server.
    pub async fn new(index_dir: &std::path::Path) -> Result<Self> {
        let search = LocalSearch::new(index_dir).await?;
        let indexer = Arc::new(LocalIndexer::new(index_dir).await?);
        Ok(Self {
            search,
            indexer,
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        description = "Search for code in your project's indexed dependencies using semantic search. Returns relevant functions, classes, types, and documentation."
    )]
    async fn search_code(
        &self,
        Parameters(input): Parameters<SearchCodeInput>,
    ) -> Result<CallToolResult, McpError> {
        let results = self
            .search
            .search(
                &input.query,
                input.package.as_deref(),
                input.registry.as_deref(),
                input.version.as_deref(),
                input.limit as usize,
            )
            .await;

        match results {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(CallToolResult::success(vec![Content::text(
                        "No results found. Try a different query or make sure dependencies are indexed.",
                    )]));
                }

                let mut output = String::new();
                for (i, r) in results.iter().enumerate() {
                    output.push_str(&format!(
                        "{}. {} `{}` in {}:{}@{}\n",
                        i + 1,
                        r.chunk_type,
                        r.name,
                        r.registry,
                        r.package,
                        r.version
                    ));
                    output.push_str(&format!(
                        "   File: {} L{}-{}\n",
                        r.file_path, r.start_line, r.end_line
                    ));

                    if let Some(ref sig) = r.signature {
                        output.push_str(&format!("   Signature: {}\n", sig));
                    }

                    if input.include_code {
                        if let Ok(code) = self.search.get_code(&r.storage_key).await {
                            output.push_str("   ```\n");
                            for line in code.lines() {
                                output.push_str(&format!("   {}\n", line));
                            }
                            output.push_str("   ```\n");
                        }
                    } else {
                        // Show snippet
                        let snippet: String = r
                            .snippet
                            .lines()
                            .take(5)
                            .map(|l| format!("   {}", l))
                            .collect::<Vec<_>>()
                            .join("\n");
                        output.push_str(&snippet);
                        output.push('\n');
                    }
                    output.push('\n');
                }

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Search failed: {}",
                e
            ))])),
        }
    }

    #[tool(description = "List all indexed packages in the local index.")]
    async fn list_packages(
        &self,
        Parameters(_input): Parameters<ListPackagesInput>,
    ) -> Result<CallToolResult, McpError> {
        match self.search.list_versions().await {
            Ok(versions) => {
                if versions.is_empty() {
                    return Ok(CallToolResult::success(vec![Content::text(
                        "No packages indexed yet. Run `idx init` to index your project's dependencies.",
                    )]));
                }

                let mut output = String::from("Indexed packages:\n\n");
                for ver in versions {
                    output.push_str(&format!(
                        "- {}:{}@{}\n",
                        ver.registry, ver.name, ver.version
                    ));
                    if let Some(desc) = ver.description {
                        output.push_str(&format!("  {}\n", desc));
                    }
                }

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to list packages: {}",
                e
            ))])),
        }
    }

    #[tool(
        description = "Index a package from a registry. Use this to add a package to the local index so it can be searched."
    )]
    async fn index_package(
        &self,
        Parameters(input): Parameters<IndexPackageInput>,
    ) -> Result<CallToolResult, McpError> {
        let registry = match Registry::from_str(&input.registry) {
            Ok(r) => r,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Invalid registry '{}': {}. Use: npm, pypi, or crates",
                    input.registry, e
                ))]));
            }
        };

        match self
            .indexer
            .index_package(registry, &input.package, &input.version)
            .await
        {
            Ok(result) => {
                if result.chunks_indexed > 0 {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Indexed {}:{}@{} ({} chunks from {} files)",
                        input.registry,
                        input.package,
                        input.version,
                        result.chunks_indexed,
                        result.files_processed
                    ))]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "{}:{}@{} was already indexed",
                        input.registry, input.package, input.version
                    ))]))
                }
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to index {}:{}@{}: {}",
                input.registry, input.package, input.version, e
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for LocalMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "index-local".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Local Code Intelligence - semantic search for your project's dependencies. \
                 Tools: search_code, list_packages, index_package."
                    .to_string(),
            ),
        }
    }
}

/// Run the local MCP server over stdio.
pub async fn run_local(index_dir: &std::path::Path) -> Result<()> {
    let server = LocalMcpServer::new(index_dir).await?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
