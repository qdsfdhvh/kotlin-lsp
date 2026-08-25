//! Tests for `call diff` (src/cli/call_diff.rs): branch-aware tree building,
//! ⇄ cycle markers, entry inference, git-diff-style snapshot resolution,
//! LCS diff, and rendering (branch rail).

use std::collections::HashMap;
use std::path::Path;

use super::call_diff::{
    build_tree, diff_entry, diff_trees, infer_entries, render_diff, resolve_snapshots_and_paths,
    scan_def_locations, DiffNode, DiffStatus, NodeKind, Snapshot, DEFAULT_MAX_DEPTH,
};
use crate::cli::call_diff::{test_node, CallNode};
use crate::cli::call_steps::{extract_functions, FuncInfo};

fn index(source: &str) -> HashMap<String, FuncInfo> {
    extract_functions(source)
        .into_iter()
        .map(|f| (f.key.clone(), f))
        .collect()
}

fn call(name: &str) -> CallNode {
    test_node(name, NodeKind::Call, vec![])
}

// ── build_tree (branch-aware, ⇄ marker) ──────────────────────────────────────

#[test]
fn tree_expands_calls_into_children() {
    let idx = index("fun e1(): String = e2()\nfun e2(): String = e3()\nfun e3(): String = \"x\"\n");
    let tree = build_tree(&idx, "e1", DEFAULT_MAX_DEPTH);
    assert_eq!(tree.name, "e1()");
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].name, "e2()");
    assert_eq!(tree.children[0].children[0].name, "e3()");
}

#[test]
fn tree_branches_appear_as_kind_branch() {
    let src = "fun pick(x: Int): String = if (x > 0) left() else right()\nfun left() = \"l\"\nfun right() = \"r\"\n";
    let idx = index(src);
    let tree = build_tree(&idx, "pick", DEFAULT_MAX_DEPTH);
    assert_eq!(tree.children.len(), 2, "if + else branches");
    let if_branch = &tree.children[0];
    assert_eq!(if_branch.kind, NodeKind::Branch);
    assert_eq!(if_branch.name, "if x > 0");
    assert_eq!(if_branch.children[0].name, "left()");
    let else_branch = &tree.children[1];
    assert_eq!(else_branch.kind, NodeKind::Branch);
    assert_eq!(else_branch.name, "else");
    assert_eq!(else_branch.children[0].name, "right()");
}

#[test]
fn tree_cycle_marks_with_cycle_sign() {
    // a → b → a: the back edge is a leaf labelled `⇄`, never recurses.
    let idx = index("fun a(): String = b()\nfun b(): String = a()\n");
    let tree = build_tree(&idx, "a", DEFAULT_MAX_DEPTH);
    assert_eq!(tree.name, "a()");
    let b = &tree.children[0];
    assert_eq!(b.name, "b()");
    let cycle = &b.children[0];
    assert_eq!(cycle.name, "a() ⇄");
    assert!(cycle.children.is_empty(), "cycle node does not recurse");
}

#[test]
fn tree_diamond_repeats_shared_callee_per_branch() {
    let src = "fun a() = b()\nfun a2() = c()\nfun b() = d()\nfun c() = d()\nfun d() = \"d\"\n";
    let idx = index(src);
    let tree = build_tree(&idx, "a", DEFAULT_MAX_DEPTH);
    let _ = tree;
    let tree2 = build_tree(&idx, "a2", DEFAULT_MAX_DEPTH);
    let _ = tree2;
    // b and c both reach d; each expansion path keeps its own d node.
    let idx2 = index("fun root() = left()\nfun root2() = right()\nfun left() = shared()\nfun right() = shared()\nfun shared() = \"s\"\n");
    let t = build_tree(&idx2, "root", DEFAULT_MAX_DEPTH);
    assert_eq!(t.children[0].children[0].name, "shared()");
    let t2 = build_tree(&idx2, "root2", DEFAULT_MAX_DEPTH);
    assert_eq!(t2.children[0].children[0].name, "shared()");
}

// ── entry inference ──────────────────────────────────────────────────────────

