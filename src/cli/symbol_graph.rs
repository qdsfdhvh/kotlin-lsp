//! Symbol graph export — serializes the full relationship graph as JSON.

use std::path::PathBuf;
use std::sync::Arc;

use crate::indexer::Indexer;

pub(crate) async fn run_symbol_graph(json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut call_edges: Vec<serde_json::Value> = Vec::new();
    for entry in index.call_edges.iter() {
        for (caller_file, caller_name) in entry.value().iter() {
            call_edges.push(serde_json::json!({
                "callee": entry.key(),
                "caller": caller_name,
                "caller_file": caller_file,
            }));
        }
    }

    let mut inherit_edges: Vec<serde_json::Value> = Vec::new();
    for entry in index.supertypes_index.iter() {
        for (super_name, _file) in entry.value().iter() {
            inherit_edges.push(serde_json::json!({
                "subtype": entry.key(),
                "supertype": super_name,
            }));
        }
    }

    let mut import_edges: Vec<serde_json::Value> = Vec::new();
    for entry in index.import_edges.iter() {
        for (file, local_name) in entry.value().iter() {
            import_edges.push(serde_json::json!({
                "fqn": entry.key(),
                "file": file,
                "local_name": local_name,
            }));
        }
    }

    let output = serde_json::json!({
        "symbols": index.definitions.iter().map(|e| e.key().clone()).collect::<Vec<_>>(),
        "edges": {
            "calls": call_edges,
            "inheritance": inherit_edges,
            "imports": import_edges,
        },
        "module": crate::cli::modules::discover_modules(),
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        let symbol_count = index.definitions.len();
        let call_count: usize = index.call_edges.iter().map(|e| e.value().len()).sum();
        let inherit_count: usize = index.supertypes_index.iter().map(|e| e.value().len()).sum();
        let import_count: usize = index.import_edges.iter().map(|e| e.value().len()).sum();
        println!("Symbol Graph: {} symbols", symbol_count);
        println!("  calls: {} edges", call_count);
        println!("  inheritance: {} edges", inherit_count);
        println!("  imports: {} edges", import_count);
    }
}
