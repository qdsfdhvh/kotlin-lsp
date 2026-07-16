//! Unit tests for SymbolGraph — typed query API over Indexer DashMaps.
//!
//! These tests directly populate the underlying DashMaps to exercise
//! SymbolGraph methods without requiring full Kotlin parsing.

use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::indexer::{Indexer, SymbolGraph};

// ── helpers ──────────────────────────────────────────────────────────────

fn make_location(uri: &str, line: u32) -> Location {
    Location {
        uri: Url::parse(uri).unwrap(),
        range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: 10,
            },
        },
    }
}

fn empty_index() -> Indexer {
    Indexer::new()
}

// ── callers_of ───────────────────────────────────────────────────────────

#[test]
fn callers_of_returns_registered_callers() {
    let idx = empty_index();
    idx.call_edges.insert(
        "bar".to_string(),
        vec![
            ("/a.kt".to_string(), "foo".to_string()),
            ("/b.kt".to_string(), "baz".to_string()),
        ],
    );

    let g = SymbolGraph::new(&idx);
    let callers = g.callers_of("bar");
    assert_eq!(callers.len(), 2);
    assert!(callers.contains(&("/a.kt".to_string(), "foo".to_string())));
    assert!(callers.contains(&("/b.kt".to_string(), "baz".to_string())));
}

#[test]
fn callers_of_returns_empty_for_missing_callee() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let callers = g.callers_of("nonexistent");
    assert!(callers.is_empty());
}

#[test]
fn callers_of_returns_empty_for_empty_edges() {
    let idx = empty_index();
    idx.call_edges.insert("bar".to_string(), vec![]);
    let g = SymbolGraph::new(&idx);
    let callers = g.callers_of("bar");
    assert!(callers.is_empty());
}

// ── callees_of ───────────────────────────────────────────────────────────

#[test]
fn callees_of_reverse_lookup_from_call_edges() {
    let idx = empty_index();
    // foo() calls bar()
    idx.call_edges.insert(
        "bar".to_string(),
        vec![("/a.kt".to_string(), "foo".to_string())],
    );
    // foo() calls baz()
    idx.call_edges.insert(
        "baz".to_string(),
        vec![("/a.kt".to_string(), "foo".to_string())],
    );

    let g = SymbolGraph::new(&idx);
    let callees = g.callees_of("foo");
    assert_eq!(callees.len(), 2);
    assert!(callees.contains(&("/a.kt".to_string(), "bar".to_string())));
    assert!(callees.contains(&("/a.kt".to_string(), "baz".to_string())));
}

#[test]
fn callees_of_returns_empty_for_function_with_no_calls() {
    let idx = empty_index();
    // other_fn() calls bar() — but nobody calls "other_fn" in our test
    idx.call_edges.insert(
        "bar".to_string(),
        vec![("/a.kt".to_string(), "other_fn".to_string())],
    );

    let g = SymbolGraph::new(&idx);
    let callees = g.callees_of("bar");
    assert!(callees.is_empty(), "bar doesn't call anything");
}

#[test]
fn callees_of_returns_empty_when_no_edges_at_all() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let callees = g.callees_of("any_function");
    assert!(callees.is_empty());
}

#[test]
fn callees_of_handles_multiple_callees_same_caller() {
    let idx = empty_index();
    idx.call_edges.insert(
        "a".to_string(),
        vec![("/f.kt".to_string(), "main".to_string())],
    );
    idx.call_edges.insert(
        "b".to_string(),
        vec![("/f.kt".to_string(), "main".to_string())],
    );
    idx.call_edges.insert(
        "c".to_string(),
        vec![("/f.kt".to_string(), "main".to_string())],
    );

    let g = SymbolGraph::new(&idx);
    let callees = g.callees_of("main");
    assert_eq!(callees.len(), 3);
}

// ── supertypes_of ────────────────────────────────────────────────────────

#[test]
fn supertypes_of_returns_supertype_info() {
    let idx = empty_index();
    idx.supertypes_index.insert(
        "Dog".to_string(),
        vec![
            (
                "Animal".to_string(),
                "/animals.kt".to_string(),
                crate::types::SuperKind::Extends,
            ),
            (
                "Walkable".to_string(),
                "/animals.kt".to_string(),
                crate::types::SuperKind::Extends,
            ),
        ],
    );

    let g = SymbolGraph::new(&idx);
    let supers = g.supertypes_of("Dog");
    assert_eq!(supers.len(), 2);
    assert!(supers.iter().any(|s| s.0 == "Animal"));
    assert!(supers.iter().any(|s| s.0 == "Walkable"));
}

#[test]
fn supertypes_of_returns_empty_for_unknown_type() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let supers = g.supertypes_of("Ghost");
    assert!(supers.is_empty());
}

