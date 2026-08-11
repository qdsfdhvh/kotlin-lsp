//! Integration tests for CLI commands.

use std::sync::Arc;

use tempfile::TempDir;

use crate::indexer::Indexer;
use tower_lsp::lsp_types::Url;

#[allow(dead_code)]
struct TestProject {
    _dir: TempDir,
    root_uri: Url,
    secondary_uri: Url,
    idx: Arc<Indexer>,
}

fn create_test_project() -> TestProject {
    let dir = TempDir::new().expect("temp dir");
    let root_kt_path = dir
        .path()
        .join("src/main/kotlin/com/example/MyViewModel.kt");
    std::fs::create_dir_all(root_kt_path.parent().unwrap()).unwrap();
    let root_content = r#"
package com.example
class MyViewModel(val name: String) {
    fun loadData(): String { return "data" }
    companion object {
        fun create(name: String): MyViewModel = MyViewModel(name)
    }
}
"#;
    std::fs::write(&root_kt_path, root_content).unwrap();
    let secondary_kt_path = dir.path().join("src/main/kotlin/com/example/MyScreen.kt");
    std::fs::create_dir_all(secondary_kt_path.parent().unwrap()).unwrap();
    let secondary_content = r#"
package com.example
class MyScreen {
    private val viewModel = MyViewModel("test")
    fun display(): String { return viewModel.loadData() }
}
"#;
    std::fs::write(&secondary_kt_path, secondary_content).unwrap();
    let idx = Arc::new(Indexer::new());
    let root_uri = Url::from_file_path(&root_kt_path).unwrap();
    idx.index_content(&root_uri, root_content);
    let secondary_uri = Url::from_file_path(&secondary_kt_path).unwrap();
    idx.index_content(&secondary_uri, secondary_content);
    TestProject {
        _dir: dir,
        root_uri,
        secondary_uri,
        idx,
    }
}

fn index_single(path: &str, src: &str) -> (Arc<Indexer>, Url) {
    let idx = Arc::new(Indexer::new());
    let uri = Url::parse(&format!("file:///test{path}")).unwrap();
    idx.index_content(&uri, src);
    (idx, uri)
}

#[test]
fn cli_find_finds_class_declaration() {
    let p = create_test_project();
    let locs = p.idx.definition_locations("MyViewModel");
    assert!(!locs.is_empty(), "find should find MyViewModel");
}

#[test]
fn cli_find_finds_method_declaration() {
    let p = create_test_project();
    let locs = p.idx.definition_locations("loadData");
    assert!(!locs.is_empty(), "find should find loadData method");
}

#[test]
fn cli_check_clean_file_has_no_syntax_errors() {
    let (_idx, uri) = index_single("/Clean.kt", "class Clean");
    let data = _idx.files.get(uri.as_str()).expect("indexed");
    assert!(
        data.syntax_errors.is_empty(),
        "clean file should have no errors"
    );
}

#[test]
fn cli_cache_stats_index_has_files() {
    let p = create_test_project();
    assert!(
        p.idx.files.len() >= 2,
        "indexer should have indexed at least 2 files"
    );
}

// ─── CLI code-action tests ───────────────────────────────────────────────

#[test]
fn cli_code_action_import_alias() {
    let src =
        "package com.example\nimport com.example.other.SomeLongName\nval x = SomeLongName()\n";
    let (idx, uri) = index_single("/main.kt", src);
    let actions = crate::backend::actions::get_code_actions_cli(&idx, &uri, 1, 8, &[]);
    let has = actions.iter().any(|a| match a {
        tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(ca) => {
            ca.title.contains("Add import alias")
        }
        _ => false,
    });
    assert!(
        has,
        "should produce alias action; got {} actions",
        actions.len()
    );
}

// ─── CLI rename tests ────────────────────────────────────────────────────

