//! Tests for `src/cli/snapshot.rs` — workspace snapshot collection.

use super::*;

#[test]
fn is_entry_point_activity() {
    assert!(is_entry_point("MainActivity", "class", ""));
    assert!(is_entry_point("SettingsActivity", "class", ""));
    assert!(is_entry_point("MainFragment", "class", ""));
}

#[test]
fn is_entry_point_application() {
    assert!(is_entry_point("MyApplication", "class", ""));
}

#[test]
fn is_not_entry_point() {
    assert!(!is_entry_point("UserRepository", "class", ""));
    assert!(!is_entry_point("Application", "function", ""));
    assert!(!is_entry_point("main", "function", ""));
    assert!(!is_entry_point("", "class", ""));
}

#[test]
fn collect_relationships_empty_index() {
    let idx = crate::indexer::Indexer::new();
    let rels = collect_relationships(&idx);
    assert!(rels.calls.is_empty());
    assert!(rels.extends.is_empty());
    assert!(rels.overrides.is_empty());
    assert!(rels.imports.is_empty());
}

#[test]
fn collect_relationships_populated_call_edges() {
    let idx = crate::indexer::Indexer::new();
    idx.call_edges.insert(
        "bar".to_string(),
        vec![("/a.kt".to_string(), "foo".to_string())],
    );
    idx.call_edges.insert(
        "baz".to_string(),
        vec![("/a.kt".to_string(), "foo".to_string())],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.calls.len(), 2);
    assert!(rels.calls.contains(&["foo".to_string(), "bar".to_string()]));
    assert!(rels.calls.contains(&["foo".to_string(), "baz".to_string()]));
}

#[test]
fn collect_relationships_populated_extends() {
    let idx = crate::indexer::Indexer::new();
    idx.supertypes_index.insert(
        "Dog".to_string(),
        vec![(
            "Animal".to_string(),
            "/a.kt".to_string(),
            crate::types::SuperKind::Extends,
        )],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.extends.len(), 1);
    assert!(rels
        .extends
        .contains(&["Dog".to_string(), "Animal".to_string()]));
}

#[test]
fn collect_relationships_populated_overrides() {
    let idx = crate::indexer::Indexer::new();
    idx.override_edges.insert(
        "onCreate".to_string(),
        vec![("/app.kt".to_string(), "MyActivity".to_string())],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.overrides.len(), 1);
    let expect = "MyActivity.onCreate".to_string();
    assert!(rels.overrides[0].contains(&expect));
}

#[test]
fn collect_relationships_populated_imports() {
    let idx = crate::indexer::Indexer::new();
    idx.import_edges.insert(
        "com.lib.Foo".to_string(),
        vec![("/a.kt".to_string(), "Foo".to_string())],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.imports.len(), 1);
    assert!(rels
        .imports
        .contains(&["/a.kt".to_string(), "com.lib.Foo".to_string()]));
}

// ── Deduplication (issue #242) ───────────────────────────────────────────────

#[test]
fn is_library_path_detects_sources_cache() {
    #[allow(deprecated)]
    let home = std::env::home_dir().expect("home dir");
    let lib = format!("{}/.kotlin-lsp/sources/net.foo/Bar.kt", home.display());
    assert!(is_library_path(&lib));
    assert!(!is_library_path("/Users/x/project/src/Main.kt"));
    assert!(!is_library_path("/a.kt"));
}

#[test]
fn collect_relationships_skips_library_edges() {
    // Edges whose file side lives in ~/.kotlin-lsp/sources are excluded even
    // when --include-libraries indexed them (issue #242: relationships are
    // scoped to the workspace).
    #[allow(deprecated)]
    let home = std::env::home_dir().expect("home dir");
    let lib_file = format!("{}/.kotlin-lsp/sources/lib/x/Util.kt", home.display());

    let idx = crate::indexer::Indexer::new();
    idx.call_edges.insert(
        "libFn".to_string(),
        vec![(lib_file.clone(), "libCaller".to_string())],
    );
    idx.call_edges.insert(
        "workspaceFn".to_string(),
        vec![("/a.kt".to_string(), "main".to_string())],
    );
    idx.import_edges.insert(
        "com.lib.Foo".to_string(),
        vec![(lib_file.clone(), "Foo".to_string())],
    );
    idx.supertypes_index.insert(
        "LibBase".to_string(),
        vec![(
            "Object".to_string(),
            lib_file.clone(),
            crate::types::SuperKind::Extends,
        )],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.calls.len(), 1);
    assert!(rels
        .calls
        .contains(&["main".to_string(), "workspaceFn".to_string()]));
    assert!(rels.imports.is_empty());
    assert!(rels.extends.is_empty());
}

#[test]
fn collect_relationships_dedupes_repeated_call_pairs() {
    // The same caller/callee pair recorded from repeated parses or multiple
    // occurrences in one file must appear exactly once in the snapshot.
    let idx = crate::indexer::Indexer::new();
    idx.call_edges.insert(
        "throwIllegalArgumentException".to_string(),
        vec![
            ("/a.kt".to_string(), "requirePrecondition".to_string()),
            ("/a.kt".to_string(), "requirePrecondition".to_string()),
            ("/b.kt".to_string(), "requirePrecondition".to_string()),
        ],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.calls.len(), 1);
    assert!(rels.calls.contains(&[
        "requirePrecondition".to_string(),
        "throwIllegalArgumentException".to_string()
    ]));
}

#[test]
fn collect_relationships_dedupes_repeated_imports() {
    let idx = crate::indexer::Indexer::new();
    idx.import_edges.insert(
        "com.lib.Foo".to_string(),
        vec![
            ("/a.kt".to_string(), "Foo".to_string()),
            ("/a.kt".to_string(), "Foo".to_string()),
        ],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.imports.len(), 1);
    assert!(rels
        .imports
        .contains(&["/a.kt".to_string(), "com.lib.Foo".to_string()]));
}

#[test]
fn collect_relationships_dedupes_repeated_extends() {
    let idx = crate::indexer::Indexer::new();
    idx.supertypes_index.insert(
        "Dog".to_string(),
        vec![
            (
                "Animal".to_string(),
                "/a.kt".to_string(),
                crate::types::SuperKind::Extends,
            ),
            (
                "Animal".to_string(),
                "/a.kt".to_string(),
                crate::types::SuperKind::Extends,
            ),
        ],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.extends.len(), 1);
}

#[test]
fn collect_relationships_dedupes_repeated_overrides() {
    let idx = crate::indexer::Indexer::new();
    idx.override_edges.insert(
        "onCreate".to_string(),
        vec![
            ("/app.kt".to_string(), "MyActivity".to_string()),
            ("/app.kt".to_string(), "MyActivity".to_string()),
        ],
    );

    let rels = collect_relationships(&idx);
    assert_eq!(rels.overrides.len(), 1);
}
