//! Query option contracts at the real CLI boundary; no user home or cache is used.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    dir: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let a = dir.path().join("A");
        let b = dir.path().join("B");
        for path in [&a, &b, &dir.path().join("home")] {
            std::fs::create_dir_all(path).expect("mkdir");
        }
        // B deliberately has no git ancestor: absolute B operands cannot hide ignored --root.
        std::fs::create_dir(a.join(".git")).expect("git marker");
        for (root, label) in [(&a, "A"), (&b, "B")] {
            write(
                &root.join("Shared.kt"),
                "open class Shared\nfun target() {}\n",
            );
            write(
                &root.join("Child.kt"),
                &format!("class {label}Child : Shared()\nfun {label}Entry() {{ target() }}\n"),
            );
            write(
                &root.join("SharedTest.kt"),
                &format!("fun test{label}() {{ Shared() }}\n"),
            );
            write(
                &root.join("commonMain/Platform.kt"),
                "expect fun Platform(): String\n",
            );
            write(
                &root.join("androidMain/Platform.kt"),
                &format!("actual fun Platform(): String {{\n return \"{label}\"\n}}\n"),
            );
        }
        Self { dir, a, b }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"));
        command
            .current_dir(&self.a)
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            // Ignore timing noise, never semantic CLI diagnostics.
            .env("RUST_LOG", "error");
        command
    }
    fn query(&self, args: &[&str], root: Option<&Path>) -> Output {
        let mut command = self.command();
        command.args(args).arg("--json");
        if let Some(root) = root {
            command.arg("--root").arg(root);
        }
        command.output().expect("query process")
    }
}
fn write(path: &Path, source: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, source).expect("write fixture");
}
fn success(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("JSON response")
}
#[path = "support/p6_fixture.rs"]
mod p6_fixture;

fn path_identity(value: &Value) -> PathBuf {
    // Semantic search retains escaped URI tails (`/C:/…` on Windows), while
    // other commands report native paths or complete URIs. Compare the full
    // filesystem identity; never discard directories or decode native `%`.
    p6_fixture::canonical_reported_file(value.as_str().expect("path or URI"))
}
fn assert_path(value: &Value, expected: &Path) {
    assert_eq!(
        path_identity(value),
        expected.canonicalize().expect("expected path")
    );
}
fn root_summary(cached: bool) {
    let f = Fixture::new();
    let mut args = vec!["search", "summarize", "Shared", "--no-stdlib"];
    if cached {
        args.push("--cached");
    }
    for (root, expected) in [
        (None, &f.a),
        (Some(f.a.as_path()), &f.a),
        (Some(f.b.as_path()), &f.b),
    ] {
        let result = success(&f.query(&args, root));
        let summary = if cached {
            assert_eq!(result.as_array().expect("summaries").len(), 1);
            &result[0]
        } else {
            &result
        };
        assert_eq!(summary["name"], "Shared");
        assert_path(&summary["file"], &expected.join("Shared.kt"));
    }
}
#[test]
fn summarize_root_selects_b_not_cwd_a() {
    root_summary(false);
}
#[test]
fn summarize_cached_root_selects_b_not_cwd_a() {
    root_summary(true);
}
#[test]
fn type_hierarchy_root_selects_b_not_cwd_a() {
    let f = Fixture::new();
    for (root, expected) in [
        (None, &f.a),
        (Some(f.a.as_path()), &f.a),
        (Some(f.b.as_path()), &f.b),
    ] {
        let result = success(&f.query(&["type", "hierarchy", "Shared", "--subtypes"], root));
        let subs = result["subtypes"].as_array().expect("subtypes");
        assert_eq!(subs.len(), 1);
        assert_path(&subs[0]["uri"], &expected.join("Child.kt"));
    }
}
#[test]
fn expect_actual_root_selects_b_not_cwd_a() {
    let f = Fixture::new();
    for (root, expected) in [
        (None, &f.a),
        (Some(f.a.as_path()), &f.a),
        (Some(f.b.as_path()), &f.b),
    ] {
        let result = success(&f.query(
            &["search", "expect-actual", "Platform", "--no-stdlib"],
            root,
        ));
        assert_eq!(result["expect_name"], "Platform");
        assert_path(
            &result["expect_file"],
            &expected.join("commonMain/Platform.kt"),
        );
        assert_eq!(result["actuals"].as_array().expect("actuals").len(), 1);
        assert_path(
            &result["actuals"][0]["file"],
            &expected.join("androidMain/Platform.kt"),
        );
    }
}
fn file_root(member: &str) {
    let f = Fixture::new();
    for (root, expected, label) in [
        (None, &f.a, "A"),
        (Some(f.a.as_path()), &f.a, "A"),
        (Some(f.b.as_path()), &f.b, "B"),
    ] {
        let file = expected.join("Shared.kt");
        let mut args = if member == "find-test" {
            vec!["search", member]
        } else {
            vec![member]
        };
        args.extend([
            file.to_str().expect("file"),
            if member == "impact" { "2" } else { "1" },
            if member == "impact" { "5" } else { "12" },
            "--no-stdlib",
        ]);
        let output = f.query(&args, root);
        assert!(
            output.status.success(),
            "{member} root={root:?} file={file:?}: {output:?}"
        );
        let result = success(&output);
        match member {
            "context" => {
                assert_eq!(result["name"], "Shared");
                assert_eq!(
                    result["definitions"].as_array().expect("definitions").len(),
                    1
                );
                assert_path(&result["definitions"][0]["uri"], &file);
            }
            "impact" => {
                assert_eq!(result["symbol"], "target");
                assert_eq!(
                    result["direct_callers"].as_array().expect("callers").len(),
                    1
                );
                assert_eq!(result["direct_callers"][0]["name"], format!("{label}Entry"));
                assert_path(
                    &result["direct_callers"][0]["file"],
                    &expected.join("Child.kt"),
                );
            }
            "find-test" => {
                assert_eq!(result["symbol"], "Shared");
                assert_eq!(result["tests"].as_array().expect("tests").len(), 1);
                assert_eq!(result["tests"][0]["test_name"], format!("test{label}"));
                assert_path(&result["tests"][0]["file"], &expected.join("SharedTest.kt"));
            }
            _ => unreachable!("fixed query cases"),
        }
    }
}
#[test]
fn context_root_selects_b_not_cwd_a() {
    file_root("context");
}
#[test]
fn impact_root_selects_b_not_cwd_a() {
    file_root("impact");
}
#[test]
fn find_test_root_selects_b_not_cwd_a() {
    file_root("find-test");
}