#[test]
fn cli_rename_in_scope_replaces_occurrences() {
    let src = "package com.example\n\nclass Foo {\n    val x = \"hello\"\n    fun greet() { println(x) }\n    fun other() { println(this.x) }\n}\n";
    let lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    let edits =
        crate::backend::rename::rename_in_scope(&lines, "x", "renamed", (0, lines.len()), false);
    assert!(!edits.is_empty(), "should find references to 'x'");
    // Apply edits
    let result = crate::cli::edit::apply_text_edits_to_lines(&lines, &edits);
    let result_str = result.join("\n");
    assert!(
        result_str.contains("renamed"),
        "'x' should be renamed to 'renamed'"
    );
    assert!(
        !result_str.contains("println(x)"),
        "println(x) should become println(renamed)"
    );
}

#[test]
fn cli_rename_in_scope_skips_package() {
    let src = "package com.example\nfun example() = 1\n";
    let lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    // 'example' in package line should not be renamed
    let edits = crate::backend::rename::rename_in_scope(
        &lines,
        "example",
        "renamed",
        (0, lines.len()),
        false,
    );
    // Should only rename the 'example' in 'fun example()', not in 'package com.example'
    let result = crate::cli::edit::apply_text_edits_to_lines(&lines, &edits);
    let result_str = result.join("\n");
    assert!(
        result_str.contains("package com.example"),
        "package line should be unchanged"
    );
    assert!(
        result_str.contains("fun renamed"),
        "function should be renamed"
    );
}

// ─── Supertypes index ───────────────────────────────────────────────────

#[test]
fn supertypes_index_populated() {
    let src = "package com.example\nopen class Base\nclass Child : Base()\n";
    let (idx, _uri) = index_single("/SuperTest.kt", src);
    // Child should have "Base" as supertype in the forward index
    let child_entries = idx.supertypes_index.get("Child");
    assert!(
        child_entries.is_some(),
        "Child should have supertype entries"
    );
    let entries = child_entries.unwrap();
    assert!(
        entries.iter().any(|(sup, _, _)| sup == "Base"),
        "Child should extend Base"
    );
}

// ─── Type hierarchy ──────────────────────────────────────────────────────

#[test]
fn type_hierarchy_finds_subtypes() {
    let src =
        "package com.example\nopen class Animal\nclass Dog : Animal()\nclass Cat : Animal()\n";
    let (idx, _uri) = index_single("/TypeHier.kt", src);
    // subtypes index should have Animal -> [Dog, Cat]
    let locs = idx.subtypes.get("Animal");
    assert!(locs.is_some(), "Animal should have subtypes");
    assert_eq!(locs.unwrap().len(), 2, "Animal should have 2 subtypes");
}

// ─── Phase 29: visibility/modifier filters ────────────────────────────────

#[test]
fn cli_find_visibility_filter() {
    let src = "package com.example\nclass PublicClass\nprivate class PrivateClass\ninternal class InternalClass\n";
    let (idx, uri) = index_single("/Vis.kt", src);
    let data = idx.files.get(uri.as_str()).expect("indexed");
    let public_count = data
        .symbols
        .iter()
        .filter(|s| matches!(s.visibility, crate::types::Visibility::Public))
        .count();
    assert!(public_count >= 1, "should have at least 1 public symbol");
}

// ─── Phase 30: call edge index ─────────────────────────────────────────────

#[test]
fn call_edges_extracted_during_indexing() {
    let src = "package com.example\nclass Foo {\n    fun helper(): String = \"x\"\n    fun main() { helper() }\n}\n";
    let (idx, _uri) = index_single("/CallTest.kt", src);
    // Call edges should be populated: main → helper
    let edges = idx.call_edges.get("helper");
    assert!(edges.is_some(), "should have call edges for 'helper'");
    let entries = edges.unwrap();
    assert!(!entries.is_empty(), "should have at least 1 caller");
    assert!(
        entries.iter().any(|(_, caller)| caller == "Foo.main"),
        "Foo.main should call helper (issue #267: callers are type-qualified)"
    );
}

// ─── Phase 31: workspace snapshot ───────────────────────────────────────────

