use crate::graph::store::Store;
use crate::query;
use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindContextArgs {
    pub task_description: String,
    #[serde(default)]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolArgs {
    pub name: String,
    #[serde(default)]
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCallersArgs {
    pub symbol: String,
    #[serde(default = "default_depth")]
    pub depth: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDependenciesArgs {
    pub repo_or_module: String,
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_depth() -> usize {
    2
}

/// Tool dispatch wrapper.
///
/// The store is wrapped in `Arc<Mutex<Store>>` because `rusqlite::Connection`
/// is `Send` but `!Sync`; the mutex serialises all query access so `Tools`
/// itself becomes `Send + Sync`, satisfying `rmcp::ServerHandler`.
pub struct Tools {
    pub store: Arc<Mutex<Store>>,
}

impl Tools {
    pub fn find_context(&self, args: FindContextArgs) -> Result<Value> {
        let store = self.store.lock().expect("store mutex poisoned");
        let result = query::find_context::find_context(
            &store,
            &args.task_description,
            args.budget_tokens.unwrap_or(5000),
        )?;
        Ok(serde_json::to_value(result)?)
    }

    pub fn get_symbol(&self, args: GetSymbolArgs) -> Result<Value> {
        let store = self.store.lock().expect("store mutex poisoned");
        let r = query::symbol::get_symbol(&store, &args.name, args.repo.as_deref())?;
        Ok(serde_json::to_value(r)?)
    }

    pub fn get_callers(&self, args: GetCallersArgs) -> Result<Value> {
        let store = self.store.lock().expect("store mutex poisoned");
        let r = query::callers::get_callers(&store, &args.symbol, args.depth)?;
        Ok(serde_json::to_value(r)?)
    }

    pub fn get_dependencies(&self, args: GetDependenciesArgs) -> Result<Value> {
        let store = self.store.lock().expect("store mutex poisoned");
        let r = query::dependencies::get_dependencies(&store, &args.repo_or_module, args.depth)?;
        Ok(serde_json::to_value(r)?)
    }

    pub fn list_repos(&self) -> Result<Value> {
        let store = self.store.lock().expect("store mutex poisoned");
        let r = query::list_repos::list_repos(&store)?;
        Ok(serde_json::to_value(r)?)
    }
}
