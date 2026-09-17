//! Defensive identity contracts, not compiler binding. Known lossy reach probes live in
//! docs/codebase/GRAPH_IDENTITY.md and P6 observation artifacts, not as blessed results.
#[path = "support/p6_fixture.rs"]
mod p6_fixture;
use p6_fixture::{expected_path, success, Fixture};
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
        let file = expected_path(&f.root.join("Unique.kt"));
        let uri = tower_lsp::lsp_types::Url::from_file_path(&file)
            .expect("URI")
            .to_string();
        for warm in [false, true] {
            assert_eq!(f.root.join(".cache/kotlin-lsp/index.bin").exists(), warm);
            let mut args = command.clone();
            args.push("--json");
            let data = success(&f.run(&args, None));
            match command[1] {
                "reach" => assert_eq!(
                    data,
                    json!({"entry":"uniqueEntry","target":"uniqueLeaf","truncated":false,"paths":[{"nodes":[{"file":uri,"line":2,"name":"uniqueEntry"},{"file":uri,"line":1,"name":"uniqueLeaf"}]}]})
                ),
                "graph" => {
                    let mut symbols: Vec<_> = data["symbols"]
                        .as_array()
                        .expect("symbols")
                        .iter()
                        .map(|v| v.as_str().expect("symbol"))
                        .collect();
                    symbols.sort_unstable();
                    assert_eq!(symbols, ["uniqueEntry", "uniqueLeaf"]);
                    assert_eq!(
                        data["edges"],
                        json!({"calls":[{"callee":"uniqueLeaf","caller":"uniqueEntry","caller_file":uri}],"imports":[],"inheritance":[],"overrides":[]})
                    );
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
                                s["file"].as_str().expect("file"),
                            )
                        })
                        .collect();
                    symbols.sort_unstable();
                    assert_eq!(
                        symbols,
                        [
                            ("uniqueEntry", 2, file.as_str()),
                            ("uniqueLeaf", 1, file.as_str())
                        ]
                    );
                }
                _ => panic!("known command"),
            }
        }
    }
}
