//! Call path enumeration — `call reach <entry> [--to <target>]`.
//!
//! Lists every call path from an entrypoint function to a target function
//! (or every reachable path when `--to` is omitted), mirroring `calldiff reach`
//! for agentic review: when an agent rewires call flow, reach shows the full
//! set of paths a call can take through the workspace call graph.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::Location;

use crate::indexer::Indexer;

// ── Constants ────────────────────────────────────────────────────────────────

/// Default recursion depth limit — keeps enumeration bounded on deep graphs.
pub(crate) const DEFAULT_MAX_DEPTH: u32 = 8;

/// Hard cap on the number of paths returned. Path enumeration is exponential
/// in the worst case, so we truncate loudly instead of hanging.
pub(crate) const MAX_PATHS: usize = 1000;

// ── Graph construction (pure, testable) ──────────────────────────────────────

/// caller_name → Vec<(caller_file, callee_name)>.
///
/// `Indexer::call_edges` is keyed by *callee* (callee → callers); reach walks
/// forward, so we reverse the map once up front instead of scanning the whole
/// edge table on every expansion step.
pub(crate) fn build_callee_map(index: &Indexer) -> HashMap<String, Vec<(String, String)>> {
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for entry in index.call_edges.iter() {
        let callee = entry.key();
        for (caller_file, caller_name) in entry.value().iter() {
            map.entry(caller_name.clone())
                .or_default()
                .push((caller_file.clone(), callee.clone()));
        }
    }
    map
}

/// Language of a source file, from its extension. Used to keep call paths
/// within one language (issue #259): `call_edges` keys callees by bare name,
/// so same-named functions in different languages share a key and paths would
/// otherwise cross the Kotlin/Java/Swift boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Lang {
    Kotlin,
    Java,
    Swift,
}

fn lang_of_file(path: &str) -> Option<Lang> {
    if path.ends_with(".kt") {
        Some(Lang::Kotlin)
    } else if path.ends_with(".java") {
        Some(Lang::Java)
    } else if path.ends_with(".swift") {
        Some(Lang::Swift)
    } else {
        None
    }
}

// ── Path enumeration ─────────────────────────────────────────────────────────

/// All paths from `entry` to `target` (or all reachable paths when `target` is
/// None), depth-bounded. Each path is a list of `(file, name)` where the first
/// element is the entry itself (file empty — its definition location is looked
/// up separately) and each later element is a callee reached from the previous
/// node. Returns `(paths, truncated)`.
///
/// Cycle handling: `visited` is per-path and removed on backtrack, so a node
/// may appear on *different* paths but never twice on the *same* path.
pub(crate) fn enumerate_paths(
    callee_map: &HashMap<String, Vec<(String, String)>>,
    entry: &str,
    target: Option<&str>,
    max_depth: u32,
    max_paths: usize,
) -> (Vec<Vec<(String, String)>>, bool) {
    let mut paths: Vec<Vec<(String, String)>> = Vec::new();
    let mut truncated = false;

    let Some(_first_callees) = callee_map.get(entry) else {
        // Entry has no callees at all — no paths (or, with a target, the
        // target is trivially unreachable).
        return (paths, truncated);
    };

    // Language of the entry: from any of its outgoing call edges' caller file.
    let entry_lang = callee_map[entry]
        .iter()
        .find_map(|(file, _)| lang_of_file(file));

    let mut path = vec![(String::new(), entry.to_string())];
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(entry.to_string());

    dfs(
        callee_map,
        entry,
        entry_lang,
        &mut path,
        &mut visited,
        target,
        0,
        max_depth,
        max_paths,
        &mut paths,
        &mut truncated,
    );
    (paths, truncated)
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    callee_map: &HashMap<String, Vec<(String, String)>>,
    current: &str,
    lang: Option<Lang>,
    path: &mut Vec<(String, String)>,
    visited: &mut HashSet<String>,
    target: Option<&str>,
    depth: u32,
    max_depth: u32,
    max_paths: usize,
    paths: &mut Vec<Vec<(String, String)>>,
    truncated: &mut bool,
) {
    if paths.len() >= max_paths {
        *truncated = true;
        return;
    }

    // With a target: the moment we reach it, record the path and stop this
    // branch — the target is a terminal, not a transit node.
    if let Some(target) = target {
        if current == target {
            paths.push(path.clone());
            return;
        }
    }

    if depth >= max_depth {
        // Depth-bounded leaves count as paths when enumerating everything.
        if target.is_none() {
            paths.push(path.clone());
        }
        return;
    }

    let Some(callees) = callee_map.get(current) else {
        // True leaf (no callees): a complete path when enumerating everything.
        if target.is_none() {
            paths.push(path.clone());
        }
        return;
    };

    // Issue #259 + #266: filter by language only when the current node has
    // same-named edges in multiple languages (a Kotlin `sharedName` and a
    // Swift `sharedName` joining into one node). A single-language node must
    // NOT be filtered — a Kotlin entry calling a Java static method (Java-only
    // edges) is a legal Kotlin→Java path and must survive.
    let edge_langs: std::collections::HashSet<Lang> = callees
        .iter()
        .filter_map(|(f, _)| lang_of_file(f))
        .collect();
    let ambiguous = edge_langs.len() > 1;

    let mut expanded = false;
    for (file, callee) in callees {
        if ambiguous {
            if let Some(lang) = lang {
                if lang_of_file(file) != Some(lang) {
                    continue;
                }
            }
        }
        if visited.contains(callee) {
            continue;
        }
        expanded = true;
        visited.insert(callee.clone());
        path.push((file.clone(), callee.clone()));
        dfs(
            callee_map,
            callee,
            lang,
            path,
            visited,
            target,
            depth + 1,
            max_depth,
            max_paths,
            paths,
            truncated,
        );
        path.pop();
        visited.remove(callee);
    }
    // Every callee was already visited (cycle back-edge): the node is a leaf
    // within this path — record it when enumerating everything.
    if !expanded && target.is_none() {
        paths.push(path.clone());
    }
}