fn failure(output: &Output, diagnostic: &str) {
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(diagnostic),
        "{output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked"),
        "{output:?}"
    );
}
fn library_cache_files(f: &Fixture) -> Vec<PathBuf> {
    let cache = f.dir.path().join("cache/kotlin-lsp");
    if !cache.exists() {
        return vec![];
    }
    std::fs::read_dir(cache)
        .expect("cache directory")
        .map(|entry| entry.expect("cache entry").path())
        .filter(|path| {
            path.file_name()
                .expect("file name")
                .to_string_lossy()
                .starts_with("library-")
                && path.extension().is_some_and(|ext| ext == "bin")
        })
        .collect()
}
fn library_query(f: &Fixture, consumer: &str, library: &Path, disabled: bool) -> Output {
    let use_home = f.a.join("Use.kt");
    let mut args = match consumer {
        "find" => vec!["find", "HomeOnly", "--smart", "--absolute"],
        "refs" => vec![
            "refs",
            "target",
            "--smart",
            "--absolute",
            "--ref-kind",
            "call",
        ],
        "hover" => vec![
            "hover",
            use_home.to_str().expect("use file"),
            "2",
            "3",
            "--smart",
        ],
        "context" => vec!["context", library.to_str().expect("library file"), "1", "7"],
        "impact" => vec!["impact", library.to_str().expect("library file"), "2", "22"],
        "find-test" => vec![
            "search",
            "find-test",
            library.to_str().expect("library file"),
            "1",
            "7",
        ],
        "inspect" => vec!["tool", "inspect", library.to_str().expect("library file")],
        "summarize" => vec!["search", "summarize", "HomeOnly"],
        "cached" => vec!["search", "summarize", "HomeOnly", "--cached"],
        _ => unreachable!("fixed consumer cases"),
    };
    if disabled {
        args.push("--no-stdlib");
    }
    f.query(&args, Some(&f.a))
}
fn assert_library_query(
    f: &Fixture,
    consumer: &str,
    library: &Path,
    disabled: bool,
) -> Option<Value> {
    let output = library_query(f, consumer, library, disabled);
    if disabled {
        match consumer {
            "refs" => {
                let value = success(&output);
                let results = value.as_array().expect("references");
                assert_eq!(results.len(), 1, "{value}");
                assert_path(&results[0]["file"], &f.a.join("Child.kt"));
            }
            "inspect" => assert_eq!(success(&output)["symbols"], serde_json::json!([])),
            "context" | "impact" | "find-test" => failure(&output, "No symbol at cursor"),
            "hover" => failure(&output, "No symbol found"),
            "summarize" => failure(&output, "Symbol not found: HomeOnly"),
            "cached" => failure(&output, "No cached summary found for 'HomeOnly'"),
            "find" => failure(&output, ""),
            _ => unreachable!("fixed consumers"),
        }
        return None;
    }
    let value = success(&output);
    match consumer {
        "find" => {
            assert_eq!(value.as_array().expect("definitions").len(), 1);
            assert_eq!(value[0]["name"], "HomeOnly");
            assert_path(&value[0]["file"], library);
        }
        "refs" => {
            // Indexed refs still discovers candidates using workspace-scoped rg.
            assert_eq!(value.as_array().expect("references").len(), 1);
            assert_path(&value[0]["file"], &f.a.join("Child.kt"));
        }
        "hover" => assert_eq!(value["signature"], "class HomeOnly"),
        "context" => {
            assert_eq!(value["name"], "HomeOnly");
            assert_eq!(
                value["definitions"].as_array().expect("definitions").len(),
                1
            );
            assert_path(&value["definitions"][0]["uri"], library);
        }
        "impact" => {
            // Impact's callers are rg-scoped to the workspace even when the cursor is in a library.
            assert_eq!(value["symbol"], "target");
            assert_eq!(
                value["direct_callers"].as_array().expect("callers").len(),
                1
            );
            assert_eq!(value["direct_callers"][0]["name"], "AEntry");
        }
        "find-test" => {
            assert_eq!(value["symbol"], "HomeOnly");
            assert_eq!(value["tests"].as_array().expect("tests").len(), 1);
            assert_path(&value["tests"][0]["file"], &f.a.join("HomeOnlyTest.kt"));
        }
        "inspect" => assert_eq!(
            value["symbols"],
            serde_json::json!(["HomeOnly", "homeEntry"])
        ),
        "summarize" => {
            assert_eq!(value["name"], "HomeOnly");
            assert_path(&value["file"], library);
        }
        "cached" => {
            assert_eq!(value.as_array().expect("summaries").len(), 1);
            assert_eq!(value[0]["name"], "HomeOnly");
            assert_path(&value[0]["file"], library);
        }
        _ => unreachable!("fixed consumers"),
    }
    Some(value)
}
fn no_stdlib(consumer: &str) {
    let f = Fixture::new();
    let library = f.dir.path().join("home/.kotlin-lsp/sources/lib/Library.kt");
    write(&library, "class HomeOnly\nfun homeEntry() { target() }\n");
    write(&f.a.join("Use.kt"), "fun useHome() {\n  HomeOnly()\n}\n");
    write(
        &f.a.join("HomeOnlyTest.kt"),
        "fun testHome() { HomeOnly() }\n",
    );
    let index = f.query(&["index", "--no-stdlib"], Some(&f.a));
    assert!(index.status.success(), "{index:?}");
    assert!(
        library_cache_files(&f).is_empty(),
        "prebuild excludes home library"
    );
    let mut cold_output = None;
    for (step, disabled) in [true, false, false, true].into_iter().enumerate() {
        let output = assert_library_query(&f, consumer, &library, disabled);
        if step == 1 {
            cold_output = output;
        } else if step == 2 {
            assert_eq!(output, cold_output, "complete cold/warm output: {consumer}");
        }
        if step == 0 {
            assert!(
                library_cache_files(&f).is_empty(),
                "disabled-cold query must not cache home sources: {consumer}"
            );
        }
        // A workspace symbol remains queryable in both modes.
        let result = success(&f.query(
            &["find", "Shared", "--smart", "--absolute", "--no-stdlib"],
            Some(&f.a),
        ));
        assert_eq!(result.as_array().expect("workspace definitions").len(), 1);
        assert_path(&result[0]["file"], &f.a.join("Shared.kt"));
        if !disabled {
            let caches = library_cache_files(&f);
            assert!(caches.len() >= 2, "full and compact library caches exist");
            for cache in caches {
                assert!(std::fs::metadata(cache).expect("cache metadata").len() > 0);
            }
        }
    }
}
#[test]
fn indexed_find_no_stdlib_cold_warm() {
    no_stdlib("find");
}
#[test]
fn indexed_refs_no_stdlib_cold_warm() {
    no_stdlib("refs");
}
#[test]
fn indexed_hover_no_stdlib_cold_warm() {
    no_stdlib("hover");
}
#[test]
fn context_no_stdlib_cold_warm() {
    no_stdlib("context");
}
#[test]
fn impact_no_stdlib_cold_warm() {
    no_stdlib("impact");
}
#[test]
fn find_test_no_stdlib_cold_warm() {
    no_stdlib("find-test");
}
#[test]
fn inspect_no_stdlib_cold_warm() {
    no_stdlib("inspect");
}
#[test]
fn summarize_no_stdlib_cold_warm() {
    no_stdlib("summarize");
}
#[test]
fn summarize_cached_no_stdlib_cold_warm() {
    no_stdlib("cached");
}