#[test]
fn inference_picks_changed_exported_function() {
    let before = index("fun entry(): String = a()\nfun a() = \"a\"\n");
    let after =
        index("fun entry(): String = a()\nfun a() = \"a\"\nfun brandNew(): String = \"n\"\n");
    let entries = infer_entries(&before, &after, &[], DEFAULT_MAX_DEPTH).expect("infer");
    assert_eq!(
        entries,
        vec!["brandNew"],
        "new exported function with a differing tree"
    );
}

#[test]
fn inference_falls_back_to_any_changed_function() {
    // No exported tree changed; a private method's call graph differs.
    let before = index(
        "fun stable() = \"s\"\nclass C {\n    private fun hidden() = one()\n    fun one() = \"1\"\n    fun two() = \"2\"\n}\n",
    );
    let after = index(
        "fun stable() = \"s\"\nclass C {\n    private fun hidden() = two()\n    fun one() = \"1\"\n    fun two() = \"2\"\n}\n",
    );
    let entries = infer_entries(&before, &after, &[], DEFAULT_MAX_DEPTH).expect("infer");
    assert_eq!(
        entries,
        vec!["C.hidden"],
        "fallback to any differing function"
    );
}

#[test]
fn inference_explicit_entry_wins_and_resolves() {
    let before = index("fun entry() = a()\nfun a() = \"a\"\n");
    let after = index("fun entry() = a()\nfun a() = \"a\"\nfun extra() = \"e\"\n");
    let entries = infer_entries(&before, &after, &["entry".to_string()], DEFAULT_MAX_DEPTH)
        .expect("infer explicit");
    assert_eq!(entries, vec!["entry"]);
}

#[test]
fn inference_unknown_explicit_entry_errors() {
    let before = index("fun a() = \"a\"\n");
    let after = index("fun a() = \"a\"\n");
    let result = infer_entries(&before, &after, &["ghost".to_string()], DEFAULT_MAX_DEPTH);
    assert!(result.is_err(), "unknown entrypoint must error");
}

#[test]
fn inference_no_changes_yields_empty() {
    let before = index("fun a() = \"a\"\n");
    let after = index("fun a() = \"a\"\n");
    let entries = infer_entries(&before, &after, &[], DEFAULT_MAX_DEPTH).expect("infer");
    assert!(entries.is_empty());
}

// ── diff_entry ───────────────────────────────────────────────────────────────

#[test]
fn diff_entry_unchanged_is_none() {
    let before = index("fun entry() = a()\nfun a() = \"a\"\n");
    let after = index("fun entry() = a()\nfun a() = \"a\"\n");
    assert!(diff_entry("entry", &before, &after, DEFAULT_MAX_DEPTH).is_none());
}

#[test]
fn diff_entry_added_marks_root() {
    let before = index("fun a() = \"a\"\n");
    let after = index("fun a() = \"a\"\nfun brandNew() = \"n\"\n");
    let diff =
        diff_entry("brandNew", &before, &after, DEFAULT_MAX_DEPTH).expect("brandNew added diff");
    assert_eq!(diff.status, DiffStatus::Added, "root forced added");
}

#[test]
fn diff_entry_removed_marks_root() {
    let before = index("fun a() = \"a\"\nfun gone() = \"g\"\n");
    let after = index("fun a() = \"a\"\n");
    let diff = diff_entry("gone", &before, &after, DEFAULT_MAX_DEPTH).expect("gone removed diff");
    assert_eq!(diff.status, DiffStatus::Removed, "root forced removed");
}

#[test]
fn diff_branch_change_detected() {
    let before = index("fun pick(x: Int) = if (x > 0) left() else right()\nfun left() = \"l\"\nfun right() = \"r\"\n");
    let after = index("fun pick(x: Int) = if (x > 0) center() else right()\nfun center() = \"c\"\nfun left() = \"l\"\nfun right() = \"r\"\n");
    let diff = diff_entry("pick", &before, &after, DEFAULT_MAX_DEPTH).expect("branch change");
    // The if-branch's callee changed left → center: an Added node under the branch.
    let if_branch = diff
        .children
        .iter()
        .find(|c| c.name == "if x > 0")
        .expect("if branch in diff");
    assert!(
        if_branch
            .children
            .iter()
            .any(|c| c.status == DiffStatus::Added && c.name == "center()"),
        "new callee under branch"
    );
}

// ── scan_def_locations ───────────────────────────────────────────────────────

