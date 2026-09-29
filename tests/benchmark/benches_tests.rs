//! Real CLI small-fixture baseline. Timings have no pass/fail threshold.
#[path = "../support/p6_fixture.rs"]
mod p6_fixture;
use p6_fixture::{
    canonical_fixture_file, canonical_reported_file, execute, expected_path, success, Fixture,
};
use serde_json::{json, Value};
use std::time::Duration;

const SOURCES: [(&str, &str); 3] = [
    ("Bench.kt", "package bench\n/** Measured beacon documentation. */\nfun kotlinBeacon() {}\nfun kotlinEntry() { kotlinBeacon() }\n"),
    ("Bench.java", "package bench;\nclass JavaBeacon {\n    void javaBeacon() {}\n    void javaEntry() { javaBeacon(); }\n}\n"),
    ("Bench.swift", "func swiftBeacon() {}\nfunc swiftEntry() { swiftBeacon() }\n"),
];
const NAMES: [&str; 4] = ["kotlinBeacon", "javaBeacon", "swiftBeacon", "AbsentBeacon"];

fn fixture() -> Fixture {
    let f = Fixture::new();
    for (file, source) in SOURCES {
        f.write(file, source);
    }
    for path in [
        f.dir.path().join("cwd/Decoy.kt"),
        f.dir.path().join("home/.kotlin-lsp/sources/Decoy.kt"),
    ] {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, "fun kotlinBeacon() {}\nfun AbsentBeacon() {}\n").expect("decoy");
    }
    f
}

fn expected(f: &Fixture, index: usize) -> Value {
    let (file, line, col) = match index {
        0 => ("Bench.kt", 3, 5),
        1 => ("Bench.java", 3, 10),
        2 => ("Bench.swift", 1, 6),
        3 => return json!({"type":"definition", "results":[]}),
        _ => panic!("fixture query index"),
    };
    json!({"type":"definition", "results":[{"file":expected_path(&f.root.join(file)), "line":line, "col":col}]})
}

fn assert_result(out: &std::process::Output, expected: &Value, label: &str) {
    assert_eq!(&success(out), expected, "{label}: {out:?}");
    assert!(out.stderr.is_empty(), "{label}: {out:?}");
}

fn measured(
    f: &Fixture,
    label: &str,
    args: &[&str],
    input: Option<&Value>,
    expected: &Value,
    receipt: bool,
) -> Duration {
    let mut cmd = f.command();
    // Suppress only timing-noise logs; diagnostics and exit status remain checked.
    cmd.env("RUST_LOG", "error")
        .args(args)
        .arg("--root")
        .arg(&f.root);
    let argv: Vec<_> = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let (out, elapsed) = execute(cmd, input);
    assert_result(&out, expected, label);
    if receipt {
        println!(
            "{}",
            json!({"label":label,"argv":argv,"stdin":input,"cwd":f.dir.path().join("cwd"),"home":f.dir.path().join("home"),"xdg_cache":f.dir.path().join("cache"),"status":out.status.code(),"elapsed_ns":elapsed.as_nanos(),"result":expected})
        );
    }
    elapsed
}

fn query(f: &Fixture, indices: &[usize], label: &str, receipt: bool) -> Duration {
    let input = Value::Array(
        indices
            .iter()
            .map(|&i| json!({"type":"definition","name":NAMES[i]}))
            .collect(),
    );
    let result = Value::Array(indices.iter().map(|&i| expected(f, i)).collect());
    measured(
        f,
        label,
        &["tool", "query", "--json", "--no-stdlib"],
        Some(&input),
        &result,
        receipt,
    )
}

