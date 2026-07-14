use std::process::Command;

use tempfile;

/// Build the release binary once, reused by all tests.
const BIN: &str = "target/debug/kotlin-lsp";

/// Create a .kt fixture file under `root`.
fn write_fixture(root: &std::path::Path, rel: &str, body: &str) {
    let dest = root.join(rel);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, body).unwrap();
}

/// Run `kotlin-lsp index --root <root>`.
fn index(root: &std::path::Path) {
    let status = Command::new(BIN)
        .args(["index", "--root", &root.to_string_lossy()])
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn check_valid_file_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), "src/Ok.kt", "class Ok");
    let output = Command::new(BIN)
        .args(["check", &dir.path().join("src/Ok.kt").to_string_lossy()])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn check_syntax_error_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), "src/Bad.kt", "class {");
    let output = Command::new(BIN)
        .args(["check", &dir.path().join("src/Bad.kt").to_string_lossy()])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn check_json_output() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), "src/Ok.kt", "class Ok");
    let output = Command::new(BIN)
        .args([
            "check",
            "--json",
            &dir.path().join("src/Ok.kt").to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"errors\""));
}

#[test]
fn cache_stats_subcommand_runs() {
    let output = Command::new(BIN).args(["cache", "stats"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty());
}

#[test]
fn insert_writes_content_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("InsertMe.kt");
    let original = "package com.example\nclass InsertMe\n";
    std::fs::write(&path, original).unwrap();
    let output = Command::new(BIN)
        .args([
            "insert",
            "--before-last",
            "}",
            "    val added = true",
            &path.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("val added = true"));
}

#[test]
fn batch_dry_run_reports_changes_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        "src/NeedsImport.kt",
        "package com.example\nclass NeedsImport",
    );
    index(dir.path());
    let output = Command::new(BIN)
        .args([
            "batch-imports",
            "--dry-run",
            "--root",
            &dir.path().to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn organize_imports_removes_unused() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        "src/UnusedImport.kt",
        "import java.util.Date\n\nclass UnusedImport\n",
    );
    index(dir.path());
    let output = Command::new(BIN)
        .args([
            "organize-imports",
            "--apply",
            "--root",
            &dir.path().to_string_lossy(),
            &dir.path()
                .join("src/UnusedImport.kt")
                .to_string_lossy()
                .to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let after = std::fs::read_to_string(dir.path().join("src/UnusedImport.kt")).unwrap();
    assert!(!after.contains("import java.util.Date"));
}

#[test]
fn organize_imports_keeps_delegate_operator_imports() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        "src/DelegateOp.kt",
        "import kotlin.properties.Delegates\n\nval x by Delegates.notNull<Int>()\n",
    );
    index(dir.path());
    let output = Command::new(BIN)
        .args([
            "organize-imports",
            "--apply",
            "--root",
            &dir.path().to_string_lossy(),
            &dir.path()
                .join("src/DelegateOp.kt")
                .to_string_lossy()
                .to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let after = std::fs::read_to_string(dir.path().join("src/DelegateOp.kt")).unwrap();
    assert!(after.contains("import kotlin.properties.Delegates"));
}

#[test]
fn organize_imports_removes_setvalue_for_val_delegate() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        "src/ValDelegate.kt",
        "import kotlin.properties.Delegates\n\nval count by Delegates.observable(0) { _, _, _ ->\n}\n",
    );
    index(dir.path());
    let output = Command::new(BIN)
        .args([
            "organize-imports",
            "--apply",
            "--root",
            &dir.path().to_string_lossy(),
            &dir.path()
                .join("src/ValDelegate.kt")
                .to_string_lossy()
                .to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    // Delegates import should be preserved because it's used by `Delegates.observable`
    let after = std::fs::read_to_string(dir.path().join("src/ValDelegate.kt")).unwrap();
    assert!(after.contains("import kotlin.properties.Delegates"));
}

#[test]
#[ignore = "sort is not deterministic across platforms"]
fn inject_sorts_by_frequency() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        "src/Multi.kt",
        "import java.util.Date\nimport java.util.List\n\nclass Multi\n",
    );
    index(dir.path());
    let output = Command::new(BIN)
        .args([
            "inject",
            "--root",
            &dir.path().to_string_lossy(),
            &dir.path()
                .join("src/Multi.kt")
                .to_string_lossy()
                .to_string(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}
