//! Shared edit routes through real argv/stdout/exit and exact filesystem bytes.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("workspace");
        std::fs::create_dir_all(root.join(".git")).expect("workspace");
        std::fs::create_dir(dir.path().join("home")).expect("home");
        Self { dir, root }
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, text).expect("source");
        path
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"))
            .current_dir(&self.root)
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("XDG_CONFIG_HOME", self.dir.path().join("config"))
            .env("XDG_DATA_HOME", self.dir.path().join("data"))
            .env("RUST_LOG", "error")
            .args(args)
            .arg("--root")
            .arg(&self.root)
            .arg("--no-stdlib")
            .output()
            .expect("CLI process")
    }
}
fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("JSON report")
}

#[test]
fn rename_unicode_identifier_uses_utf16_and_preserves_mixed_bytes() {
    let f = Fixture::new();
    let path = f.write(
        "File.kt",
        "fun café() {}\r\nfun use() { café() }\n// untouched",
    );
    let output = f.run(&[
        "edit", "rename", "File.kt", "1", "5", "renamed", "--apply", "--json",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 1);
    assert_eq!(
        std::fs::read(path).expect("bytes"),
        "fun renamed() {}\r\nfun use() { renamed() }\n// untouched".as_bytes()
    );
}

#[test]
fn rename_readonly_reports_failure_exit_and_untouched_bytes() {
    let f = Fixture::new();
    let path = f.write("File.kt", "fun target() {}\nfun use() { target() }\n");
    let permissions = std::fs::metadata(&path).expect("metadata").permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).expect("readonly");
    let output = f.run(&[
        "edit", "rename", "File.kt", "1", "5", "renamed", "--apply", "--json",
    ]);
    std::fs::set_permissions(&path, permissions).expect("restore permissions");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let report = json(&output);
    assert_eq!(report["files_modified"], 0);
    assert_eq!(report["files"][0]["status"], "error");
    assert_eq!(
        std::fs::read(path).expect("bytes"),
        b"fun target() {}\nfun use() { target() }\n"
    );
}

#[test]
fn imports_equal_position_preview_matches_apply_and_retains_endings() {
    let f = Fixture::new();
    f.write("Alpha.kt", "package library\nclass Alpha\n");
    f.write("Beta.kt", "package library\nclass Beta\n");
    let path = f.write(
        "Use.kt",
        "package app\r\n\r\nfun use(a: Alpha, b: Beta) {}\n// last",
    );
    let before = std::fs::read(&path).expect("before");
    let preview = f.run(&["edit", "imports", "Use.kt", "--json"]);
    assert!(preview.status.success(), "{preview:?}");
    assert_eq!(
        json(&preview)["unique"],
        serde_json::json!(["library.Alpha", "library.Beta"])
    );
    assert_eq!(std::fs::read(&path).expect("dry bytes"), before);
    let output = f.run(&["edit", "imports", "Use.kt", "--apply"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 1);
    assert_eq!(std::fs::read(&path).expect("bytes"), b"package app\r\nimport library.Beta\nimport library.Alpha\n\r\nfun use(a: Alpha, b: Beta) {}\n// last");
    let lines: Vec<_> = std::fs::read_to_string(&path)
        .expect("text")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        json(&preview)["preview"]["new_lines"],
        serde_json::json!(lines)
    );
}

#[test]
fn imports_java_preview_and_apply_are_unicode_safe() {
    let f = Fixture::new();
    f.write("Alpha.java", "package library;\npublic class Alpha {}\n");
    let path = f.write(
        "Use.java",
        "package app;\r\n// 😀漢\r\nclass Use { Alpha a; }",
    );
    let preview = f.run(&["edit", "imports", "Use.java", "--json"]);
    assert!(preview.status.success(), "{preview:?}");
    let applied = f.run(&["edit", "imports", "Use.java", "--apply"]);
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(json(&applied)["files_modified"], 1);
    assert_eq!(
        std::fs::read(&path).expect("bytes"),
        "package app;\r\n\nimport library.Alpha;\n// 😀漢\r\nclass Use { Alpha a; }".as_bytes()
    );
}

#[test]
fn imports_readonly_failure_exit_and_json_preview_dry_run() {
    let f = Fixture::new();
    f.write("Alpha.kt", "package library\nclass Alpha\n");
    let path = f.write("Use.kt", "fun use(a: Alpha) {}\r\n");
    let permissions = std::fs::metadata(&path).expect("metadata").permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).expect("readonly");
    let output = f.run(&["edit", "imports", "Use.kt", "--apply"]);
    std::fs::set_permissions(&path, permissions).expect("restore");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 0);
    assert_eq!(json(&output)["files"][0]["status"], "error");
    assert_eq!(
        std::fs::read(&path).expect("bytes"),
        b"fun use(a: Alpha) {}\r\n"
    );
}

