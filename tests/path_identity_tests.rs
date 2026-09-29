//! Root spellings must not create duplicate index identities across CLI processes.
#[path = "support/p6_fixture.rs"]
mod p6_fixture;
use p6_fixture::{canonical_reported_file, execute, success, Fixture};

#[test]
fn alternate_root_spellings_keep_cold_and_warm_query_identity() {
    let f = Fixture::new();
    let file = "space 文 #%.kt";
    f.write(file, "fun leaf() {}\nfun entry() { leaf() }\n");
    let target = f.root.join(file).canonicalize().expect("source identity");
    let canonical_root = f.root.canonicalize().expect("canonical root");
    let dotted_root = f.root.join(".");
    // Windows temp paths may use 8.3 spelling; canonicalize expands it and
    // adds the verbatim prefix. All variants must hit the same cached entries.
    for root in [&f.root, &canonical_root, &dotted_root, &f.root] {
        let mut graph = f.command();
        graph.args(["tool", "graph", "--json", "--root"]).arg(root);
        let graph = success(&execute(graph, None).0);
        let mut names: Vec<_> = graph["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .map(|symbol| symbol.as_str().expect("symbol name"))
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["entry", "leaf"], "{graph}");
        let calls = graph["edges"]["calls"].as_array().expect("calls");
        assert_eq!(calls.len(), 1, "{graph}");
        assert_eq!(calls[0]["caller"], "entry");
        assert_eq!(calls[0]["callee"], "leaf");
        assert_eq!(
            canonical_reported_file(calls[0]["caller_file"].as_str().expect("caller URI")),
            target
        );
        assert!(f.root.join(".cache/kotlin-lsp/index.bin").is_file());

        let mut hover = f.command();
        hover
            .args(["hover"])
            .arg(&target)
            .args(["2", "5", "--smart", "--json", "--no-stdlib", "--root"])
            .arg(root);
        let hover = success(&execute(hover, None).0);
        assert_eq!(
            hover,
            serde_json::json!({"signature": "fun entry() { leaf() }"})
        );
    }
}
