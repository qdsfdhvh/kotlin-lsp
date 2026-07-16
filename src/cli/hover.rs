//! Hover implementation for the CLI.

use std::path::Path;

use tower_lsp::lsp_types::Url;

use crate::indexer::resolution::{enrich_at_line, ResolveOptions, SubstitutionContext};
use crate::query::engine::WorkspaceQueryEngine;

/// Return a hover string for `file:line:col` using the pre-built index.
/// Line and col are 1-based (human-friendly) and converted internally to 0-based.
pub(crate) fn hover_at(
    engine: &WorkspaceQueryEngine,
    file: &Path,
    line: u32,
    col: u32,
) -> Option<String> {
    let abs = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let uri = Url::from_file_path(&abs).ok()?;

    // Index on-demand if this file wasn't already in cache.
    engine.index.ensure_indexed(&uri);

    let resolved = enrich_at_line(
        engine.index.as_ref(),
        uri.as_str(),
        line.saturating_sub(1), // 1-based → 0-based
        col.saturating_sub(1),
        SubstitutionContext::None,
        &ResolveOptions::hover(),
    )?;

    let mut out = resolved.signature;
    if !resolved.doc.is_empty() {
        out.push_str("\n\n");
        out.push_str(&resolved.doc);
    }
    Some(out)
}
