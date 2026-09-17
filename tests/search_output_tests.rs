//! Semantic search output through real CLI processes and isolated filesystem fixtures.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("workspace");
        for path in [&root, &dir.path().join("home"), &dir.path().join("cwd")] {
            std::fs::create_dir_all(path).expect("mkdir");
        }
        Self { dir, root }
    }
    fn write(&self, file: &str, source: &str) {
        let path = self.root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, source).expect("source");
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"));
        cmd.current_dir(&self.root)
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("GRADLE_USER_HOME", self.dir.path().join("gradle"))
            // Suppress timing noise, not CLI diagnostics.
            .env("RUST_LOG", "error");
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        run(self.command().args(args))
    }
}

fn run(cmd: &mut Command) -> Output {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("CLI process");
    let started = Instant::now();
    // These tiny fixtures cannot fill a pipe. Bound every CLI invocation.
    loop {
        if child.try_wait().expect("poll CLI").is_some() {
            return child.wait_with_output().expect("CLI output");
        }
        if started.elapsed() > Duration::from_secs(15) {
            child.kill().expect("kill timed out CLI");
            let out = child.wait_with_output().expect("timed out CLI output");
            panic!("CLI timeout: {out:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn success(out: &Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let value: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(
        out.stdout.iter().filter(|&&c| c == b'\n').count(),
        1,
        "single-line JSON"
    );
    let mut quoted = false;
    let mut escaped = false;
    for &byte in &out.stdout[..out.stdout.len() - 1] {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == b'"' {
            quoted = !quoted;
        }
        if !quoted {
            assert!(!byte.is_ascii_whitespace(), "compact JSON: {out:?}");
        }
    }
    value
}

#[test]
fn semantic_envelope_no_hit_is_compact_and_not_truncated() {
    let f = Fixture::new();
    f.write("Known.kt", "class Known\n");
    assert_eq!(
        success(&f.run(&[
            "search",
            "absentquartz",
            "--json",
            "--json-envelope",
            "--no-stdlib"
        ])),
        json!({"results": [], "truncated": false})
    );
}

// Search retains its existing URI-tail file representation (including escaping
// and forward slashes), unlike lookup commands' native filesystem paths.
fn search_file(path: &Path) -> String {
    tower_lsp::lsp_types::Url::from_file_path(path.canonicalize().expect("canonical source"))
        .expect("source URI")
        .as_str()
        .strip_prefix("file://")
        .expect("file URI")
        .to_owned()
}

fn class_result(f: &Fixture, file: &str, name: &str) -> Value {
    json!({"name":name, "kind":"class",
        "file":search_file(&f.root.join(file)),
        "line":1, "signature":format!("class {name}"), "doc":null,
        "generated":false, "score":1.0})
}

#[test]
fn scored_ties_have_fixed_cross_file_order_on_repeated_cold_and_warm_runs() {
    let f = Fixture::new();
    for file in ["C.kt", "A.kt", "B.kt"] {
        f.write(file, "class Match\n");
    }
    f.write("Decoy.kt", "class Decoy\n");
    let expected = json!([
        class_result(&f, "A.kt", "Match"),
        class_result(&f, "B.kt", "Match"),
        class_result(&f, "C.kt", "Match")
    ]);
    let cache = f.root.join(".cache/kotlin-lsp/index.bin");
    assert!(!cache.exists());
    for cold in 0..3 {
        if cold > 0 {
            std::fs::remove_file(&cache).expect("reset workspace cache");
        }
        for _ in 0..4 {
            assert_eq!(
                success(&f.run(&["search", "kind:class match", "--json", "--no-stdlib"])),
                expected
            );
            assert!(cache.metadata().expect("persisted workspace cache").len() > 0);
        }
    }
}

#[test]
fn matching_prefix_terms_have_stable_scores_across_processes_and_cache_states() {
    let f = Fixture::new();
    f.write("Alpha.kt", "/** prelude prevent prevent prefix prefix prefix prepare prepare prepare prepare preorder preorder preorder preorder preorder prefetch prefetch preview previous precision precondition */\nclass MatchAlpha\n");
    f.write("Beta.kt", "/** prefix prepare */\nclass MatchBeta\n");
    f.write("Gamma.kt", "/** prelude */\nclass MatchGamma\n");
    f.write("Decoy.kt", "class Decoy\n");
    let args = ["search", "kind:class pre", "--json", "--no-stdlib"];
    let first = f.run(&args);
    let results = success(&first);
    assert_eq!(results.as_array().expect("results").len(), 3);
    for (i, (file, name)) in [
        ("Alpha.kt", "MatchAlpha"),
        ("Beta.kt", "MatchBeta"),
        ("Gamma.kt", "MatchGamma"),
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(results[i]["name"], *name);
        assert_eq!(results[i]["file"], json!(search_file(&f.root.join(file))));
        assert_eq!(results[i]["signature"], format!("class {name}"));
        assert_eq!(results[i]["generated"], false);
    }
    assert_eq!(results[0]["score"], 1.0);
    let second = results[1]["score"].as_f64().expect("score");
    let third = results[2]["score"].as_f64().expect("score");
    assert!(1.0 > second && second > third && third > 0.0);
    // The lookahead must not change normalization for non-top results.
    for limit in ["1", "2", "3"] {
        let mut limited = args.to_vec();
        limited.extend(["--limit", limit]);
        let array = success(&f.run(&limited));
        let count: usize = limit.parse().expect("literal limit");
        assert_eq!(array, json!(&results.as_array().expect("results")[..count]));
        limited.push("--json-envelope");
        assert_eq!(
            success(&f.run(&limited)),
            json!({"results":array,"truncated":count < 3})
        );
    }
    let cache = f.root.join(".cache/kotlin-lsp/index.bin");
    assert!(cache.metadata().expect("cache").len() > 0);
    for i in 0..24 {
        if i % 8 == 0 {
            std::fs::remove_file(&cache).expect("cold cache");
        }
        let out = f.run(&args);
        success(&out);
        assert_eq!(
            out.stdout, first.stdout,
            "process {i}: prefix floating sums must be stable"
        );
    }
}

fn assert_limit_matrix(query: &str, nohit: &str) {
    let f = Fixture::new();
    let mut expected = Vec::new();
    for (file, name) in [
        ("Alpha.kt", "MatchAlpha"),
        ("Beta.kt", "MatchBeta"),
        ("Gamma.kt", "MatchGamma"),
    ] {
        f.write(file, &format!("class {name}\n"));
        expected.push(class_result(&f, file, name));
    }
    f.write("Decoy.kt", "class Decoy\nfun matchDecoy() {}\n");
    let maximum = usize::MAX.to_string();
    for (limit, count, truncated) in [
        (None, 3, false),
        (Some("0"), 0, true),
        (Some("1"), 1, true),
        (Some("3"), 3, false),
        (Some("4"), 3, false),
        (Some(maximum.as_str()), 3, false),
    ] {
        for (query, expected, truncated) in [
            (query, &expected[..count], truncated),
            (nohit, &[][..], false),
        ] {
            for explicit in [false, true] {
                let mut args = vec!["search"];
                if explicit {
                    args.push("semantic");
                }
                args.push(query);
                if let Some(limit) = limit {
                    args.extend(["--limit", limit]);
                }
                args.extend(["--json", "--no-stdlib"]);
                let array = success(&f.run(&args));
                assert_eq!(array, json!(expected), "{args:?}");
                // Flags both before and after the query/member, including reversed JSON order.
                for at_start in [false, true] {
                    let mut envelope_args = args.clone();
                    if at_start {
                        envelope_args.insert(1, "--json-envelope");
                    } else {
                        envelope_args.push("--json-envelope");
                    }
                    assert_eq!(
                        success(&f.run(&envelope_args)),
                        json!({"results":array,"truncated":truncated}),
                        "{envelope_args:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn scored_limits_preserve_array_fields_scores_and_truthful_envelopes() {
    assert_limit_matrix("kind:class match", "kind:class absentquartz");
}

#[test]
fn filter_only_limits_preserve_array_fields_scores_and_truthful_envelopes() {
    assert_limit_matrix("kind:class name:Match", "kind:class name:AbsentQuartz");
}

#[test]
fn omitted_limit_remains_twenty_with_actual_overflow_in_both_branches() {
    let f = Fixture::new();
    let mut expected = Vec::new();
    for i in 0..21 {
        let file = format!("Match{i:02}.kt");
        let name = format!("Match{i:02}");
        f.write(&file, &format!("class {name}\n"));
        expected.push(class_result(&f, &file, &name));
    }
    f.write("Decoy.kt", "class Decoy\n");
    for query in ["kind:class match", "kind:class name:Match"] {
        let args = ["search", query, "--json", "--no-stdlib"];
        let array = success(&f.run(&args));
        assert_eq!(array, json!(&expected[..20]));
        let mut envelope = args.to_vec();
        envelope.push("--json-envelope");
        assert_eq!(
            success(&f.run(&envelope)),
            json!({"results":expected[..20], "truncated":true})
        );
    }
}

fn argument_error(out: &Output, message: &str) {
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        out.stdout.is_empty(),
        "must fail before command output: {out:?}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(message), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn envelope_requires_json_for_both_semantic_spellings_before_indexing() {
    for explicit in [false, true] {
        let f = Fixture::new();
        f.write("Known.kt", "class Known\n");
        let mut args = vec!["search", "--json-envelope"];
        if explicit {
            args.push("semantic");
        }
        args.push("Known");
        argument_error(&f.run(&args), "--json-envelope requires --json");
        assert!(!f.root.join(".cache").exists());
        assert!(!f.dir.path().join("home/.kotlin-lsp").exists());
    }
}

#[test]
fn envelope_rejects_every_nonsemantic_search_member_and_other_commands_before_work() {
    let cases = [
        vec!["search", "docs", "Known"],
        vec!["search", "summarize", "Known"],
        vec!["search", "summarize", "Known", "--cached"],
        vec!["search", "cache-stats"],
        vec!["search", "imports", "Known"],
        vec!["search", "annotated", "Known"],
        vec!["search", "find-test", "Known.kt", "1", "7"],
        vec!["search", "expect-actual", "Known"],
        vec!["docs", "Known"],
        vec!["find", "Known"],
        vec!["refs", "Known"],
        vec!["hover", "Known.kt", "1", "7"],
        vec!["complete", "Known.kt", "1", "7"],
        vec!["context", "Known.kt", "1", "7"],
        vec!["check", "Known.kt"],
        vec!["index"],
        vec!["tool", "query"],
        vec!["extract-sources"],
        vec!["capabilities"],
        vec!["call", "hierarchy", "Known"],
        vec!["edit", "rename", "Known.kt", "1", "7", "Renamed"],
    ];
    for case in cases {
        for first_flag in [false, true] {
            let f = Fixture::new();
            f.write("Known.kt", "class Known\n");
            let mut args = case.clone();
            if first_flag {
                args.insert(1, "--json-envelope");
            } else {
                args.push("--json-envelope");
            }
            args.push("--json");
            argument_error(
                &f.run(&args),
                "--json-envelope is only supported by semantic search",
            );
            assert!(!f.root.join(".cache").exists(), "{args:?}");
            assert_eq!(
                std::fs::read_to_string(f.root.join("Known.kt")).expect("source"),
                "class Known\n"
            );
            assert!(!f.dir.path().join("home/.kotlin-lsp").exists(), "{args:?}");
        }
    }
}

#[test]
fn invalid_search_limits_fail_before_indexing_in_array_and_envelope_modes() {
    let overflow = (usize::MAX as u128 + 1).to_string();
    for limit in ["-1", "one", overflow.as_str()] {
        for explicit in [false, true] {
            for envelope in [false, true] {
                let f = Fixture::new();
                f.write("Known.kt", "class Known\n");
                let mut args = vec!["search"];
                if explicit {
                    args.push("semantic");
                }
                args.extend(["Known", "--limit", limit, "--json"]);
                if envelope {
                    args.push("--json-envelope");
                }
                argument_error(&f.run(&args), "--limit");
                assert!(!f.root.join(".cache").exists());
            }
        }
    }
}

#[test]
fn same_file_distinct_signatures_and_cross_file_filter_ties_keep_fixed_order() {
    let f = Fixture::new();
    f.write("B.kt", "fun matchToken(value: Int) {}\n");
    f.write(
        "A.kt",
        "fun matchToken(value: Int) {}\nfun matchToken(value: String) {}\n",
    );
    f.write("Decoy.kt", "fun otherToken() {}\n");
    let expected = json!([
        {"name":"matchToken", "kind":"function", "file":search_file(&f.root.join("A.kt")),"line":1,"signature":"fun matchToken(value: Int)","doc":null,"generated":false,"score":1.0},
        {"name":"matchToken", "kind":"function", "file":search_file(&f.root.join("A.kt")),"line":2,"signature":"fun matchToken(value: String)","doc":null,"generated":false,"score":1.0},
        {"name":"matchToken", "kind":"function", "file":search_file(&f.root.join("B.kt")),"line":1,"signature":"fun matchToken(value: Int)","doc":null,"generated":false,"score":1.0}
    ]);
    for i in 0..12 {
        if i == 6 {
            std::fs::remove_file(f.root.join(".cache/kotlin-lsp/index.bin")).expect("cold cache");
        }
        for query in ["kind:function name:matchToken", "kind:function match"] {
            assert_eq!(
                success(&f.run(&["search", query, "--json", "--no-stdlib"])),
                expected
            );
        }
    }
}

#[test]
fn generated_real_before_stub_and_generated_only_remains_searchable() {
    let f = Fixture::new();
    f.write(
        "Real.kt",
        "/** Actual transport implementation with detailed documentation. */\nclass Request\n",
    );
    f.write(
        "Stub.kt",
        "// Generated by protocol compiler. DO NOT EDIT.\nclass Request\n",
    );
    f.write(
        "Only.kt",
        "// Generated by protocol compiler. DO NOT EDIT.\nclass GeneratedOnly\n",
    );
    f.write("Decoy.kt", "class Decoy\n");
    for query in ["kind:class request", "kind:class name:Request"] {
        for _ in 0..4 {
            let value = success(&f.run(&[
                "search",
                query,
                "--json",
                "--json-envelope",
                "--limit",
                "1",
                "--no-stdlib",
            ]));
            assert_eq!(value["truncated"], true);
            assert_eq!(value["results"].as_array().expect("results").len(), 1);
            assert_eq!(
                value["results"][0]["file"],
                json!(search_file(&f.root.join("Real.kt")))
            );
            assert_eq!(value["results"][0]["name"], "Request");
            assert_eq!(value["results"][0]["generated"], false);
            let all = success(&f.run(&["search", query, "--json", "--no-stdlib"]));
            assert_eq!(all.as_array().expect("results").len(), 2);
            assert_eq!(all[0], value["results"][0]);
            assert_eq!(all[1]["file"], json!(search_file(&f.root.join("Stub.kt"))));
            assert_eq!(all[1]["generated"], true);
            assert_eq!(all[1]["name"], "Request");
        }
    }
    // Excluding the real implementation must still expose its generated stub.
    for query in [
        "kind:class path:Stub.kt request",
        "kind:class path:Stub.kt name:Request",
    ] {
        let value = success(&f.run(&["search", query, "--json", "--json-envelope", "--no-stdlib"]));
        assert_eq!(
            value,
            json!({"results":[{"name":"Request","kind":"class","file":search_file(&f.root.join("Stub.kt")),"line":2,"signature":"class Request","doc":null,"generated":true,"score":1.0}],"truncated":false})
        );
    }
    for query in ["kind:class generated", "kind:class name:GeneratedOnly"] {
        let only = success(&f.run(&["search", query, "--json", "--json-envelope", "--no-stdlib"]));
        assert_eq!(only["truncated"], false);
        assert_eq!(only["results"].as_array().expect("results").len(), 1);
        assert_eq!(only["results"][0]["name"], "GeneratedOnly");
        assert_eq!(only["results"][0]["generated"], true);
        assert_eq!(
            only["results"][0]["file"],
            json!(search_file(&f.root.join("Only.kt")))
        );
        assert_eq!(only["results"][0]["score"], 1.0);
    }
}

#[test]
fn kotlin_java_swift_search_filters_return_meaningful_results_with_decoys() {
    let f = Fixture::new();
    for (file, source) in [
        ("Kotlin.kt", "class MatchKotlin\n"),
        ("Java.java", "class MatchJava {}\n"),
        ("Swift.swift", "class MatchSwift {}\n"),
    ] {
        f.write(file, source);
    }
    f.write("Decoy.kt", "class Decoy\n");
    for (lang, file, name) in [
        ("kotlin", "Kotlin.kt", "MatchKotlin"),
        ("java", "Java.java", "MatchJava"),
        ("swift", "Swift.swift", "MatchSwift"),
    ] {
        for text in ["match", "name:Match"] {
            let query = format!("kind:class lang:{lang} {text}");
            let value = success(&f.run(&[
                "search",
                "semantic",
                &query,
                "--json",
                "--json-envelope",
                "--no-stdlib",
            ]));
            assert_eq!(
                value,
                json!({"results":[class_result(&f,file,name)],"truncated":false})
            );
        }
    }
}

#[test]
fn semantic_explicit_root_selects_workspace_b_not_decoy_cwd_a() {
    let f = Fixture::new();
    f.write("B.kt", "class MatchB\n");
    std::fs::write(f.dir.path().join("cwd/A.kt"), "class MatchA\n").expect("cwd decoy");
    let cwd = f.dir.path().join("cwd");
    for query in ["kind:class match", "kind:class name:Match"] {
        let out = run(f.command().current_dir(&cwd).args([
            "search",
            query,
            "--json",
            "--json-envelope",
            "--no-stdlib",
            "--root",
            f.root.to_str().expect("root"),
        ]));
        assert_eq!(
            success(&out),
            json!({"results":[class_result(&f,"B.kt","MatchB")],"truncated":false})
        );
        let control =
            run(f
                .command()
                .current_dir(&cwd)
                .args(["search", query, "--json", "--no-stdlib"]));
        assert_eq!(
            success(&control),
            json!([{"name":"MatchA","kind":"class","file":search_file(&cwd.join("A.kt")),"line":1,"signature":"class MatchA","doc":null,"generated":false,"score":1.0}])
        );
    }
}

#[test]
fn help_and_generated_capabilities_describe_semantic_envelope_and_extraction_options() {
    let f = Fixture::new();
    let out = f.run(&["--help"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--json-envelope"), "{help}");
    assert!(help.contains("requires --json"), "{help}");
    assert!(help.contains("semantic search only"), "{help}");
    assert!(
        help.contains("unions, not promises for every member"),
        "{help}"
    );
    // Only search output is under the compact contract in this slice.
    let out = f.run(&["capabilities", "--json"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let caps: Value = serde_json::from_slice(&out.stdout).expect("capabilities JSON");
    let commands = caps["commands"].as_object().expect("commands");
    for (name, entry) in commands {
        let flags = entry["flags"].as_array().expect("flags");
        assert_eq!(
            flags.contains(&json!("--json-envelope")),
            name == "search",
            "{name}"
        );
    }
    for flag in ["--json", "--root", "--no-stdlib", "--kind", "--limit"] {
        assert!(
            commands["search"]["flags"]
                .as_array()
                .expect("flags")
                .contains(&json!(flag)),
            "{flag}"
        );
    }
    assert_eq!(
        commands["extract-sources"]["flags"],
        json!(["--gradle-home", "--output", "--dry-run"])
    );
    // An advertised example must actually return an envelope, not just parse.
    f.write("Known.kt", "class Known\n");
    assert_eq!(
        success(&f.run(&[
            "search",
            "semantic",
            "Known",
            "--json",
            "--json-envelope",
            "--no-stdlib"
        ])),
        json!({"results":[class_result(&f,"Known.kt","Known")],"truncated":false})
    );
}

#[test]
fn search_file_field_preserves_full_identity_and_existing_uri_escaping() {
    let f = Fixture::new();
    let file = "space 文 #%.kt";
    f.write(file, "class Known\n");
    let value = success(&f.run(&["search", "Known", "--json", "--no-stdlib"]));
    assert_eq!(value, json!([class_result(&f, file, "Known")]));
    let emitted = value[0]["file"].as_str().expect("file");
    assert!(emitted.contains("%20") && emitted.contains("%23") && emitted.contains("%25"));
    let actual = tower_lsp::lsp_types::Url::parse(&format!("file://{emitted}"))
        .expect("URI")
        .to_file_path()
        .expect("filesystem path");
    assert_eq!(
        actual.canonicalize().expect("actual file"),
        f.root.join(file).canonicalize().expect("fixture file")
    );
}