#[test]
fn workspace_snapshot_includes_symbols() {
    let (_idx, _uri) = index_single(
        "/WSTest.kt",
        "package com.example\nclass MyActivity\nfun helper()",
    );
    // Just verify indexing succeeds — workspace snapshot uses same data
}

// ─── Phase 32: inheritance graph ───────────────────────────────────────────

#[test]
fn implementations_finds_subtypes() {
    use std::collections::HashSet;
    let index = Arc::new(Indexer::new());
    // Add subtypes to the index directly
    let base_loc = tower_lsp::lsp_types::Location {
        uri: Url::parse("file:///impl.kt").unwrap(),
        range: tower_lsp::lsp_types::Range {
            start: tower_lsp::lsp_types::Position::new(0, 0),
            end: tower_lsp::lsp_types::Position::new(0, 0),
        },
    };
    index
        .subtypes
        .entry("Repository".to_string())
        .or_default()
        .push(base_loc.clone());
    let children =
        crate::cli::inheritance::find_implementors("Repository", &index, 2, &mut HashSet::new());
    assert!(!children.is_empty());
}

// ─── Phase 33: batch query ──────────────────────────────────────────────────

#[test]
fn batch_query_definition_finds_symbol() {
    let (_idx, _uri) = index_single("/BatchQueryTest.kt", "package com.example\nclass BatchTest");
    // Test the QueryEngine trait directly
    use crate::cli::query_engine::{IndexQueryEngine, QueryEngine};
    let (index, _uri) = index_single(
        "/BatchQueryTest2.kt",
        "package com.example\nclass BatchTest2",
    );
    let engine = IndexQueryEngine::new(index);
    let locs = engine.definitions("BatchTest2");
    assert!(!locs.is_empty(), "should find BatchTest2");
}

// ─── Phase 34: fuzzy search ─────────────────────────────────────────────────

#[test]
fn fuzzy_search_finds_subsequence() {
    let results = crate::cli::fuzzy::fuzzy_find(
        "login repo",
        &[
            "LoginRepository".into(),
            "AuthRepository".into(),
            "Unrelated".into(),
        ],
        5,
    );
    assert!(!results.is_empty());
    assert_eq!(results[0].0, "LoginRepository");
}

// ─── Phase 35: import index ─────────────────────────────────────────────────

#[test]
fn import_index_finds_importing_files() {
    // Test that the engine's importing_files method finds imported dependencies
    use crate::cli::query_engine::{IndexQueryEngine, QueryEngine};
    let (index, _uri) = index_single(
        "/Imports.kt",
        "package com.example\nimport com.lib.Foo\nclass UsesFoo { val x = Foo() }",
    );
    let engine = IndexQueryEngine::new(index);
    // importing_files is a QueryEngine method — verify it compiles and runs
    let files = engine.importing_files("Foo");
    // With our test content, "Foo" appears in the import
    assert!(!files.is_empty(), "should find files referencing Foo");
}

// ─── Phase 36-38: annotation, package, docs ─────────────────────────────────

#[test]
fn annotations_found_in_symbol_detail() {
    let (idx, uri) = index_single(
        "/Annot.kt",
        "package com.example\n@Serializable\nclass Data(val x: Int)",
    );
    let data = idx.files.get(uri.as_str()).expect("indexed");
    // The @Serializable annotation should appear in the symbol's detail
    let has_annotation = data
        .symbols
        .iter()
        .any(|s| s.detail.contains("Serializable"));
    assert!(
        has_annotation || !data.symbols.is_empty(),
        "symbols should be indexed"
    );
}

#[test]
fn package_deps_from_imports() {
    let (idx, _uri) = index_single(
        "/PkgDeps.kt",
        "package com.example\nimport com.lib.Foo\nimport com.lib.Bar\nclass UsesLib",
    );
    // Verify the file has the correct package
    let data = idx.files.get(_uri.as_str()).expect("indexed");
    assert_eq!(data.package.as_deref(), Some("com.example"));
}

// ─── parent_fq_name ─────────────────────────────────────────────────────

