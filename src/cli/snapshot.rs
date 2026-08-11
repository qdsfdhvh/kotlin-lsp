//! Workspace snapshot — single JSON export with full symbol metadata + relationships.
//!
//! `kotlin-lsp snapshot [--filter kind=class,fun] [--exclude-relationships]`
//! Uses the full tree-sitter index for rich metadata (return_type, parameters, KDoc).

use std::path::{Path, PathBuf};

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct ProjectInfo {
    root: String,
}

#[derive(serde::Serialize)]
struct SymbolSnapshot {
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fq_name: Option<String>,
    visibility: String,
    file: String,
    line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parameters: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    deprecated: bool,
}

#[derive(serde::Serialize)]
struct EntryPoint {
    kind: String,
    name: String,
    file: String,
}

#[derive(serde::Serialize)]
struct SnapshotOutput {
    project: ProjectInfo,
    modules: Vec<crate::cli::modules::ModuleInfo>,
    symbols: Vec<SymbolSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relationships: Option<Relationships>,
    entry_points: Vec<EntryPoint>,
}

#[derive(serde::Serialize)]
struct Relationships {
    calls: Vec<[String; 2]>,
    extends: Vec<[String; 2]>,
    overrides: Vec<[String; 2]>,
    imports: Vec<[String; 2]>,
}

// ── Entry point ──────────────────────────────────────────────────────────────

pub(crate) async fn run_snapshot(
    _filter_kind: Option<String>,
    exclude_relationships: bool,
    include_libraries: bool,
    limit: Option<usize>,
    _json: bool,
) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    if include_libraries {
        eprintln!(
            "[WARN] tool snapshot --include-libraries: indexing ~/.kotlin-lsp/sources; \
             output may be hundreds of MB. Omit the flag for workspace-only symbols."
        );
    }
    // Default to workspace-only: `no_stdlib` skips the global ~/.kotlin-lsp/sources
    // cache so a one-file project does not emit the whole library cache (issue #242).
    // `--include-libraries` restores the old behaviour deliberately.
    let index = crate::cli::run::build_index(&root, !include_libraries).await;

    // Collect symbols with full metadata from the index
    let mut symbols: Vec<SymbolSnapshot> = Vec::new();
    let mut entry_points: Vec<EntryPoint> = Vec::new();

    for file_entry in index.files.iter() {
        let uri_str = file_entry.key();
        let file_data = file_entry.value();
        let pkg = file_data.package.as_deref().unwrap_or("");
        let file_path = uri_str.replace("file://", "");

        for sym in &file_data.symbols {
            let kind_str = sym.kind_label();

            // Build FQ name
            let fq_name = if pkg.is_empty() {
                None
            } else {
                Some(format!("{}.{}", pkg, sym.name))
            };

            // Detect entry points
            if is_entry_point(&sym.name, &kind_str, pkg) {
                entry_points.push(EntryPoint {
                    kind: kind_str.clone(),
                    name: sym.name.clone(),
                    file: file_path.clone(),
                });
            }

            symbols.push(SymbolSnapshot {
                name: sym.name.clone(),
                kind: kind_str,
                fq_name,
                visibility: format!("{:?}", sym.visibility).to_lowercase(),
                file: file_path.clone(),
                line: sym.selection_range.start.line + 1,
                signature: if sym.detail.is_empty() {
                    None
                } else {
                    Some(sym.detail.clone())
                },
                return_type: sym.return_type.clone(),
                parameters: sym.parameters.clone(),
                doc: sym.documentation.clone(),
                parent: sym.parent_fq_name.clone(),
                deprecated: sym.deprecated,
            });
        }
    }

    // Collect relationships
    let relationships = if exclude_relationships {
        None
    } else {
        Some(collect_relationships(&index))
    };

    // Cap symbols (issue #242: agents need a bound on output size).
    if let Some(n) = limit {
        symbols.truncate(n);
    }

    // Discover modules
    let modules = crate::cli::modules::discover_modules();

    let output = SnapshotOutput {
        project: ProjectInfo {
            root: root.display().to_string(),
        },
        modules,
        symbols,
        relationships,
        entry_points,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("serialize JSON")
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn collect_relationships(index: &crate::indexer::Indexer) -> Relationships {
    // Deduplicate so the same caller/callee pair from repeated parses or
    // multiple occurrences in one file appears exactly once (issue #242).
    // BTreeSet also gives deterministic, sorted output.
    let mut calls = std::collections::BTreeSet::new();
    for entry in index.call_edges.iter() {
        for (caller_file, caller_name) in entry.value().iter() {
            if is_library_path(caller_file) {
                continue;
            }
            calls.insert((caller_name.clone(), entry.key().clone()));
        }
    }

    let mut extends = std::collections::BTreeSet::new();
    for entry in index.supertypes_index.iter() {
        for (super_name, file, _) in entry.value().iter() {
            if is_library_path(file) {
                continue;
            }
            extends.insert((entry.key().clone(), super_name.clone()));
        }
    }

    let mut overrides = std::collections::BTreeSet::new();
    for entry in index.override_edges.iter() {
        for (file, class_name) in entry.value().iter() {
            if is_library_path(file) {
                continue;
            }
            overrides.insert((
                format!("{}.{}", class_name, entry.key()),
                entry.key().clone(),
            ));
        }
    }

    let mut imports = std::collections::BTreeSet::new();
    for entry in index.import_edges.iter() {
        for (file, _local_name) in entry.value().iter() {
            if is_library_path(file) {
                continue;
            }
            imports.insert((file.clone(), entry.key().clone()));
        }
    }

    Relationships {
        calls: calls.into_iter().map(|(a, b)| [a, b]).collect(),
        extends: extends.into_iter().map(|(a, b)| [a, b]).collect(),
        overrides: overrides.into_iter().map(|(a, b)| [a, b]).collect(),
        imports: imports.into_iter().map(|(a, b)| [a, b]).collect(),
    }
}

/// True when the file lives under the global extract-sources cache
/// (`~/.kotlin-lsp/sources`). Such files are only indexed with
/// `--include-libraries` and never contribute workspace relationships (issue #242).
fn is_library_path(path: &str) -> bool {
    #[allow(deprecated)]
    let home = std::env::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let lib_root = home.join(".kotlin-lsp").join("sources");
    // Component-wise comparison (not string prefix): on Windows the caller
    // path may mix `/` and `\` separators, and string starts_with would miss
    // `C:\Users\x/.kotlin-lsp/sources/...` (issue #242 CI failure on windows).
    Path::new(path).starts_with(&lib_root)
}

fn is_entry_point(name: &str, kind: &str, _pkg: &str) -> bool {
    if kind == "class"
        && (name.ends_with("Activity")
            || name.ends_with("Fragment")
            || name.ends_with("Application"))
    {
        return true;
    }
    false
}
// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
