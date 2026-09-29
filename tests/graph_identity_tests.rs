//! Defensive identity contracts, not compiler binding. Known lossy reach probes live in
//! docs/codebase/GRAPH_IDENTITY.md and P6 observation artifacts, not as blessed results.
#[path = "support/p6_fixture.rs"]
mod p6_fixture;
use p6_fixture::{
    canonical_fixture_file, canonical_reported_file, expected_path, success, Fixture,
};
use serde_json::{json, Value};

fn identity_case(
    sources: &[(&str, &str)],
    declarations: &[(&str, u32, u32)],
    key: &str,
    outgoing: Option<&str>,
) {
    for positional in [false, true] {
        let f = Fixture::new();
        for (file, source) in sources {
            f.write(file, source);
        }
        std::fs::write(f.dir.path().join("cwd/Decoy.kt"), "fun shared() {}\n").expect("decoy");
        let checked = f
            .command()
            .args(["check", f.root.to_str().expect("root"), "--json"])
            .output()
            .expect("check");
        assert_eq!(
            success(&checked),
            json!({"empty_dirs":[],"errors":[],"files_ok":sources.len(),"files_with_errors":0})
        );
        let cache = f.root.join(".cache/kotlin-lsp/index.bin");
        assert!(!cache.exists());
        for warm in [false, true] {
            assert_eq!(cache.exists(), warm);
            let (file, line, col) = declarations[0];
            let line = line.to_string();
            let col = col.to_string();
            let mut args = vec!["call", "hierarchy"];
            if positional {
                args.extend([file, &line, &col, "--outgoing"]);
            } else {
                args.push("shared");
            }
            args.extend(["--json", "--no-stdlib"]);
            let out = f.run(&args, None);
            if let Some(leaf) = outgoing.filter(|_| positional) {
                assert_eq!(
                    success(&out),
                    json!({"name":"shared","incoming":[],"outgoing":[leaf]})
                );
            } else {
                assert_eq!(out.status.code(), Some(1), "{out:?}");
                let data: Value = serde_json::from_slice(&out.stdout).expect("ambiguity JSON");
                assert!(data["error"]
                    .as_str()
                    .expect("error")
                    .contains("Ambiguous callable"));
                let expected: Vec<_> = declarations.iter().map(|(file,line,col)|json!({"file":expected_path(&f.root.join(file)),"line":line,"col":col,"name":key})).collect();
                assert_eq!(data["candidates"], json!(expected));
            }
        }
    }
}

#[test]
fn package_names_require_candidates_but_declaration_outgoing_keeps_file_identity() {
    identity_case(
        &[
            (
                "a/A.kt",
                "package alpha\nfun shared() { alphaLeaf() }\nfun alphaLeaf() {}\n",
            ),
            (
                "b/B.kt",
                "package beta\nfun shared() { betaLeaf() }\nfun betaLeaf() {}\n",
            ),
        ],
        &[("a/A.kt", 2, 5), ("b/B.kt", 2, 5)],
        "shared",
        Some("alphaLeaf"),
    );
}

#[test]
fn same_class_names_across_packages_keep_declaration_outgoing_separate() {
    identity_case(&[("a/A.kt","package alpha\nclass Worker {\n fun shared() { alphaLeaf() }\n}\nfun alphaLeaf() {}\n"),("b/B.kt","package beta\nclass Worker {\n fun shared() { betaLeaf() }\n}\nfun betaLeaf() {}\n")],&[("a/A.kt",3,6),("b/B.kt",3,6)],"Worker.shared",Some("alphaLeaf"));
}

#[test]
fn different_file_overloads_keep_declaration_outgoing_separate() {
    identity_case(
        &[
            (
                "A.kt",
                "fun shared(x: Int) { intLeaf() }\nfun intLeaf() {}\n",
            ),
            (
                "B.kt",
                "fun shared(x: String) { stringLeaf() }\nfun stringLeaf() {}\n",
            ),
        ],
        &[("A.kt", 1, 5), ("B.kt", 1, 5)],
        "shared",
        Some("intLeaf"),
    );
}

#[test]
fn same_file_overloads_refuse_to_merge_even_at_declaration_position() {
    identity_case(&[("Over.kt","fun shared(x: Int) { intLeaf() }\nfun shared(x: String) { stringLeaf() }\nfun intLeaf() {}\nfun stringLeaf() {}\n")],&[("Over.kt",1,5),("Over.kt",2,5)],"shared",None);
}

#[test]
fn same_line_overloads_remain_distinct_candidates_without_blessing_bad_signatures() {
    identity_case(&[("Over.kt","fun shared(x: Int) { intLeaf() }; fun shared(x: String) { stringLeaf() }\nfun intLeaf() {}\nfun stringLeaf() {}\n")],&[("Over.kt",1,5),("Over.kt",1,39)],"shared",None);
}

#[test]
fn source_set_names_do_not_merge_in_declaration_outgoing() {
    identity_case(
        &[
            (
                "src/commonMain/kotlin/Same.kt",
                "package demo\nfun shared() { commonLeaf() }\nfun commonLeaf() {}\n",
            ),
            (
                "src/jvmMain/kotlin/Same.kt",
                "package demo\nfun shared() { jvmLeaf() }\nfun jvmLeaf() {}\n",
            ),
        ],
        &[
            ("src/commonMain/kotlin/Same.kt", 2, 5),
            ("src/jvmMain/kotlin/Same.kt", 2, 5),
        ],
        "shared",
        Some("commonLeaf"),
    );
}