// ── subtypes_of ──────────────────────────────────────────────────────────

#[test]
fn subtypes_of_returns_registered_subtypes() {
    let idx = empty_index();
    let loc1 = make_location("file:///a.kt", 5);
    let loc2 = make_location("file:///b.kt", 12);
    idx.subtypes
        .insert("Animal".to_string(), vec![loc1.clone(), loc2.clone()]);

    let g = SymbolGraph::new(&idx);
    let subtypes = g.subtypes_of("Animal");
    assert_eq!(subtypes.len(), 2);
    assert_eq!(subtypes[0].uri.as_str(), "file:///a.kt");
    assert_eq!(subtypes[1].uri.as_str(), "file:///b.kt");
}

#[test]
fn subtypes_of_returns_empty_for_unknown_type() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let subtypes = g.subtypes_of("Ghost");
    assert!(subtypes.is_empty());
}

// ── importers_of ─────────────────────────────────────────────────────────

#[test]
fn importers_of_returns_files_that_import_fqn() {
    let idx = empty_index();
    idx.import_edges.insert(
        "com.lib.Foo".to_string(),
        vec![
            ("/a.kt".to_string(), "Foo".to_string()),
            ("/b.kt".to_string(), "FooAlias".to_string()),
        ],
    );

    let g = SymbolGraph::new(&idx);
    let importers = g.importers_of("com.lib.Foo");
    assert_eq!(importers.len(), 2);
    assert!(importers.contains(&("/a.kt".to_string(), "Foo".to_string())));
    assert!(importers.contains(&("/b.kt".to_string(), "FooAlias".to_string())));
}

#[test]
fn importers_of_returns_empty_for_unimported_fqn() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let importers = g.importers_of("com.unknown.Bar");
    assert!(importers.is_empty());
}

// ── overrides_of ─────────────────────────────────────────────────────────

#[test]
fn overrides_of_returns_override_files() {
    let idx = empty_index();
    idx.override_edges.insert(
        "onCreate".to_string(),
        vec![
            ("/MyActivity.kt".to_string(), "MyActivity".to_string()),
            ("/BaseFragment.kt".to_string(), "BaseFragment".to_string()),
        ],
    );

    let g = SymbolGraph::new(&idx);
    let overrides = g.overrides_of("onCreate");
    assert_eq!(overrides.len(), 2);
    assert!(overrides.contains(&("/MyActivity.kt".to_string(), "MyActivity".to_string())));
    assert!(overrides.contains(&("/BaseFragment.kt".to_string(), "BaseFragment".to_string())));
}

#[test]
fn overrides_of_returns_empty_for_non_overridden_method() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let overrides = g.overrides_of("helper");
    assert!(overrides.is_empty());
}

// ── stats ────────────────────────────────────────────────────────────────

#[test]
fn stats_empty_index_all_zeroes() {
    let idx = empty_index();
    let g = SymbolGraph::new(&idx);
    let s = g.stats();
    assert_eq!(s.call_edges, 0);
    assert_eq!(s.import_edges, 0);
    assert_eq!(s.override_edges, 0);
    assert_eq!(s.supertype_edges, 0);
    assert_eq!(s.subtype_edges, 0);
}

#[test]
fn stats_counts_populated_edges() {
    let idx = empty_index();
    idx.call_edges.insert("a".to_string(), vec![]);
    idx.call_edges.insert("b".to_string(), vec![]);
    idx.import_edges.insert("c".to_string(), vec![]);
    idx.supertypes_index.insert("d".to_string(), vec![]);
    idx.subtypes.insert("e".to_string(), vec![]);

    let g = SymbolGraph::new(&idx);
    let s = g.stats();
    assert_eq!(s.call_edges, 2);
    assert_eq!(s.import_edges, 1);
    assert_eq!(s.override_edges, 0);
    assert_eq!(s.supertype_edges, 1);
    assert_eq!(s.subtype_edges, 1);
}

#[test]
fn stats_all_edges_have_distinct_counts() {
    let idx = empty_index();
    idx.call_edges.insert("a".to_string(), vec![]);
    idx.import_edges.insert("b".to_string(), vec![]);
    idx.import_edges.insert("c".to_string(), vec![]);
    idx.override_edges.insert("d".to_string(), vec![]);
    idx.override_edges.insert("e".to_string(), vec![]);
    idx.override_edges.insert("f".to_string(), vec![]);

    let g = SymbolGraph::new(&idx);
    let s = g.stats();
    assert_eq!(s.call_edges, 1);
    assert_eq!(s.import_edges, 2);
    assert_eq!(s.override_edges, 3);
    assert_eq!(s.supertype_edges, 0);
    assert_eq!(s.subtype_edges, 0);
}
