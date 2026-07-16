//! Integration tests for the workspace-level rename command.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::cli::edit::FileEdit;
use crate::indexer::Indexer;
use crate::query::engine::WorkspaceQueryEngine;

#[test]
fn rename_cross_file_call_sites() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    let a_kt = dir.path().join("A.kt");
    let b_kt = dir.path().join("B.kt");
    std::fs::write(&a_kt, "fun oldName() = Unit\n").unwrap();
    std::fs::write(&b_kt, "fun use() = oldName()\n").unwrap();

    let idx = Arc::new(Indexer::new());
    let uri_a = tower_lsp::lsp_types::Url::from_file_path(&a_kt).unwrap();
    let uri_b = tower_lsp::lsp_types::Url::from_file_path(&b_kt).unwrap();
    idx.index_content(&uri_a, "fun oldName() = Unit\n");
    idx.index_content(&uri_b, "fun use() = oldName()\n");

    let engine = WorkspaceQueryEngine::new(idx);
    let refs = crate::cli::run::smart_refs(&engine, "oldName", &root);
    assert!(
        refs.len() >= 2,
        "should find decl + call site: {}",
        refs.len()
    );

    // Apply rename
    let mut fm: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new();
    for r in &refs {
        let p = PathBuf::from(&r.file);
        fm.entry(p).or_default().push(TextEdit {
            range: Range {
                start: Position::new(r.line.saturating_sub(1), r.col.saturating_sub(1)),
                end: Position::new(
                    r.line.saturating_sub(1),
                    r.col.saturating_sub(1) + r.name.len() as u32,
                ),
            },
            new_text: "newName".to_string(),
        });
    }
    let fed: Vec<FileEdit> = fm
        .into_iter()
        .map(|(p, e)| FileEdit { path: p, edits: e })
        .collect();
    crate::cli::edit::apply_file_edits(&fed, Some(&root), false);

    let a = std::fs::read_to_string(&a_kt).unwrap();
    let b = std::fs::read_to_string(&b_kt).unwrap();
    assert!(a.contains("newName") && !a.contains("oldName"), "A.kt: {a}");
    assert!(b.contains("newName") && !b.contains("oldName"), "B.kt: {b}");
}

#[test]
fn rename_collision_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a_kt = dir.path().join("A.kt");
    std::fs::write(&a_kt, "fun oldName() = Unit\nfun newName() = Unit\n").unwrap();

    let idx = Arc::new(Indexer::new());
    let uri = tower_lsp::lsp_types::Url::from_file_path(&a_kt).unwrap();
    idx.index_content(&uri, "fun oldName() = Unit\nfun newName() = Unit\n");

    let engine = WorkspaceQueryEngine::new(idx);
    let existing = engine.definition_locations("newName");
    assert!(!existing.is_empty(), "collision: newName already exists");
}

#[test]
fn rename_dry_run_preserves_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let a_kt = dir.path().join("A.kt");
    let original = "fun oldName() = Unit\n";
    std::fs::write(&a_kt, original).unwrap();

    let idx = Arc::new(Indexer::new());
    let uri = tower_lsp::lsp_types::Url::from_file_path(&a_kt).unwrap();
    idx.index_content(&uri, original);

    let engine = WorkspaceQueryEngine::new(idx);
    let refs = crate::cli::run::smart_refs(&engine, "oldName", &root);

    let mut fm: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new();
    for r in &refs {
        let p = PathBuf::from(&r.file);
        fm.entry(p).or_default().push(TextEdit {
            range: Range {
                start: Position::new(r.line.saturating_sub(1), r.col.saturating_sub(1)),
                end: Position::new(
                    r.line.saturating_sub(1),
                    r.col.saturating_sub(1) + r.name.len() as u32,
                ),
            },
            new_text: "newName".to_string(),
        });
    }
    let fed: Vec<FileEdit> = fm
        .into_iter()
        .map(|(p, e)| FileEdit { path: p, edits: e })
        .collect();
    crate::cli::edit::apply_file_edits(&fed, Some(&root), true);
    let after = std::fs::read_to_string(&a_kt).unwrap();
    assert_eq!(after, original, "dry-run must not modify");
}
