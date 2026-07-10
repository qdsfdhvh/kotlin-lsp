//! Test finder — `find-test <file> <line> <col>` locates tests for a symbol.
//!
//! Searches by naming convention, import matching, and source set discovery.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Serialize)]
struct TestInfo {
    file: String,
    line: u32,
    test_name: Option<String>,
    convention: String,
}

#[derive(Debug, Serialize)]
struct TestResults {
    symbol: String,
    tests: Vec<TestInfo>,
}

pub(crate) async fn run_find_test(file: &Path, line: u32, col: u32, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, file);
    let index = crate::cli::run::build_index(&root, false).await;
    let uri = Url::from_file_path(file).expect("valid file path");

    let word = extract_word_at_position(&index, &uri, line, col);
    if word.is_empty() {
        eprintln!("No symbol at cursor");
        std::process::exit(1);
    }

    let tests = find_tests_for_symbol(&word, file, &root);

    if json {
        let results = TestResults {
            symbol: word,
            tests,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&results).expect("serialize JSON")
        );
    } else {
        if tests.is_empty() {
            println!("No tests found for `{word}`");
        } else {
            println!("Tests for `{word}`:");
            for t in &tests {
                let name = t.test_name.as_deref().unwrap_or("?");
                println!("  - {name} @ {}:{} ({})", t.file, t.line, t.convention);
            }
        }
    }
}

fn find_tests_for_symbol(name: &str, source_file: &Path, root: &Path) -> Vec<TestInfo> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    // Determine the source file's module context.
    let module_dir = find_module_src_dir(source_file);

    // Collect all test files in the project.
    let test_files = find_all_test_files(root);

    for test_file in &test_files {
        let _file_name = test_file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let file_stem = test_file.file_stem().and_then(|n| n.to_str()).unwrap_or("");

        let Ok(content) = std::fs::read_to_string(test_file) else {
            continue;
        };

        // Convention 1: file name matches (FooTest.kt, TestFoo.kt, FooTests.kt)
        let mut convention = String::new();

        if let Some(base) = file_stem.strip_suffix("Test") {
            if camel_eq(base, name) {
                convention = format!("{name}Test.kt naming");
            }
        } else if let Some(base) = file_stem.strip_prefix("Test") {
            if camel_eq(base, name) {
                convention = format!("Test{name}.kt naming");
            }
        }

        if convention.is_empty() && file_stem.to_lowercase().contains(&name.to_lowercase()) {
            convention = format!("{name} appears in filename");
        }

        // Convention 2: file imports the symbol's package
        if convention.is_empty() {
            if let Some(src_pkg) = extract_package(source_file) {
                if content.contains(&format!("import {src_pkg}.")) {
                    convention = "same-package import".to_string();
                } else if content.contains(&format!("import {src_pkg}")) {
                    convention = "package import".to_string();
                }
            }
        }

        // Convention 3: test file in corresponding test source set
        if convention.is_empty() {
            if let Some(ref _mod_dir) = module_dir {
                let test_path = test_file.to_string_lossy();
                if test_path.contains("/test/") || test_path.contains("/androidTest/") {
                    convention = "test source set".to_string();
                }
            }
        }

        if !convention.is_empty() {
            // Find specific test methods that reference the symbol.
            let test_methods = find_test_methods_for_symbol(&content, name);

            let key = test_file.display().to_string();
            if seen.insert(key.clone()) {
                if test_methods.is_empty() {
                    results.push(TestInfo {
                        file: key,
                        line: 1,
                        test_name: None,
                        convention,
                    });
                } else {
                    for (fn_name, fn_line) in test_methods {
                        results.push(TestInfo {
                            file: key.clone(),
                            line: fn_line,
                            test_name: Some(fn_name),
                            convention: convention.clone(),
                        });
                    }
                }
            }
        }
    }

    results
}