#[test]
fn scan_finds_fun_definition_lines() {
    let src = "package x\n\nfun alpha(): String = \"a\"\n\nfun beta(x: Int): Int = x\n";
    let locs = scan_def_locations([("Main.kt", src)]);
    assert_eq!(locs["alpha"], ("Main.kt".to_string(), 3));
    assert_eq!(locs["beta"], ("Main.kt".to_string(), 5));
}

// ── diff_trees (LCS) ─────────────────────────────────────────────────────────

#[test]
fn diff_identical_trees_are_all_same() {
    let before = test_node("root", NodeKind::Call, vec![call("a"), call("b")]);
    let after = test_node("root", NodeKind::Call, vec![call("a"), call("b")]);
    let diff = diff_trees(&before, &after);
    assert_eq!(diff.status, DiffStatus::Same);
    assert!(diff.children.iter().all(|c| c.status == DiffStatus::Same));
}

#[test]
fn diff_added_child() {
    let before = test_node("root", NodeKind::Call, vec![call("a")]);
    let after = test_node("root", NodeKind::Call, vec![call("a"), call("b")]);
    let diff = diff_trees(&before, &after);
    let statuses: Vec<DiffStatus> = diff.children.iter().map(|c| c.status).collect();
    assert_eq!(statuses, vec![DiffStatus::Same, DiffStatus::Added]);
}

#[test]
fn diff_removed_child() {
    let before = test_node("root", NodeKind::Call, vec![call("a"), call("b")]);
    let after = test_node("root", NodeKind::Call, vec![call("a")]);
    let diff = diff_trees(&before, &after);
    let statuses: Vec<DiffStatus> = diff.children.iter().map(|c| c.status).collect();
    assert_eq!(statuses, vec![DiffStatus::Same, DiffStatus::Removed]);
}

#[test]
fn diff_reordered_children_match_by_name() {
    let before = test_node(
        "root",
        NodeKind::Call,
        vec![call("a"), call("b"), call("c")],
    );
    let after = test_node(
        "root",
        NodeKind::Call,
        vec![call("b"), call("c"), call("d")],
    );
    let diff = diff_trees(&before, &after);
    let names: Vec<&str> = diff.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "c", "d"]);
    assert_eq!(diff.children[0].status, DiffStatus::Removed);
    assert_eq!(diff.children[1].status, DiffStatus::Same);
    assert_eq!(diff.children[3].status, DiffStatus::Added);
}

// ── rendering (branch rail, root connector) ─────────────────────────────────

#[test]
fn render_marks_status_prefixes() {
    let diff = DiffNode {
        name: "root".to_string(),
        kind: NodeKind::Call,
        file: String::new(),
        line: 0,
        status: DiffStatus::Same,
        children: vec![
            DiffNode {
                name: "a".to_string(),
                kind: NodeKind::Call,
                file: String::new(),
                line: 0,
                status: DiffStatus::Added,
                children: vec![],
            },
            DiffNode {
                name: "b".to_string(),
                kind: NodeKind::Call,
                file: String::new(),
                line: 0,
                status: DiffStatus::Removed,
                children: vec![],
            },
        ],
    };
    let out = render_diff(&diff);
    assert!(
        out.starts_with("  root\n"),
        "root has no connector: got {out:?}"
    );
    assert!(out.contains("+    ├─ a\n"), "added child: got {out:?}");
    assert!(out.contains("-    └─ b\n"), "removed child: got {out:?}");
}

#[test]
fn render_branch_children_omit_rail() {
    let diff = DiffNode {
        name: "root".to_string(),
        kind: NodeKind::Call,
        file: String::new(),
        line: 0,
        status: DiffStatus::Same,
        children: vec![DiffNode {
            name: "if x > 0".to_string(),
            kind: NodeKind::Branch,
            file: String::new(),
            line: 0,
            status: DiffStatus::Same,
            children: vec![DiffNode {
                name: "left()".to_string(),
                kind: NodeKind::Call,
                file: String::new(),
                line: 0,
                status: DiffStatus::Same,
                children: vec![],
            }],
        }],
    };
    let out = render_diff(&diff);
    assert!(
        out.contains("└─ left()"),
        "branch child present: got {out:?}"
    );
    assert!(
        !out.contains('│'),
        "branch children render without the continuing rail: got {out:?}"
    );
}

