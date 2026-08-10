//! Tests for `call diff` (src/cli/call_diff.rs): tree building, the
//! LCS-aligned structural diff, definition scanning, and rendering.

use super::call_diff::{
    build_callee_map, build_tree, diff_trees, render_diff, scan_def_locations, CallNode, DiffNode,
    DiffStatus,
};

fn node(name: &str, children: Vec<CallNode>) -> CallNode {
    CallNode {
        name: name.to_string(),
        file: String::new(),
        line: 0,
        children,
    }
}

// ── build_callee_map / build_tree ────────────────────────────────────────────

#[test]
fn callee_map_groups_by_caller() {
    let edges = vec![
        ("a".to_string(), "b".to_string()),
        ("a".to_string(), "c".to_string()),
        ("b".to_string(), "d".to_string()),
    ];
    let map = build_callee_map(&edges);
    assert_eq!(map["a"], vec!["b", "c"]);
    assert_eq!(map["b"], vec!["d"]);
    assert!(!map.contains_key("d"), "leaf has no callees");
}

#[test]
fn tree_expands_entry_chain() {
    let edges = vec![
        ("e1".to_string(), "e2".to_string()),
        ("e2".to_string(), "e3".to_string()),
    ];
    let tree = build_tree(&edges, "e1");
    assert_eq!(tree.name, "e1");
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].name, "e2");
    assert_eq!(tree.children[0].children[0].name, "e3");
}

#[test]
fn tree_cycle_terminates() {
    // a → b → a: the back edge must not recurse on the same path. The cycle
    // node still appears as a leaf (calldiff renders it ⇄) with no children.
    let edges = vec![
        ("a".to_string(), "b".to_string()),
        ("b".to_string(), "a".to_string()),
    ];
    let tree = build_tree(&edges, "a");
    assert_eq!(tree.name, "a");
    assert_eq!(tree.children.len(), 1, "a -> b");
    assert_eq!(tree.children[0].name, "b");
    let cycle = &tree.children[0].children;
    assert_eq!(cycle.len(), 1, "b -> a is the cycle back-edge");
    assert_eq!(cycle[0].name, "a");
    assert!(cycle[0].children.is_empty(), "cycle node does not recurse");
}

#[test]
fn tree_diamond_repeats_shared_callee_per_branch() {
    // b → d and c → d: d appears under both branches (calldiff calltree
    // semantics — one expansion per call site, not one per definition).
    let edges = vec![
        ("a".to_string(), "b".to_string()),
        ("a".to_string(), "c".to_string()),
        ("b".to_string(), "d".to_string()),
        ("c".to_string(), "d".to_string()),
    ];
    let tree = build_tree(&edges, "a");
    assert_eq!(tree.children.len(), 2);
    assert_eq!(tree.children[0].children[0].name, "d");
    assert_eq!(tree.children[1].children[0].name, "d");
}

// ── scan_def_locations ───────────────────────────────────────────────────────

#[test]
fn scan_finds_fun_definition_lines() {
    let src = "package x\n\nfun alpha(): String = \"a\"\n\nfun beta(x: Int): Int = x\n";
    let locs = scan_def_locations([("Main.kt", src)]);
    assert_eq!(locs["alpha"], ("Main.kt".to_string(), 3));
    assert_eq!(locs["beta"], ("Main.kt".to_string(), 5));
}

#[test]
fn scan_skips_non_fun_lines() {
    // Best-effort POC scan: only line-leading `fun` definitions are found
    // (nested `fun method()` inside a class body needs a full CST walk, out
    // of POC scope).
    let src = "val x = 1\nfun alpha() = 2\nclass C { fun method() = 3 }\n";
    let locs = scan_def_locations([("Main.kt", src)]);
    assert_eq!(locs.len(), 1, "only line-leading fun is found (alpha)");
    assert!(locs.contains_key("alpha"));
}

// ── diff_trees (LCS-aligned structural diff) ─────────────────────────────────

#[test]
fn diff_identical_trees_are_all_same() {
    let before = node("root", vec![node("a", vec![]), node("b", vec![])]);
    let after = node("root", vec![node("a", vec![]), node("b", vec![])]);
    let diff = diff_trees(&before, &after);
    assert_eq!(diff.status, DiffStatus::Same);
    assert!(diff.children.iter().all(|c| c.status == DiffStatus::Same));
}

#[test]
fn diff_added_child() {
    let before = node("root", vec![node("a", vec![])]);
    let after = node("root", vec![node("a", vec![]), node("b", vec![])]);
    let diff = diff_trees(&before, &after);
    let statuses: Vec<DiffStatus> = diff.children.iter().map(|c| c.status).collect();
    assert_eq!(statuses, vec![DiffStatus::Same, DiffStatus::Added]);
}