#[test]
fn expect_actual_no_stdlib_preserves_workspace_rg_scope_and_skips_home_cache() {
    let f = Fixture::new();
    let library = f.dir.path().join("home/.kotlin-lsp/sources/lib/Home.kt");
    write(&library, "expect fun HomePlatform(): String\n");
    for (step, disabled) in [true, false, false, true].into_iter().enumerate() {
        let mut args = vec!["search", "expect-actual", "Platform"];
        if disabled {
            args.push("--no-stdlib");
        }
        let value = success(&f.query(&args, Some(&f.b)));
        assert_path(&value["expect_file"], &f.b.join("commonMain/Platform.kt"));
        assert_eq!(value["actuals"].as_array().expect("actuals").len(), 1);
        assert_path(
            &value["actuals"][0]["file"],
            &f.b.join("androidMain/Platform.kt"),
        );
        if step == 0 {
            assert!(
                library_cache_files(&f).is_empty(),
                "disabled does not cache home"
            );
        }
        if !disabled {
            assert!(library_cache_files(&f).len() >= 2, "enabled cache exists");
        }
        args[2] = "HomePlatform";
        let home = success(&f.query(&args, Some(&f.b)));
        assert_eq!(
            home,
            serde_json::json!({"expect_name":"HomePlatform", "expect_file":"", "expect_line":0, "actuals":[]})
        );
    }
}