// ── git-diff semantics ───────────────────────────────────────────────────────

fn git(cwd: &Path, args: &[&str]) {
    // Scrub inherited git environment: when these tests run inside the
    // pre-commit hook (spawned by the outer `git commit`), GIT_DIR /
    // GIT_INDEX_FILE / GIT_OBJECT_DIRECTORY / GIT_CONFIG_* point at the
    // OUTER repo, so nested `git init`/`config`/`commit` would write into
    // the real repository (corrupting its config and index) instead of the
    // temp fixture repo. Unsetting them makes each nested git operate on its
    // own temp repo regardless of the caller's context.
    let mut cmd = std::process::Command::new("git");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_QUARANTINE_PATH",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        cmd.env_remove(var);
    }
    for i in 0..10 {
        cmd.env_remove(format!("GIT_CONFIG_KEY_{i}"));
        cmd.env_remove(format!("GIT_CONFIG_VALUE_{i}"));
    }
    let out = cmd.args(args).current_dir(cwd).output().expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn make_repo(dir: &Path, committed: &str, worktree: Option<&str>) {
    std::fs::write(dir.join("Main.kt"), committed).expect("write committed version");
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["add", "Main.kt"]);
    git(dir, &["commit", "-qm", "v1"]);
    if let Some(wt) = worktree {
        std::fs::write(dir.join("Main.kt"), wt).expect("write worktree version");
    }
}

#[test]
fn snapshots_default_to_head_vs_worktree() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), "fun entry() = a()\nfun a() = \"a\"\n", None);
    let (from, to, paths) =
        resolve_snapshots_and_paths(dir.path(), None, None, &[]).expect("resolve defaults");
    assert_eq!(
        from,
        Snapshot::Commit {
            ref_name: "HEAD".into()
        }
    );
    assert_eq!(to, Snapshot::Worktree);
    assert!(paths.is_empty());
}

#[test]
fn snapshots_one_ref_vs_worktree() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), "fun a() = \"a\"\n", None);
    let (from, to, _) =
        resolve_snapshots_and_paths(dir.path(), Some("HEAD"), None, &[]).expect("one ref");
    assert_eq!(
        from,
        Snapshot::Commit {
            ref_name: "HEAD".into()
        }
    );
    assert_eq!(to, Snapshot::Worktree);
}

#[test]
fn snapshots_on_disk_positional_becomes_path_filter() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), "fun a() = \"a\"\n", None);
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    let (from, to, paths) =
        resolve_snapshots_and_paths(dir.path(), Some("src"), None, &[]).expect("path filter");
    assert_eq!(
        from,
        Snapshot::Commit {
            ref_name: "HEAD".into()
        }
    );
    assert_eq!(to, Snapshot::Worktree);
    assert_eq!(paths, vec!["src"], "on-disk positional is a path filter");
}

#[test]
fn snapshots_two_refs_both_commits() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), "fun a() = \"a\"\n", None);
    // HEAD~0 is HEAD itself; both refs valid.
    let (from, to, _) =
        resolve_snapshots_and_paths(dir.path(), Some("HEAD"), Some("HEAD"), &[]).expect("two refs");
    assert_eq!(
        from,
        Snapshot::Commit {
            ref_name: "HEAD".into()
        }
    );
    assert_eq!(
        to,
        Snapshot::Commit {
            ref_name: "HEAD".into()
        }
    );
}

#[test]
fn snapshots_unknown_ref_errors() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), "fun a() = \"a\"\n", None);
    let result = resolve_snapshots_and_paths(dir.path(), Some("nope-nope"), None, &[]);
    assert!(result.is_err(), "unknown ref must error");
}

