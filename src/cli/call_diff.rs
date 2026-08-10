//! Call-tree git diff — `call diff [<ref1> [<ref2>]] [<name>]`.
//!
//! Complete implementation mirroring calldiff (`src/{git,infer,calltree,
//! render,cli}.ts`): branch-aware call trees (if/else/try/catch/when as tree
//! nodes), ⇄ cycle markers, automatic entrypoint inference (exported-first,
//! tree-text comparison, fallback), git-diff-style default semantics (no refs
//! → HEAD vs working tree; one ref → that vs working tree; trailing positionals
//! that exist on disk are path filters), and a CTA hint after text-mode output.
//!
//! Built for agentic review: when an agent rewires call flow, this shows the
//! shape of the change instead of a flat line diff.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::cli::call_steps::{extract_functions, CallStep, FuncInfo};

/// Max call-tree depth per ref — keeps expansion bounded on deep graphs.
pub(crate) const DEFAULT_MAX_DEPTH: u32 = 12;

/// Directories skipped when walking the working tree (calldiff `git.ts`).
const SKIP_DIRS: [&str; 8] = [
    "node_modules",
    "dist",
    "build",
    "coverage",
    ".git",
    ".gradle",
    ".idea",
    "out",
];

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

fn is_commit_ref(cwd: &Path, ref_name: &str) -> bool {
    git(
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{ref_name}^{{commit}}"),
        ],
        cwd,
    )
    .is_ok()
}

fn is_path_on_disk(cwd: &Path, value: &str) -> bool {
    cwd.join(value).exists()
}

// ── snapshots ────────────────────────────────────────────────────────────────

/// One side of a diff: a git commit, or the working tree on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Snapshot {
    Commit { ref_name: String },
    Worktree,
}

impl Snapshot {
    fn label(&self) -> String {
        match self {
            Snapshot::Commit { ref_name } => ref_name.clone(),
            Snapshot::Worktree => "working tree".to_string(),
        }
    }
}

/// git-diff style resolution: trailing positionals that are not valid git
/// refs but exist on disk are path filters (calldiff `resolveDiffSnapshotsAndPaths`).
pub(crate) fn resolve_snapshots_and_paths(
    cwd: &Path,
    from: Option<&str>,
    to: Option<&str>,
    paths: &[String],
) -> Result<(Snapshot, Snapshot, Vec<String>), String> {
    match (from, to) {
        (None, None) => Ok((
            Snapshot::Commit {
                ref_name: "HEAD".into(),
            },
            Snapshot::Worktree,
            paths.to_vec(),
        )),
        (Some(from), None) => {
            if is_commit_ref(cwd, from) {
                Ok((
                    Snapshot::Commit {
                        ref_name: from.into(),
                    },
                    Snapshot::Worktree,
                    paths.to_vec(),
                ))
            } else if is_path_on_disk(cwd, from) {
                let mut p = vec![from.to_string()];
                p.extend(paths.iter().cloned());
                Ok((
                    Snapshot::Commit {
                        ref_name: "HEAD".into(),
                    },
                    Snapshot::Worktree,
                    p,
                ))
            } else {
                Err(format!("Unknown git ref: {from}"))
            }
        }
        (Some(from), Some(to)) => {
            if !is_commit_ref(cwd, from) {
                return Err(format!("Unknown git ref: {from}"));
            }
            if is_commit_ref(cwd, to) {
                Ok((
                    Snapshot::Commit {
                        ref_name: from.into(),
                    },
                    Snapshot::Commit {
                        ref_name: to.into(),
                    },
                    paths.to_vec(),
                ))
            } else if is_path_on_disk(cwd, to) {
                let mut p = vec![to.to_string()];
                p.extend(paths.iter().cloned());
                Ok((
                    Snapshot::Commit {
                        ref_name: from.into(),
                    },
                    Snapshot::Worktree,
                    p,
                ))
            } else {
                Err(format!("Unknown git ref: {to}"))
            }
        }
        (None, Some(_)) => Err("call diff: a second ref without a first is not supported".into()),
    }
}

