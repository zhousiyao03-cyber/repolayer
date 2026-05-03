pub mod tools;

use crate::graph::store::Store;
use anyhow::Result;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerInfo},
    transport::stdio,
    ServiceExt,
};
use std::sync::{Arc, Mutex};
use tools::{FindContextArgs, GetCallersArgs, GetDependenciesArgs, GetSymbolArgs, Tools};

/// MCP server exposing repolayer's 5 query tools via stdio transport.
///
/// `rusqlite::Connection` is `Send` but `!Sync`. Wrapping `Store` in
/// `Arc<Mutex<Store>>` (via `Tools`) makes the server `Send + Sync`, which
/// is required by `rmcp::ServerHandler`.
#[derive(Clone)]
pub struct RepolayerServer {
    tools: Arc<Tools>,
}

impl RepolayerServer {
    fn new(store: Arc<Mutex<Store>>) -> Self {
        Self {
            tools: Arc::new(Tools { store }),
        }
    }
}

fn into_result(v: anyhow::Result<serde_json::Value>) -> Result<CallToolResult, rmcp::ErrorData> {
    match v {
        Ok(val) => Ok(CallToolResult::success(vec![Content::text(
            val.to_string(),
        )])),
        Err(e) => Err(rmcp::ErrorData::internal_error(e.to_string(), None)),
    }
}

#[rmcp::tool_router]
impl RepolayerServer {
    #[rmcp::tool(
        description = "Find the minimal set of relevant files/symbols for a coding task across all indexed repos."
    )]
    fn find_context(
        &self,
        Parameters(args): Parameters<FindContextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        into_result(self.tools.find_context(args))
    }

    #[rmcp::tool(
        description = "Get a symbol's definition, signature, and callers list across repos."
    )]
    fn get_symbol(
        &self,
        Parameters(args): Parameters<GetSymbolArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        into_result(self.tools.get_symbol(args))
    }

    #[rmcp::tool(description = "Walk the reverse call graph (callers) of a symbol across repos.")]
    fn get_callers(
        &self,
        Parameters(args): Parameters<GetCallersArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        into_result(self.tools.get_callers(args))
    }

    #[rmcp::tool(description = "Walk the forward dependency graph (imports) of a repo or module.")]
    fn get_dependencies(
        &self,
        Parameters(args): Parameters<GetDependenciesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        into_result(self.tools.get_dependencies(args))
    }

    #[rmcp::tool(description = "List all indexed repos with their metadata.")]
    fn list_repos(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        into_result(self.tools.list_repos())
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for RepolayerServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "repolayer",
            env!("CARGO_PKG_VERSION"),
        ))
    }
}

pub async fn run_stdio(store: Arc<Mutex<Store>>) -> Result<()> {
    let server = RepolayerServer::new(store);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
