use colored::Colorize;
use serde::{Serialize, Serializer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Default)]
pub enum DeclarationKind {
    #[default]
    Namespace,
    Class,
    Struct,
    Interface,
    Record,
    Enum,
    EnumMember,
    Method,
    Function,
    Constructor,
    Destructor,
    Property,
    Indexer,
    Field,
    Event,
    Delegate,
    Operator,
    Heading,
    CodeBlock,
}

impl DeclarationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Interface => "interface",
            Self::Record => "record",
            Self::Enum => "enum",
            Self::EnumMember => "enum_member",
            Self::Method => "method",
            Self::Function => "function",
            Self::Constructor => "ctor",
            Self::Destructor => "dtor",
            Self::Property => "property",
            Self::Indexer => "indexer",
            Self::Field => "field",
            Self::Event => "event",
            Self::Delegate => "delegate",
            Self::Operator => "operator",
            Self::Heading => "heading",
            Self::CodeBlock => "code_block",
        }
    }
}

impl std::fmt::Display for DeclarationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for DeclarationKind {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub docs: Vec<String>,
    pub docs_inside: bool,
    pub visibility: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub doc_start_byte: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "_is_false")]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Declaration>,
}

fn _is_false(b: &bool) -> bool {
    !*b
}

impl Declaration {
    pub fn lines_suffix(&self) -> String {
        if self.start_line == 0 {
            String::new()
        } else if self.start_line == self.end_line {
            format!("  L{}", self.start_line)
                .truecolor(150, 150, 150)
                .to_string()
        } else {
            format!("  L{}-{}", self.start_line, self.end_line)
                .truecolor(150, 150, 150)
                .to_string()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ParseResult {
    #[serde(serialize_with = "_serialize_path")]
    pub path: PathBuf,
    pub language: &'static str,
    #[serde(skip)]
    pub source: Vec<u8>,
    pub line_count: usize,
    pub error_count: usize,
    pub declarations: Vec<Declaration>,
}

fn _serialize_path<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&p.to_string_lossy())
}

#[derive(Debug, Clone)]
pub struct OutlineOptions {
    pub include_private: bool,
    pub include_fields: bool,
    pub include_docs: bool,
    pub include_attributes: bool,
    pub include_line_numbers: bool,
    pub max_doc_lines: usize,
    pub max_members: Option<usize>,
}

impl Default for OutlineOptions {
    fn default() -> Self {
        Self {
            include_private: true,
            include_fields: true,
            include_docs: true,
            include_attributes: true,
            include_line_numbers: true,
            max_doc_lines: 6,
            max_members: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DigestOptions {
    pub include_private: bool,
    pub include_fields: bool,
    pub max_members_per_type: usize,
    pub max_heading_depth: usize,
}

impl Default for DigestOptions {
    fn default() -> Self {
        Self {
            include_private: false,
            include_fields: false,
            max_members_per_type: 50,
            max_heading_depth: 3,
        }
    }
}