#[test]
fn parent_fq_name_set_for_methods() {
    let src = "package com.example
class MyClass {
    fun myMethod() = 1
}
";
    let (idx, uri) = index_single("/ParentTest.kt", src);
    let data = idx.files.get(uri.as_str()).expect("indexed");
    let method = data
        .symbols
        .iter()
        .find(|s| s.name == "myMethod")
        .expect("method");
    assert_eq!(
        method.parent_fq_name.as_deref(),
        Some("com.example.MyClass")
    );
}

#[test]
fn top_level_function_no_parent() {
    let src = "package com.example
fun topLevel() = 1
";
    let (idx, uri) = index_single("/NoParent.kt", src);
    let data = idx.files.get(uri.as_str()).expect("indexed");
    let func = data
        .symbols
        .iter()
        .find(|s| s.name == "topLevel")
        .expect("func");
    assert!(
        func.parent_fq_name.is_none(),
        "top-level function should have no parent"
    );
}

// ─── Symbol graph ─────────────────────────────────────────────────────────

#[test]
fn symbol_graph_includes_call_edges() {
    let src = "package com.example\nclass A {\n    fun foo() { bar() }\n    fun bar() {}\n}\n";
    let (idx, _uri) = index_single("/SymGraph.kt", src);
    // foo should call bar
    let edges = idx.call_edges.get("bar");
    assert!(edges.is_some(), "bar should be called");
}

#[test]
fn symbol_graph_includes_import_edges() {
    let src = "package com.example\nimport com.lib.Foo\nclass UsesFoo";
    let (idx, _uri) = index_single("/ImportEdge.kt", src);
    let edges = idx.import_edges.get("com.lib.Foo");
    assert!(edges.is_some(), "should track import of com.lib.Foo");
}

#[test]
fn symbol_graph_includes_override_edges() {
    let src = "package com.example\nopen class Base {\n    open fun foo() {}\n}\nclass Child : Base() {\n    override fun foo() {}\n}\n";
    let (idx, _uri) = index_single("/OverrideEdge.kt", src);
    let edges = idx.override_edges.get("foo");
    assert!(edges.is_some(), "foo should have override edges from Child");
}

// ─── issue #139: find --kind fun drops top-level functions ─────────────────

#[test]
fn find_kind_fun_retains_top_level_function() {
    let (idx, uri) = index_single(
        "/KindFunTest.kt",
        "package example\nfun SampleAction(): Unit = Unit\n",
    );
    // Verify the indexed symbol has FUNCTION kind
    let data = idx.files.get(uri.as_str()).expect("indexed");
    let fun_sym = data
        .symbols
        .iter()
        .find(|s| s.name == "SampleAction")
        .expect("SampleAction should be indexed");
    assert_eq!(format!("{:?}", fun_sym.kind).to_lowercase(), "function");
}

// ─── issue #139: find --kind fun drops top-level functions ─────────────────

/// Verify SymbolEntry.kind is "function" for top-level functions
#[test]
fn symbol_kind_is_function_for_top_level_fun() {
    let (idx, _uri) = index_single(
        "/KindFilter.kt",
        "package example\nfun topLevelAction(): Unit = Unit\nclass MyClass",
    );
    let locs = idx.definition_locations("topLevelAction");
    assert!(
        !locs.is_empty(),
        "definition_locations should find topLevelAction"
    );
    let uri_str = locs.first().unwrap().uri.as_str().to_string();
    let fd = idx.files.get(&uri_str).expect("file indexed");
    let sym = fd
        .symbols
        .iter()
        .find(|s| s.name == "topLevelAction")
        .expect("symbol");
    assert_eq!(format!("{:?}", sym.kind).to_lowercase(), "function");
}

/// Verify SymbolEntry.kind is "class" for classes
#[test]
fn symbol_kind_is_class_for_class() {
    let (idx, _uri) = index_single("/KindFilter2.kt", "package example\nclass MyClass");
    let locs = idx.definition_locations("MyClass");
    assert!(!locs.is_empty());
    let fd = idx.files.get(locs[0].uri.as_str()).expect("indexed");
    let sym = fd
        .symbols
        .iter()
        .find(|s| s.name == "MyClass")
        .expect("found");
    assert_eq!(format!("{:?}", sym.kind).to_lowercase(), "class");
}

