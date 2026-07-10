//! Tests for call graph CLI commands (`callers` and `callees`).

use std::sync::Arc;

use tempfile::TempDir;
use tower_lsp::lsp_types::Url;

use crate::indexer::Indexer;

fn build_test_index() -> (TempDir, Arc<Indexer>, Url, Url) {
    let dir = TempDir::new().expect("temp dir");
    let main_path = dir.path().join("Main.kt");
    let helper_path = dir.path().join("Helper.kt");

    let main_content = r#"
package com.example

fun helper(): String {
    return "ok"
}

fun caller(): String {
    return helper()
}

fun transitiveCaller(): String {
    return caller()
}
"#;

    let helper_content = r#"
package com.example

fun externalHelper(): String {
    return "from external"
}

fun externalCaller(): String {
    return externalHelper()
}
"#;

    std::fs::write(&main_path, main_content).unwrap();
    std::fs::write(&helper_path, helper_content).unwrap();

    let idx = Arc::new(Indexer::new());
    let main_uri = Url::from_file_path(&main_path).unwrap();
    let helper_uri = Url::from_file_path(&helper_path).unwrap();
    idx.index_content(&main_uri, main_content);
    idx.index_content(&helper_uri, helper_content);

    (dir, idx, main_uri, helper_uri)
}

fn kotlin_parser() -> tree_sitter::Parser {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_kotlin::LANGUAGE.into())
        .ok();
    parser
}

#[test]
fn extract_callee_name_simple() {
    let (_dir, _idx, _main_uri, _helper_uri) = build_test_index();

    let mut parser = kotlin_parser();
    let source = "fun test() { helper() }";
    let tree = parser.parse(source, None).unwrap();

    let root = tree.root_node();
    let mut stack = vec![root];
    let mut found = false;
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            let name = super::extract_callee_name(&node, source);
            assert_eq!(
                name, "helper",
                "should extract 'helper' from call_expression"
            );
            found = true;
            break;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    assert!(found, "should find call_expression in test source");
}

#[test]
fn extract_callee_name_navigation() {
    let mut parser = kotlin_parser();
    let source = "fun test() { obj.method() }";
    let tree = parser.parse(source, None).unwrap();

    let root = tree.root_node();
    let mut stack = vec![root];
    let mut found = false;
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            let name = super::extract_callee_name(&node, source);
            assert_eq!(
                name, "method",
                "should extract 'method' from navigation call, got: '{name}'"
            );
            found = true;
            break;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    assert!(found, "should find call_expression in test source");
}

#[test]
fn collect_callee_names_from_function() {
    let mut parser = kotlin_parser();
    // Single-line functions to simplify line-based lookup.
    let source = "fun outer(): String { return inner() }\nfun inner(): String { return util() }\nfun util(): String { return \"util\" }";
    let tree = parser.parse(source, None).unwrap();

    let root = tree.root_node();
    let decl = super::find_function_decl_near(root, 0, source)
        .expect("should find outer() declaration near line 0");

    let names = super::collect_callee_names(&decl, source);
    assert!(
        names.contains(&"inner".to_string()),
        "outer() should call inner(); got: {names:?}"
    );
}

#[test]
fn keyword_filter_excludes_control_flow() {
    assert!(super::is_keyword("if"));
    assert!(super::is_keyword("return"));
    assert!(super::is_keyword("when"));
    assert!(super::is_keyword("class"));
    assert!(!super::is_keyword("myFunction"));
    assert!(!super::is_keyword("login"));
}

#[test]
fn find_function_decl_near_returns_nearest() {
    let mut parser = kotlin_parser();
    let source = r#"
fun a() {}
fun b() {}
fun c() {}
"#;
    let tree = parser.parse(source, None).unwrap();

    let root = tree.root_node();
    let found = super::find_function_decl_near(root, 1, source);
    assert!(found.is_some(), "should find nearest function near line 1");
}

#[test]
fn extract_function_name_and_pos_returns_correct_name() {
    let mut parser = kotlin_parser();
    let source = "fun myFunc(param: String): Int { return 0 }";
    let tree = parser.parse(source, None).unwrap();

    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            let result = super::extract_function_name_and_pos(&child, source);
            assert!(result.is_some(), "should extract function name and pos");
            let (name, _line, _col) = result.unwrap();
            assert_eq!(name, "myFunc", "should extract 'myFunc'");
            return;
        }
    }
    panic!("should find function_declaration");
}
