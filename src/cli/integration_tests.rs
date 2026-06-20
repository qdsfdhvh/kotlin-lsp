//! Integration tests for CLI commands.

use std::sync::Arc;

use tempfile::TempDir;

use crate::indexer::Indexer;
use tower_lsp::lsp_types::Url;

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
    let edits = crate::backend::rename::rename_in_scope(
        &lines, "x", "renamed", (0, lines.len()), false,
    );
    assert!(!edits.is_empty(), "should find references to 'x'");
    // Apply edits
    let result = crate::cli::edit::apply_text_edits_to_lines(&lines, &edits);
    let result_str = result.join("\n");
    assert!(result_str.contains("renamed"), "'x' should be renamed to 'renamed'");
    assert!(!result_str.contains("println(x)"), "println(x) should become println(renamed)");
}

#[test]
fn cli_rename_in_scope_skips_package() {
    let src = "package com.example\nfun example() = 1\n";
    let lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    // 'example' in package line should not be renamed
    let edits = crate::backend::rename::rename_in_scope(
        &lines, "example", "renamed", (0, lines.len()), false,
    );
    // Should only rename the 'example' in 'fun example()', not in 'package com.example'
    let result = crate::cli::edit::apply_text_edits_to_lines(&lines, &edits);
    let result_str = result.join("\n");
    assert!(result_str.contains("package com.example"), "package line should be unchanged");
    assert!(result_str.contains("fun renamed"), "function should be renamed");
}