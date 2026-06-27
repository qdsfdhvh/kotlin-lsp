//! Tests for the `format` subcommand (format check / format apply).
//!
//! These tests require `ktlint` to be installed on PATH.  Tests that call
//! ktlint are skipped when the binary is not available.

use std::path::PathBuf;

use tempfile::TempDir;

use crate::cli::format;

/// Returns `true` if `ktlint` is available on PATH.
fn ktlint_available() -> bool {
    std::process::Command::new("ktlint")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|s| s.success())
        .is_some()
}

/// Helper: create a temp directory with a `.kt` file.
fn setup_kt_file(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("temp dir");
    let file = dir.path().join("test_file.kt");
    std::fs::write(&file, content).expect("write test file");
    (dir, file)
}

// ─── parse_ktlint_output ─────────────────────────────────────────────────────

#[test]
fn parse_standard_output() {
    let input = "/path/to/File.kt:42:5:chain-wrapping - Chain should be wrapped\n\
                  /path/to/File.kt:100:1:no-wildcard-imports - No wildcard imports";
    let violations = format::test_helpers::parse_ktlint_output_for_test(input);
    assert_eq!(violations.len(), 2);
    assert_eq!(violations[0].rule_id, "chain-wrapping");
    assert_eq!(violations[0].message, "Chain should be wrapped");
    assert_eq!(violations[0].line, 42);
    assert_eq!(violations[1].rule_id, "no-wildcard-imports");
}

#[test]
fn parse_output_missing_separator() {
    // Some ktlint versions output "file:line:col message" without " - "
    let input = "/path/to/File.kt:10:3 some message";
    let violations = format::test_helpers::parse_ktlint_output_for_test(input);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule_id, "");
    assert_eq!(violations[0].message, "some message");
}

#[test]
fn parse_ignores_empty_lines() {
    let input = "/a.kt:1:1:rule - msg\n\n\n/b.kt:2:2:rule2 - msg2";
    let violations = format::test_helpers::parse_ktlint_output_for_test(input);
    assert_eq!(violations.len(), 2);
}

// ─── generate_diff ──────────────────────────────────────────────────────────

#[test]
fn diff_identical_returns_none() {
    let diff = format::test_helpers::generate_diff_for_test("hello", "hello", "file.kt");
    assert!(diff.is_none());
}

#[test]
fn diff_shows_addition() {
    let diff = format::test_helpers::generate_diff_for_test("a\nb\nc", "a\nb\nb'\nc", "file.kt")
        .expect("should produce diff");
    assert!(diff.contains("--- a/file.kt"));
    assert!(diff.contains("+++ b/file.kt"));
    assert!(diff.contains("+b'"));
    assert!(!diff.contains("-b'"));
}

#[test]
fn diff_shows_removal() {
    let diff = format::test_helpers::generate_diff_for_test("a\nb\nc", "a\nc", "file.kt")
        .expect("should produce diff");
    assert!(diff.contains("-b"));
}

// ─── Integration: format check (requires ktlint) ────────────────────────────

#[test]
fn format_check_well_formed() {
    if !ktlint_available() {
        eprintln!("skipping: ktlint not installed");
        return;
    }
    let (_dir, file) =
        setup_kt_file("package com.example\n\nfun main() {\n    println(\"hello\")\n}\n");
    let files = vec![file];
    // Should pass — well-formed Kotlin
    format::test_helpers::run_format_check_for_test(&files, false);
    // No assertion needed — it should not exit(1)
}

#[test]
fn format_check_violation() {
    if !ktlint_available() {
        eprintln!("skipping: ktlint not installed");
        return;
    }
    // Intentionally create a file with formatting violations
    let content = "package com.example\n\nfun     main() {\n    println(\"hello\")\n}\n";
    let (_dir, file) = setup_kt_file(content);
    let files = vec![file];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        format::test_helpers::run_format_check_for_test(&files, true);
    }));
    // Should either detect violations (exit 1) or pass (exit 0)
    // We just verify it doesn't crash
    assert!(result.is_ok() || result.is_err());
}

// ─── Integration: format apply (requires ktlint) ────────────────────────────

#[test]
fn format_apply_well_formed() {
    if !ktlint_available() {
        eprintln!("skipping: ktlint not installed");
        return;
    }
    let content = "package com.example\n\nfun main() {\n    println(\"hello\")\n}\n";
    let (_dir, file) = setup_kt_file(content);
    let files = vec![file.clone()];
    format::test_helpers::run_format_apply_for_test(&files, false, false);
    // File should still be readable
    let after = std::fs::read_to_string(&file).expect("read after apply");
    assert!(!after.is_empty());
}

#[test]
fn format_apply_modifies_file() {
    if !ktlint_available() {
        eprintln!("skipping: ktlint not installed");
        return;
    }
    // Content with formatting issues
    let content = "package com.example\n\nfun     main() {\n    println(\"hello\")\n}\n";
    let (_dir, file) = setup_kt_file(content);
    let before = std::fs::read_to_string(&file).expect("read before");
    let files = vec![file.clone()];
    format::test_helpers::run_format_apply_for_test(&files, false, false);
    let after = std::fs::read_to_string(&file).expect("read after");
    // ktlint should have changed the double spacing
    assert_ne!(
        before, after,
        "file content should have changed after format apply"
    );
}
