//! Workspace snapshot — single JSON export with full symbol metadata + relationships.
//!
//! `kotlin-lsp snapshot [--filter kind=class,fun] [--exclude-relationships]`
//! Uses the full tree-sitter index for rich metadata (return_type, parameters, KDoc).

use std::path::PathBuf;

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
    _json: bool,
) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    // Collect symbols with full metadata from the index
    let mut symbols: Vec<SymbolSnapshot> = Vec::new();
    let mut entry_points: Vec<EntryPoint> = Vec::new();

    for file_entry in index.files.iter() {
        let uri_str = file_entry.key();
        let file_data = file_entry.value();
        let pkg = file_data.package.as_deref().unwrap_or("");
        let file_path = uri_str.replace("file://", "");

        for sym in &file_data.symbols {
            let kind_str = format!("{:?}", sym.kind).to_lowercase();

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
    let mut calls = Vec::new();
    for entry in index.call_edges.iter() {
        for (_caller_file, caller_name) in entry.value().iter() {
            calls.push([caller_name.clone(), entry.key().clone()]);
        }
    }

    let mut extends = Vec::new();
    for entry in index.supertypes_index.iter() {
        for (super_name, _f, _) in entry.value().iter() {
            extends.push([entry.key().clone(), super_name.clone()]);
        }
    }

    let mut overrides = Vec::new();
    for entry in index.override_edges.iter() {
        for (_file, class_name) in entry.value().iter() {
            overrides.push([
                format!("{}.{}", class_name, entry.key()),
                entry.key().clone(),
            ]);
        }
    }

    let mut imports = Vec::new();
    for entry in index.import_edges.iter() {
        for (_file, _local_name) in entry.value().iter() {
            imports.push([_file.clone(), entry.key().clone()]);
        }
    }

    Relationships {
        calls,
        extends,
        overrides,
        imports,
    }
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
mod tests {
    use super::*;

    #[test]
    fn is_entry_point_activity() {
        assert!(is_entry_point("MainActivity", "class", ""));
        assert!(is_entry_point("SettingsActivity", "class", ""));
        assert!(is_entry_point("MainFragment", "class", ""));
    }

    #[test]
    fn is_entry_point_application() {
        assert!(is_entry_point("MyApplication", "class", ""));
    }

    #[test]
    fn is_not_entry_point() {
        assert!(!is_entry_point("UserRepository", "class", ""));
        assert!(!is_entry_point("Application", "function", ""));
        assert!(!is_entry_point("main", "function", ""));
        assert!(!is_entry_point("", "class", ""));
    }

    #[test]
    fn collect_relationships_empty_index() {
        let idx = crate::indexer::Indexer::new();
        let rels = collect_relationships(&idx);
        assert!(rels.calls.is_empty());
        assert!(rels.extends.is_empty());
        assert!(rels.overrides.is_empty());
        assert!(rels.imports.is_empty());
    }

    #[test]
    fn collect_relationships_populated_call_edges() {
        let idx = crate::indexer::Indexer::new();
        idx.call_edges.insert(
            "bar".to_string(),
            vec![("/a.kt".to_string(), "foo".to_string())],
        );
        idx.call_edges.insert(
            "baz".to_string(),
            vec![("/a.kt".to_string(), "foo".to_string())],
        );

        let rels = collect_relationships(&idx);
        assert_eq!(rels.calls.len(), 2);
        assert!(rels.calls.contains(&["foo".to_string(), "bar".to_string()]));
        assert!(rels.calls.contains(&["foo".to_string(), "baz".to_string()]));
    }

    #[test]
    fn collect_relationships_populated_extends() {
        let idx = crate::indexer::Indexer::new();
        idx.supertypes_index.insert(
            "Dog".to_string(),
            vec![(
                "Animal".to_string(),
                "/a.kt".to_string(),
                crate::types::SuperKind::Extends,
            )],
        );

        let rels = collect_relationships(&idx);
        assert_eq!(rels.extends.len(), 1);
        assert!(rels
            .extends
            .contains(&["Dog".to_string(), "Animal".to_string()]));
    }

    #[test]
    fn collect_relationships_populated_overrides() {
        let idx = crate::indexer::Indexer::new();
        idx.override_edges.insert(
            "onCreate".to_string(),
            vec![("/app.kt".to_string(), "MyActivity".to_string())],
        );

        let rels = collect_relationships(&idx);
        assert_eq!(rels.overrides.len(), 1);
        let expect = "MyActivity.onCreate".to_string();
        assert!(rels.overrides[0].contains(&expect));
    }

    #[test]
    fn collect_relationships_populated_imports() {
        let idx = crate::indexer::Indexer::new();
        idx.import_edges.insert(
            "com.lib.Foo".to_string(),
            vec![("/a.kt".to_string(), "Foo".to_string())],
        );

        let rels = collect_relationships(&idx);
        assert_eq!(rels.imports.len(), 1);
        assert!(rels
            .imports
            .contains(&["/a.kt".to_string(), "com.lib.Foo".to_string()]));
    }
}