#[test]
fn rename_dry_run_prospective_counts_no_bytes_or_permission_change() {
    let f = Fixture::new();
    let path = f.write("File.swift", "func target() {}\r\nfunc use() { target() }");
    let metadata = std::fs::metadata(&path).expect("metadata");
    let output = f.run(&[
        "edit",
        "rename",
        "File.swift",
        "1",
        "6",
        "renamed",
        "--json",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 1);
    assert_eq!(json(&output)["files"][0]["dry_run"], true);
    assert_eq!(
        std::fs::read(&path).expect("bytes"),
        b"func target() {}\r\nfunc use() { target() }"
    );
    let after = std::fs::metadata(&path).expect("metadata");
    assert_eq!(after.permissions(), metadata.permissions());
    assert_eq!(
        after.modified().expect("mtime"),
        metadata.modified().expect("mtime")
    );
    assert!(!std::fs::read_dir(&f.root)
        .expect("listing")
        .any(|entry| entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .starts_with(".kotlin-lsp-edit-")));
}

#[test]
fn rename_two_files_valid_batch_and_invalid_utf8_operand_fail_honestly() {
    let f = Fixture::new();
    let definition = f.write("A.kt", "fun target() {}\r\n");
    let call = f.write("B.kt", "fun use() { target() }");
    let output = f.run(&[
        "edit", "rename", "A.kt", "1", "5", "renamed", "--apply", "--json",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 2);
    assert_eq!(
        std::fs::read(definition).expect("definition"),
        b"fun renamed() {}\r\n"
    );
    assert_eq!(
        std::fs::read(call).expect("call"),
        b"fun use() { renamed() }"
    );
    std::fs::write(f.root.join("Bad.kt"), [0xff]).expect("invalid UTF8");
    let output = f.run(&["edit", "imports", "Bad.kt", "--apply"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("read error"));
    assert_eq!(
        std::fs::read(f.root.join("Bad.kt")).expect("bad bytes"),
        [0xff]
    );
}

#[test]
fn code_action_apply_uses_one_based_cursor_and_preserves_crlf() {
    let f = Fixture::new();
    let path = f.write("Action.kt", "import library.Alpha\r\nclass Action\r\n");
    let output = f.run(&[
        "tool",
        "code-action",
        "Action.kt",
        "1",
        "9",
        "--apply",
        "--json",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 1);
    assert_eq!(
        std::fs::read(path).expect("bytes"),
        b"import library.Alpha as Alpha\r\nclass Action\r\n"
    );
}

#[test]
fn code_action_readonly_failure_retains_summary_and_untouched_bytes() {
    let f = Fixture::new();
    let path = f.write("Action.kt", "import library.Alpha\r\nclass Action\r\n");
    let permissions = std::fs::metadata(&path).expect("metadata").permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).expect("readonly");
    let output = f.run(&[
        "tool",
        "code-action",
        "Action.kt",
        "1",
        "9",
        "--apply",
        "--json",
    ]);
    std::fs::set_permissions(&path, permissions).expect("restore");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(json(&output)["files_modified"], 0);
    assert_eq!(json(&output)["files"][0]["status"], "error");
    assert_eq!(
        std::fs::read(path).expect("bytes"),
        b"import library.Alpha\r\nclass Action\r\n"
    );
}

#[test]
fn code_action_invalid_position_and_missing_operand_fail_without_panic() {
    let f = Fixture::new();
    let path = f.write("Action.kt", "// 😀漢\n");
    for (file, line, col) in [
        ("Action.kt", "1", "5"),
        ("Action.kt", "4", "1"),
        ("Missing.kt", "1", "1"),
    ] {
        let output = f.run(&["tool", "code-action", file, line, col, "--apply", "--json"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
    assert_eq!(std::fs::read(path).expect("bytes"), "// 😀漢\n".as_bytes());
}

#[test]
fn rename_missing_operand_fails_without_panic() {
    let f = Fixture::new();
    let output = f.run(&[
        "edit",
        "rename",
        "Missing.kt",
        "1",
        "1",
        "name",
        "--apply",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn code_action_outside_root_fails_without_writing() {
    let f = Fixture::new();
    let path = f.dir.path().join("Outside.kt");
    std::fs::write(&path, "import library.Alpha\n").expect("outside");
    let output = f.run(&[
        "tool",
        "code-action",
        path.to_str().expect("fixture path"),
        "1",
        "9",
        "--apply",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        std::fs::read(path).expect("outside bytes"),
        b"import library.Alpha\n"
    );
}