#[test]
fn indexed_find_native_library_kind_parity_and_nonhome_source_selection() {
    for home_source in [false, true] {
        let f = Fixture::new();
        let library_root = if home_source {
            f.dir
                .path()
                .join("home/.kotlin-lsp/sources/escaped % # 中文")
        } else {
            f.dir.path().join("external escaped % # 中文")
        };
        let library = library_root.join("Library % # 中文.kt");
        write(
            &library,
            "class HomeOnly\nfun externalFunction() {}\nval externalProperty = 1\n",
        );
        if !home_source {
            write(&f.a.join("workspace.json"), &serde_json::json!({
                "modules": [{"contentRoots": [{"sourceRoots": [{"path": library_root, "type": "java-source"}]}]}]
            }).to_string());
        }
        let index = f.query(&["index", "--no-stdlib"], Some(&f.a));
        assert!(index.status.success(), "{index:?}");
        for (name, kind) in [
            ("HomeOnly", "class"),
            ("externalFunction", "function"),
            ("externalProperty", "property"),
        ] {
            let mut enabled = None;
            for disabled in [true, false, false, true] {
                let mut args = vec!["find", name, "--smart", "--absolute"];
                if disabled {
                    args.push("--no-stdlib");
                }
                let output = f.query(&args, Some(&f.a));
                if home_source && disabled {
                    failure(&output, "");
                } else {
                    let value = success(&output);
                    assert_eq!(value.as_array().expect("definitions").len(), 1);
                    assert_eq!(value[0]["name"], name);
                    assert_eq!(value[0]["kind"], kind);
                    assert_path(&value[0]["file"], &library);
                    if let Some(ref previous) = enabled {
                        assert_eq!(&value, previous);
                    }
                    enabled = Some(value);
                }
                let workspace = success(&f.query(
                    &["find", "Shared", "--smart", "--absolute", "--no-stdlib"],
                    Some(&f.a),
                ));
                assert_path(&workspace[0]["file"], &f.a.join("Shared.kt"));
            }
        }
    }
}

#[test]
fn cached_summary_discovers_all_requested_files_without_private_symbols() {
    let f = Fixture::new();
    let library = f.dir.path().join("home/.kotlin-lsp/sources/lib");
    let files = [
        f.a.join("Requested.kt"),
        library.join("One.kt"),
        library.join("Two.kt"),
    ];
    for (file, package) in files.iter().zip(["workspace", "one", "two"]) {
        write(
            file,
            &format!("package {package}\n/** Requested documentation. */\nclass Requested\n"),
        );
    }
    write(
        &library.join("Unrelated.kt"),
        "class Unrelated\nprivate class Hidden\n",
    );
    let mut cold = None;
    for disabled in [true, false, false, true] {
        let mut args = vec!["search", "summarize", "Requested", "--cached"];
        if disabled {
            args.push("--no-stdlib");
        }
        let value = success(&f.query(&args, Some(&f.a)));
        let mut summaries = value.as_array().expect("summaries").clone();
        summaries.sort_by_key(|summary| path_identity(&summary["file"]));
        assert_eq!(summaries.len(), if disabled { 1 } else { 3 });
        let mut expected: Vec<_> = files
            .iter()
            .take(if disabled { 1 } else { 3 })
            .map(|file| file.canonicalize().expect("file"))
            .collect();
        expected.sort();
        assert_eq!(
            summaries
                .iter()
                .map(|summary| path_identity(&summary["file"]))
                .collect::<Vec<_>>(),
            expected
        );
        for summary in &summaries {
            assert_eq!(summary["name"], "Requested");
            assert_eq!(summary["signature"], "class Requested");
            assert_eq!(summary["doc"], "Requested documentation.");
        }
        if !disabled {
            if let Some(ref previous) = cold {
                assert_eq!(&summaries, previous, "all summary fields cold/warm");
            }
            cold = Some(summaries);
        }
    }
    failure(
        &f.query(&["search", "summarize", "Hidden", "--cached"], Some(&f.a)),
        "No cached summary found",
    );
    failure(
        &f.query(&["search", "summarize", "Missing"], Some(&f.a)),
        "Symbol not found",
    );
}

