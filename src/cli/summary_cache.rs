//! AI Summary Cache — pre-computed symbol summaries stored in the index.
//!
//! During `kotlin-lsp index`, public symbols get a structured summary cached.
//! Agents load these via `summarize --cached` without re-parsing source files.
//!
//! Commands:
//! - `summary-cache stats` — show cache stats (count, freshness)
//! - `summarize <name> --cached` — use cached summary instead of re-parsing

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ── Cached summary type ─────────────────────────────────────────────────────

/// Pre-computed summary for a single symbol, stored in the index cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedSummary {
    /// Symbol name.
    pub name: String,
    /// Lowercase kind: class, function, interface, etc.
    pub kind: String,
    /// Visibility: public, internal, protected, private.
    pub visibility: String,
    /// Modifiers: abstract, open, suspend, inline, etc.
    pub modifiers: Vec<String>,
    /// Full signature (first line).
    pub signature: String,
    /// Return type, if any.
    pub return_type: Option<String>,
    /// Parameter list: (name, type).
    pub parameters: Vec<(String, String)>,
    /// KDoc summary.
    pub doc: Option<String>,
    /// Names of referenced types.
    pub dependencies: Vec<String>,
    /// File URI.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub col: u32,
    /// Member summaries for expand mode.
    pub members: Vec<MemberCacheEntry>,
}

/// A member symbol inside a class-like summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemberCacheEntry {
    pub name: String,
    pub kind: String,
    pub signature: String,
}

// ── Summary builder ─────────────────────────────────────────────────────────

/// Build a CachedSummary from an indexed SymbolEntry and FileData.
pub(crate) fn build_cached_summary(
    name: &str,
    sym: &crate::types::SymbolEntry,
    file_uri: &str,
    all_symbols: &[crate::types::SymbolEntry],
) -> CachedSummary {
    let visibility = format!("{:?}", sym.visibility).to_lowercase();
    let kind = format!("{:?}", sym.kind).to_lowercase();

    // Detect modifiers from detail
    let mut modifiers = Vec::new();
    if sym.deprecated {
        modifiers.push("deprecated".to_string());
    }
    let detail_lower = sym.detail.to_lowercase();
    for keyword in &[
        "abstract",
        "open",
        "data",
        "sealed",
        "suspend",
        "inline",
        "override",
        "tailrec",
        "operator",
        "infix",
        "expect",
        "actual",
        "external",
        "const",
        "lateinit",
        "inner",
        "annotation",
        "companion",
    ] {
        if detail_lower.contains(keyword) {
            modifiers.push(keyword.to_string());
        }
    }

    // Members: not tracked in cache (use `summarize --expand` for CST-based members).
    // The cached summary returns an empty member list to avoid listing unrelated
    // file-level symbols as members.
    let members: Vec<MemberCacheEntry> = Vec::new();
    let _ = all_symbols; // suppress unused warning

    CachedSummary {
        name: name.to_string(),
        kind,
        visibility,
        modifiers,
        signature: sym.detail.clone(),
        return_type: sym.return_type.clone(),
        parameters: sym.parameters.clone(),
        doc: sym.documentation.clone(),
        dependencies: vec![],
        file: file_uri.to_string(),
        line: sym.selection_range.start.line + 1,
        col: sym.selection_range.start.character + 1,
        members,
    }
}

// ── Cache storage ───────────────────────────────────────────────────────────

/// Cache summaries keyed by "<file_uri>::<symbol_name>".
pub(crate) type SummaryCache = Arc<DashMap<String, CachedSummary>>;

/// Build the summary cache from all indexed files.
pub(crate) fn build_summary_cache(index: &Arc<crate::indexer::Indexer>) -> SummaryCache {
    let cache: SummaryCache = Arc::new(DashMap::new());

    for file_entry in index.files.iter() {
        let uri = file_entry.key();
        let file_data = file_entry.value();

        for sym in &file_data.symbols {
            // Only cache public/internal symbols (agents rarely need private)
            if sym.visibility == crate::types::Visibility::Private {
                continue;
            }
            let key = format!("{}::{}", uri, sym.name);
            let summary = build_cached_summary(&sym.name, sym, uri, &file_data.symbols);
            cache.insert(key, summary);
        }
    }

    cache
}

/// Look up a cached summary by name.
pub(crate) fn lookup_summary(cache: &SummaryCache, name: &str) -> Vec<CachedSummary> {
    cache
        .iter()
        .filter(|entry| entry.key().ends_with(&format!("::{name}")))
        .map(|entry| entry.value().clone())
        .collect()
}

// ── CLI commands ────────────────────────────────────────────────────────────

/// Run `summary-cache stats` — print cache statistics.
pub(crate) async fn run_summary_cache_stats(root: Option<&Path>, no_stdlib: bool) {
    let root = crate::cli::run::resolve_root_for_file(root, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, no_stdlib).await;

    let cache = build_summary_cache(&index);

    let total = cache.len();
    let with_doc = cache.iter().filter(|e| e.value().doc.is_some()).count();
    let with_sig = cache
        .iter()
        .filter(|e| !e.value().signature.is_empty())
        .count();
    let with_return = cache
        .iter()
        .filter(|e| e.value().return_type.is_some())
        .count();

    // Kind distribution
    let mut kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in cache.iter() {
        *kinds.entry(entry.value().kind.clone()).or_default() += 1;
    }
    let mut kind_list: Vec<_> = kinds.into_iter().collect();
    kind_list.sort_by_key(|(_, c)| std::cmp::Reverse(*c));

    println!("AI Summary Cache Stats:");
    println!("  Total cached summaries: {total}");
    println!("  With documentation:     {with_doc}");
    println!("  With signature:         {with_sig}");
    println!("  With return type:       {with_return}");
    println!("  By kind:");
    for (kind, count) in kind_list.iter().take(10) {
        println!("    {kind}: {count}");
    }
}

/// Run `summarize <name> --cached` — use cached summary instead of re-parsing.
pub(crate) async fn run_summarize_cached(name: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let cache = build_summary_cache(&index);
    let summaries = lookup_summary(&cache, name);

    if summaries.is_empty() {
        eprintln!("No cached summary found for '{name}'");
        std::process::exit(1);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summaries).expect("serialize JSON")
        );
    } else {
        for summary in &summaries {
            println!("{}", summary.name);
            println!("  Kind: {} {}", summary.visibility, summary.kind);
            if !summary.modifiers.is_empty() {
                println!("  Modifiers: {}", summary.modifiers.join(", "));
            }
            if !summary.signature.is_empty() {
                println!("  Signature: {}", summary.signature);
            }
            if let Some(ref doc) = summary.doc {
                println!("  Doc: {doc}");
            }
            if let Some(ref ret) = summary.return_type {
                println!("  Returns: {ret}");
            }
            if !summary.parameters.is_empty() {
                let params: Vec<String> = summary
                    .parameters
                    .iter()
                    .map(|(n, t)| format!("{n}: {t}"))
                    .collect();
                println!("  Parameters: {}", params.join(", "));
            }
            println!(
                "  Location: {}:{}:{}",
                summary.file, summary.line, summary.col
            );
            if !summary.members.is_empty() {
                println!("  Members:");
                for m in &summary.members {
                    print!("    {} {} ", m.kind, m.name);
                    if !m.signature.is_empty() {
                        print!("{}", m.signature);
                    }
                    println!();
                }
            } else {
                println!("  Members: (use `summarize --expand` for full CST-based members)");
            }
            println!();
        }
    }
}
