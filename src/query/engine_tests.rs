//! Unit tests for WorkspaceQueryEngine — unified query API over Indexer + SymbolGraph.

use std::sync::Arc;

use crate::indexer::Indexer;
use crate::query::engine::WorkspaceQueryEngine;
use crate::types::FileData;

use tower_lsp::lsp_types::{Location, Position, Range, Url};

// ── helpers ──────────────────────────────────────────────────────────────

fn empty_engine() -> WorkspaceQueryEngine {
    WorkspaceQueryEngine::new(Arc::new(Indexer::new()))
}

fn location(uri: &str, line: u32, col: u32) -> Location {
    Location {
        uri: Url::parse(uri).unwrap(),
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + 5,
            },
        },
    }
}

fn engine_with_file(uri: &str, lines: Vec<String>) -> WorkspaceQueryEngine {
    let engine = empty_engine();
    let fd = FileData {
        lines: Arc::new(lines),
        ..FileData::default()
    };
    engine.index.files.insert(uri.to_string(), Arc::new(fd));
    engine
}

// ── find_definitions ─────────────────────────────────────────────────────

#[test]
fn find_definitions_returns_registered_definition() {
    let engine = empty_engine();
    let loc = location("file:///a.kt", 3, 0);
    engine
        .index
        .definitions
        .insert("MyClass".to_string(), vec![loc.clone()]);

    let result = engine.find_definitions("MyClass");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].uri.as_str(), "file:///a.kt");
}

#[test]
fn find_definitions_returns_empty_for_unknown_symbol() {
    let engine = empty_engine();
    let result = engine.find_definitions("NoSuchSymbol");
    assert!(result.is_empty());
}

#[test]
fn find_definitions_returns_multiple_locations() {
    let engine = empty_engine();
    let loc1 = location("file:///a.kt", 1, 0);
    let loc2 = location("file:///b.kt", 10, 0);
    engine
        .index
        .definitions
        .insert("bar".to_string(), vec![loc1.clone(), loc2.clone()]);

    let result = engine.find_definitions("bar");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].uri.as_str(), "file:///a.kt");
    assert_eq!(result[1].uri.as_str(), "file:///b.kt");
}

// ── callers_of / callees_of ──────────────────────────────────────────────

#[test]
fn callers_of_delegates_to_symbol_graph() {
    let engine = empty_engine();
    engine.index.call_edges.insert(
        "target".to_string(),
        vec![("/a.kt".to_string(), "caller".to_string())],
    );

    let callers = engine.callers_of("target");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].1, "caller");
}

#[test]
fn callers_of_returns_empty_for_uncalled_function() {
    let engine = empty_engine();
    let callers = engine.callers_of("uncalled");
    assert!(callers.is_empty());
}

#[test]
fn callees_of_reverse_lookup_from_edges() {
    let engine = empty_engine();
    engine.index.call_edges.insert(
        "bar".to_string(),
        vec![("/f.kt".to_string(), "foo".to_string())],
    );
    engine.index.call_edges.insert(
        "baz".to_string(),
        vec![("/f.kt".to_string(), "foo".to_string())],
    );

    let callees = engine.callees_of("foo");
    assert_eq!(callees.len(), 2);
}

#[test]
fn callees_of_returns_empty_when_no_calls() {
    let engine = empty_engine();
    let callees = engine.callees_of("hermit_function");
    assert!(callees.is_empty());
}

// ── supertypes_of / subtypes_of ──────────────────────────────────────────

#[test]
fn supertypes_of_returns_direct_supertypes() {
    let engine = empty_engine();
    engine.index.supertypes_index.insert(
        "Child".to_string(),
        vec![("Parent".to_string(), "/model.kt".to_string())],
    );

    let supers = engine.supertypes_of("Child");
    assert_eq!(supers.len(), 1);
    assert_eq!(supers[0].0, "Parent");
}

#[test]
fn supertypes_of_returns_empty_for_unknown_type() {
    let engine = empty_engine();
    let supers = engine.supertypes_of("Orphan");
    assert!(supers.is_empty());
}

#[test]
fn subtypes_of_returns_registered_subtypes() {
    let engine = empty_engine();
    let sub_loc = location("file:///child.kt", 5, 0);
    engine
        .index
        .subtypes
        .insert("Parent".to_string(), vec![sub_loc.clone()]);

    let subtypes = engine.subtypes_of("Parent");
    assert_eq!(subtypes.len(), 1);
    assert_eq!(subtypes[0].uri.as_str(), "file:///child.kt");
}

#[test]
fn subtypes_of_returns_empty_for_unknown_type() {
    let engine = empty_engine();
    let subtypes = engine.subtypes_of("Final");
    assert!(subtypes.is_empty());
}

// ── all_symbol_names ─────────────────────────────────────────────────────