#[test]
fn indexed_hover_ambiguity_includes_warm_library_candidates() {
    let f = Fixture::new();
    write(&f.a.join("Target.kt"), "class Target\n");
    write(&f.a.join("Use.kt"), "fun use() {\n  Target()\n}\n");
    write(
        &f.dir.path().join("home/.kotlin-lsp/sources/lib/Target.kt"),
        "package library\nclass Target\n",
    );
    let index = f.query(&["index", "--no-stdlib"], Some(&f.a));
    assert!(index.status.success(), "{index:?}");
    for disabled in [true, false, false, true] {
        let mut args = vec!["hover", "Use.kt", "2", "3", "--smart"];
        if disabled {
            args.push("--no-stdlib");
        }
        let output = f.query(&args, Some(&f.a));
        if disabled {
            assert_eq!(success(&output)["signature"], "class Target");
        } else {
            failure(&output, "No symbol found");
        }
    }
}

#[test]
fn selected_queries_reject_missing_and_file_roots_before_results() {
    let f = Fixture::new();
    let cases = [
        vec!["context", "Shared.kt", "1", "12"],
        vec!["impact", "Shared.kt", "2", "5"],
        vec!["search", "find-test", "Shared.kt", "1", "12"],
        vec!["search", "summarize", "Shared"],
        vec!["search", "summarize", "Shared", "--cached"],
        vec!["search", "expect-actual", "Platform"],
        vec!["type", "hierarchy", "Shared", "--subtypes"],
        vec!["find", "Shared", "--smart"],
        vec!["refs", "Shared", "--smart"],
        vec!["hover", "Shared.kt", "1", "12", "--smart"],
        vec!["tool", "inspect", "Shared.kt"],
    ];
    for mut args in cases {
        args.push("--no-stdlib");
        for (root, message) in [
            (f.b.join("missing"), "does not exist"),
            (f.b.join("Shared.kt"), "not a directory"),
        ] {
            failure(&f.query(&args, Some(&root)), message);
        }
    }
}

#[test]
fn selected_file_queries_reject_unreadable_operands_without_panic_or_results() {
    let f = Fixture::new();
    std::fs::write(f.a.join("Invalid.kt"), [0xff, 0xfe]).expect("invalid UTF-8 fixture");
    let absolute_missing = f.b.join("Missing.kt");
    for file in [
        "Missing.kt",
        "Invalid.kt",
        ".",
        absolute_missing.to_str().expect("path"),
    ] {
        for mut args in [
            vec!["context", file, "1", "1"],
            vec!["impact", file, "1", "1"],
            vec!["search", "find-test", file, "1", "1"],
            vec!["hover", file, "1", "1"],
            vec!["tool", "inspect", file],
        ] {
            args.push("--no-stdlib");
            failure(&f.query(&args, Some(&f.b)), "Cannot read file");
        }
    }
}

#[test]
fn selected_cursor_queries_reject_invalid_positions_and_arguments() {
    let f = Fixture::new();
    write(&f.a.join("Empty.kt"), "");
    for prefix in [
        vec!["context"],
        vec!["impact"],
        vec!["search", "find-test"],
        vec!["hover"],
    ] {
        for (file, line, col, diagnostic) in [
            ("Shared.kt", "0", "1", "LINE must be >= 1"),
            ("Shared.kt", "1", "0", "COL must be >= 1"),
            (
                "Shared.kt",
                "invalid",
                "1",
                "LINE must be a positive integer",
            ),
            (
                "Shared.kt",
                "1",
                "invalid",
                "COL must be a positive integer",
            ),
            ("Shared.kt", "999", "1", "No symbol"),
            ("Shared.kt", "1", "999", "No symbol"),
            ("Empty.kt", "1", "1", "No symbol"),
        ] {
            let mut args = prefix.clone();
            args.extend([file, line, col, "--no-stdlib"]);
            failure(&f.query(&args, Some(&f.a)), diagnostic);
        }
        for operands in [vec![], vec!["Shared.kt"], vec!["Shared.kt", "1"]] {
            let mut args = prefix.clone();
            args.extend(operands);
            failure(&f.query(&args, Some(&f.a)), "requires");
        }
    }
    for args in [
        vec!["type", "hierarchy"],
        vec!["search", "summarize"],
        vec!["search", "expect-actual"],
        vec!["tool", "inspect"],
        vec!["find"],
        vec!["refs"],
    ] {
        failure(&f.query(&args, Some(&f.a)), "requires");
    }
}

