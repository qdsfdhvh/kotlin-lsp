//! Hover implementation for the CLI.

use std::path::Path;

use tower_lsp::lsp_types::{Position, Url};

use crate::indexer::live_tree::{lang_for_path, parse_live, utf16_col_to_byte};
use crate::indexer::resolution::{
    enrich_at_line, enrich_at_location, ResolveOptions, ResolvedSymbol, SubstitutionContext,
};
use crate::queries::{
    KIND_FIELD_ACCESS, KIND_GENERIC_TYPE, KIND_IDENTIFIER, KIND_INTERP_IDENT,
    KIND_METHOD_INVOCATION, KIND_METHOD_REFERENCE, KIND_NAV_SUFFIX, KIND_OBJECT_CREATION_EXPR,
    KIND_SCOPED_IDENT, KIND_SCOPED_TYPE_IDENT, KIND_SIMPLE_IDENT, KIND_TYPE_IDENT,
};
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
    )
    .or_else(|| {
        hover_reference(
            engine,
            &uri,
            Position::new(line.saturating_sub(1), col.saturating_sub(1)),
        )
    })?;

    let mut out = resolved.signature;
    if !resolved.doc.is_empty() {
        out.push_str("\n\n");
        out.push_str(&resolved.doc);
    }
    Some(out)
}

/// Conservative CLI reference fallback: never discard a receiver or pick an
/// arbitrary definition. Declaration hover above retains its existing behavior.
fn hover_reference(
    engine: &WorkspaceQueryEngine,
    uri: &Url,
    position: Position,
) -> Option<ResolvedSymbol> {
    let file = engine.get_file(uri.as_str())?;
    let line = file.lines.get(position.line as usize)?;
    if position.character as usize >= line.encode_utf16().count() {
        return None;
    }
    // Validate the exact cursor against the CST, not a textual word match in
    // comments/literals or the word immediately before punctuation. Parse only
    // the requested cursor file when no live tree is available.
    let doc = engine.live_doc(uri).or_else(|| {
        parse_live(&file.lines.join("\n"), lang_for_path(uri.path())?).map(std::sync::Arc::new)
    })?;
    let point = tree_sitter::Point::new(
        position.line as usize,
        utf16_col_to_byte(line, position.character as usize),
    );
    let node = doc
        .tree
        .root_node()
        .descendant_for_point_range(point, point)?;
    if !matches!(
        node.kind(),
        KIND_SIMPLE_IDENT | KIND_TYPE_IDENT | KIND_IDENTIFIER | KIND_INTERP_IDENT
    ) || point >= node.end_position()
    {
        return None;
    }
    // A selected member stays qualified with safe calls, call receivers, or
    // whitespace around the dot. Check its CST role, not arbitrary ancestors:
    // the unqualified Target in receiver.consume(Target()) is still eligible.
    if is_selected_member(node) {
        return None;
    }
    let (name, qualifier) = engine.word_and_qualifier_at(uri, position)?;
    if qualifier.is_some() || node.utf8_text(&doc.bytes).ok()? != name {
        return None;
    }
    // Even a workspace hit may have another library candidate after fast start.
    // Compact discovery is cheap relative to hydrating unrelated library files.
    engine.index.lazy_load_library_symbols();
    let mut locations = engine.definition_locations(&name);
    locations.sort_by(|a, b| {
        (
            a.uri.as_str(),
            a.range.start.line,
            a.range.start.character,
            a.range.end.line,
            a.range.end.character,
        )
            .cmp(&(
                b.uri.as_str(),
                b.range.start.line,
                b.range.start.character,
                b.range.end.line,
                b.range.end.character,
            ))
    });
    locations.dedup();
    let [location] = locations.as_slice() else {
        return None;
    };
    engine.get_file(location.uri.as_str())?;
    enrich_at_location(
        engine.index.as_ref(),
        location,
        &name,
        SubstitutionContext::None,
        &ResolveOptions::hover(),
    )
}

/// Match the selected child, never all descendants of a qualified expression.
fn is_selected_member(node: tree_sitter::Node<'_>) -> bool {
    // Java's generic constructor type wraps its base name, not its arguments.
    let node = match node.parent() {
        Some(parent)
            if parent.kind() == KIND_GENERIC_TYPE && parent.named_child(0) == Some(node) =>
        {
            parent
        }
        _ => node,
    };
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        KIND_NAV_SUFFIX => true,
        KIND_METHOD_INVOCATION => {
            parent.child_by_field_name("object").is_some()
                && parent.child_by_field_name("name") == Some(node)
        }
        KIND_FIELD_ACCESS => {
            parent.child_by_field_name("object").is_some()
                && parent.child_by_field_name("field") == Some(node)
        }
        KIND_SCOPED_IDENT => parent.child_by_field_name("name") == Some(node),
        // These Java roles have no selected-name field. The final child is the
        // selected identifier; in Target::new it is the unnamed `new` token,
        // so the receiver Target remains eligible.
        KIND_SCOPED_TYPE_IDENT | KIND_METHOD_REFERENCE => node.next_sibling().is_none(),
        KIND_OBJECT_CREATION_EXPR => {
            parent.child_by_field_name("type") == Some(node)
                // Qualified creation starts with a named receiver expression;
                // unqualified creation starts with the unnamed `new` token.
                && parent.child(0).is_some_and(|first| first.is_named())
        }
        _ => false,
    }
}