// ── CLI entry ────────────────────────────────────────────────────────────────

/// Best-effort definition location for `name` (first definition wins).
fn node_location(
    definitions: &dashmap::DashMap<String, Vec<Location>>,
    name: &str,
) -> Option<(String, u32)> {
    let guard = definitions.get(name)?;
    let loc = guard.first()?;
    Some((loc.uri.to_string(), loc.range.start.line.saturating_add(1)))
}

pub(crate) async fn run_reach(
    root: std::path::PathBuf,
    entry: &str,
    target: Option<&str>,
    max_depth: u32,
    json: bool,
    no_stdlib: bool,
) {
    let index = crate::cli::run::build_index(&root, no_stdlib).await;
    let callee_map = build_callee_map(&index);

    if !callee_map.contains_key(entry) {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "entry": entry,
                    "error": format!("No symbol '{entry}' in call graph"),
                }))
                .expect("serialize reach error")
            );
        } else {
            eprintln!("No symbol '{entry}' found in call graph");
        }
        std::process::exit(1);
    }

    let (paths, truncated) = enumerate_paths(&callee_map, entry, target, max_depth, MAX_PATHS);

    if json {
        let out_paths: Vec<serde_json::Value> = paths
            .iter()
            .map(|p| {
                serde_json::json!({
                    "nodes": p.iter().map(|(file, name)| {
                        let (loc_file, line) = node_location(&index.definitions, name)
                            .unwrap_or_else(|| (file.clone(), 0));
                        serde_json::json!({"name": name, "file": loc_file, "line": line})
                    }).collect::<Vec<_>>()
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "entry": entry,
                "target": target,
                "truncated": truncated,
                "paths": out_paths,
            }))
            .expect("serialize reach json")
        );
    } else {
        if paths.is_empty() {
            if let Some(target) = target {
                println!("No call path from '{entry}' to '{target}'");
            } else {
                println!("No call paths from '{entry}'");
            }
        } else {
            for p in &paths {
                let chain: Vec<&str> = p.iter().map(|(_, name)| name.as_str()).collect();
                println!("{}", chain.join(" -> "));
            }
        }
        if truncated {
            eprintln!("[WARN] path list truncated at {MAX_PATHS} paths");
        }
    }
}