#[test]
fn alternate_index_root_does_not_rebase_same_named_relative_file_operands() {
    let f = Fixture::new();
    let a_file = f.a.join("Operand.kt");
    let b_file = f.b.join("Operand.kt");
    write(&a_file, "class CwdOnly\n");
    write(&b_file, "class RootOnly\n");
    write(
        &f.b.join("RootOnlyTest.kt"),
        "fun testRoot() { RootOnly() }\n",
    );
    for member in ["context", "impact", "find-test", "hover", "inspect"] {
        for absolute in [false, true] {
            let operand = if absolute {
                b_file.to_str().expect("path")
            } else {
                "Operand.kt"
            };
            let mut args = match member {
                "find-test" => vec!["search", member, operand, "1", "7"],
                "inspect" => vec!["tool", member, operand],
                _ => vec![member, operand, "1", "7"],
            };
            args.push("--no-stdlib");
            let output = f.query(&args, Some(&f.b));
            if !absolute && matches!(member, "context" | "impact" | "find-test") {
                // The cwd file is not in B's index. Rebasing would falsely succeed as RootOnly.
                failure(&output, "No symbol at cursor");
                continue;
            }
            let value = success(&output);
            match member {
                "context" => {
                    assert_eq!(value["name"], "RootOnly");
                    assert_eq!(
                        value["definitions"].as_array().expect("definitions").len(),
                        1
                    );
                    assert_path(&value["definitions"][0]["uri"], &b_file);
                }
                "impact" => {
                    assert_eq!(value["symbol"], "RootOnly");
                    assert_path(&value["file"], &b_file);
                }
                "find-test" => {
                    assert_eq!(value["symbol"], "RootOnly");
                    assert_eq!(value["tests"].as_array().expect("tests").len(), 1);
                    assert_path(&value["tests"][0]["file"], &f.b.join("RootOnlyTest.kt"));
                }
                "hover" => assert_eq!(
                    value["signature"],
                    if absolute {
                        "class RootOnly"
                    } else {
                        "class CwdOnly"
                    }
                ),
                "inspect" => assert_eq!(
                    value["symbols"],
                    if absolute {
                        serde_json::json!(["RootOnly"])
                    } else {
                        serde_json::json!([])
                    }
                ),
                _ => unreachable!("selected members"),
            }
        }
    }
}

#[test]
fn rootless_queries_discover_ancestor_from_nested_cwd_without_rebasing_files() {
    let f = Fixture::new();
    let nested = f.a.join("nested");
    write(
        &nested.join("Use.kt"),
        "fun nestedUse() {\n Shared()\n target()\n}\n",
    );
    for args in [
        vec!["context", "Use.kt", "2", "2"],
        vec!["impact", "Use.kt", "3", "2"],
        vec!["search", "find-test", "Use.kt", "2", "2"],
        vec!["search", "summarize", "Shared"],
        vec!["search", "summarize", "Shared", "--cached"],
        vec!["search", "expect-actual", "Platform"],
        vec!["type", "hierarchy", "Shared", "--subtypes"],
    ] {
        let output = f
            .command()
            .current_dir(&nested)
            .args(&args)
            .args(["--json", "--no-stdlib"])
            .output()
            .expect("nested query");
        let value = success(&output);
        match args[0] {
            "context" => {
                assert_eq!(value["name"], "Shared");
                assert_path(&value["definitions"][0]["uri"], &f.a.join("Shared.kt"));
            }
            "impact" => {
                assert_eq!(value["symbol"], "target");
                let callers = value["direct_callers"].as_array().expect("callers");
                assert_eq!(callers.len(), 2);
                assert!(callers.iter().any(|caller| caller["name"] == "AEntry"
                    && path_identity(&caller["file"])
                        == f.a.join("Child.kt").canonicalize().expect("child")));
            }
            "type" => {
                assert_eq!(value["subtypes"].as_array().expect("subtypes").len(), 1);
                assert_path(&value["subtypes"][0]["uri"], &f.a.join("Child.kt"));
            }
            _ => match args[1] {
                "find-test" => {
                    assert_eq!(value["symbol"], "Shared");
                    assert_eq!(value["tests"].as_array().expect("tests").len(), 1);
                    assert_path(&value["tests"][0]["file"], &f.a.join("SharedTest.kt"));
                }
                "expect-actual" => {
                    assert_path(&value["expect_file"], &f.a.join("commonMain/Platform.kt"))
                }
                "summarize" => assert_path(
                    if args.contains(&"--cached") {
                        &value[0]["file"]
                    } else {
                        &value["file"]
                    },
                    &f.a.join("Shared.kt"),
                ),
                _ => unreachable!("named queries"),
            },
        }
    }
}

