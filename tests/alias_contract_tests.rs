//! Public parser aliases, error channel and real grouped-command controls.
#[path = "support/p6_fixture.rs"]
mod p6_fixture;
use p6_fixture::{canonical_fixture_file, canonical_reported_file, success, Fixture};
use serde_json::Value;

#[test]
fn removed_flat_aliases_fail_before_execution_even_with_json() {
    let f = Fixture::new();
    for name in [
        "summarize",
        "summary-cache",
        "imports-of",
        "annotated",
        "find-test",
        "expect-actual",
        "rename",
        "batch",
        "batch-imports",
        "inject",
        "insert",
        "insert-before",
        "insert-after",
        "new-file",
        "organize-imports",
        "tokens",
        "tree",
        "inspect",
        "symbol-graph",
        "snapshot",
        "benchmark",
        "doctor",
        "workspace",
        "query",
        "skills",
        "code-action",
        "callers",
        "callees",
        "call-hierarchy",
        "implementations",
        "subclasses",
        "type-hierarchy",
        "modules",
        "module-deps",
        "android-activities",
        "android-composables",
    ] {
        let out = f.run(&[name, "--json"], None);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        assert!(
            out.stdout.is_empty(),
            "no JSON envelope for parser errors: {out:?}"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains(&format!("error: unknown subcommand '{name}'")),
            "{out:?}"
        );
    }
}

#[test]
fn docs_and_search_shorthands_are_live_with_nonempty_semantic_results() {
    let f = Fixture::new();
    f.write(
        "Beacon.kt",
        "package demo\n/** Searchable beacon description. */\nfun beacon() {}\n",
    );
    for args in [
        vec!["docs", "beacon"],
        vec!["search", "docs", "beacon"],
        vec!["search", "beacon"],
        vec!["search", "semantic", "beacon"],
    ] {
        let mut args = args;
        args.extend(["--json", "--no-stdlib"]);
        let data = success(&f.run(&args, None));
        let items = data.as_array().expect("array");
        assert_eq!(items.len(), 1, "{data}");
        assert_eq!(items[0]["name"], "beacon");
        assert_eq!(items[0]["line"], 3);
        let file = items[0]["file"].as_str().expect("file");
        assert_eq!(
            canonical_reported_file(file),
            canonical_fixture_file(&f.root.join("Beacon.kt"))
        );
    }
}

#[test]
fn tool_bench_current_command_reports_real_nonzero_fixture_counts() {
    let f = Fixture::new();
    f.write("Beacon.kt", "fun beacon() {}\nfun entry() { beacon() }\n");
    let out = f.run(&["tool", "bench", "--no-stdlib"], None);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("(1 files, 2 symbols)"), "{out:?}");
    assert!(
        text.contains("Cache load:") && text.contains("(1 files)"),
        "{out:?}"
    );
    let manifest: Value = success(&f.run(&["capabilities", "--json"], None));
    assert!(manifest.is_object());
    let help = f.run(&["--help"], None);
    assert!(help.status.success(), "{help:?}");
    assert!(String::from_utf8_lossy(&help.stdout).contains("tool bench"));
}