/// Find test method names that reference the given symbol.
fn find_test_methods_for_symbol(source: &str, name: &str) -> Vec<(String, u32)> {
    let mut methods = Vec::new();
    let _lang = crate::Language::from_path("test.kt");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_kotlin::LANGUAGE.into())
        .ok();
    let Some(tree) = parser.parse(source, None) else {
        return methods;
    };

    let root = tree.root_node();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_declaration" || node.kind() == "method_declaration" {
            let fn_name = first_child_simple_id(&node, source);
            let is_test =
                fn_name.starts_with("test") || fn_name.contains("Test") || fn_name.contains(name);
            if is_test {
                // Check if body contains a reference to the symbol
                // (tree-sitter-based, not raw text, to avoid comment false positives).
                if let Some(body) = find_body(&node) {
                    if body_contains_identifier(&body, name, source) {
                        let line = node.start_position().row as u32 + 1;
                        methods.push((fn_name, line));
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    methods
}
fn body_contains_identifier(body: &tree_sitter::Node<'_>, name: &str, source: &str) -> bool {
    let mut stack = vec![*body];
    while let Some(node) = stack.pop() {
        if node.kind() == "simple_identifier" {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                if text == name {
                    return true;
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn first_child_simple_id(node: &tree_sitter::Node, source: &str) -> String {
    for child in children(node) {
        if child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

fn find_body<'a>(decl: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    children(decl)
        .into_iter()
        .find(|&child| child.kind() == "function_body" || child.kind() == "class_body")
}

fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn camel_eq(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn extract_package(file: &Path) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    #[allow(clippy::manual_strip)]
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("package ") {
            return Some(trimmed["package ".len()..].trim().to_string());
        }
    }
    None
}

fn find_module_src_dir(file: &Path) -> Option<PathBuf> {
    let mut cur = file.parent()?;
    while let Some(parent) = cur.parent() {
        if cur
            .file_name()
            .map(|n| n == "main" || n == "test")
            .unwrap_or(false)
            && parent.file_name().map(|n| n == "src").unwrap_or(false)
        {
            return Some(parent.to_path_buf());
        }
        cur = parent;
    }
    None
}

fn find_all_test_files(root: &Path) -> Vec<PathBuf> {
    use std::process::Command;
    let mut cmd = Command::new("rg");
    cmd.args(["--files", "--glob", "**/test/**/*.kt"]);
    cmd.arg(root);

    let out = match cmd.output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let mut files: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .collect();

    // Also search for Test*.kt files in main src.
    let mut cmd2 = Command::new("rg");
    cmd2.args(["--files", "--glob", "*Test*"]);
    cmd2.arg(root);
    if let Ok(o) = cmd2.output() {
        files.extend(
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(PathBuf::from),
        );
    }

    files
}

fn extract_word_at_position(
    index: &std::sync::Arc<crate::indexer::Indexer>,
    uri: &Url,
    line: u32,
    col: u32,
) -> String {
    let lines = index.mem_lines_for(uri.as_str());
    lines
        .as_ref()
        .and_then(|l| {
            let li = line.saturating_sub(1) as usize;
            l.get(li).map(|ln| {
                crate::StrExt::word_at_utf16_col(ln.as_str(), col.saturating_sub(1) as usize)
            })
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_eq_matches_same_case() {
        assert!(camel_eq("LoginViewModel", "LoginViewModel"));
    }

    #[test]
    fn camel_eq_matches_case_insensitive() {
        assert!(camel_eq("LoginViewModel", "loginViewModel"));
        assert!(camel_eq("loginViewModel", "LoginViewModel"));
    }

    #[test]
    fn extract_package_from_source() {
        let dir = tempfile::TempDir::new().unwrap();
        let f = dir.path().join("Foo.kt");
        std::fs::write(&f, "package com.example\n\nclass Foo").unwrap();
        assert_eq!(extract_package(&f), Some("com.example".to_string()));
    }

    #[test]
    fn find_test_methods_detects_references() {
        let source = r#"
import org.junit.Test

class LoginViewModelTest {
    @Test
    fun testLogin() {
        val vm = LoginViewModel()
        assertNotNull(vm)
    }

    fun testLogout() {
        // no reference to LoginViewModel
    }
}
"#;
        let methods = find_test_methods_for_symbol(source, "LoginViewModel");
        // testLogin should be found (contains "LoginViewModel" in body)
        assert!(
            methods.iter().any(|(n, _)| n == "testLogin"),
            "should find testLogin, got: {methods:?}"
        );
        // testLogout should NOT be found (no reference)
        assert!(
            !methods.iter().any(|(n, _)| n == "testLogout"),
            "should not find testLogout"
        );
    }
}