#[test]
fn find_refs_hover_modes_respect_index_precondition_and_fast_workspace_scope() {
    let f = Fixture::new();
    let home = f.dir.path().join("home/.kotlin-lsp/sources/lib/Home.kt");
    write(&home, "class HomeOnly\n");
    for args in [
        vec!["find", "Shared", "--smart"],
        vec!["refs", "target", "--smart"],
        vec!["hover", "Shared.kt", "1", "12", "--smart"],
    ] {
        failure(&f.query(&args, Some(&f.a)), "requires a pre-built index");
    }
    for mode in [None, Some("--fast")] {
        let mut args = vec!["find", "Shared", "--absolute"];
        if let Some(mode) = mode {
            args.push(mode);
        }
        let value = success(&f.query(&args, Some(&f.b)));
        assert_eq!(value.as_array().expect("definitions").len(), 1);
        assert_path(&value[0]["file"], &f.b.join("Shared.kt"));
        args[1] = "HomeOnly";
        failure(&f.query(&args, Some(&f.b)), "");
    }
    for disabled in [false, true] {
        let mut args = vec!["refs", "target", "--fast", "--absolute"];
        if disabled {
            args.push("--no-stdlib");
        }
        let value = success(&f.query(&args, Some(&f.b)));
        let results = value.as_array().expect("fast references");
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|item| path_identity(&item["file"]).starts_with(f.b.canonicalize().expect("B"))));
        assert!(results.iter().any(|item| path_identity(&item["file"])
            == f.b.join("Child.kt").canonicalize().expect("child")));
    }
    failure(
        &f.query(&["hover", "Shared.kt", "1", "12", "--fast"], Some(&f.a)),
        "hover requires index",
    );
    assert!(
        library_cache_files(&f).is_empty(),
        "fast mode never loads home sources"
    );
    let index = f.query(&["index", "--no-stdlib"], Some(&f.a));
    assert!(index.status.success(), "{index:?}");
    let find = success(&f.query(&["find", "HomeOnly", "--smart", "--absolute"], Some(&f.a)));
    assert_eq!(find.as_array().expect("library definitions").len(), 1);
    assert_path(&find[0]["file"], &home);
    let refs = success(&f.query(
        &[
            "refs",
            "target",
            "--smart",
            "--absolute",
            "--ref-kind",
            "call",
            "--no-stdlib",
        ],
        Some(&f.a),
    ));
    assert_eq!(refs.as_array().expect("calls").len(), 1);
    assert_path(&refs[0]["file"], &f.a.join("Child.kt"));
    assert_eq!(
        success(&f.query(
            &["hover", "Shared.kt", "1", "12", "--smart", "--no-stdlib"],
            Some(&f.a)
        ))["signature"],
        "open class Shared"
    );
    failure(&f.query(&["find", "HomeOnly", "--fast"], Some(&f.a)), "");
}

#[test]
fn type_hierarchy_preserves_home_library_exclusion_even_after_enabled_index() {
    let f = Fixture::new();
    let home = f.dir.path().join("home/.kotlin-lsp/sources/lib/Home.kt");
    write(
        &home,
        "class HomeChild : Shared()\nopen class HomeBase\nclass HomeSub : HomeBase()\n",
    );
    let mut cold = None;
    for warm in [false, true] {
        if warm {
            let index = f.query(&["index"], Some(&f.a));
            assert!(index.status.success(), "{index:?}");
            let value =
                success(&f.query(&["find", "HomeBase", "--smart", "--absolute"], Some(&f.a)));
            assert_path(&value[0]["file"], &home);
        }
        for disabled in [false, true] {
            let mut args = vec!["type", "hierarchy", "Shared", "--subtypes"];
            if disabled {
                args.push("--no-stdlib");
            }
            let value = success(&f.query(&args, Some(&f.a)));
            assert_eq!(value["subtypes"].as_array().expect("subtypes").len(), 1);
            assert_path(&value["subtypes"][0]["uri"], &f.a.join("Child.kt"));
            if let Some(previous) = &cold {
                assert_eq!(&value, previous);
            }
            cold = Some(value);
            args[2] = "HomeBase";
            assert_eq!(
                success(&f.query(&args, Some(&f.a))),
                serde_json::json!({"name":"HomeBase","subtypes":[]})
            );
        }
        if !warm {
            assert!(library_cache_files(&f).is_empty());
        }
    }
}

#[test]
fn query_options_preserve_kotlin_java_swift_files_and_utf16_positions() {
    for (extension, declaration, usage) in [
        (
            "kt",
            "class UnicodeTarget\n",
            "fun use() {\n  /* 😀 */ UnicodeTarget()\n}\n",
        ),
        (
            "java",
            "class UnicodeTarget {}\n",
            "class Use { void use() {\n  /* 😀 */ UnicodeTarget value = null;\n} }\n",
        ),
        (
            "swift",
            "class UnicodeTarget {}\n",
            "func use() {\n  /* 😀 */ UnicodeTarget()\n}\n",
        ),
    ] {
        let f = Fixture::new();
        let target = f.b.join(format!("UnicodeTarget.{extension}"));
        let use_file = f.b.join(format!("Use.{extension}"));
        write(&target, declaration);
        write(&use_file, usage);
        write(
            &f.b.join("UnicodeTargetTest.kt"),
            "fun testUnicode() { UnicodeTarget() }\n",
        );
        for file in [&target, &use_file] {
            let checked = success(&f.query(&["check", file.to_str().expect("path")], Some(&f.b)));
            assert_eq!(
                checked["errors"],
                serde_json::json!([]),
                "{extension}: {checked}"
            );
        }
        let mut cold = None;
        for _ in 0..2 {
            // Column 12 counts the emoji as two UTF-16 code units, not four bytes.
            let context = success(&f.query(
                &[
                    "context",
                    use_file.to_str().expect("path"),
                    "2",
                    "12",
                    "--no-stdlib",
                ],
                Some(&f.b),
            ));
            assert_eq!(context["name"], "UnicodeTarget", "{extension}");
            assert_eq!(
                context["definitions"]
                    .as_array()
                    .expect("definitions")
                    .len(),
                1
            );
            assert_path(&context["definitions"][0]["uri"], &target);
            let summary = success(&f.query(
                &["search", "summarize", "UnicodeTarget", "--no-stdlib"],
                Some(&f.b),
            ));
            assert_eq!(summary["name"], "UnicodeTarget");
            assert_path(&summary["file"], &target);
            let inspect = success(&f.query(
                &[
                    "tool",
                    "inspect",
                    target.to_str().expect("path"),
                    "--no-stdlib",
                ],
                Some(&f.b),
            ));
            assert_eq!(inspect["symbols"], serde_json::json!(["UnicodeTarget"]));
            let values = (context, summary, inspect);
            if let Some(previous) = &cold {
                assert_eq!(&values, previous, "{extension}: full cold/warm");
            }
            cold = Some(values);
        }
        if extension == "kt" {
            let impact = success(&f.query(
                &[
                    "impact",
                    use_file.to_str().expect("path"),
                    "2",
                    "12",
                    "--no-stdlib",
                ],
                Some(&f.b),
            ));
            assert_eq!(impact["symbol"], "UnicodeTarget");
            assert_path(&impact["file"], &use_file);
            let tests = success(&f.query(
                &[
                    "search",
                    "find-test",
                    use_file.to_str().expect("path"),
                    "2",
                    "12",
                    "--no-stdlib",
                ],
                Some(&f.b),
            ));
            assert_eq!(tests["symbol"], "UnicodeTarget");
            assert_eq!(tests["tests"].as_array().expect("tests").len(), 1);
            assert_path(
                &tests["tests"][0]["file"],
                &f.b.join("UnicodeTargetTest.kt"),
            );
        }
    }
}

