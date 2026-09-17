//! Process-level regression tests for `kotlin-lsp check` input reliability.
//!
//! Contract: missing paths and failed directory traversals are structured
//! errors with nonzero exit; existing directories without checkable sources
//! are reported separately (`empty_dirs`) and do not fail the run.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_kotlin-lsp");

fn check_command(home: &Path) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CACHE_HOME", home.join("cache"));
    cmd
}

fn write_fixture(dir: &Path, rel_path: &str, content: &str) {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent directory");
    }
    std::fs::write(&full, content).expect("write source fixture");
}

// ── check input reliability: missing/failed inputs must not silently pass ───

#[test]
fn check_missing_file_text_fails() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let missing = dir.path().join("Missing.kt");
    let mut cmd = check_command(dir.path());
    let env: std::collections::BTreeMap<_, _> = cmd.get_envs().collect();
    for key in ["HOME", "USERPROFILE"] {
        assert_eq!(env[std::ffi::OsStr::new(key)], Some(dir.path().as_os_str()));
    }
    for key in ["CARGO_HOME", "RUSTUP_HOME"] {
        assert!(
            !env.contains_key(std::ffi::OsStr::new(key)),
            "inherit {key}"
        );
    }
    let output = cmd
        .args(["check", &missing.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    assert!(
        !output.status.success(),
        "missing input must not exit 0 (got {:?})",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Missing.kt"),
        "stdout should name the missing input: {stdout}"
    );
}

#[test]
fn check_missing_file_json_fails_with_structured_error() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let missing = dir.path().join("Missing.kt");
    let output = check_command(dir.path())
        .args(["check", "--json", &missing.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    assert!(!output.status.success(), "missing input must not exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("one check JSON object");
    assert_eq!(v["files_ok"], 0);
    assert_eq!(v["files_with_errors"], 1);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("Missing.kt"))),
        "errors should name the missing input: {errors:?}"
    );
}

#[test]
fn check_mixed_valid_and_missing_inputs_fail() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    write_fixture(dir.path(), "src/Ok.kt", "class Ok(val x: Int)");
    let ok = dir.path().join("src/Ok.kt");
    let missing = dir.path().join("Missing.kt");
    let output = check_command(dir.path())
        .args([
            "check",
            "--json",
            &ok.to_string_lossy(),
            &missing.to_string_lossy(),
        ])
        .output()
        .expect("run kotlin-lsp check");
    assert!(
        !output.status.success(),
        "mixed valid+missing inputs must not exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("one check JSON object");
    assert_eq!(v["files_ok"], 1, "valid file should still be checked");
    assert_eq!(v["files_with_errors"], 1);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("Missing.kt"))),
        "errors should name the missing input: {errors:?}"
    );
}