#[test]
fn diff_removed_child() {
    let before = node("root", vec![node("a", vec![]), node("b", vec![])]);
    let after = node("root", vec![node("a", vec![])]);
    let diff = diff_trees(&before, &after);
    let statuses: Vec<DiffStatus> = diff.children.iter().map(|c| c.status).collect();
    assert_eq!(statuses, vec![DiffStatus::Same, DiffStatus::Removed]);
}

#[test]
fn diff_reordered_children_match_by_name() {
    // LCS matches b and c as Same across the reorder; with the calldiff
    // backtrack rule (removed-first), the output surfaces the removed `a`
    // before the added `d`.
    let before = node(
        "root",
        vec![node("a", vec![]), node("b", vec![]), node("c", vec![])],
    );
    let after = node(
        "root",
        vec![node("b", vec![]), node("c", vec![]), node("d", vec![])],
    );
    let diff = diff_trees(&before, &after);
    let names: Vec<&str> = diff.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "c", "d"]);
    assert_eq!(diff.children[0].status, DiffStatus::Removed);
    assert_eq!(diff.children[1].status, DiffStatus::Same);
    assert_eq!(diff.children[3].status, DiffStatus::Added);
}

#[test]
fn diff_nested_changes_recurse() {
    let before = node("root", vec![node("a", vec![node("x", vec![])])]);
    let after = node(
        "root",
        vec![node("a", vec![node("x", vec![]), node("y", vec![])])],
    );
    let diff = diff_trees(&before, &after);
    let a = &diff.children[0];
    assert_eq!(a.status, DiffStatus::Same);
    let statuses: Vec<DiffStatus> = a.children.iter().map(|c| c.status).collect();
    assert_eq!(statuses, vec![DiffStatus::Same, DiffStatus::Added]);
}

#[test]
fn diff_added_subtree_marked_recursively() {
    let before = node("root", vec![]);
    let after = node("root", vec![node("a", vec![node("b", vec![])])]);
    let diff = diff_trees(&before, &after);
    assert_eq!(diff.children[0].status, DiffStatus::Added);
    assert_eq!(diff.children[0].children[0].status, DiffStatus::Added);
}

// ── render_diff ──────────────────────────────────────────────────────────────

#[test]
fn render_marks_status_prefixes() {
    let diff = DiffNode {
        name: "root".to_string(),
        file: String::new(),
        line: 0,
        status: DiffStatus::Same,
        children: vec![
            DiffNode {
                name: "a".to_string(),
                file: String::new(),
                line: 0,
                status: DiffStatus::Added,
                children: vec![],
            },
            DiffNode {
                name: "b".to_string(),
                file: String::new(),
                line: 0,
                status: DiffStatus::Removed,
                children: vec![],
            },
        ],
    };
    let out = render_diff(&diff);
    assert!(out.starts_with("  └─ root\n"), "root is Same: got {out:?}");
    assert!(out.contains("+    ├─ a\n"), "added child: got {out:?}");
    assert!(out.contains("-    └─ b\n"), "removed child: got {out:?}");
}

// ── git integration (slow under heavy machine load — normally ignored) ───────

/// End-to-end: a temp git repo with two commits (entry→a, then entry→a+b);
/// `call diff` semantics should report `b` as added.
#[test]
fn git_diff_end_to_end() {
    use std::process::Command;
    use tempfile::TempDir;

    let dir = TempDir::new().expect("temp dir");
    let v1 = "package demo\n\nfun a(): String = \"a\"\n\nfun entry(): String = a()\n";
    let v2 = "package demo\n\nfun a(): String = \"a\"\nfun b(): String = \"b\"\n\nfun entry(): String = if (true) a() else b()\n";
    std::fs::write(dir.path().join("Main.kt"), v1).expect("write v1");

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    git(&["add", "Main.kt"]);
    git(&["commit", "-qm", "v1"]);
    std::fs::write(dir.path().join("Main.kt"), v2).expect("write v2");
    git(&["add", "Main.kt"]);
    git(&["commit", "-qm", "v2"]);

    let before =
        crate::cli::call_diff::load_tree("HEAD~1", "entry", dir.path()).expect("load v1 tree");
    let after =
        crate::cli::call_diff::load_tree("HEAD", "entry", dir.path()).expect("load v2 tree");
    let diff = diff_trees(&before, &after);
    // v2 entry calls a and b; v1 entry calls only a → b is Added.
    let b_node = diff
        .children
        .iter()
        .find(|c| c.name == "b")
        .expect("b present in diff");
    assert_eq!(b_node.status, DiffStatus::Added);
    assert!(
        !b_node.file.is_empty(),
        "definition location filled from scan"
    );
}