/// Path filter: file equals the filter, is under it, or ends with it.
fn path_allowed(file: &str, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|filter| {
        let f = filter.trim_start_matches("./").trim_end_matches('/');
        file == f || file.starts_with(&format!("{f}/")) || file.ends_with(f)
    })
}

/// List Kotlin source files under a snapshot, sorted, honoring path filters.
pub(crate) fn list_source_files(
    cwd: &Path,
    snapshot: &Snapshot,
    filters: &[String],
) -> Result<Vec<String>, String> {
    let files = match snapshot {
        Snapshot::Worktree => walk_worktree(cwd),
        Snapshot::Commit { ref_name } => {
            let out = git(&["ls-tree", "-r", "--name-only", ref_name], cwd)?;
            out.lines()
                .filter(|l| l.ends_with(".kt"))
                .map(str::to_string)
                .collect()
        }
    };
    Ok(files
        .into_iter()
        .filter(|f| path_allowed(f, filters))
        .collect())
}

fn walk_worktree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && name != "." {
                continue;
            }
            let full = entry.path();
            if full.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                walk(&full, root, out);
            } else if name.ends_with(".kt") {
                if let Ok(rel) = full.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn read_snapshot_file(cwd: &Path, snapshot: &Snapshot, file: &str) -> Option<String> {
    match snapshot {
        Snapshot::Worktree => std::fs::read_to_string(cwd.join(file)).ok(),
        Snapshot::Commit { ref_name } => git(&["show", &format!("{ref_name}:{file}")], cwd).ok(),
    }
}

// ── call trees ───────────────────────────────────────────────────────────────

/// Node identity for diffing / rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum NodeKind {
    Call,
    Branch,
}

/// A call-tree node: a callee (or branch) plus a best-effort definition
/// location. `kind: Branch` renders without the continuing rail (alternate
/// paths, calldiff render.ts).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallNode {
    pub(crate) name: String,
    pub(crate) kind: NodeKind,
    pub(crate) file: String,
    pub(crate) line: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) children: Vec<CallNode>,
}

fn empty_node(name: &str, kind: NodeKind) -> CallNode {
    CallNode {
        name: name.to_string(),
        kind,
        file: String::new(),
        line: 0,
        children: Vec::new(),
    }
}

/// Resolve a user-supplied entry name to a function key (calldiff
/// `resolveEntry`): exact key, `name()`→`name`, `Class.method`, or `new X`.
fn resolve_entry(entry: &str, functions: &HashMap<String, FuncInfo>) -> Option<String> {
    if functions.contains_key(entry) {
        return Some(entry.to_string());
    }
    let stripped = entry.strip_suffix("()").unwrap_or(entry).to_string();
    if functions.contains_key(&stripped) {
        return Some(stripped);
    }
    let matches: Vec<&String> = functions
        .keys()
        .filter(|key| {
            key.as_str() == entry
                || key.ends_with(&format!(".{entry}"))
                || key.as_str() == format!("new {entry}")
        })
        .collect();
    match matches.len() {
        0 => None,
        1 => Some(matches[0].clone()),
        _ => {
            let exported: Vec<&String> = matches
                .iter()
                .copied()
                .filter(|k| functions.get(*k).map(|f| f.exported).unwrap_or(false))
                .collect();
            if exported.len() == 1 {
                Some(exported[0].clone())
            } else {
                let mut sorted = matches.clone();
                sorted.sort();
                Some(sorted[0].clone())
            }
        }
    }
}

/// Expand a function into a branch-aware call tree, mirroring calldiff
/// `calltree.ts`: branch steps become Branch nodes whose children are the
/// branch-body steps; a visiting-set hit appends `⇄` to the label instead of
/// recursing (cycles are leaves, never infinite).
pub(crate) fn build_tree(
    functions: &HashMap<String, FuncInfo>,
    entry: &str,
    max_depth: u32,
) -> CallNode {
    let resolved = resolve_entry(entry, functions).unwrap_or_else(|| entry.to_string());
    expand_call(&resolved, functions, max_depth, 0, &mut HashSet::new())
}