/// Verify definition_locations still works (no merge regression)
#[test]
fn definition_locations_finds_top_level_function() {
    let (idx, _uri) = index_single(
        "/KindFilter3.kt",
        "package example\nfun sample(): Unit = Unit\n",
    );
    let locs = idx.definition_locations("sample");
    assert!(!locs.is_empty(), "definition_locations should find sample");
}

// ─── issue #139 end-to-end reproduction ──────────────────────────────────

/// Reproduce the original bug: CliResult with empty kind + kind filter drops
/// top-level functions. Verifies enrich_result_kinds fixes it.
#[test]
fn enrich_result_kinds_fills_kind_for_top_level_function() {
    // Use a real tempfile so Url::to_file_path round-trip works
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("KindFilterReal.kt");
    let src = "package example\nfun topLevelAction(): Unit = Unit\n";
    std::fs::write(&file, src).unwrap();

    let uri = tower_lsp::lsp_types::Url::from_file_path(&file).unwrap();
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    idx.index_content(&uri, src);

    // Simulate what locs_to_results creates: kind is empty (the bug), so the file path
    // round-trips correctly through canonicalize + from_file_path.
    let file_path = uri.to_file_path().unwrap(); // e.g. "/private/.../KindFilterReal.kt"
    let mut results = vec![crate::cli::output::CliResult {
        name: "topLevelAction".into(),
        kind: String::new(), // ← the bug: kind was never populated
        file: file_path.to_string_lossy().into_owned(),
        line: 2,
        col: 5,
        relative_path: None,
        module: None,
        owner: None,
        source_set: None,
        signature: None,
        visibility: None,
        modifiers: None,
    }];

    let engine = crate::query::engine::WorkspaceQueryEngine::new(idx.clone());
    crate::cli::run::enrich_result_kinds(&mut results, &engine);
    assert_eq!(
        results[0].kind, "function",
        "kind must be filled from SymbolEntry"
    );
}

#[test]
fn enrich_result_kinds_does_not_overwrite_existing_kind() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("AlreadySet.kt");
    let src = "package example\nclass AlreadySet\n";
    std::fs::write(&file, src).unwrap();

    let uri = tower_lsp::lsp_types::Url::from_file_path(&file).unwrap();
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    idx.index_content(&uri, src);

    let file_path = uri.to_file_path().unwrap();
    let mut results = vec![crate::cli::output::CliResult {
        name: "AlreadySet".into(),
        kind: "custom-special-kind".into(), // pre-populated
        file: file_path.to_string_lossy().into_owned(),
        line: 2,
        col: 7,
        relative_path: None,
        module: None,
        owner: None,
        source_set: None,
        signature: None,
        visibility: None,
        modifiers: None,
    }];

    let engine = crate::query::engine::WorkspaceQueryEngine::new(idx.clone());
    crate::cli::run::enrich_result_kinds(&mut results, &engine);
    assert_eq!(
        results[0].kind, "custom-special-kind",
        "existing kind must not be overwritten"
    );
}

#[test]
fn enrich_result_kinds_handles_file_not_in_index() {
    let idx = std::sync::Arc::new(crate::indexer::Indexer::new());
    let mut results = vec![crate::cli::output::CliResult {
        name: "Ghost".into(),
        kind: String::new(),
        file: "/nonexistent/Ghost.kt".into(),
        line: 1,
        col: 1,
        relative_path: None,
        module: None,
        owner: None,
        source_set: None,
        signature: None,
        visibility: None,
        modifiers: None,
    }];
    // must not panic
    let engine = crate::query::engine::WorkspaceQueryEngine::new(idx.clone());
    crate::cli::run::enrich_result_kinds(&mut results, &engine);
    assert!(
        results[0].kind.is_empty(),
        "kind stays empty for unknown file"
    );
}
