//! Tests for `call reach` path enumeration (src/cli/reach.rs).

use std::sync::Arc;

use tower_lsp::lsp_types::Url;

use crate::indexer::Indexer;

use super::reach::{
    bare_name_matches, build_callee_map, build_impls_map, enumerate_paths, resolve_entry_key,
    DEFAULT_MAX_DEPTH, MAX_PATHS,
};

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
    let (paths, truncated) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "entry",
        Some("c"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(!truncated, "no truncation expected");
    assert_eq!(paths.len(), 1, "exactly one direct path");
    assert_eq!(names(&paths[0]), vec!["entry", "c"]);
}

#[test]
fn multi_hop_path() {
    let idx = build_graph(CHAIN_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "e1",
        Some("e3"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "exactly one transitive path");
    assert_eq!(names(&paths[0]), vec!["e1", "e2", "e3"]);
}

#[test]
fn cycle_terminates_with_target() {
    // a → b → a: target c lives outside the cycle — must terminate with no paths.
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, truncated) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "a",
        Some("c"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(!truncated, "no truncation expected");
    assert!(paths.is_empty(), "c is unreachable from a");
}

#[test]
fn cycle_terminates_without_target() {
    // a → b → a: the back edge must not be re-traversed on the same path.
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "a",
        None,
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "exactly one path [a, b]");
    assert_eq!(names(&paths[0]), vec!["a", "b"]);
}

#[test]
fn unreachable_target_is_empty() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
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
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "ghost",
        None,
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(paths.is_empty(), "unknown entry yields no paths");
}

#[test]
fn all_paths_without_target() {
    let idx = build_graph(BRANCH_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "branch",
        None,
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
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
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "branch",
        Some("left"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0]), vec!["branch", "left"]);
}

#[test]
fn max_depth_bounds_enumeration() {
    let idx = build_graph(CHAIN_SOURCE);
    let map = build_callee_map(&idx);
    // Depth 1: e1 → e2 recorded, e3 never reached.
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "e1",
        None,
        1,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1);
    assert_eq!(names(&paths[0]), vec!["e1", "e2"]);
}

#[test]
fn target_at_entry_is_immediate_path() {
    let idx = build_graph(CYCLE_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "a",
        Some("a"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "entry == target is a single-node path");
    assert_eq!(names(&paths[0]), vec!["a"]);
}

#[test]
fn path_cap_truncates() {
    let idx = build_graph(BRANCH_SOURCE);
    let map = build_callee_map(&idx);
    let (paths, truncated) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "branch",
        None,
        DEFAULT_MAX_DEPTH,
        1,
    );
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
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "entryPoint",
        None,
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(!paths.is_empty(), "kotlin path exists");
    for p in &paths {
        let names: Vec<&str> = p.iter().map(|(_, n)| n.as_str()).collect();
        assert!(
            !names.contains(&"swiftOnlyLeaf"),
            "kotlin path must not reach Swift leaf: {names:?}"
        );
    }
    // Swift path: swiftEntry -> sharedName -> swiftOnlyLeaf only.
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "swiftEntry",
        None,
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
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
        &std::collections::HashMap::new(),
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
        &std::collections::HashMap::new(),
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
        &std::collections::HashMap::new(),
        "kotlinEntry",
        Some("javaLeaf"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "kotlin entry reaches java leaf: {paths:?}");
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, vec!["kotlinEntry", "JavaHelper.javaMid", "javaLeaf"]);
}

// ── same-named methods on different types (issue #267) ───────────────────────

/// entry calls ExampleReader.process (constructor receiver), which calls
/// readFromDisk. ExampleWriter.process (same name) calls writeToDisk. The
/// writer's method must NOT be reachable from entry — no false-positive paths.
#[test]
fn same_named_methods_do_not_merge_across_types() {
    let idx = Arc::new(Indexer::new());
    let src = r#"
class ExampleReader {
    fun process() { readFromDisk() }
    fun readFromDisk() {}
}
class ExampleWriter {
    fun process() { writeToDisk() }
    fun writeToDisk() {}
}
fun entry() { ExampleReader().process() }
"#;
    let uri = Url::from_file_path(std::env::temp_dir().join("Types.kt")).expect("uri");
    idx.index_content(&uri, src);

    let map = build_callee_map(&idx);
    // The real path exists: entry → ExampleReader.process → readFromDisk.
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "entry",
        Some("readFromDisk"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "reader path found: {paths:?}");
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["entry", "ExampleReader.process", "readFromDisk"]
    );

    // The false path must not exist: writeToDisk is unreachable from entry.
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "entry",
        Some("writeToDisk"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert!(
        paths.is_empty(),
        "writer path must not be reachable: {paths:?}"
    );
}

// ── bare-name entry + variable receiver (issue #273) ─────────────────────────