#[test]
fn unique_control_has_exact_reach_graph_snapshot_edges_cold_and_warm() {
    for command in [
        vec![
            "call",
            "reach",
            "uniqueEntry",
            "--to",
            "uniqueLeaf",
            "--no-stdlib",
        ],
        vec!["tool", "graph"],
        vec!["tool", "snapshot"],
    ] {
        let f = Fixture::new();
        f.write(
            "Unique.kt",
            "fun uniqueLeaf() {}\nfun uniqueEntry() { uniqueLeaf() }\n",
        );
        std::fs::write(f.dir.path().join("cwd/Decoy.kt"), "fun foreignDecoy() {}\n")
            .expect("decoy");
        let canonical = canonical_fixture_file(&f.root.join("Unique.kt"));
        for warm in [false, true] {
            assert_eq!(f.root.join(".cache/kotlin-lsp/index.bin").exists(), warm);
            let mut args = command.clone();
            args.push("--json");
            let data = success(&f.run(&args, None));
            // Reported path spellings differ per command and platform (native
            // path, full `file://` URI, `/C:/…` URL tail, 8.3 short names);
            // compare canonical filesystem identity and keep every non-path
            // field, key set and result-set size exact.
            match command[1] {
                "reach" => {
                    let mut keys: Vec<&str> = data
                        .as_object()
                        .expect("reach object")
                        .keys()
                        .map(String::as_str)
                        .collect();
                    keys.sort_unstable();
                    assert_eq!(keys, ["entry", "paths", "target", "truncated"], "{data}");
                    assert_eq!(data["entry"], "uniqueEntry", "{data}");
                    assert_eq!(data["target"], "uniqueLeaf", "{data}");
                    assert_eq!(data["truncated"], false, "{data}");
                    let paths = data["paths"].as_array().expect("paths");
                    assert_eq!(paths.len(), 1, "{data}");
                    // Path spellings forced whole-object equality to be replaced
                    // with per-field checks, but the key sets of nested objects
                    // stay exact: an unexpected field must still fail.
                    let mut path_keys: Vec<&str> = paths[0]
                        .as_object()
                        .expect("path object")
                        .keys()
                        .map(String::as_str)
                        .collect();
                    path_keys.sort_unstable();
                    assert_eq!(path_keys, ["nodes"], "{data}");
                    let nodes = paths[0]["nodes"].as_array().expect("nodes");
                    assert_eq!(nodes.len(), 2, "{data}");
                    for node in nodes {
                        let mut node_keys: Vec<&str> = node
                            .as_object()
                            .expect("node object")
                            .keys()
                            .map(String::as_str)
                            .collect();
                        node_keys.sort_unstable();
                        assert_eq!(node_keys, ["file", "line", "name"], "{data}");
                    }
                    let files: Vec<&str> = nodes
                        .iter()
                        .map(|node| node["file"].as_str().expect("URI"))
                        .collect();
                    assert!(
                        files.iter().all(|file| file.starts_with("file:")),
                        "reach nodes report file URIs: {files:?}"
                    );
                    assert_eq!(files[0], files[1], "{data}");
                    assert_eq!(canonical_reported_file(files[0]), canonical, "{data}");
                    assert_eq!(nodes[0]["name"], "uniqueEntry", "{data}");
                    assert_eq!(nodes[0]["line"], 2, "{data}");
                    assert_eq!(nodes[1]["name"], "uniqueLeaf", "{data}");
                    assert_eq!(nodes[1]["line"], 1, "{data}");
                }
                "graph" => {
                    let mut symbols: Vec<_> = data["symbols"]
                        .as_array()
                        .expect("symbols")
                        .iter()
                        .map(|v| v.as_str().expect("symbol"))
                        .collect();
                    symbols.sort_unstable();
                    assert_eq!(symbols, ["uniqueEntry", "uniqueLeaf"]);
                    let edges = data["edges"].as_object().expect("edges");
                    let mut edge_keys: Vec<&str> = edges.keys().map(String::as_str).collect();
                    edge_keys.sort_unstable();
                    assert_eq!(
                        edge_keys,
                        ["calls", "imports", "inheritance", "overrides"],
                        "{data}"
                    );
                    let calls = edges["calls"].as_array().expect("calls");
                    assert_eq!(calls.len(), 1, "{data}");
                    let call = calls[0].as_object().expect("call");
                    let mut call_keys: Vec<&str> = call.keys().map(String::as_str).collect();
                    call_keys.sort_unstable();
                    assert_eq!(call_keys, ["callee", "caller", "caller_file"], "{data}");
                    assert_eq!(call["callee"], "uniqueLeaf", "{data}");
                    assert_eq!(call["caller"], "uniqueEntry", "{data}");
                    let caller_file = call["caller_file"].as_str().expect("URI");
                    assert!(
                        caller_file.starts_with("file:"),
                        "graph caller_file is a URI: {caller_file}"
                    );
                    assert_eq!(canonical_reported_file(caller_file), canonical, "{data}");
                    for empty in ["imports", "inheritance", "overrides"] {
                        assert_eq!(edges[empty], json!([]), "{data}");
                    }
                }
                "snapshot" => {
                    assert_eq!(
                        data["relationships"],
                        json!({"calls":[["uniqueEntry","uniqueLeaf"]],"extends":[],"overrides":[],"imports":[]})
                    );
                    let mut symbols: Vec<_> = data["symbols"]
                        .as_array()
                        .expect("symbols")
                        .iter()
                        .map(|s| {
                            (
                                s["name"].as_str().expect("name"),
                                s["line"].as_u64().expect("line"),
                                canonical_reported_file(s["file"].as_str().expect("file")),
                            )
                        })
                        .collect();
                    symbols.sort_unstable();
                    assert_eq!(
                        symbols,
                        [
                            ("uniqueEntry", 2, canonical.clone()),
                            ("uniqueLeaf", 1, canonical.clone())
                        ]
                    );
                }
                _ => panic!("known command"),
            }
        }
    }
}