#[test]
fn check_missing_directory_fails() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let missing = dir.path().join("no-such-dir");
    let output = check_command(dir.path())
        .args(["check", &missing.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    assert!(
        !output.status.success(),
        "missing directory must not exit 0 (got {:?})",
        output.status.code()
    );
}

#[test]
fn check_empty_directory_text_notes_no_sources_and_exits_zero() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let empty = dir.path().join("empty-src");
    std::fs::create_dir_all(&empty).expect("create empty source directory");
    let output = check_command(dir.path())
        .args(["check", &empty.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    assert!(
        output.status.success(),
        "an existing but source-less directory is not a missing path (got {:?})",
        output.status.code()
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("empty-src") && stdout.contains("no checkable sources"),
        "empty directory should be called out explicitly: {stdout}"
    );
}

#[test]
fn check_empty_directory_json_lists_empty_dirs() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let empty = dir.path().join("empty-src");
    std::fs::create_dir_all(&empty).expect("create empty source directory");
    let output = check_command(dir.path())
        .args(["check", "--json", &empty.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("one check JSON object");
    assert_eq!(v["files_ok"], 0);
    assert_eq!(
        v["errors"].as_array().expect("errors array").len(),
        0,
        "a source-less directory must not be reported as an error"
    );
    let empty_dirs = v["empty_dirs"].as_array().expect("empty_dirs array");
    assert!(
        empty_dirs
            .iter()
            .any(|d| d.as_str().is_some_and(|s| s.ends_with("empty-src"))),
        "empty_dirs should list the directory: {empty_dirs:?}"
    );
}

#[test]
fn check_mixed_missing_empty_dir_and_valid_distinguishes_each() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    write_fixture(dir.path(), "src/Ok.kt", "class Ok(val x: Int)");
    let empty = dir.path().join("empty-src");
    std::fs::create_dir_all(&empty).expect("create empty source directory");
    let missing = dir.path().join("Missing.kt");
    let output = check_command(dir.path())
        .args([
            "check",
            "--json",
            &dir.path().join("src").to_string_lossy(),
            &empty.to_string_lossy(),
            &missing.to_string_lossy(),
        ])
        .output()
        .expect("run kotlin-lsp check");
    assert!(
        !output.status.success(),
        "a missing input among valid ones must fail the run"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("one check JSON object");
    assert_eq!(v["files_ok"], 1);
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["file"]
            .as_str()
            .is_some_and(|f| f.ends_with("Missing.kt"))),
        "missing input should be a structured error: {errors:?}"
    );
    let empty_dirs = v["empty_dirs"].as_array().expect("empty_dirs array");
    assert!(
        empty_dirs
            .iter()
            .any(|d| d.as_str().is_some_and(|s| s.ends_with("empty-src"))),
        "empty directory should stay distinct from the missing path: {empty_dirs:?}"
    );
}

#[cfg(unix)]
#[test]
fn check_unreadable_directory_reports_traversal_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let locked = dir.path().join("locked");
    std::fs::create_dir_all(&locked).expect("create traversal fixture directory");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("make traversal fixture unreadable");
    // If the process can still read the directory (e.g. running as root),
    // the permission model does not apply in this environment — skip.
    if std::fs::read_dir(&locked).is_ok() {
        let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
        eprintln!("permission assertions not applicable: locked directory remains readable");
        return;
    }
    eprintln!("permission assertions executed: whole locked directory");
    let output = check_command(dir.path())
        .args(["check", "--json", &locked.to_string_lossy()])
        .output()
        .expect("run kotlin-lsp check");
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
    assert!(
        !output.status.success(),
        "failed directory traversal must not exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("one check JSON object");
    let errors = v["errors"].as_array().expect("errors array");
    assert!(
        errors.iter().any(|e| e["message"]
            .as_str()
            .is_some_and(|m| m.contains("traversal error"))),
        "failed traversal should be a structured error: {errors:?}"
    );
}

#[test]
fn check_diagnose_empty_directory_json_reports_input_error() {
    let dir = tempfile::tempdir().expect("create empty source directory");
    let output = check_command(dir.path())
        .args(["check", "--diagnose", "--json"])
        .arg(dir.path())
        .output()
        .expect("run check diagnose on empty directory");
    assert_eq!(output.status.code(), Some(1));
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one check JSON object");
    assert_eq!(v["files_ok"], 0);
    assert_eq!(v["files_with_errors"], 1);
    let errors = v["errors"].as_array().expect("errors array");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["file"], dir.path().to_string_lossy().as_ref());
    assert_eq!(
        errors[0]["message"],
        "diagnose requires at least one checkable file"
    );
    assert_eq!(v["empty_dirs"][0], dir.path().to_string_lossy().as_ref());
    assert!(
        output.stderr.is_empty(),
        "JSON input failure should be explained on stdout: {:?}",
        output
    );
}

#[test]
fn check_diagnose_missing_file_json_preserves_input_error() {
    let dir = tempfile::tempdir().expect("create temporary test directory");
    let missing = dir.path().join("Missing.kt");
    let output = check_command(dir.path())
        .args(["check", "--diagnose", "--json"])
        .arg(&missing)
        .output()
        .expect("run check diagnose on missing file");
    assert_eq!(output.status.code(), Some(1));
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one check JSON object");
    let errors = v["errors"].as_array().expect("errors array");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["file"], missing.to_string_lossy().as_ref());
    assert_eq!(errors[0]["message"], "no such file or directory");
    assert_eq!(v["empty_dirs"], serde_json::json!([]));
    assert!(output.stderr.is_empty());
}

#[test]
fn check_invalid_utf8_preserves_valid_input_in_text_and_json() {
    let dir = tempfile::tempdir().expect("fixture");
    write_fixture(dir.path(), "Ok.kt", "class Ok");
    std::fs::write(dir.path().join("Bad.kt"), [0xff]).expect("invalid UTF8 fixture");
    for json in [false, true] {
        let mut cmd = check_command(dir.path());
        cmd.arg("check").arg(dir.path());
        if json {
            cmd.arg("--json");
        }
        let out = cmd.output().expect("check");
        assert_eq!(out.status.code(), Some(1));
        if json {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("check object");
            assert_eq!(v["files_ok"], 1);
            assert_eq!(v["files_with_errors"], 1);
            assert_eq!(v["errors"].as_array().expect("errors").len(), 1);
            assert_eq!(
                v["errors"][0]["file"],
                dir.path().join("Bad.kt").to_string_lossy().as_ref()
            );
            assert_eq!(v["errors"][0]["line"], 0);
            assert_eq!(v["errors"][0]["col"], 0);
            assert!(v["errors"][0]["message"]
                .as_str()
                .expect("message")
                .contains("read error"));
            assert!(out.stderr.is_empty());
        } else {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.contains("Bad.kt:0:0: read error"), "{text}");
            assert!(String::from_utf8_lossy(&out.stderr).contains("1 error(s) in 1 file(s)"));
        }
    }
}