#[test]
fn all_symbol_names_returns_unique_sorted_names() {
    let engine = empty_engine();
    engine.index.definitions.insert("zeta".to_string(), vec![]);
    engine.index.definitions.insert("alpha".to_string(), vec![]);
    engine.index.definitions.insert("beta".to_string(), vec![]);
    // duplicate
    engine.index.definitions.insert("alpha".to_string(), vec![]);

    let names = engine.all_symbol_names();
    assert_eq!(
        names,
        vec!["alpha".to_string(), "beta".to_string(), "zeta".to_string()]
    );
}

#[test]
fn all_symbol_names_returns_empty_for_empty_index() {
    let engine = empty_engine();
    let names = engine.all_symbol_names();
    assert!(names.is_empty());
}

// ── word_at ──────────────────────────────────────────────────────────────

#[test]
fn word_at_returns_empty_for_unknown_uri() {
    let engine = empty_engine();
    let uri = Url::parse("file:///unknown.kt").unwrap();
    let word = engine.word_at(&uri, 1, 5);
    assert!(word.is_empty());
}

#[test]
fn word_at_extracts_identifier_at_position() {
    let engine = engine_with_file(
        "file:///test.kt",
        vec!["fun calculateTotal() = 42".to_string()],
    );
    let uri = Url::parse("file:///test.kt").unwrap();
    let word = engine.word_at(&uri, 1, 8);
    assert_eq!(word, "calculateTotal");
}

// ── file_data ────────────────────────────────────────────────────────────

#[test]
fn file_data_returns_none_for_unknown_uri() {
    let engine = empty_engine();
    let uri = Url::parse("file:///nope.kt").unwrap();
    let data = engine.file_data(&uri);
    assert!(data.is_none());
}

#[test]
fn file_data_returns_file_data_for_known_uri() {
    let engine = engine_with_file("file:///test.kt", vec!["package com.example".to_string()]);
    let uri = Url::parse("file:///test.kt").unwrap();
    let data = engine.file_data(&uri);
    assert!(data.is_some());
    assert_eq!(data.unwrap().lines[0], "package com.example");
}

// ── definition_locations (includes JAR defs) ─────────────────────────────

#[test]
fn definition_locations_includes_jar_definitions() {
    let engine = empty_engine();
    let loc = location("file:///a.kt", 3, 0);
    engine
        .index
        .definitions
        .insert("MyClass".to_string(), vec![loc.clone()]);
    engine
        .index
        .jar_definitions
        .insert("MyClass".to_string(), vec![location("jar:///lib.kt", 1, 0)]);

    let result = engine.definition_locations("MyClass");
    assert_eq!(
        result.len(),
        2,
        "must include both workspace and JAR definitions"
    );
    let uris: Vec<&str> = result.iter().map(|l| l.uri.as_str()).collect();
    assert!(uris.contains(&"file:///a.kt"));
    assert!(uris.contains(&"jar:///lib.kt"));
}

#[test]
fn definition_locations_returns_empty_for_unknown() {
    let engine = empty_engine();
    let result = engine.definition_locations("NoSuchSymbol");
    assert!(result.is_empty());
}

#[test]
fn definition_locations_falls_back_to_jar_only() {
    let engine = empty_engine();
    engine.index.jar_definitions.insert(
        "JavaClass".to_string(),
        vec![location("jar:///rt.kt", 0, 0)],
    );

    let result = engine.definition_locations("JavaClass");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].uri.as_str(), "jar:///rt.kt");
}

// ── file_by_uri_str ──────────────────────────────────────────────────────

#[test]
fn file_by_uri_str_returns_data_for_known_uri_str() {
    let engine = engine_with_file("file:///foo.kt", vec!["val x = 1".to_string()]);
    let fd = engine.file_by_uri_str("file:///foo.kt");
    assert!(fd.is_some());
    assert_eq!(fd.unwrap().lines[0], "val x = 1");
}

#[test]
fn file_by_uri_str_returns_none_for_unknown_uri_str() {
    let engine = empty_engine();
    let fd = engine.file_by_uri_str("file:///nonexistent.kt");
    assert!(fd.is_none());
}

// ── mem_lines_for ────────────────────────────────────────────────────────

#[test]
fn mem_lines_for_returns_lines_for_known_uri() {
    let engine = engine_with_file(
        "file:///lines.kt",
        vec!["line1".to_string(), "line2".to_string()],
    );
    let lines = engine.mem_lines_for("file:///lines.kt");
    assert!(lines.is_some());
    let lines = lines.unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
}

#[test]
fn mem_lines_for_returns_none_for_unknown_uri() {
    let engine = empty_engine();
    let lines = engine.mem_lines_for("file:///ghost.kt");
    assert!(lines.is_none());
}