fn expand_call(
    key: &str,
    functions: &HashMap<String, FuncInfo>,
    max_depth: u32,
    depth: u32,
    visiting: &mut HashSet<String>,
) -> CallNode {
    let label = functions
        .get(key)
        .map(|f| f.label.clone())
        .unwrap_or_else(|| format!("{key}()"));
    let mut node = empty_node(&label, NodeKind::Call);

    if let Some(loc) = functions.get(key) {
        node.file = String::new(); // filled later from definition scan
        node.line = loc.line;
    }

    if depth >= max_depth {
        return node;
    }
    let Some(info) = functions.get(key) else {
        return node;
    };
    if visiting.contains(key) {
        node.name.push_str(" ⇄");
        return node;
    }

    visiting.insert(key.to_string());
    for step in &info.steps {
        match step {
            CallStep::Call { key: callee, line } => {
                let mut child = expand_call(callee, functions, max_depth, depth + 1, visiting);
                if child.line == 0 {
                    child.line = *line;
                }
                node.children.push(child);
            }
            CallStep::Branch {
                label,
                line,
                children,
            } => {
                let mut branch = empty_node(label, NodeKind::Branch);
                branch.line = *line;
                branch.children = expand_steps(children, functions, max_depth, depth + 1, visiting);
                node.children.push(branch);
            }
        }
    }
    visiting.remove(key);
    node
}

