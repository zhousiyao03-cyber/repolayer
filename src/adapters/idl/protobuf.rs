use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct IdlFile {
    pub package: String,
    pub services: Vec<IdlService>,
}

#[derive(Debug, Clone)]
pub struct IdlService {
    pub name: String,
    pub methods: Vec<IdlMethod>,
}

#[derive(Debug, Clone)]
pub struct IdlMethod {
    pub name: String,
    pub input: String,
    pub output: String,
}

pub struct ProtobufParser;

impl ProtobufParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, path: &Path) -> Result<IdlFile> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let package = extract_package(&source).unwrap_or_default();
        let services = extract_services(&source);
        Ok(IdlFile { package, services })
    }
}

impl Default for ProtobufParser {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_package(s: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?m)^\s*package\s+([a-zA-Z0-9_.]+)\s*;").ok()?;
    re.captures(s).map(|c| c[1].to_string())
}

fn extract_services(s: &str) -> Vec<IdlService> {
    // Match `service Name { ... }` greedily until matching closing brace.
    // Note: this regex assumes services don't have nested braces other than rpc bodies,
    // which holds for typical proto3 service definitions.
    let svc_re = regex::Regex::new(r"(?ms)service\s+(\w+)\s*\{(.*?)\n\}").unwrap();
    let rpc_re =
        regex::Regex::new(r"(?m)rpc\s+(\w+)\s*\(\s*([\w.]+)\s*\)\s*returns\s*\(\s*([\w.]+)\s*\)")
            .unwrap();
    svc_re
        .captures_iter(s)
        .map(|svc| {
            let name = svc[1].to_string();
            let body = &svc[2];
            let methods = rpc_re
                .captures_iter(body)
                .map(|m| IdlMethod {
                    name: m[1].to_string(),
                    input: m[2].to_string(),
                    output: m[3].to_string(),
                })
                .collect();
            IdlService { name, methods }
        })
        .collect()
}
