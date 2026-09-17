//! Process regressions for ordered batch queries, using isolated workspaces and homes.
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_kotlin-lsp");

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    cwd: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("B");
        let cwd = dir.path().join("A");
        let home = dir.path().join("home");
        for path in [&root, &cwd, &home] {
            std::fs::create_dir_all(path).expect("mkdir");
        }
        Self {
            dir,
            root,
            cwd,
            home,
        }
    }
    fn write(&self, file: &str, source: &str) {
        write(&self.root.join(file), source);
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.current_dir(&self.cwd)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"));
        cmd
    }
    fn query(&self, specs: Value, no_stdlib: bool) -> Output {
        let mut cmd = self.command();
        cmd.args(["tool", "query", "--json", "--root"])
            .arg(&self.root);
        if no_stdlib {
            cmd.arg("--no-stdlib");
        }
        self.raw_query(cmd, specs.to_string().as_bytes())
    }
    fn raw_query(&self, mut cmd: Command, input: &[u8]) -> Output {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn query");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input)
            .expect("write specs");
        child.wait_with_output().expect("query output")
    }
}
fn write(path: &Path, source: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, source).expect("write fixture");
}
// Location JSON uses filesystem paths after a file-URI roundtrip, which removes
// Windows canonicalize's verbatim prefix. Caller/subtype JSON keeps the URI.
fn expected_file_path(path: &Path) -> PathBuf {
    tower_lsp::lsp_types::Url::from_file_path(path.canonicalize().expect("canonical fixture file"))
        .expect("fixture file URI")
        .to_file_path()
        .expect("fixture filesystem path")
}

#[test]
fn expected_file_path_preserves_identity_without_becoming_a_uri() {
    let f = Fixture::new();
    f.write("space 文 #%.kt", "class Fixture");
    let original = f.root.join("space 文 #%.kt");
    let expected = expected_file_path(&original);
    assert!(expected.is_absolute(), "filesystem-valued schema");
    assert_eq!(
        expected.canonicalize().expect("roundtripped file"),
        original.canonicalize().expect("original file")
    );
    assert_eq!(expected.file_name(), original.file_name());
    assert_ne!(
        expected.to_str().expect("UTF8 fixture path"),
        tower_lsp::lsp_types::Url::from_file_path(&original)
            .expect("URI")
            .as_str()
    );
}