fn expand_steps(
    steps: &[CallStep],
    functions: &HashMap<String, FuncInfo>,
    max_depth: u32,
    depth: u32,
    visiting: &mut HashSet<String>,
) -> Vec<CallNode> {
    steps
        .iter()
        .map(|step| match step {
            CallStep::Call { key, line } => {
                let mut child = expand_call(key, functions, max_depth, depth, visiting);
                if child.line == 0 {
                    child.line = *line;
                }
                child
            }
            CallStep::Branch {
                label,
                line,
                children,
            } => {
                let mut branch = empty_node(label, NodeKind::Branch);
                branch.line = *line;
                branch.children = expand_steps(children, functions, max_depth, depth, visiting);
                branch
            }
        })
        .collect()
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

// ── entry inference ──────────────────────────────────────────────────────────

/// Render a call tree to indented text (key per line) for change detection.
fn tree_text(functions: &HashMap<String, FuncInfo>, entry: &str, max_depth: u32) -> String {
    let tree = build_tree(functions, entry, max_depth);
    let mut parts: Vec<String> = Vec::new();
    fn walk(node: &CallNode, depth: u32, parts: &mut Vec<String>) {
        parts.push(format!("{}{}", "  ".repeat(depth as usize), node.name));
        for child in &node.children {
            walk(child, depth + 1, parts);
        }
    }
    walk(&tree, 0, &mut parts);
    parts.join("\n")
}

/// Infer entrypoints: exported functions whose expanded call trees differ,
/// plus any explicitly requested entries (calldiff `inferEntries`).
pub(crate) fn infer_entries(
    before: &HashMap<String, FuncInfo>,
    after: &HashMap<String, FuncInfo>,
    explicit: &[String],
    max_depth: u32,
) -> Result<Vec<String>, String> {
    if !explicit.is_empty() {
        let mut resolved: Vec<String> = Vec::new();
        for entry in explicit {
            let from_before = resolve_entry(entry, before);
            let from_after = resolve_entry(entry, after);
            let key = from_after
                .or(from_before)
                .ok_or_else(|| format!("Entrypoint not found: {entry}"))?;
            if !resolved.contains(&key) {
                resolved.push(key);
            }
        }
        return Ok(resolved);
    }

    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();

    let differs = |key: &str| -> bool {
        let b = before.get(key);
        let a = after.get(key);
        let before_text = b
            .map(|_| tree_text(before, key, max_depth))
            .unwrap_or_default();
        let after_text = a
            .map(|_| tree_text(after, key, max_depth))
            .unwrap_or_default();
        before_text != after_text
    };

    let mut candidates: Vec<String> = Vec::new();
    for key in &keys {
        if key.starts_with("new ") {
            continue;
        }
        let exported = before.get(*key).map(|f| f.exported).unwrap_or(false)
            || after.get(*key).map(|f| f.exported).unwrap_or(false);
        if exported && differs(key) {
            candidates.push((*key).clone());
        }
    }

    if candidates.is_empty() {
        for key in &keys {
            if key.starts_with("new ") {
                continue;
            }
            if differs(key) {
                candidates.push((*key).clone());
            }
        }
    }

    candidates.sort();
    Ok(candidates)
}

/// Diff one entry between two function indexes. Returns None when unchanged.
pub(crate) fn diff_entry(
    entry: &str,
    before: &HashMap<String, FuncInfo>,
    after: &HashMap<String, FuncInfo>,
    max_depth: u32,
) -> Option<DiffNode> {
    let before_key = resolve_entry(entry, before).unwrap_or_else(|| entry.to_string());
    let after_key = resolve_entry(entry, after).unwrap_or_else(|| entry.to_string());

    let has_before = before.contains_key(&before_key);
    let has_after = after.contains_key(&after_key);
    if !has_before && !has_after {
        return None;
    }

    let before_tree = if has_before {
        build_tree(before, &before_key, max_depth)
    } else {
        empty_node(
            after
                .get(&after_key)
                .map(|f| f.label.as_str())
                .unwrap_or(after_key.as_str()),
            NodeKind::Call,
        )
    };
    let after_tree = if has_after {
        build_tree(after, &after_key, max_depth)
    } else {
        empty_node(
            before
                .get(&before_key)
                .map(|f| f.label.as_str())
                .unwrap_or(before_key.as_str()),
            NodeKind::Call,
        )
    };

    let diff = if !has_before && has_after {
        let mut d = diff_trees(&empty_node(&before_tree.name, NodeKind::Call), &after_tree);
        d.status = DiffStatus::Added;
        d
    } else if has_before && !has_after {
        let mut d = diff_trees(&before_tree, &empty_node(&after_tree.name, NodeKind::Call));
        d.status = DiffStatus::Removed;
        d
    } else {
        diff_trees(&before_tree, &after_tree)
    };

    if !tree_has_changes(&diff) {
        return None;
    }
    Some(diff)
}

fn tree_has_changes(node: &DiffNode) -> bool {
    if node.status != DiffStatus::Same {
        return true;
    }
    node.children.iter().any(tree_has_changes)
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
    pub(crate) kind: NodeKind,
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
            kind: n.kind,
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
                kind: a.kind,
                file,
                line,
                status: DiffStatus::Same,
                children: diff_children(&b.children, &a.children),
            }
        }
        (None, Some(a)) => DiffNode {
            name: a.name.clone(),
            kind: a.kind,
            file: a.file.clone(),
            line: a.line,
            status: DiffStatus::Added,
            children: mark_subtree(&a.children, DiffStatus::Added),
        },
        (Some(b), None) => DiffNode {
            name: b.name.clone(),
            kind: b.kind,
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
/// Branch children omit the continuing `│` rail — they are alternate paths,
/// not a nested stack that continues past the branch.
pub(crate) fn render_diff(root: &DiffNode) -> String {
    let mut out = String::new();
    render_node(root, "", true, true, &mut out);
    out
}

fn render_node(node: &DiffNode, prefix: &str, is_last: bool, is_root: bool, out: &mut String) {
    let mark = match node.status {
        DiffStatus::Same => " ",
        DiffStatus::Added => "+",
        DiffStatus::Removed => "-",
    };
    let connector = if is_root {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };
    out.push_str(&format!("{mark} {prefix}{connector}{}\n", node.name));
    let rail = if node.kind == NodeKind::Branch || is_root || is_last {
        "   "
    } else {
        "│  "
    };
    let child_prefix = format!("{prefix}{rail}");
    let last = node.children.len().saturating_sub(1);
    for (i, child) in node.children.iter().enumerate() {
        render_node(child, &child_prefix, i == last, false, out);
    }
}

// ── snapshot loading ─────────────────────────────────────────────────────────

/// Load one snapshot's function index (key → FuncInfo).
pub(crate) fn load_index(
    cwd: &Path,
    snapshot: &Snapshot,
    filters: &[String],
) -> Result<HashMap<String, FuncInfo>, String> {
    let files = list_source_files(cwd, snapshot, filters)?;
    let mut functions: HashMap<String, FuncInfo> = HashMap::new();
    let mut loc_sources: Vec<(String, String)> = Vec::new();
    for file in &files {
        let Some(source) = read_snapshot_file(cwd, snapshot, file) else {
            continue;
        };
        for info in extract_functions(&source) {
            let mut info = info;
            info.file = file.clone();
            loc_sources.push((file.clone(), source.clone()));
            functions.entry(info.key.clone()).or_insert(info);
        }
    }
    // Fill best-effort definition locations into every function.
    let locs = scan_def_locations(loc_sources.iter().map(|(f, s)| (f.as_str(), s.as_str())));
    for info in functions.values_mut() {
        if info.file.is_empty() {
            if let Some((file, line)) = locs.get(&info.key) {
                info.file.clone_from(file);
                info.line = *line;
            }
        }
    }
    Ok(functions)
}

/// `call diff [<ref1> [<ref2>]] [<name>]` — git-diff-style semantics.
pub(crate) fn run_call_diff(
    positionals: &[String],
    explicit_entry: Option<&str>,
    max_depth: u32,
    json: bool,
    root: &Path,
) {
    if let Err(e) = git(&["rev-parse", "--is-inside-work-tree"], root) {
        eprintln!("Not a git repository at {}: {e}", root.display());
        std::process::exit(1);
    }

    // Positionals: leading valid git refs (up to 2), then an optional entry
    // name (a non-ref positional that is not on disk), then trailing path
    // filters. git-diff defaults are applied inside resolve_snapshots_and_paths.
    let mut refs: Vec<&str> = Vec::new();
    let mut entry: Option<&str> = explicit_entry;
    let mut tail: Vec<String> = Vec::new();
    for p in positionals {
        if refs.len() < 2 && is_commit_ref(root, p) {
            refs.push(p);
        } else if entry.is_none() && !is_path_on_disk(root, p) {
            entry = Some(p);
        } else {
            tail.push(p.clone());
        }
    }

    let (from, to, paths) =
        match resolve_snapshots_and_paths(root, refs.first().copied(), refs.get(1).copied(), &tail)
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };

    let before = match load_index(root, &from, &paths) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("failed to load {}: {e}", from.label());
            std::process::exit(1);
        }
    };
    let after = match load_index(root, &to, &paths) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("failed to load {}: {e}", to.label());
            std::process::exit(1);
        }
    };

    let explicit: Vec<String> = entry.map(|e| vec![e.to_string()]).unwrap_or_default();
    let entries = match infer_entries(&before, &after, &explicit, max_depth) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if entries.is_empty() {
        let message = format!(
            "No callstack changes between {} and {}.",
            from.label(),
            to.label()
        );
        if json {
            let payload = serde_json::json!({
                "from": from.label(),
                "to": to.label(),
                "trees": [],
                "message": message,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).expect("serialize call diff json")
            );
        } else {
            println!("{message}");
        }
        return;
    }

    let mut trees: Vec<(String, DiffNode)> = Vec::new();
    let mut ascii_parts: Vec<String> = vec![
        format!("call diff {} → {}", from.label(), to.label()),
        String::new(),
    ];
    for entry in &entries {
        let Some(diff) = diff_entry(entry, &before, &after, max_depth) else {
            continue;
        };
        let ascii = render_diff(&diff);
        ascii_parts.push(ascii.clone());
        ascii_parts.push(String::new());
        trees.push((entry.clone(), diff));
    }

    if json {
        let payload = serde_json::json!({
            "from": from.label(),
            "to": to.label(),
            "trees": trees.iter().map(|(entry, root)| serde_json::json!({
                "entry": entry,
                "root": root,
            })).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).expect("serialize call diff json")
        );
    } else {
        for part in &ascii_parts {
            if !part.is_empty() {
                println!("{part}");
            }
        }
        // CTA: suggest the focused per-entry follow-up for the first entries.
        for (entry, _) in trees.iter().take(2) {
            println!("hint: call diff --entry {entry} for a focused view");
        }
    }
}

// ── test helpers (shared with call_diff_tests) ───────────────────────────────

#[cfg(test)]
pub(crate) fn test_node(name: &str, kind: NodeKind, children: Vec<CallNode>) -> CallNode {
    CallNode {
        name: name.to_string(),
        kind,
        file: String::new(),
        line: 0,
        children,
    }
}