#[test]
fn git_default_diff_head_vs_dirty_worktree() {
    use tempfile::TempDir;
    let v1 = "fun entry(): String = a()\nfun a() = \"a\"\n";
    let v2 = "fun entry(): String = if (true) a() else b()\nfun a() = \"a\"\nfun b() = \"b\"\n";
    let dir = TempDir::new().expect("temp dir");
    make_repo(dir.path(), v1, Some(v2));

    let (from, to, paths) =
        resolve_snapshots_and_paths(dir.path(), None, None, &[]).expect("defaults");
    let before = super::call_diff::load_index(dir.path(), &from, &paths).expect("before index");
    let after = super::call_diff::load_index(dir.path(), &to, &paths).expect("after index");

    // entry's tree changed (b added under else) → diff_entry finds the change.
    let diff = diff_entry("entry", &before, &after, DEFAULT_MAX_DEPTH).expect("changed entry");
    assert!(
        diff.children
            .iter()
            .any(|c| c.name == "if true" || c.name == "if (true)"),
        "branch change surfaced: {}",
        render_diff(&diff)
    );
}

#[test]
fn path_filter_limits_loaded_files() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("a")).expect("mkdir a");
    std::fs::write(dir.path().join("a/One.kt"), "fun one() = \"1\"\n").expect("write one");
    std::fs::write(dir.path().join("Two.kt"), "fun two() = \"2\"\n").expect("write two");
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "v1"]);

    let (_, _, paths) = resolve_snapshots_and_paths(dir.path(), None, None, &["a".to_string()])
        .expect("path filter");
    let files = super::call_diff::list_source_files(dir.path(), &Snapshot::Worktree, &paths)
        .expect("list files");
    assert_eq!(files, vec!["a/One.kt"], "only files under the filter");
}

// ── worktree snapshot file set (issues #260 + #268) ──────────────────────────

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
}

/// #260: gitignored files must not appear in the worktree snapshot.
#[test]
fn worktree_snapshot_excludes_gitignored() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("generated")).expect("mkdir");
    std::fs::write(dir.path().join(".gitignore"), "/generated/\n").expect("gitignore");
    std::fs::write(dir.path().join("Tracked.kt"), "fun tracked() = \"t\"\n").expect("tracked");
    std::fs::write(
        dir.path().join("generated/Gen.kt"),
        "fun sameName() = \"g\"\n",
    )
    .expect("gen");
    init_repo(dir.path());
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "v1"]);

    let files = super::call_diff::list_source_files(dir.path(), &Snapshot::Worktree, &[])
        .expect("list files");
    assert_eq!(
        files,
        vec!["Tracked.kt"],
        "gitignored file excluded: {files:?}"
    );
}

/// #268: a new, unstaged file must be visible in the worktree snapshot.
#[test]
fn worktree_snapshot_includes_unstaged_new_file() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("Base.kt"), "fun base() = \"b\"\n").expect("base");
    init_repo(dir.path());
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);

    // New file, not git-added yet.
    std::fs::write(dir.path().join("New.kt"), "fun brandNew() = \"n\"\n").expect("new");

    let files = super::call_diff::list_source_files(dir.path(), &Snapshot::Worktree, &[])
        .expect("list files");
    assert!(
        files.iter().any(|f| f == "New.kt"),
        "unstaged new file visible: {files:?}"
    );
}

/// #268: a tracked file's uncommitted edits must still be visible (contents are
/// read from disk) — the regression that started this thread.
#[test]
fn worktree_snapshot_reads_edited_tracked_content() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("temp dir");
    let f = dir.path().join("Tracked.kt");
    std::fs::write(&f, "fun entry() = helper()\nfun helper() = \"v1\"\n").expect("v1");
    init_repo(dir.path());
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "v1"]);
    // entry now routes through a new function — the tree change must surface.
    std::fs::write(
        &f,
        "fun entry() = added()\nfun added() = helper()\nfun helper() = \"v2\"\n",
    )
    .expect("v2");

    let before = super::call_diff::load_index(
        dir.path(),
        &Snapshot::Commit {
            ref_name: "HEAD".into(),
        },
        &[],
    )
    .expect("before index");
    let after =
        super::call_diff::load_index(dir.path(), &Snapshot::Worktree, &[]).expect("after index");
    let before_tree =
        super::call_diff::build_tree(&before, "entry", super::call_diff::DEFAULT_MAX_DEPTH);
    let after_tree =
        super::call_diff::build_tree(&after, "entry", super::call_diff::DEFAULT_MAX_DEPTH);
    let diff = diff_trees(&before_tree, &after_tree);
    assert!(
        diff.children.iter().any(|c| c.name.contains("added")),
        "edited tracked file change surfaced in diff"
    );
}