#[cfg(unix)]
#[test]
fn check_permission_failures_preserve_partial_scan_and_are_not_empty() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("fixture");
    write_fixture(dir.path(), "Ok.kt", "class Ok");
    write_fixture(dir.path(), "locked/Hidden.kt", "class Hidden");
    write_fixture(dir.path(), "Denied.kt", "class Denied");
    for (rel, traversal) in [("locked", true), ("Denied.kt", false)] {
        let locked = dir.path().join(rel);
        let original = std::fs::metadata(&locked).expect("metadata").permissions();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("lock");
        let denied = if traversal {
            std::fs::read_dir(&locked).is_err()
        } else {
            std::fs::read(&locked).is_err()
        };
        if !denied {
            std::fs::set_permissions(&locked, original).expect("restore");
            eprintln!("permission assertions not applicable: {rel} remains readable");
            continue;
        }
        let outputs: Vec<_> = [false, true]
            .into_iter()
            .map(|json| {
                let mut cmd = check_command(dir.path());
                cmd.arg("check").arg(dir.path());
                if json {
                    cmd.arg("--json");
                }
                cmd.output().expect("check")
            })
            .collect();
        std::fs::set_permissions(&locked, original).expect("restore");
        let message = if traversal {
            "traversal error"
        } else {
            "read error"
        };
        for (json, out) in [false, true].into_iter().zip(outputs) {
            assert_eq!(out.status.code(), Some(1));
            if json {
                let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("object");
                assert_eq!(v["files_ok"], 2);
                assert_eq!(v["files_with_errors"], 1);
                assert_eq!(v["empty_dirs"], serde_json::json!([]));
                assert_eq!(v["errors"][0]["file"], locked.to_string_lossy().as_ref());
                assert!(v["errors"][0]["message"]
                    .as_str()
                    .expect("message")
                    .contains(message));
            } else {
                let text = String::from_utf8_lossy(&out.stdout);
                assert!(text.contains(rel) && text.contains(message), "{text}");
                assert!(!text.contains("no checkable sources"));
            }
        }
        eprintln!("permission assertions executed: {rel} ({message})");
    }
}

#[test]
fn check_diagnose_text_input_failures_are_actionable() {
    let dir = tempfile::tempdir().expect("fixture");
    for (path, message) in [
        (
            dir.path().to_path_buf(),
            "diagnose requires at least one checkable file",
        ),
        (dir.path().join("Missing.kt"), "no such file or directory"),
    ] {
        let out = check_command(dir.path())
            .args(["check", "--diagnose"])
            .arg(&path)
            .output()
            .expect("check");
        assert_eq!(out.status.code(), Some(1));
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains(path.to_string_lossy().as_ref()) && text.contains(message),
            "{text}"
        );
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    }
}

#[test]
fn check_without_operands_is_an_argument_error() {
    let dir = tempfile::tempdir().expect("isolated home");
    for flags in [
        vec![],
        vec!["--json"],
        vec!["--diagnose"],
        vec!["--diagnose", "--json"],
    ] {
        let out = check_command(dir.path())
            .arg("check")
            .args(flags)
            .output()
            .expect("check");
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&out.stderr).trim(),
            "check requires at least one FILE argument"
        );
    }
}

#[test]
fn check_directory_selects_supported_extensions_and_counts_empty_source() {
    let dir = tempfile::tempdir().expect("fixture");
    for (file, source) in [
        ("K.kt", "class K"),
        ("script.kts", "val n = 1"),
        ("J.java", "public class J { private int n = 1; }"),
        ("nested/S.swift", "struct S {}"),
        ("Empty.kt", ""),
        ("ignored.txt", "class Broken {"),
    ] {
        write_fixture(dir.path(), file, source);
    }
    for json in [false, true] {
        let mut cmd = check_command(dir.path());
        cmd.arg("check").arg(dir.path());
        if json {
            cmd.arg("--json");
        }
        let out = cmd.output().expect("check");
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(out.stderr.is_empty());
        if json {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("object");
            assert_eq!(
                v,
                serde_json::json!({"files_ok":5,"files_with_errors":0,"errors":[],"empty_dirs":[]})
            );
        } else {
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                "All 5 files OK."
            );
        }
    }
}
