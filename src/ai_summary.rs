//! AI Summary types — pre-computed LLM-friendly symbol summaries.
//!
//! Cached in the workspace index (index.bin). Agents load the summary
//! instead of re-analyzing source files, saving tokens and round-trips.

use serde::{Deserialize, Serialize};

/// Pre-computed summary for a single symbol, optimized for LLM consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AISummary {
    // ── identity ──
    pub name: String,
    pub kind: String,
    pub package: Option<String>,
    pub file: String,
    pub line: u32,
    pub visibility: Option<String>,

    // ── signature + docs ──
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub deprecated: bool,

    // ── members (for classes/interfaces) ──
    pub members: Vec<MemberInfo>,

    // ── relationships ──
    pub supertypes: Vec<String>,
    pub subtypes: Vec<String>,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    pub importers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemberInfo {
    pub name: String,
    pub kind: String,
    pub signature: Option<String>,
}

impl Default for AISummary {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: String::new(),
            package: None,
            file: String::new(),
            line: 0,
            visibility: None,
            signature: None,
            doc: None,
            deprecated: false,
            members: Vec::new(),
            supertypes: Vec::new(),
            subtypes: Vec::new(),
            callers: Vec::new(),
            callees: Vec::new(),
            importers: Vec::new(),
        }
    }
}