fn exercise(receipt: bool) {
    let f = fixture();
    let cache = f.root.join(".cache/kotlin-lsp/index.bin");
    assert!(!cache.exists());
    measured(
        &f,
        "check-fresh-source-no-index",
        &["check", f.root.to_str().expect("root"), "--json"],
        None,
        &json!({"empty_dirs":[],"errors":[],"files_ok":3,"files_with_errors":0}),
        receipt,
    );
    assert!(!cache.exists(), "check is not an index warmup");
    query(&f, &[0], "cold-new-workspace-single-query", receipt);
    let persisted = std::fs::read(&cache).expect("cold query persisted an index");
    assert!(!persisted.is_empty());
    let persisted_at = std::fs::metadata(&cache)
        .expect("cache metadata")
        .modified()
        .expect("cache timestamp");
    let mut singles = Duration::ZERO;
    for i in 0..4 {
        singles += query(&f, &[i], "warm-persisted-single-query", receipt);
    }
    let batch = query(
        &f,
        &[0, 1, 2, 3],
        "warm-persisted-four-query-batch",
        receipt,
    );
    assert_eq!(
        std::fs::read(&cache).expect("persisted cache"),
        persisted,
        "warm invocations must not rebuild cache bytes"
    );
    assert_eq!(
        std::fs::metadata(&cache)
            .expect("cache metadata")
            .modified()
            .expect("cache timestamp"),
        persisted_at,
        "warm invocations must not rewrite the persisted cache"
    );
    if receipt {
        println!(
            "{}",
            json!({"label":"startup-amortization-samples-not-speedup-claim","four_single_processes_ns":singles.as_nanos(),"one_four_query_process_ns":batch.as_nanos()})
        );
    }
    let snapshot = success(&f.run(&["tool", "snapshot", "--json"], None));
    let symbols = snapshot["symbols"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 7);
    let mut names: Vec<_> = symbols
        .iter()
        .map(|s| s["name"].as_str().expect("name"))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "JavaBeacon",
            "javaBeacon",
            "javaEntry",
            "kotlinBeacon",
            "kotlinEntry",
            "swiftBeacon",
            "swiftEntry"
        ]
    );
    for symbol in symbols {
        let path = symbol["file"].as_str().expect("file");
        assert!(
            SOURCES.iter().any(|(file, _)| canonical_reported_file(path)
                == canonical_fixture_file(&f.root.join(file))),
            "decoy: {symbol}"
        );
    }
    let find = success(&f.run(
        &[
            "find",
            "kotlinBeacon",
            "--smart",
            "--json",
            "--absolute",
            "--no-stdlib",
        ],
        None,
    ));
    let hits = find.as_array().expect("array");
    assert_eq!(hits.len(), 1, "{find}");
    let hit = hits[0].as_object().expect("object");
    let mut keys: Vec<&str> = hit.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["col", "file", "kind", "line", "name", "relativePath"],
        "{find}"
    );
    assert_eq!(hit["name"], "kotlinBeacon", "{find}");
    assert_eq!(hit["line"], 3, "{find}");
    assert_eq!(hit["col"], 5, "{find}");
    assert_eq!(hit["kind"], "function", "{find}");
    assert_eq!(hit["relativePath"], "Bench.kt", "{find}");
    assert_eq!(
        canonical_reported_file(hit["file"].as_str().expect("file")),
        canonical_fixture_file(&f.root.join("Bench.kt")),
        "{find}"
    );
}

#[test]
fn benchmark_fixture_semantics_cold_warm_batch_and_decoys() {
    exercise(false);
}

#[test]
fn benchmark_contract_rejects_empty_success_and_wrong_workspace() {
    let f = fixture();
    let empty = f.run(
        &["tool", "query", "--json", "--no-stdlib"],
        Some(&json!([{"type":"definition","name":"AbsentBeacon"}])),
    );
    assert_eq!(success(&empty), json!([{"type":"definition","results":[]}]));
    assert!(
        std::panic::catch_unwind(|| assert_result(
            &empty,
            &json!([expected(&f, 0)]),
            "empty-positive"
        ))
        .is_err(),
        "validator must reject a successful empty answer for a positive query"
    );
    let foreign = fixture();
    let wrong_workspace = foreign.run(
        &["tool", "query", "--json", "--no-stdlib"],
        Some(&json!([{"type":"definition","name":"kotlinBeacon"}])),
    );
    assert_eq!(success(&wrong_workspace), json!([expected(&foreign, 0)]));
    assert!(
        std::panic::catch_unwind(|| assert_result(
            &wrong_workspace,
            &json!([expected(&f, 0)]),
            "wrong-workspace"
        ))
        .is_err(),
        "validator must reject same-named hits from another workspace"
    );
    let out = f.run(
        &["tool", "query", "--json", "--no-stdlib"],
        Some(&json!([{"type":"not-a-query"}])),
    );
    assert!(!out.status.success(), "{out:?}");
    let data: Value = serde_json::from_slice(&out.stdout).expect("error JSON");
    assert!(data[0]["error"]
        .as_str()
        .expect("item error")
        .contains("unknown variant"));
}

#[test]
#[ignore = "timing receipt; run explicitly with --ignored --nocapture --test-threads=1"]
fn benchmark_small_fixture_timing_receipt() {
    println!(
        "{}",
        json!({"profile":if cfg!(debug_assertions) {"debug (unoptimized + debuginfo)"} else {"optimized test build; inspect cargo profile"},"fixture":"small, synthetic Kotlin/Java/Swift","iterations":3,"sources":3,"source_bytes":SOURCES.iter().map(|(_,s)|s.len()).sum::<usize>(),"symbols":7,"batch_queries":4,"positive_batch_items":3,"negative_batch_items":1,"boundary":"spawn through wait_with_output, including stdin write and stdout/stderr collection; excludes command/input construction, fixture setup, JSON parsing/assertions, cache verification, snapshot and find controls","exclusions":"no release or large-library claim, no OS page-cache flush, no installation, no threshold; warm means prior same workspace tool query persisted cache, not a daemon"})
    );
    for iteration in 0..3 {
        println!("iteration={iteration}");
        exercise(true);
    }
}
