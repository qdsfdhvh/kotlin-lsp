use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::cli::insert::find_class_body_insert_point;
use crate::LinesExt;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn temp_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("kotlin-lsp-insert-tests");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(name);
    let mut f = fs::File::create(&path).expect("create test file");
    f.write_all(content.as_bytes()).expect("write test file");
    path
}

fn lines_of_file(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

// ─── insert-import tests ──────────────────────────────────────────────────────

#[test]
fn insert_import_after_existing_imports() {
    let src = "package com.example\n\nimport android.os.Bundle\nimport kotlinx.coroutines.flow.Flow\n\nclass MyClass {\n}\n";
    let path = temp_file("import_after.kt", src);
    let lines = lines_of_file(&path);
    let insert_line = lines.import_insertion_line();
    // Should be after the last import (line 3, 0-indexed).
    assert_eq!(insert_line, 3);
    // Clean up
    let _ = fs::remove_file(&path);
}

#[test]
fn insert_import_after_package_no_imports() {
    let src = "package com.example\n\nclass MyClass {\n}\n";
    let path = temp_file("import_after_pkg.kt", src);
    let lines = lines_of_file(&path);
    let insert_line = lines.import_insertion_line();
    // Should be after package declaration (line 1, 0-indexed).
    assert_eq!(insert_line, 1);
    let _ = fs::remove_file(&path);
}

#[test]
fn insert_import_no_package_no_imports() {
    let src = "class MyClass {\n}\n";
    let path = temp_file("import_no_pkg.kt", src);
    let lines = lines_of_file(&path);
    let insert_line = lines.import_insertion_line();
    // No package and no imports — should return 0.
    assert_eq!(insert_line, 0);
    let _ = fs::remove_file(&path);
}

// ─── insert-member tests (tree-sitter class body) ─────────────────────────────

#[test]
fn find_class_body_simple() {
    let src = "class MyClass {\n    val x = 1\n}\n";
    let path = temp_file("class_body_simple.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "MyClass");
    assert!(
        result.is_ok(),
        "should find class MyClass: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    // Closing } is at line 2, so insert at line 1 (before }).
    assert_eq!(line, 1);
    assert_eq!(indent, "    ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_empty() {
    let src = "class Empty {\n}\n";
    let path = temp_file("class_body_empty.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "Empty");
    assert!(
        result.is_ok(),
        "should find Empty class: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    // Closing } is at line 1, so insert at line 0 (before }).
    assert_eq!(line, 0);
    // Indent should be 4 spaces (class indent 0 + 4).
    assert_eq!(indent, "    ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_with_members() {
    let src = "class WithMembers {\n    fun one() {}\n    fun two() {}\n}\n";
    let path = temp_file("class_body_members.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "WithMembers");
    assert!(
        result.is_ok(),
        "should find WithMembers class: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    // Closing } is at line 3, insert at line 2 (before }).
    assert_eq!(line, 2);
    assert_eq!(indent, "    ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_nested() {
    let src = "object Outer {\n    class Inner {\n        fun method() = 42\n    }\n}\n";
    let path = temp_file("class_body_nested.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "Inner");
    assert!(
        result.is_ok(),
        "should find Inner class: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    // Inner's } is at line 3, insert at line 2.
    assert_eq!(line, 2);
    assert_eq!(indent, "        ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_interface() {
    let src = "interface MyInterface {\n    fun required(): String\n}\n";
    let path = temp_file("class_body_interface.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "MyInterface");
    assert!(
        result.is_ok(),
        "should find MyInterface: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    assert_eq!(line, 1);
    assert_eq!(indent, "    ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_enum() {
    let src = "enum class Color {\n    RED,\n    GREEN,\n}\n";
    let path = temp_file("class_body_enum.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "Color");
    assert!(
        result.is_ok(),
        "should find Color enum: {:?}",
        result.err()
    );
    let (line, indent) = result.unwrap();
    assert_eq!(line, 2);
    assert_eq!(indent, "    ");
    let _ = fs::remove_file(&path);
}

#[test]
fn find_class_body_not_found() {
    let src = "class A {}\n";
    let path = temp_file("class_body_notfound.kt", src);
    let lines = lines_of_file(&path);
    let result = find_class_body_insert_point(&lines, "NonExistent");
    assert!(result.is_err());
    let _ = fs::remove_file(&path);
}

// ─── generate_override tests ──────────────────────────────────────────────────

#[test]
fn generate_override_basic() {
    let result = crate::cli::insert::generate_override_test("onCreate", "    ");
    assert!(result.contains("override fun onCreate()"));
    assert!(result.contains("TODO"));
}

#[test]
fn generate_override_indent() {
    let result = crate::cli::insert::generate_override_test("onResume", "        ");
    assert!(result.starts_with("        override fun onResume()"));
}

// ─── duplicate import detection ───────────────────────────────────────────────

#[test]
fn already_imported_detects_exact_match() {
    let src = "package com.example\n\nimport android.os.Bundle\n\nclass Foo\n";
    let path = temp_file("dup_import_exact.kt", src);
    let lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let imports = lines.parse_imports();
    assert!(crate::resolver::already_imported(
        "android.os.Bundle",
        &imports
    ));
    let _ = fs::remove_file(&path);
}

#[test]
fn already_imported_detects_star_import() {
    let src = "package com.example\n\nimport android.os.*\n\nclass Foo\n";
    let path = temp_file("dup_import_star.kt", src);
    let lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let imports = lines.parse_imports();
    assert!(crate::resolver::already_imported(
        "android.os.Bundle",
        &imports
    ));
    let _ = fs::remove_file(&path);
}

#[test]
fn not_imported_different_package() {
    let src = "package com.example\n\nimport com.google.common.Foo\n\nclass Bar\n";
    let path = temp_file("dup_import_diff.kt", src);
    let lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect();
    let imports = lines.parse_imports();
    assert!(!crate::resolver::already_imported(
        "android.os.Bundle",
        &imports
    ));
    let _ = fs::remove_file(&path);
}
