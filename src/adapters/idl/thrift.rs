use crate::adapters::idl::protobuf::{IdlFile, IdlMethod, IdlService};
use anyhow::{Context, Result};
use std::path::Path;

pub struct ThriftParser;

impl ThriftParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, path: &Path) -> Result<IdlFile> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let package = extract_namespace(&source).unwrap_or_default();
        let services = extract_services(&source);
        Ok(IdlFile { package, services })
    }
}

impl Default for ThriftParser {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_namespace(s: &str) -> Option<String> {
    // Take any namespace (`namespace go x.y` / `namespace py x.y` etc.)
    let re = regex::Regex::new(r"(?m)^\s*namespace\s+\S+\s+([a-zA-Z0-9_.]+)").ok()?;
    re.captures(s).map(|c| c[1].to_string())
}

fn extract_services(s: &str) -> Vec<IdlService> {
    // Match `service Name { ... }` block
    let svc_re = regex::Regex::new(r"(?ms)service\s+(\w+)\s*\{(.*?)\n\}").unwrap();
    // Method line: `<ReturnType> <name>(<args>)` — first arg pattern: `1: <Type> <name>`
    // group 1 = output type, group 2 = method name, group 3 = input type
    let m_re =
        regex::Regex::new(r"(?m)([\w.]+)\s+(\w+)\s*\(\s*1\s*:\s*([\w.]+)\s+\w+\s*\)").unwrap();
    svc_re
        .captures_iter(s)
        .map(|svc| {
            let name = svc[1].to_string();
            let body = &svc[2];
            let methods = m_re
                .captures_iter(body)
                .map(|m| IdlMethod {
                    name: m[2].to_string(),
                    input: m[3].to_string(),
                    output: m[1].to_string(),
                })
                .collect();
            IdlService { name, methods }
        })
        .collect()
}