#[test]
fn already_wired_queries_keep_representative_root_and_no_stdlib_controls() {
    let f = Fixture::new();
    let target = f.b.join("Control.kt");
    let usage = f.b.join("Consumer.kt");
    write(&target, "package library\nannotation class Marker\n@Marker\nclass ControlTarget\nsealed class ControlBase\nclass ControlChild : ControlBase()\nfun routeTarget() {}\nfun routeEntry() { routeTarget() }\n");
    write(&usage, "package app\nimport library.ControlTarget\nfun use() {\n val localControl = 42\n localControl\n}\n");
    write(
        &f.dir.path().join("home/.kotlin-lsp/sources/lib/Home.kt"),
        "class HomeOnly\n",
    );
    for args in [
        vec!["search", "name:ControlTarget"],
        vec!["search", "docs", "ControlTarget"],
        vec!["search", "imports", "ControlTarget"],
        vec!["search", "annotated", "Marker"],
        vec!["module", "packages", "app"],
        vec!["type", "sealed", "ControlBase"],
        vec!["call", "reach", "routeEntry", "--to", "routeTarget"],
        vec!["complete", usage.to_str().expect("path"), "5", "14"],
    ] {
        let mut args = args;
        args.push("--no-stdlib");
        let value = success(&f.query(&args, Some(&f.b)));
        match (args[0], args[1]) {
            ("search", "name:ControlTarget" | "docs") => {
                assert_eq!(value.as_array().expect("results").len(), 1);
                assert_eq!(value[0]["name"], "ControlTarget");
                assert_path(&value[0]["file"], &target);
            }
            ("search", "imports") => {
                assert_eq!(value["count"], 1);
                assert_path(&value["importing_files"][0], &usage);
            }
            ("search", "annotated") => {
                assert_eq!(value.as_array().expect("annotations").len(), 1);
                assert_eq!(value[0]["name"], "ControlTarget");
                assert_path(&value[0]["file"], &target);
            }
            ("module", _) => assert_eq!(
                value,
                serde_json::json!({"package":"app","dependencies":["library"],"details":[{"package":"library","file_count":1}]})
            ),
            ("type", _) => {
                assert_eq!(value["count"], 1);
                assert_path(&value["subclasses"][0]["file"], &target);
                assert_eq!(value["subclasses"][0]["line"], 6);
            }
            ("call", _) => {
                assert_eq!(value["truncated"], false);
                assert_eq!(value["paths"].as_array().expect("paths").len(), 1);
                let nodes = value["paths"][0]["nodes"].as_array().expect("nodes");
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0]["name"], "routeEntry");
                assert_eq!(nodes[1]["name"], "routeTarget");
                for node in nodes {
                    assert_path(&node["file"], &target);
                }
            }
            ("complete", _) => {
                let items = value.as_array().expect("completions");
                assert!(items.iter().any(|item| item["label"] == "localControl"));
                assert!(!items.iter().any(|item| item["label"] == "HomeOnly"));
            }
            _ => unreachable!("representative controls"),
        }
        if matches!(args[0], "search" | "type" | "module") {
            let decoy = success(&f.query(&args, Some(&f.a)));
            match (args[0], args[1]) {
                ("search", "imports") | ("type", _) => assert_eq!(decoy["count"], 0),
                ("module", _) => assert_eq!(decoy["dependencies"], serde_json::json!([])),
                _ => assert_eq!(decoy, serde_json::json!([])),
            }
        }
    }
    assert!(
        library_cache_files(&f).is_empty(),
        "all controls skip home sources"
    );
}
