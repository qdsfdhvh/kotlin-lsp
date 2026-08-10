//! Call-tree git diff — `call diff <ref1> <ref2> --entry NAME`.
//!
//! POC (Phase 3a): single entry, Kotlin only. Reads source from both git
//! refs via `git show`, expands the entry into a callee tree per ref, then
//! diffs the two trees with an LCS-aligned structural diff (mirrors calldiff
//! `src/diff.ts`) — which callees appeared, disappeared, or stayed under an
//! entrypoint between two commits. Built for agentic review: when an agent
//! rewires call flow, this shows the shape of the change instead of a flat
//! line diff.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// Max callee-tree depth per ref — keeps expansion bounded on deep graphs.
pub(crate) const DEFAULT_MAX_DEPTH: u32 = 8;

// ── git helpers ──────────────────────────────────────────────────────────────

fn git(args: &[&str], cwd: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to run `git {}`: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn list_kt_files(ref_name: &str, cwd: &Path) -> Result<Vec<String>, String> {
    let out = git(&["ls-tree", "-r", "--name-only", ref_name], cwd)?;
    Ok(out
        .lines()
        .filter(|l| l.ends_with(".kt"))
        .map(str::to_string)
        .collect())
}

fn read_file_at(ref_name: &str, path: &str, cwd: &Path) -> Result<String, String> {
    git(&["show", &format!("{ref_name}:{path}")], cwd)
}

// ── call trees ───────────────────────────────────────────────────────────────

/// A callee-tree node: the callee name plus a best-effort definition location.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallNode {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<CallNode>,
}

/// caller → callees, built once from `(caller, callee)` edge pairs.
pub(crate) fn build_callee_map(edges: &[(String, String)]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (caller, callee) in edges {
        map.entry(caller.clone()).or_default().push(callee.clone());
    }
    map
}

/// Expand `entry` into a callee tree. Cycle handling: `visiting` is a
/// per-path set (removed on backtrack), so a callee may appear under multiple
/// branches (mirrors calldiff `calltree.ts`) but never recurses into itself
/// on the same path. Depth is bounded by [`DEFAULT_MAX_DEPTH`].
pub(crate) fn build_tree(edges: &[(String, String)], entry: &str) -> CallNode {
    let map = build_callee_map(edges);
    fn expand(
        name: &str,
        map: &HashMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
        depth: u32,
    ) -> CallNode {
        let mut node = CallNode {
            name: name.to_string(),
            file: String::new(),
            line: 0,
            children: Vec::new(),
        };
        if depth >= DEFAULT_MAX_DEPTH || !visiting.insert(name.to_string()) {
            return node;
        }
        if let Some(callees) = map.get(name) {
            for callee in callees {
                node.children.push(expand(callee, map, visiting, depth + 1));
            }
        }
        visiting.remove(name);
        node
    }
    expand(entry, &map, &mut HashSet::new(), 0)
}

/// Best-effort definition locations: scan each file for `fun <name>(` lines.
/// Pure function over (file, source) pairs so it is testable.
pub(crate) fn scan_def_locations<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> HashMap<String, (String, u32)> {
    let mut locs: HashMap<String, (String, u32)> = HashMap::new();
    for (file, source) in files {
        for (idx, line) in source.lines().enumerate() {
            let Some(rest) = line.trim_start().strip_prefix("fun ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !rest.starts_with('(') {
                locs.entry(name)
                    .or_insert_with(|| (file.to_string(), u32::try_from(idx + 1).unwrap_or(0)));
            }
        }
    }
    locs
}

/// Fill best-effort definition locations into a built tree.
fn fill_locs(node: &mut CallNode, locs: &HashMap<String, (String, u32)>) {
    if let Some((file, line)) = locs.get(&node.name) {
        node.file.clone_from(file);
        node.line = *line;
    }
    for child in &mut node.children {
        fill_locs(child, locs);
    }
}

// ── structural tree diff (LCS-aligned) ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum DiffStatus {
    Same,
    Added,
    Removed,
}

/// A diff node: one entry of the merged tree, tagged with its status.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DiffNode {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) status: DiffStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<DiffNode>,
}

fn mark_subtree(nodes: &[CallNode], status: DiffStatus) -> Vec<DiffNode> {
    nodes
        .iter()
        .map(|n| DiffNode {
            name: n.name.clone(),
            file: n.file.clone(),
            line: n.line,
            status,
            children: mark_subtree(&n.children, status),
        })
        .collect()
}

