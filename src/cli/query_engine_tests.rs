//! Unit tests for IndexQueryEngine — the QueryEngine trait implementation.

use crate::cli::args::ResultFilters;
use crate::cli::query_engine::{IndexQueryEngine, QueryEngine};

use tower_lsp::lsp_types::{Location, Position, Range, Url};

// ── helpers ──────────────────────────────────────────────────────────────

fn test_engine(uri: &str, source: &str) -> (IndexQueryEngine, Url) {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let u = Url::parse(&format!("file:///test{uri}")).unwrap();
    idx.index_content(&u, source);
    let engine = IndexQueryEngine::new(idx);
    (engine, u)
}

// ── definitions ──────────────────────────────────────────────────────────

#[test]
fn definitions_returns_registered_symbol() {
    let (engine, _uri) = test_engine("/Def.kt", "package com.example\nclass MyClass");
    let locs = engine.definitions("MyClass");
    assert!(!locs.is_empty());
    assert_eq!(locs[0].range.start.line, 1); // line 2 of source (0-indexed)
}

#[test]
fn definitions_returns_empty_for_unknown() {
    let (engine, _uri) = test_engine("/Def.kt", "class Foo");
    let locs = engine.definitions("Bar");
    assert!(locs.is_empty());
}

// ── references ───────────────────────────────────────────────────────────

#[test]
fn references_returns_locations() {
    let (engine, _uri) = test_engine(
        "/Ref.kt",
        "package com.example\nfun topLevel() {}\nfun entry() { topLevel() }",
    );
    let locs = engine.references("topLevel");
    assert!(
        locs.iter().any(|loc| loc.range.start.line == 2),
        "must include invocation, not only definition"
    );
}

#[test]
fn references_returns_empty_for_unknown() {
    let (engine, _uri) = test_engine("/Ref.kt", "fun x() {}");
    let locs = engine.references("missing");
    assert!(locs.is_empty());
}

#[test]
fn references_resolve_mem_only_uris_without_native_paths() {
    // A non-file URI has no native path on any platform (file://host would
    // instead be a valid UNC path on Windows). In-memory content must still
    // supply occurrences, as for drive-less file:///test/… URIs on Windows.
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let uri = Url::parse("untitled:Ref.kt").expect("mem-only URI");
    assert!(
        uri.to_file_path().is_err(),
        "fixture URI must have no native filesystem path"
    );
    idx.index_content(
        &uri,
        "package com.example\nfun topLevel() {}\nfun entry() { topLevel() }",
    );
    let engine = IndexQueryEngine::new(idx);
    let locs = engine.references("topLevel");
    assert!(
        locs.iter().any(|loc| loc.range.start.line == 2),
        "mem-only occurrences must not require a native filesystem path"
    );
}

// ── find_symbols ─────────────────────────────────────────────────────────

#[test]
fn find_symbols_finds_by_name() {
    let (engine, _uri) = test_engine("/FindSym.kt", "package com.example\nclass FindMe");
    let results = engine.find_symbols("FindMe", &ResultFilters::default());
    assert!(!results.is_empty());
    assert_eq!(results[0].name, "FindMe");
}

#[test]
fn find_symbols_returns_empty_for_unknown() {
    let (engine, _uri) = test_engine("/FindSym.kt", "class X");
    let results = engine.find_symbols("NoMatch", &ResultFilters::default());
    assert!(results.is_empty());
}

#[test]
fn find_symbols_respects_limit() {
    let (engine, _uri) = test_engine(
        "/Limit.kt",
        "package com.example\nclass A\nclass B\nclass C",
    );
    let filters = ResultFilters {
        limit: Some(2),
        ..Default::default()
    };
    let results = engine.find_symbols("A", &filters);
    // A appears only once, but limit should still work
    assert!(results.len() <= 2);
}

// ── hover ────────────────────────────────────────────────────────────────

#[test]
fn hover_finds_symbol_from_indexed_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("Hover.kt");
    let content = "package com.example\nclass HoverTarget";
    std::fs::write(&path, content).unwrap();

    let uri = Url::from_file_path(&path).unwrap();
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    idx.index_content(&uri, content);
    let engine = IndexQueryEngine::new(idx);

    // "HoverTarget" occupies cols 7-18; hover works with cursor past the word end
    let result = engine.hover(&path, 2, 19);
    assert!(result.is_some(), "hover should find HoverTarget");
    assert_eq!(result.unwrap().name, "HoverTarget");
}

// ── summarize ────────────────────────────────────────────────────────────