#[test]
fn bare_name_entry_resolves_to_unique_class_method() {
    let idx = Arc::new(Indexer::new());
    let src = "class ExampleService {\n    fun handle() { doSend() }\n    fun doSend() {}\n}\nfun entry() { ExampleService().handle() }\n";
    let uri = Url::from_file_path(std::env::temp_dir().join("Bare273.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);

    let resolved = resolve_entry_key("handle", &map).expect("bare name resolves");
    assert_eq!(resolved, "ExampleService.handle");
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "handle",
        Some("doSend"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "bare entry path: {paths:?}");
}

#[test]
fn ambiguous_bare_name_resolves_to_none() {
    let idx = Arc::new(Indexer::new());
    let src = "class ExampleReader {\n    fun process() { readFromDisk() }\n}\nclass ExampleWriter {\n    fun process() { writeToDisk() }\n}\n";
    let uri = Url::from_file_path(std::env::temp_dir().join("Amb273.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    assert!(
        resolve_entry_key("process", &map).is_none(),
        "ambiguous bare name must not silently pick one"
    );
    assert_eq!(bare_name_matches("process", &map), 2);
}

#[test]
fn variable_receiver_callee_follows_unique_method() {
    // `client.send()` — variable receiver, bare callee "send" — must follow
    // the unique ExampleClient.send (issue #273 truncation).
    let idx = Arc::new(Indexer::new());
    let src = "class ExampleClient {\n    fun send() { doSend() }\n    fun doSend() {}\n}\nclass ExampleService {\n    fun handle(c: ExampleClient) {\n        c.send()\n    }\n}\nfun entry() {\n    val client = ExampleClient()\n    ExampleService().handle(client)\n}\n";
    let uri = Url::from_file_path(std::env::temp_dir().join("Var273.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "entry",
        Some("doSend"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "variable receiver followed: {paths:?}");
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "entry",
            "ExampleService.handle",
            "ExampleClient.send",
            "doSend"
        ],
        "receiver type resolution (issue #278) qualifies the variable receiver"
    );
}

// ── interface/abstract member expansion (issue #285) ─────────────────────────

#[test]
fn interface_receiver_expands_implementor_body() {
    let idx = Arc::new(Indexer::new());
    let src = "interface Reader {\n    fun process()\n}\nclass ImplReader : Reader {\n    override fun process() {\n        readFromDisk()\n    }\n    fun readFromDisk() {}\n}\nfun useReader(r: Reader) {\n    r.process()\n}\nfun entry() {\n    useReader(ImplReader())\n}\n";
    let uri = Url::from_file_path(std::env::temp_dir().join("Iface285.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    let impls_map = build_impls_map(&idx);
    let (paths, _) = enumerate_paths(
        &map,
        &impls_map,
        "entry",
        Some("readFromDisk"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(
        paths.len(),
        1,
        "implementor body reached through interface: {paths:?}"
    );
}

// ── chained-call receiver resolution (issue #295) ────────────────────────────

#[test]
fn chained_call_keys_outer_callee_against_inner_return_type() {
    // `api.fetch().onFailure()`: onFailure is declared on ExampleResult (what
    // fetch returns), not on ExampleApi. The path must run through
    // ExampleResult.onFailure — the old behavior fabricated ExampleApi.onFailure
    // (a node that does not exist) and dropped the real edge.
    let idx = Arc::new(Indexer::new());
    let src = r#"
class ExampleResult {
    fun onFailure(): ExampleResult {
        handleFailure()
        return this
    }
}
fun handleFailure() {
    println("failure")
}
class ExampleApi {
    fun fetch(): ExampleResult {
        doNetworkIO()
        return ExampleResult()
    }
}
fun doNetworkIO() {
    println("io")
}
class Caller(private val api: ExampleApi) {
    fun chained() {
        api.fetch().onFailure()
    }
}
"#;
    let uri = Url::from_file_path(std::env::temp_dir().join("Chain295.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    assert!(
        !map.keys().any(|k| k == "ExampleApi.onFailure"),
        "fabricated ExampleApi.onFailure must not be a graph node"
    );
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "Caller.chained",
        Some("handleFailure"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(
        paths.len(),
        1,
        "chained call reaches handleFailure: {paths:?}"
    );
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["Caller.chained", "ExampleResult.onFailure", "handleFailure"],
        "outer callee keyed by fetch's return type"
    );
}

#[test]
fn nested_chained_call_resolves_recursively() {
    // `api.fetch().onFailure().again()` — the parser emits compound keys for
    // each level; the resolver must chase fetch → ExampleResult, then
    // ExampleResult.onFailure → ExampleResult again. Each compound key
    // resolves to the callee keyed by the return type at that hop.
    let idx = Arc::new(Indexer::new());
    let src = r#"
class ExampleResult {
    fun onFailure(): ExampleResult { noteFailure(); return this }
    fun noteFailure() {}
    fun again(): ExampleResult { doRecovery(); return this }
}
fun doRecovery() {}
class ExampleApi {
    fun fetch(): ExampleResult { return ExampleResult() }
}
class Caller(private val api: ExampleApi) {
    fun chained() {
        api.fetch().onFailure().again()
    }
}
"#;
    let uri = Url::from_file_path(std::env::temp_dir().join("ChainNested295.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    // Both hops resolve to real keys: `ExampleResult.onFailure` (from the
    // one-hop compound) and `ExampleResult.again` (from the two-hop one).
    let callers = map.get("Caller.chained").expect("caller present");
    let callees: Vec<&str> = callers.iter().map(|(_, c)| c.as_str()).collect();
    assert!(
        callees.contains(&"ExampleResult.onFailure"),
        "one-hop compound resolves: {callees:?}"
    );
    assert!(
        callees.contains(&"ExampleResult.again"),
        "two-hop compound resolves through both return types: {callees:?}"
    );
    // Each real callee expands its own body.
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "Caller.chained",
        Some("doRecovery"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    assert_eq!(paths.len(), 1, "nested chain reaches doRecovery: {paths:?}");
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["Caller.chained", "ExampleResult.again", "doRecovery"],
        "outermost callee keys against the final return type"
    );
    let (paths, _) = enumerate_paths(
        &map,
        &std::collections::HashMap::new(),
        "Caller.chained",
        Some("noteFailure"),
        DEFAULT_MAX_DEPTH,
        MAX_PATHS,
    );
    let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(
        names,
        vec!["Caller.chained", "ExampleResult.onFailure", "noteFailure"],
        "intermediate hop follows its own body"
    );
}

#[test]
fn chained_call_unknown_return_type_never_attributes_to_root_receiver() {
    // `someApi.getThings().toResult().map { }` where getThings's return type
    // is unknown (inferred / stdlib): the chain must NOT be reported as
    // SomeApi.toResult / SomeApi.map. The methods stay bare — dropped edges
    // are recoverable, a wrong edge is not (issue #295 point 2).
    let idx = Arc::new(Indexer::new());
    let src = r#"
class SomeApi {
    fun getThings() {
        doIO()
    }
}
fun doIO() {}
class Caller(private val api: SomeApi) {
    fun chained() {
        api.getThings().toResult().map { }
    }
}
"#;
    let uri = Url::from_file_path(std::env::temp_dir().join("ChainUnknown295.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    assert!(
        !map.keys()
            .any(|k| k == "SomeApi.toResult" || k == "SomeApi.map"),
        "no root-receiver fabrication: {}",
        map.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    // The real inner edge survives; the outer methods stay bare.
    let callers = map.get("Caller.chained").expect("caller present");
    let callees: Vec<&str> = callers.iter().map(|(_, c)| c.as_str()).collect();
    assert!(
        callees.contains(&"SomeApi.getThings"),
        "real inner call: {callees:?}"
    );
    assert!(
        callees.contains(&"toResult"),
        "bare fallback keeps the method: {callees:?}"
    );
    assert!(
        callees.contains(&"map"),
        "bare fallback keeps the method: {callees:?}"
    );
}

// ── delegated property receiver resolution (issue #296) ─────────────────────

#[test]
fn delegated_property_receivers_reach_through_delegate_type() {
    // `by lazy { ExampleClient() }` and `val client by client` with
    // `client: Lazy<ExampleClient>` both give the property the delegate's
    // getValue result type — ExampleClient — so client.send() is
    // ExampleClient.send, and the unique-name fallback cannot fire.
    let idx = Arc::new(Indexer::new());
    let src = r#"
class ExampleClient {
    fun send() {
        transmit()
    }
}
fun transmit() {
    println("sent")
}
class UnrelatedSender {
    fun send() {
        println("unrelated")
    }
}
class ViaLazyDelegateParam(client: Lazy<ExampleClient>) {
    private val client by client
    fun go() { client.send() }
}
class ViaByLazyBlock {
    private val client by lazy { ExampleClient() }
    fun go() { client.send() }
}
"#;
    let uri = Url::from_file_path(std::env::temp_dir().join("Delegate296.kt")).expect("uri");
    idx.index_content(&uri, src);
    let map = build_callee_map(&idx);
    assert!(
        !map.keys().any(|k| k == "Lazy.send"),
        "delegate's own type must not be used"
    );
    for (entry, label) in [
        ("ViaLazyDelegateParam.go", "Lazy-param delegate"),
        ("ViaByLazyBlock.go", "lazy-block delegate"),
    ] {
        let (paths, _) = enumerate_paths(
            &map,
            &std::collections::HashMap::new(),
            entry,
            Some("transmit"),
            DEFAULT_MAX_DEPTH,
            MAX_PATHS,
        );
        assert_eq!(paths.len(), 1, "{label} reaches transmit: {paths:?}");
        let names: Vec<&str> = paths[0].iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![entry, "ExampleClient.send", "transmit"],
            "{label} keys the property by the delegate getValue result"
        );
    }
}