/// Structural tree diff keyed by node name. Children are aligned with LCS so
/// output order stays close to the "after" tree (calldiff `diff.ts`).
pub(crate) fn diff_trees(before: &CallNode, after: &CallNode) -> DiffNode {
    diff_node(Some(before), Some(after))
}

fn diff_node(before: Option<&CallNode>, after: Option<&CallNode>) -> DiffNode {
    match (before, after) {
        (Some(b), Some(a)) => {
            let (file, line) = if !a.file.is_empty() {
                (a.file.clone(), a.line)
            } else {
                (b.file.clone(), b.line)
            };
            DiffNode {
                name: a.name.clone(),
                file,
                line,
                status: DiffStatus::Same,
                children: diff_children(&b.children, &a.children),
            }
        }
        (None, Some(a)) => DiffNode {
            name: a.name.clone(),
            file: a.file.clone(),
            line: a.line,
            status: DiffStatus::Added,
            children: mark_subtree(&a.children, DiffStatus::Added),
        },
        (Some(b), None) => DiffNode {
            name: b.name.clone(),
            file: b.file.clone(),
            line: b.line,
            status: DiffStatus::Removed,
            children: mark_subtree(&b.children, DiffStatus::Removed),
        },
        (None, None) => unreachable!("diff_node called with no trees"),
    }
}

fn diff_children(before: &[CallNode], after: &[CallNode]) -> Vec<DiffNode> {
    let n = before.len();
    let m = after.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if before[i].name == after[j].name {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut result = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if before[i].name == after[j].name {
            result.push(diff_node(Some(&before[i]), Some(&after[j])));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            result.push(diff_node(Some(&before[i]), None));
            i += 1;
        } else {
            result.push(diff_node(None, Some(&after[j])));
            j += 1;
        }
    }
    while i < n {
        result.push(diff_node(Some(&before[i]), None));
        i += 1;
    }
    while j < m {
        result.push(diff_node(None, Some(&after[j])));
        j += 1;
    }
    result
}

// ── rendering ────────────────────────────────────────────────────────────────

/// ASCII call-diff tree: `-`/`+`/` ` prefix per node (calldiff-style).
pub(crate) fn render_diff(root: &DiffNode) -> String {
    let mut out = String::new();
    render_node(root, "", true, &mut out);
    out
}

fn render_node(node: &DiffNode, prefix: &str, is_last: bool, out: &mut String) {
    let mark = match node.status {
        DiffStatus::Same => " ",
        DiffStatus::Added => "+",
        DiffStatus::Removed => "-",
    };
    let connector = if is_last { "└─ " } else { "├─ " };
    out.push_str(&format!("{mark} {prefix}{connector}{}\n", node.name));
    let child_prefix = format!("{prefix}{}", if is_last { "   " } else { "│  " });
    let last = node.children.len().saturating_sub(1);
    for (i, child) in node.children.iter().enumerate() {
        render_node(child, &child_prefix, i == last, out);
    }
}

// ── CLI entry ────────────────────────────────────────────────────────────────

/// Load one ref's entry call tree from the git worktree at `root`.
pub(crate) fn load_tree(ref_name: &str, entry: &str, root: &Path) -> Result<CallNode, String> {
    let files = list_kt_files(ref_name, root)?;
    let mut edges: Vec<(String, String)> = Vec::new();
    let mut loc_sources: Vec<(&str, String)> = Vec::new();
    for f in &files {
        let src = read_file_at(ref_name, f, root)?;
        edges.extend(crate::parser::extract_call_edges(
            &src,
            crate::Language::Kotlin,
        ));
        loc_sources.push((f.as_str(), src));
    }
    let locs = scan_def_locations(loc_sources.iter().map(|(f, s)| (*f, s.as_str())));
    let mut tree = build_tree(&edges, entry);
    fill_locs(&mut tree, &locs);
    Ok(tree)
}

/// `call diff <ref1> <ref2> --entry NAME` — POC: single entry, Kotlin only.
pub(crate) fn run_call_diff(ref1: &str, ref2: &str, entry: &str, json: bool, root: &Path) {
    for r in [ref1, ref2] {
        if let Err(e) = git(&["rev-parse", "--verify", "--quiet", r], root) {
            eprintln!("Invalid git ref '{r}': {e}");
            std::process::exit(1);
        }
    }

    let (before, after) = match (load_tree(ref1, entry, root), load_tree(ref2, entry, root)) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let diff = diff_trees(&before, &after);
    if json {
        let payload = serde_json::json!({
            "ref1": ref1,
            "ref2": ref2,
            "entry": entry,
            "root": diff,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).expect("serialize call diff json")
        );
    } else {
        print!("{}", render_diff(&diff));
    }
}