fn items(output: &Output) -> Vec<Value> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}: {output:?}"))
}
fn lines(item: &Value) -> Vec<u64> {
    item["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["line"].as_u64().expect("line"))
        .collect()
}

#[test]
fn references_are_cst_usages_with_real_filters_and_normal_refs_parity() {
    let f = Fixture::new();
    f.write("Use.kt", "package demo\nimport other.target\nfun target() {}\nfun entry() {\n    target()\n    val read = target\n    consume(target)\n    // target()\n    val text = \"target()\"\n}\n");
    let specs = json!([
        {"type":"definition","name":"target"},
        {"type":"references","name":"target"},
        {"type":"references","name":"target","refKind":"call"},
        {"type":"references","name":"target","refKind":"import"},
        {"type":"references","name":"target","refKind":"read"},
        {"type":"references","name":"target","refKind":"declaration"},
        {"type":"references","name":"target","refKind":"all"},
        {"type":"references","name":"target","refKind":"reference"}
    ]);
    let out = f.query(specs, true);
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [3]);
    assert_eq!(lines(&data[1]), [3, 5, 6, 7]);
    assert_eq!(lines(&data[2]), [5]);
    assert_eq!(lines(&data[3]), [2]);
    assert_eq!(lines(&data[4]), [6, 7]);
    assert_eq!(lines(&data[5]), [3]);
    assert_eq!(data[1]["results"], data[6]["results"]);
    assert_eq!(data[1]["results"], data[7]["results"]);
    for (i, kind) in [(2, "call"), (3, "import"), (4, "read"), (5, "declaration")] {
        assert_eq!(data[i]["filter_applied"], kind);
    }
    let normal = f
        .command()
        .args([
            "refs",
            "target",
            "--smart",
            "--json",
            "--absolute",
            "--root",
        ])
        .arg(&f.root)
        .output()
        .expect("normal refs");
    assert!(normal.status.success(), "{normal:?}");
    let refs: Vec<Value> = serde_json::from_slice(&normal.stdout).expect("refs array");
    let mut normal_positions: Vec<_> = refs
        .iter()
        .map(|r| (r["line"].as_u64(), r["col"].as_u64()))
        .collect();
    normal_positions.sort();
    let positions: Vec<_> = data[1]["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| (r["line"].as_u64(), r["col"].as_u64()))
        .collect();
    assert_eq!(normal_positions, positions);
}

#[test]
fn invalid_items_fail_without_losing_success_order() {
    let f = Fixture::new();
    f.write("Use.kt", "fun target() {}\nfun entry() { target() }\n");
    let out = f.query(
        json!([
            {"type":"definition","name":"target"},
            {"type":"references","name":"target","refKind":"bogus"},
            {"type":"callers","file":"Use.kt","line":1,"col":5,"depth":0},
            {"type":"callers","file":"Use.kt","line":1,"col":5,"depth":2},
            {"type":"unknown"},
            {"type":"hover","file":"Use.kt","line":"bad","col":1},
            {"type":"references","name":"target","refKind":"call"}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(1));
    let data = items(&out);
    assert_eq!(data.len(), 7);
    assert_eq!(lines(&data[0]), [1]);
    for item in &data[1..6] {
        assert!(item["error"].is_string(), "{item}");
    }
    assert_eq!(data[1]["type"], "references");
    assert_eq!(lines(&data[6]), [2]);
}

#[test]
fn explicit_root_overrides_cwd_and_json_is_compact() {
    let f = Fixture::new();
    f.write("B.kt", "class OnlyB");
    write(&f.cwd.join("A.kt"), "class OnlyA");
    let out = f.query(
        json!([{"type":"definition","name":"OnlyB"},{"type":"definition","name":"OnlyA"}]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [1]);
    assert!(lines(&data[1]).is_empty());
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 1);
}

#[test]
fn no_stdlib_excludes_fake_home_library() {
    let f = Fixture::new();
    let cmd = f.command();
    let env: std::collections::BTreeMap<_, _> = cmd.get_envs().collect();
    for key in ["HOME", "USERPROFILE"] {
        assert_eq!(env[std::ffi::OsStr::new(key)], Some(f.home.as_os_str()));
    }
    for key in ["CARGO_HOME", "RUSTUP_HOME"] {
        assert!(
            !env.contains_key(std::ffi::OsStr::new(key)),
            "inherit {key}"
        );
    }
    f.write("B.kt", "class WorkspaceOnly");
    write(
        &f.home.join(".kotlin-lsp/sources/lib/Lib.kt"),
        "package fake\nclass FakeHomeLibrary",
    );
    let specs = json!([{"type":"definition","name":"FakeHomeLibrary"}]);
    let disabled = f.query(specs.clone(), true);
    assert!(disabled.status.success(), "{disabled:?}");
    assert!(lines(&items(&disabled)[0]).is_empty());
    let enabled = f.query(specs, false);
    assert!(enabled.status.success(), "{enabled:?}");
    assert_eq!(lines(&items(&enabled)[0]), [2]);
}

#[test]
fn relative_hover_and_callers_use_root_and_utf16_identifier_start() {
    let f = Fixture::new();
    let line = "fun entry() { val 文 = \"😀\"; target() }";
    f.write(
        "Use.kt",
        &format!("fun target(value: Int = 0): Int = value\n{line}\n"),
    );
    let col = line[..line.find("target").expect("target")]
        .encode_utf16()
        .count()
        + 1;
    let out = f.query(
        json!([
            {"type":"hover","file":"./Use.kt","line":2,"col":col},
            {"type":"callers","file":"Use.kt","line":1,"col":5},
            {"type":"callers","file":"./Use.kt","line":2,"col":col,"depth":1},
            {"type":"hover","file":"Use.kt","line":1,"col":5}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    for item in [&data[0], &data[3]] {
        assert_eq!(item["name"], "target");
        assert!(
            item["signature"]
                .as_str()
                .is_some_and(|s| s.contains("fun target") && s.contains("Int")),
            "{item}"
        );
    }
    for item in [&data[1], &data[2]] {
        assert_eq!(item["name"], "target");
        assert_eq!(item["depth"], 1);
        assert!(
            item["callers"]
                .as_array()
                .expect("callers")
                .iter()
                .any(|c| c["name"] == "entry"),
            "{item}"
        );
    }
}

#[test]
fn bad_file_or_position_returns_ordered_errors_not_fabricated_hover() {
    let f = Fixture::new();
    f.write("Use.kt", "fun target() {}\nfun entry() { missing() }\n");
    let out = f.query(
        json!([
            {"type":"hover","file":"missing.kt","line":1,"col":1},
            {"type":"callers","file":"Use.kt","line":0,"col":1},
            {"type":"hover","file":"Use.kt","line":1,"col":0},
            {"type":"hover","file":"Use.kt","line":1,"col":1000},
            {"type":"hover","file":"Use.kt","line":100,"col":1},
            {"type":"hover","file":"Use.kt","line":2,"col":15}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(1));
    let data = items(&out);
    assert_eq!(data.len(), 6);
    for item in &data[..5] {
        assert!(item["error"].is_string(), "{item}");
    }
    assert_eq!(data[5]["name"], "missing");
    assert!(data[5]["signature"].is_null());
}

#[test]
fn references_return_multiple_occurrences_and_utf16_columns() {
    let f = Fixture::new();
    let line = "fun entry() { val 文 = \"😀 target\"; target(); target() }";
    f.write("Use.kt", &format!("fun target() {{}}\n{line}\n"));
    let out = f.query(
        json!([{"type":"references","name":"target","refKind":"call"}]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [2, 2]);
    let expected: Vec<_> = line
        .match_indices("target()")
        .map(|(byte, _)| line[..byte].encode_utf16().count() as u64 + 1)
        .collect();
    let actual: Vec<_> = data[0]["results"]
        .as_array()
        .expect("refs")
        .iter()
        .map(|r| r["col"].as_u64().expect("col"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn java_and_swift_calls_are_distinct_from_declarations_and_text() {
    for (file, source) in [
        ("Use.java", "class Use {\n  void target() {}\n  void entry() { target(); }\n  String text = \"target\"; // target\n}\n"),
        ("Use.swift", "func target() {}\nfunc entry() { target() }\nlet text = \"target\" // target\n"),
    ] {
        let f = Fixture::new();
        f.write(file, source);
        let out = f.query(json!([
            {"type":"definition","name":"target"},
            {"type":"references","name":"target","refKind":"call"},
            {"type":"references","name":"target"}
        ]), true);
        assert!(out.status.success(), "{out:?}");
        let data = items(&out);
        let declaration = if file.ends_with("java") { 2 } else { 1 };
        assert_eq!(lines(&data[0]), [declaration]);
        assert_eq!(lines(&data[1]), [declaration + 1], "{file}");
        assert_eq!(lines(&data[2]), [declaration, declaration + 1], "{file}");
    }
}

#[test]
fn other_reference_kinds_discriminate_writes_overrides_and_types() {
    let f = Fixture::new();
    f.write("Use.kt", "var target = 0\nfun entry() {\n    target = 1\n    consume(target)\n}\nopen class Base { open fun work() {} }\nclass Child : Base() { override fun work() {} }\nval instance: Base? = null\n");
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"target","refKind":"read"},
            {"type":"references","name":"work","refKind":"override"},
            {"type":"references","name":"Base","refKind":"type-use"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [3]);
    assert_eq!(lines(&data[1]), [4]);
    assert_eq!(lines(&data[2]), [7]);
    assert_eq!(lines(&data[3]), [7, 8]);
}

#[test]
fn references_include_interpolated_reads_but_not_literal_text() {
    let f = Fixture::new();
    let line = "fun render() = \"😀 文 $target\"";
    f.write(
        "Use.kt",
        &format!("var target = 1\n{line}\nval text = \"target\" // target\n"),
    );
    let out = f.query(
        json!([
            {"type":"references","name":"target"},
            {"type":"references","name":"target","refKind":"read"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [1, 2]);
    assert_eq!(lines(&data[1]), [2]);
    let col = line[..line.find("target").expect("identifier")]
        .encode_utf16()
        .count()
        + 1;
    assert_eq!(data[1]["results"][0]["col"], col);
    let normal = f
        .command()
        .args([
            "refs",
            "target",
            "--smart",
            "--json",
            "--absolute",
            "--no-stdlib",
            "--root",
        ])
        .arg(&f.root)
        .output()
        .expect("normal refs");
    assert!(normal.status.success(), "{normal:?}");
    let normal_refs = items(&normal);
    let mut normal_positions: Vec<_> = normal_refs
        .iter()
        .map(|r| (r["line"].as_u64(), r["col"].as_u64()))
        .collect();
    normal_positions.sort();
    let positions: Vec<_> = data[0]["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| (r["line"].as_u64(), r["col"].as_u64()))
        .collect();
    assert_eq!(normal_positions, positions);
}

#[test]
fn reference_writes_include_kotlin_compound_assignments() {
    let f = Fixture::new();
    f.write("Use.kt", "var target = 0\nfun entry() {\n    target += 1\n    target -= 1\n    target *= 2\n    target /= 2\n    target %= 2\n    consume(target)\n}\n");
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"target","refKind":"read"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [3, 4, 5, 6, 7]);
    assert_eq!(lines(&data[1]), [8]);
}

#[test]
fn reference_writes_include_java_assignments() {
    let f = Fixture::new();
    f.write("Use.java", "class Use {\n  int target = 0;\n  void entry() {\n    target = 1;\n    target += 2;\n    consume(target);\n  }\n}\n");
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"target","refKind":"read"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [4, 5]);
    assert_eq!(lines(&data[1]), [6]);
}

#[test]
fn references_separate_parameter_and_variable_declarations_from_reads() {
    for (file, source, declarations, reads) in [
        ("Use.kt", "fun entry(target: Int) {\n    println(target)\n    val local = target\n}\nclass Box(val target: Int) {\n    val copy = target\n}\n", vec![1, 5], vec![2, 3, 6]),
        ("Use.java", "class Use {\n  int target = 1;\n  void entry(int target) {\n    consume(target);\n    int local = target;\n  }\n  void other() {\n    int target = 2;\n    int local = target;\n  }\n}\n", vec![2, 3, 8], vec![4, 5, 9]),
    ] {
        let f = Fixture::new();
        f.write(file, source);
        let out = f.query(json!([
            {"type":"references","name":"target","refKind":"declaration"},
            {"type":"references","name":"target","refKind":"read"}
        ]), true);
        assert!(out.status.success(), "{out:?}");
        let data = items(&out);
        assert_eq!(lines(&data[0]), declarations, "{file}");
        assert_eq!(lines(&data[1]), reads, "{file}");
    }
}

#[test]
fn explicit_import_filter_includes_kotlin_alias_only_in_import_context() {
    let f = Fixture::new();
    f.write(
        "Use.kt",
        "import other.target as Alias\nfun entry() { Alias() }\n",
    );
    let out = f.query(
        json!([
            {"type":"references","name":"Alias","refKind":"import"},
            {"type":"references","name":"Alias","refKind":"type-use"},
            {"type":"references","name":"Alias"},
            {"type":"references","name":"Alias","refKind":"call"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [1]);
    assert!(lines(&data[1]).is_empty());
    assert_eq!(lines(&data[2]), [2]);
    assert_eq!(lines(&data[3]), [2]);
}

#[test]
fn assignment_lhs_subscript_index_and_array_are_reads() {
    let f = Fixture::new();
    f.write(
        "Use.kt",
        "fun entry(target: Int, values: IntArray) {\n    values[target] = 1\n}\n",
    );
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"read"},
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"values","refKind":"read"},
            {"type":"references","name":"values","refKind":"write"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [2]);
    assert!(lines(&data[1]).is_empty());
    assert_eq!(lines(&data[2]), [2]);
    assert!(lines(&data[3]).is_empty());
}

#[test]
fn assignment_lhs_navigation_receiver_is_read_and_field_is_write() {
    let f = Fixture::new();
    f.write(
        "Use.kt",
        "class Box(var field: Int)\nfun entry(target: Box) {\n    target.field = 1\n}\n",
    );
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"read"},
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"field","refKind":"write"},
            {"type":"references","name":"field","refKind":"read"}
        ]),
        true,
    );
    assert!(out.status.success(), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [3]);
    assert!(lines(&data[1]).is_empty());
    assert_eq!(lines(&data[2]), [3]);
    assert!(lines(&data[3]).is_empty());
}

#[test]
fn assignment_lhs_nested_members_and_indices_preserve_evaluation_reads() {
    for (file, source) in [
        ("Use.kt", "class Box(var field: Int, val values: Array<Box>)\nfun entry(target: Box, index: Int) {\n    target.values[index].field += 1\n    target.values[index] = target\n}\n"),
        ("Use.java", "class Box { int field; Box[] values;\nvoid entry(Box target, int index) {\n    target.values[index].field += 1;\n    target.values[index] = target;\n}}\n"),
    ] {
        let f = Fixture::new();
        f.write(file, source);
        let out = f.query(
            json!([
                {"type":"references","name":"target","refKind":"read"},
                {"type":"references","name":"target","refKind":"write"},
                {"type":"references","name":"values","refKind":"read"},
                {"type":"references","name":"values","refKind":"write"},
                {"type":"references","name":"index","refKind":"read"},
                {"type":"references","name":"index","refKind":"write"},
                {"type":"references","name":"field","refKind":"write"},
                {"type":"references","name":"field","refKind":"read"}
            ]),
            true,
        );
        assert!(out.status.success(), "{file}: {out:?}");
        let data = items(&out);
        assert_eq!(lines(&data[0]), [3, 4, 4], "{file}");
        assert!(lines(&data[1]).is_empty(), "{file}");
        assert_eq!(lines(&data[2]), [3, 4], "{file}");
        assert!(lines(&data[3]).is_empty(), "{file}");
        assert_eq!(lines(&data[4]), [3, 4], "{file}");
        assert!(lines(&data[5]).is_empty(), "{file}");
        assert_eq!(lines(&data[6]), [3], "{file}");
        assert!(lines(&data[7]).is_empty(), "{file}");
    }
}

#[test]
fn malformed_or_non_array_stdin_fails_without_results() {
    let f = Fixture::new();
    for input in ["", "[", "{}", "null", "42", "\"text\""] {
        let mut cmd = f.command();
        cmd.args(["tool", "query", "--json", "--no-stdlib", "--root"])
            .arg(&f.root);
        let out = f.raw_query(cmd, input.as_bytes());
        assert_eq!(out.status.code(), Some(1), "{input}: {out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("Invalid query JSON:"),
            "{out:?}"
        );
    }
}

#[test]
fn empty_batch_returns_an_empty_array() {
    let f = Fixture::new();
    let out = f.query(json!([]), true);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[]\n");
}

#[test]
fn invalid_item_shapes_and_numeric_types_preserve_following_success() {
    let f = Fixture::new();
    f.write("Use.kt", "fun target() {}\n");
    let invalid = [
        (json!(null), "unknown", "invalid type"),
        (json!(42), "unknown", "invalid type"),
        (json!({}), "unknown", "missing field `type`"),
        (
            json!({"type":"definition"}),
            "definition",
            "missing field `name`",
        ),
        (
            json!({"type":"references","name":"target","refKind":7}),
            "references",
            "invalid type",
        ),
        (
            json!({"type":"callers","file":"Use.kt","line":1,"col":5,"depth":-1}),
            "callers",
            "invalid value",
        ),
        (
            json!({"type":"callers","file":"Use.kt","line":1,"col":5,"depth":1.5}),
            "callers",
            "invalid type",
        ),
        (
            json!({"type":"hover","file":"Use.kt","line":-1,"col":5}),
            "hover",
            "invalid value",
        ),
        (
            json!({"type":"hover","file":"Use.kt","line":1,"col":1.5}),
            "hover",
            "invalid type",
        ),
    ];
    let mut specs: Vec<Value> = invalid.iter().map(|(value, _, _)| value.clone()).collect();
    specs.push(json!({"type":"definition","name":"target"}));
    let out = f.query(json!(specs), true);
    assert_eq!(out.status.code(), Some(1));
    let data = items(&out);
    assert_eq!(data.len(), invalid.len() + 1);
    for (item, (_, kind, message)) in data.iter().zip(&invalid) {
        assert_eq!(item["type"], *kind);
        assert!(
            item["error"]
                .as_str()
                .expect("item error")
                .contains(message),
            "{item}"
        );
    }
    assert_eq!(lines(data.last().expect("success")), [1]);
}

#[test]
fn text_batch_preserves_order_and_item_failure_exit() {
    let f = Fixture::new();
    f.write("Use.kt", "fun target() {}\nfun entry() { target() }\n");
    let mut cmd = f.command();
    cmd.args(["tool", "query", "--no-stdlib", "--root"])
        .arg(&f.root);
    let out = f.raw_query(cmd, br#"[{"type":"definition","name":"target"},{"type":"references","name":"target","refKind":"bogus"},{"type":"references","name":"target","refKind":"call"}]"#);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<_> = text.lines().collect();
    assert_eq!(rows.len(), 3, "{text}");
    let mut data = Vec::new();
    for (row, prefix) in rows
        .iter()
        .zip(["[definition] ", "[references] ", "[references] "])
    {
        data.push(
            serde_json::from_str::<Value>(row.strip_prefix(prefix).expect("ordered type label"))
                .expect("text payload"),
        );
    }
    assert_eq!(lines(&data[0]), [1]);
    assert_eq!(data[1]["error"], "invalid refKind: bogus");
    assert_eq!(lines(&data[2]), [2]);
    assert_eq!(data[2]["filter_applied"], "call");
}

#[test]
fn file_operands_use_explicit_root_or_nested_cwd_not_discovered_ancestor() {
    let f = Fixture::new();
    std::fs::create_dir(f.root.join(".git")).expect("workspace marker");
    f.write("Root.kt", "class RootOnly\n");
    f.write(
        "nested/Use.kt",
        "fun nestedTarget(): Int = 1\nfun entry() { nestedTarget() }\n",
    );
    let nested = f.root.join("nested");
    let absolute = nested
        .join("Use.kt")
        .canonicalize()
        .expect("canonical file");
    for explicit_root in [None, Some(".."), Some(".")] {
        let relative = if explicit_root == Some("..") {
            "nested/Use.kt"
        } else {
            "Use.kt"
        };
        let mut cmd = f.command();
        cmd.current_dir(&nested)
            .args(["tool", "query", "--json", "--no-stdlib"]);
        if let Some(root) = explicit_root {
            cmd.args(["--root", root]);
        }
        let specs = json!([
            {"type":"hover","file":relative,"line":1,"col":5},
            {"type":"hover","file":absolute,"line":1,"col":5},
            {"type":"callers","file":relative,"line":1,"col":5},
            {"type":"definition","name":"nestedTarget"},
            {"type":"definition","name":"RootOnly"}
        ]);
        let out = f.raw_query(cmd, specs.to_string().as_bytes());
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let data = items(&out);
        for item in &data[..2] {
            assert_eq!(item["name"], "nestedTarget");
            assert!(item["signature"]
                .as_str()
                .expect("signature")
                .contains("fun nestedTarget"));
        }
        assert_eq!(
            data[2]["callers"],
            json!([{"name":"entry","file":tower_lsp::lsp_types::Url::from_file_path(&absolute).expect("file URI").as_str()}])
        );
        assert_eq!(
            data[3]["results"],
            json!([{"file":expected_file_path(&absolute),"line":1,"col":5}])
        );
        assert_eq!(
            lines(&data[4]),
            if explicit_root == Some(".") {
                vec![]
            } else {
                vec![1]
            }
        );
    }
}

#[test]
fn missing_explicit_root_fails_on_stderr_without_results() {
    let f = Fixture::new();
    let mut cmd = f.command();
    cmd.args([
        "tool",
        "query",
        "--json",
        "--no-stdlib",
        "--root",
        "missing-root",
    ]);
    let out = f.raw_query(cmd, br#"[{"type":"definition","name":"target"}]"#);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Invalid query root") && stderr.contains("missing-root"),
        "{stderr}"
    );
}

#[test]
fn hover_identifier_end_empty_and_surrogate_boundaries() {
    let f = Fixture::new();
    f.write("Use.kt", "fun café() {}\n\n;\nval text = \"😀\"\n");
    f.write("Empty.kt", "");
    let out = f.query(
        json!([
            {"type":"hover","file":"Use.kt","line":1,"col":9},
            {"type":"hover","file":"Use.kt","line":2,"col":1},
            {"type":"hover","file":"Use.kt","line":3,"col":1},
            {"type":"hover","file":"Empty.kt","line":1,"col":1},
            {"type":"hover","file":"Use.kt","line":4,"col":14}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(1));
    let data = items(&out);
    assert_eq!(data.len(), 5);
    assert_eq!(data[0]["name"], "café");
    assert!(data[0]["signature"]
        .as_str()
        .expect("signature")
        .contains("fun café"));
    for i in [1, 2, 4] {
        assert_eq!(
            data[i]["error"], "no identifier at position",
            "{i}: {}",
            data[i]
        );
    }
    assert_eq!(data[3]["error"], "line is outside the file");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
}

#[test]
fn ambiguous_hover_preserves_name_without_fabricating_signature() {
    let f = Fixture::new();
    f.write("A.kt", "fun target(value: Int): Int = value\n");
    f.write(
        "B.kt",
        "fun target(value: String): String = value\nfun entry() { target(1) }\n",
    );
    let out = f.query(
        json!([
            {"type":"definition","name":"target"},
            {"type":"hover","file":"B.kt","line":2,"col":15}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [1, 1]);
    assert_eq!(
        data[1],
        json!({"type":"hover","name":"target","signature":null})
    );
}

#[test]
fn import_only_files_are_discovered_but_package_tokens_are_excluded() {
    let f = Fixture::new();
    f.write("Import.kt", "import other.target\nclass ImportOnly\n");
    f.write("Package.kt", "package target\nclass PackageOnly\n");
    f.write("Use.kt", "fun target() {}\nfun entry() { target() }\n");
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"import"},
            {"type":"references","name":"target"}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let data = items(&out);
    let import = expected_file_path(&f.root.join("Import.kt"));
    let usage = expected_file_path(&f.root.join("Use.kt"));
    assert_eq!(
        data[0]["results"],
        json!([{"file":import,"line":1,"col":14}])
    );
    assert_eq!(
        data[1]["results"],
        json!([
            {"file":usage,"line":1,"col":5},
            {"file":usage,"line":2,"col":15}
        ])
    );
}

#[test]
fn same_spelling_receiver_callee_and_argument_keep_occurrence_identity() {
    for (file, source) in [
        (
            "Use.kt",
            "fun entry(target: Box) {\n    target.target(target)\n}\n",
        ),
        (
            "Use.java",
            "class Use { void entry(Box target) {\n    target.target(target);\n}}\n",
        ),
    ] {
        let f = Fixture::new();
        f.write(file, source);
        let out = f.query(
            json!([
                {"type":"references","name":"target","refKind":"call"},
                {"type":"references","name":"target","refKind":"read"},
                {"type":"references","name":"target","refKind":"write"}
            ]),
            true,
        );
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let data = items(&out);
        let path = expected_file_path(&f.root.join(file));
        assert_eq!(
            data[0]["results"],
            json!([{"file":path,"line":2,"col":12}]),
            "{file}"
        );
        assert_eq!(
            data[1]["results"],
            json!([
                {"file":path,"line":2,"col":5}, {"file":path,"line":2,"col":19}
            ]),
            "{file}"
        );
        assert_eq!(data[2]["results"], json!([]));
    }
}

#[test]
fn swift_declarations_exclude_initializer_and_interpolation_reads() {
    let f = Fixture::new();
    f.write("Use.swift", "func entry(target: Int) {\n    print(target)\n    print(\"value: \\(target)\")\n}\nvar target = 0\nlet copy = target\nlet text = \"target\" // target\n");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("Swift grammar");
    let source = std::fs::read_to_string(f.root.join("Use.swift")).expect("source");
    let tree = parser.parse(&source, None).expect("tree");
    assert!(!tree.root_node().has_error());
    let check = f
        .command()
        .args(["check", "--json"])
        .arg(f.root.join("Use.swift"))
        .output()
        .expect("syntax check");
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"declaration"},
            {"type":"references","name":"target","refKind":"read"},
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"target","refKind":"call"}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [1, 5]);
    assert_eq!(lines(&data[1]), [2, 3, 6]);
    assert!(lines(&data[2]).is_empty());
    assert!(lines(&data[3]).is_empty());
}

#[test]
fn parsed_summary_and_subtype_dispatch_are_meaningful_and_cache_stable() {
    let f = Fixture::new();
    f.write(
        "Base.kt",
        "interface Service\nopen class Base\nfun target(value: Int): Int = value\n",
    );
    f.write(
        "Child.kt",
        "class Worker : Service\nclass Child : Base()\nfun entry() { target(1) }\n",
    );
    let specs = json!([
        {"type":"summarize","name":"Service"},
        {"type":"implementations","name":"Service"},
        {"type":"subclasses","name":"Base"},
        {"type":"summarize","name":"Absent"},
        {"type":"implementations","name":"Absent"},
        {"type":"subclasses","name":"Absent"},
        {"type":"hover","file":"Base.kt","line":3,"col":5},
        {"type":"references","name":"target","refKind":"call"}
    ]);
    let cache = f.root.join(".cache/kotlin-lsp/index.bin");
    assert!(!cache.exists(), "fresh workspace starts cold");
    let cold = f.query(specs.clone(), true);
    assert!(
        std::fs::metadata(&cache)
            .expect("persisted workspace cache")
            .len()
            > 0
    );
    assert_eq!(cold.status.code(), Some(1), "{cold:?}");
    let data = items(&cold);
    assert_eq!(data.len(), 8);
    assert_eq!(
        data[0],
        json!({"type":"summarize","name":"Service","kind":"interface","visibility":"public","signature":"interface Service","deprecated":false})
    );
    let child = f.root.join("Child.kt").canonicalize().expect("child");
    let uri = tower_lsp::lsp_types::Url::from_file_path(&child).expect("URI");
    assert_eq!(
        data[1],
        json!({"type":"implementations","name":"Service","results":[{"file":uri.as_str(),"line":1}]})
    );
    assert_eq!(
        data[2],
        json!({"type":"subclasses","name":"Base","results":[{"file":uri.as_str(),"line":2}]})
    );
    assert_eq!(
        data[3],
        json!({"type":"summarize","error":"symbol not found"})
    );
    for item in &data[4..6] {
        assert_eq!(item["name"], "Absent");
        assert_eq!(item["results"], json!([]));
    }
    assert_eq!(data[6]["name"], "target");
    assert!(data[6]["signature"]
        .as_str()
        .expect("hover signature")
        .contains("fun target(value: Int): Int"));
    assert_eq!(
        data[7]["results"],
        json!([{"file":expected_file_path(&child),"line":3,"col":15}])
    );
    let warm = f.query(specs, true);
    assert_eq!(warm.status.code(), Some(1));
    assert_eq!(items(&warm), data, "cold/warm complete item parity");
}

#[test]
fn callers_are_deduplicated_and_sorted_across_files_before_twenty_cap() {
    let f = Fixture::new();
    f.write("Target.kt", "fun target() {}\n");
    // Reverse creation order makes filesystem order different from result order.
    for i in (0..23).rev() {
        f.write(
            &format!("Caller{i:02}.kt"),
            &format!("fun entry{i:02}() {{ target(); target() }}\n"),
        );
    }
    let out = f.query(
        json!([{ "type":"callers","file":"Target.kt","line":1,"col":5 }]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let expected: Vec<_> = (0..20).map(|i| {
        let path = f.root.join(format!("Caller{i:02}.kt")).canonicalize().expect("caller path");
        json!({"name":format!("entry{i:02}"),"file":tower_lsp::lsp_types::Url::from_file_path(path).expect("URI").as_str()})
    }).collect();
    assert_eq!(
        items(&out)[0],
        json!({"type":"callers","name":"target","depth":1,"callers":expected})
    );
}

#[test]
fn subtypes_are_sorted_across_files_before_fifty_cap() {
    let f = Fixture::new();
    f.write("Base.kt", "interface Service\nopen class Base\n");
    for i in (0..53).rev() {
        f.write(
            &format!("Child{i:02}.kt"),
            &format!("class Worker{i:02} : Service\nclass Child{i:02} : Base()\n"),
        );
    }
    let out = f.query(
        json!([
            {"type":"implementations","name":"Service"},
            {"type":"subclasses","name":"Base"}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let data = items(&out);
    for (item, line) in data.iter().zip([1, 2]) {
        let expected: Vec<_> = (0..50).map(|i| {
            let path = f.root.join(format!("Child{i:02}.kt")).canonicalize().expect("subtype path");
            json!({"file":tower_lsp::lsp_types::Url::from_file_path(path).expect("URI").as_str(),"line":line})
        }).collect();
        assert_eq!(item["results"], json!(expected));
    }
}

#[test]
fn swift_adjacent_assignments_select_identifier_not_statement_boundary() {
    let f = Fixture::new();
    f.write(
        "Use.swift",
        "var target = 0\nfunc change() {\n    target = 1\n    target += 2\n}\n",
    );
    let out = f.query(
        json!([
            {"type":"references","name":"target","refKind":"write"},
            {"type":"references","name":"target","refKind":"read"}
        ]),
        true,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let data = items(&out);
    assert_eq!(lines(&data[0]), [3, 4]);
    assert!(lines(&data[1]).is_empty());
}