#[test]
fn summarize_returns_symbol_entries() {
    let (engine, _uri) = test_engine("/Sum.kt", "package com.example\nclass SummaryTarget");
    let entries = engine.summarize("SummaryTarget");
    assert!(entries.is_some());
    let entries = entries.unwrap();
    assert!(!entries.is_empty());
    assert!(entries.iter().any(|e| e.name == "SummaryTarget"));
}

#[test]
fn summarize_returns_none_for_unknown() {
    let (engine, _uri) = test_engine("/Sum.kt", "class X");
    let entries = engine.summarize("NoSuchThing");
    assert!(entries.is_none());
}

// ── callers_of ───────────────────────────────────────────────────────────

#[test]
fn callers_of_returns_registered_callers() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    idx.call_edges.insert(
        "bar".to_string(),
        vec![("/a.kt".to_string(), "foo".to_string())],
    );
    let engine = IndexQueryEngine::new(idx);
    let callers = engine.callers_of("bar");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].1, "foo");
}

#[test]
fn callers_of_returns_empty_for_uncalled() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let engine = IndexQueryEngine::new(idx);
    let callers = engine.callers_of("ghost");
    assert!(callers.is_empty());
}

// ── implementations_of ───────────────────────────────────────────────────

#[test]
fn implementations_of_returns_subtypes() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let loc = Location {
        uri: Url::parse("file:///impl.kt").unwrap(),
        range: Range {
            start: Position {
                line: 3,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 10,
            },
        },
    };
    idx.subtypes.insert("Base".to_string(), vec![loc]);
    let engine = IndexQueryEngine::new(idx);
    let impls = engine.implementations_of("Base");
    assert_eq!(impls.len(), 1);
    assert_eq!(impls[0].uri.as_str(), "file:///impl.kt");
}

#[test]
fn implementations_of_returns_empty_for_unknown() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let engine = IndexQueryEngine::new(idx);
    let impls = engine.implementations_of("Nothing");
    assert!(impls.is_empty());
}

// ── all_symbol_names ─────────────────────────────────────────────────────

#[test]
fn all_symbol_names_returns_registered_names() {
    let (engine, _uri) = test_engine("/Names.kt", "package com.example\nclass Alpha\nclass Beta");
    let names = engine.all_symbol_names();
    assert!(names.contains(&"Alpha".to_string()));
    assert!(names.contains(&"Beta".to_string()));
}

#[test]
fn all_symbol_names_returns_empty_for_empty_index() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let engine = IndexQueryEngine::new(idx);
    let names = engine.all_symbol_names();
    assert!(names.is_empty());
}

// ── importing_files ──────────────────────────────────────────────────────

#[test]
fn importing_files_finds_files_with_matching_import() {
    let (engine, _uri) = test_engine(
        "/ImportTest.kt",
        "package com.example\nimport com.lib.Foo\nimport com.lib.Bar\nclass UsesBoth",
    );
    let files = engine.importing_files("Foo");
    assert!(!files.is_empty());
}

#[test]
fn importing_files_returns_empty_for_no_match() {
    let (engine, _uri) = test_engine("/ImportEmpty.kt", "package com.example\nclass NoImports");
    let files = engine.importing_files("com.nonexistent.Whatever");
    assert!(files.is_empty());
}

#[test]
fn references_unicode_names_have_exact_utf16_start_and_end_ranges() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("Unicode.kt");
    let source = "fun café() {}\nfun entry() { val 文 = \"😀\"; café(); café() }\n";
    std::fs::write(&path, source).expect("source");
    let uri = Url::from_file_path(&path).expect("URI");
    let index = std::sync::Arc::new(crate::indexer::Indexer::new());
    index.index_content(&uri, source);
    let engine = IndexQueryEngine::new(index);
    // Zero-based UTF-16: `fun entry() { ` = 14, `val 文 = ` = 8,
    // `"😀"; ` = 6, so the first café starts at 28. Each `café(); ` = 8.
    let locations = engine.references("café");
    assert_eq!(
        locations,
        vec![
            Location::new(
                uri.clone(),
                Range::new(Position::new(0, 4), Position::new(0, 8))
            ),
            Location::new(
                uri.clone(),
                Range::new(Position::new(1, 28), Position::new(1, 32))
            ),
            Location::new(uri, Range::new(Position::new(1, 36), Position::new(1, 40))),
        ]
    );
    let calls = engine
        .references_with_kind("café", Some("call"))
        .expect("call filter");
    assert_eq!(calls, locations[1..]);
}
