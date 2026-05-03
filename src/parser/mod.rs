pub mod python;
pub mod treesitter;
pub mod typescript;

use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub path: String,
    pub symbols: Vec<ExportedSymbol>,
    pub imports: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExportedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub loc_start: u32,
    pub loc_end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Interface,
    TypeAlias,
    Const,
}

pub trait Parser {
    fn parse_file(&self, path: &Path) -> Result<ParsedFile>;
}
