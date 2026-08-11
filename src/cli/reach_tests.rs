//! Tests for `call reach` path enumeration (src/cli/reach.rs).

use std::sync::Arc;

use tower_lsp::lsp_types::Url;

use crate::indexer::Indexer;

use super::reach::{build_callee_map, enumerate_paths, DEFAULT_MAX_DEPTH, MAX_PATHS};

// ── test helpers ─────────────────────────────────────────────────────────────

fn build_graph(source: &str) -> Arc<Indexer> {
    let idx = Arc::new(Indexer::new());
    // Platform-independent absolute path (Windows has no /tmp).
    let uri = Url::from_file_path(std::env::temp_dir().join("Reach.kt")).expect("valid file path");
    idx.index_content(&uri, source);
    idx
}

fn names(path: &[(String, String)]) -> Vec<&str> {
    path.iter().map(|(_, name)| name.as_str()).collect()
}

// ── fixtures ─────────────────────────────────────────────────────────────────

const CYCLE_SOURCE: &str = r#"
package test

fun a(): String { return b() }
fun b(): String { return a() }
fun c(): String { return "ok" }
fun entry(): String { return c() }
"#;

const CHAIN_SOURCE: &str = r#"
package test

fun e1(): String { return e2() }
fun e2(): String { return e3() }
fun e3(): String { return "ok" }
"#;

const BRANCH_SOURCE: &str = r#"
package test

fun branch(x: Boolean): String {
    if (x) { return left() } else { return right() }
}
fun left(): String { return "l" }
fun right(): String { return "r" }
"#;

// ── enumerate_paths ──────────────────────────────────────────────────────────

#[test]
fn direct_call_one_hop() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, truncated) =
        enumerate_paths(&map, "entry", Some("c"), DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert!(!truncated, "no truncation expected");
    assert_eq!(paths.len(), 1, "exactly one direct path");
    assert_eq!(names(&paths[0]), vec!["entry", "c"]);
}

#[test]
fn multi_hop_path() {
    let idx = build_graph(CHAIN_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "e1", Some("e3"), DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert_eq!(paths.len(), 1, "exactly one transitive path");
    assert_eq!(names(&paths[0]), vec!["e1", "e2", "e3"]);
}

#[test]
fn cycle_terminates_with_target() {
    // a → b → a: target c lives outside the cycle — must terminate with no paths.
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, truncated) = enumerate_paths(&map, "a", Some("c"), DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert!(!truncated, "no truncation expected");
    assert!(paths.is_empty(), "c is unreachable from a");
}

#[test]
fn cycle_terminates_without_target() {
    // a → b → a: the back edge must not be re-traversed on the same path.
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "a", None, DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert_eq!(paths.len(), 1, "exactly one path [a, b]");
    assert_eq!(names(&paths[0]), vec!["a", "b"]);
}

#[test]
fn unreachable_target_is_empty() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        "entry",
        Some("nonexistent"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(paths.is_empty(), "unknown target yields no paths");
}

#[test]
fn entry_missing_from_graph_is_empty() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "ghost", None, DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert!(paths.is_empty(), "unknown entry yields no paths");
}

#[test]
fn all_paths_without_target() {
    let idx = build_graph(BRANCH_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "branch", None, DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert_eq!(paths.len(), 2, "two leaves (if / else arms)");
    let mut leaves: Vec<Vec<&str>> = paths.iter().map(|p| names(p)).collect();
    leaves.sort();
    assert_eq!(
        leaves,
        vec![vec!["branch", "left"], vec!["branch", "right"]]
    );
}

#[test]
fn branch_paths_to_specific_target() {
    let idx = build_graph(BRANCH_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "branch", Some("left"), DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0]), vec!["branch", "left"]);
}

#[test]
fn max_depth_bounds_enumeration() {
    let idx = build_graph(CHAIN_SOURCE);
    let map = build_callee_map(&idx);
    // Depth 1: e1 → e2 recorded, e3 never reached.
    let (paths, _) = enumerate_paths(&map, "e1", None, 1, MAX_PATHS);
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0]), vec!["e1", "e2"]);
}

#[test]
fn target_at_entry_is_immediate_path() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(&map, "a", Some("a"), DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert_eq!(paths.len(), 1, "entry == target is a single-node path");
    assert_eq!(names(&paths[0]), vec!["a"]);
}

#[test]
fn path_cap_truncates() {
    let idx = build_graph(BRANCH_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, truncated) = enumerate_paths(&map, "branch", None, DEFAULT_MAX_DEPTH, 1);
    assert!(truncated, "cap 1 must set the truncation flag");
    assert!(paths.len() <= 1);
}

// ── build_callee_map ─────────────────────────────────────────────────────────

#[test]
fn callee_map_reverses_edge_direction() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    // entry calls c → map["entry"] = [(file, "c")]
    let entry_callees = map.get("entry").expect("entry has callees");
    assert_eq!(entry_callees.len(), 1);
    assert_eq!(entry_callees[0].1, "c", "callee name is c");
    // a calls b → map["a"] = [(file, "b")]
    let a_callees = map.get("a").expect("a has callees");
    assert_eq!(a_callees[0].1, "b");
    // c has no callers → not a key in the reversed map
    assert!(!map.contains_key("c"), "leaf c is not a caller");
}

// ── language isolation (issue #259) ──────────────────────────────────────────

/// Same-named callees in Kotlin and Swift must not join: a path starting in
/// Kotlin must not cross into Swift edges.
#[test]
fn paths_do_not_cross_language_boundary() {
    let idx = Arc::new(Indexer::new());
    // Kotlin side: entryPoint -> sharedName -> kotlinOnlyLeaf
    let kotlin = "package a\nfun entryPoint() { sharedName() }\nfun sharedName() { kotlinOnlyLeaf() }\nfun kotlinOnlyLeaf() = 1\n";
    // Swift side: swiftEntry -> sharedName -> swiftOnlyLeaf (same `sharedName`)
    let swift = "func swiftEntry() { sharedName() }\nfunc sharedName() { swiftOnlyLeaf() }\nfunc swiftOnlyLeaf() -> Int { 1 }\n";
    let kt_uri = Url::from_file_path(std::env::temp_dir().join("KtEntry.kt")).expect("uri");
    let sw_uri = Url::from_file_path(std::env::temp_dir().join("SwiftEntry.swift")).expect("uri");
    idx.index_content(&kt_uri, kotlin);
    idx.index_content(&sw_uri, swift);

    let map = build_callee_map(&idx);
    // Kotlin path: entryPoint -> sharedName -> kotlinOnlyLeaf only.
    let (paths, _) = enumerate_paths(&map, "entryPoint", None, DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert!(!paths.is_empty(), "kotlin path exists");
    for p in &paths {
        let names: Vec<&str> = p.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            !names.contains(&"swiftOnlyLeaf"),
            "kotlin path must not reach Swift leaf: {names:?}"
        );
    }
    // Swift path: swiftEntry -> sharedName -> swiftOnlyLeaf only.
    let (paths, _) = enumerate_paths(&map, "swiftEntry", None, DEFAULT_MAX_DEPTH, MAX_PATHS);
    assert!(!paths.is_empty(), "swift path exists");
    for p in &paths {
        let names: Vec<&str> = p.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            !names.contains(&"kotlinOnlyLeaf"),
            "swift path must not reach Kotlin leaf: {names:?}"
        );
    }
}

/// With a target, the language filter still applies.
#[test]
fn target_path_respects_language() {
    let idx = Arc::new(Indexer::new());
    let kotlin = "package a\nfun entryPoint() { sharedName() }\nfun sharedName() { kotlinOnlyLeaf() }\nfun kotlinOnlyLeaf() = 1\n";
    let swift = "func swiftEntry() { sharedName() }\nfunc sharedName() { swiftOnlyLeaf() }\nfunc swiftOnlyLeaf() -> Int { 1 }\n";
    let kt_uri = Url::from_file_path(std::env::temp_dir().join("KtEntry2.kt")).expect("uri");
    let sw_uri = Url::from_file_path(std::env::temp_dir().join("SwiftEntry2.swift")).expect("uri");
    idx.index_content(&kt_uri, kotlin);
    idx.index_content(&sw_uri, swift);

    let map = build_callee_map(&idx);
    // Kotlin entry → Swift-only target: unreachable (language boundary).
    let (paths, _) = enumerate_paths(
        &map,
        "entryPoint",
        Some("swiftOnlyLeaf"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(
        paths.is_empty(),
        "cross-language target unreachable from kotlin"
    );
    // Kotlin entry → Kotlin leaf: reachable.
    let (paths, _) = enumerate_paths(
        &map,
        "entryPoint",
        Some("kotlinOnlyLeaf"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1);
}

// ── Kotlin→Java paths (issue #266) ───────────────────────────────────────────

/// A Kotlin entry calling a Java static method must keep following Java edges:
/// single-language nodes are NOT filtered, only same-named multi-language
/// ambiguity is (that is the #259 rule).
#[test]
fn kotlin_entry_follows_java_callees() {
    let idx = Arc::new(Indexer::new());
    let kotlin = "package a\nfun kotlinEntry() { JavaHelper.javaMid() }\n";
    let java = "public final class JavaHelper {\n    public static void javaMid() {\n        javaLeaf();\n    }\n}\n";
    let kt_uri = Url::from_file_path(std::env::temp_dir().join("KtCaller.kt")).expect("uri");
    let java_uri = Url::from_file_path(std::env::temp_dir().join("JavaHelper.java")).expect("uri");
    idx.index_content(&kt_uri, kotlin);
    idx.index_content(&java_uri, java);

    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        "kotlinEntry",
        Some("javaLeaf"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "kotlin entry reaches java leaf: {paths:?}");
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, vec!["kotlinEntry", "javaMid", "javaLeaf"]);
}
