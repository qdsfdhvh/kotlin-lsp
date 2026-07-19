//! Batch query CLI — `kotlin-lsp query` accepts a JSON array of query specs
//! via stdin and returns results in order. Loads the index only once.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::indexer::Indexer;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum QuerySpec {
    #[serde(rename = "definition")]
    Definition { name: String },
    #[serde(rename = "references")]
    References {
        name: String,
        #[serde(rename = "refKind")]
        ref_kind: Option<String>,
    },
    #[serde(rename = "hover")]
    Hover { file: String, line: u32, col: u32 },
    #[serde(rename = "summarize")]
    Summarize { name: String },
    #[serde(rename = "callers")]
    Callers {
        file: String,
        line: u32,
        col: u32,
        depth: Option<u32>,
    },
    #[serde(rename = "implementations")]
    Implementations { name: String },
    #[serde(rename = "subclasses")]
    Subclasses { name: String },
}

#[derive(Debug, Serialize)]
struct QueryResult {
    #[serde(rename = "type")]
    query_type: String,
    #[serde(flatten)]
    data: serde_json::Value,
}

pub(crate) async fn run_query(json: bool) {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).expect("failed to read stdin");

    let specs: Vec<QuerySpec> = serde_json::from_str(&input).unwrap_or_else(|e| {
        eprintln!("Invalid query JSON: {e}");
        std::process::exit(1);
    });

    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut results: Vec<QueryResult> = Vec::new();

    for spec in &specs {
        let result = execute_query(spec, &index, &root);
        results.push(result);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&results).expect("serialize JSON")
        );
    } else {
        for r in &results {
            println!(
                "[{}] {}",
                r.query_type,
                serde_json::to_string(&r.data).unwrap_or_default()
            );
        }
    }
}

fn execute_query(spec: &QuerySpec, index: &Arc<Indexer>, _root: &std::path::Path) -> QueryResult {
    match spec {
        QuerySpec::Definition { name } => {
            let locs = index.definition_locations(name);
            let results: Vec<serde_json::Value> = locs
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.uri.to_file_path().map(|p| p.display().to_string()).unwrap_or_default(),
                        "line": loc.range.start.line + 1,
                        "col": loc.range.start.character + 1,
                    })
                })
                .collect();
            QueryResult {
                query_type: "definition".to_string(),
                data: serde_json::json!({ "results": results }),
            }
        }
        QuerySpec::References { name, ref_kind } => {
            let _filters = crate::cli::args::ResultFilters {
                ref_kind: ref_kind.clone(),
                ..Default::default()
            };
            let locs = index.definition_locations(name);
            let results: Vec<serde_json::Value> = locs
                .iter()
                .map(|loc| {
                    serde_json::json!({
                        "file": loc.uri.to_file_path().map(|p| p.display().to_string()).unwrap_or_default(),
                        "line": loc.range.start.line + 1,
                        "col": loc.range.start.character + 1,
                    })
                })
                .collect();
            QueryResult {
                query_type: "references".to_string(),
                data: serde_json::json!({
                    "results": results,
                    "filter_applied": ref_kind,
                }),
            }
        }
        QuerySpec::Hover { file, line, col } => {
            let path = std::path::Path::new(file);
            let _uri = tower_lsp::lsp_types::Url::from_file_path(path)
                .unwrap_or_else(|_| tower_lsp::lsp_types::Url::parse("file:///").unwrap());
            // Simple word extraction: re-read the file
            let word = std::fs::read_to_string(path)
                .ok()
                .and_then(|src| {
                    let lines: Vec<&str> = src.lines().collect();
                    let line_idx = (*line as usize).saturating_sub(1);
                    lines.get(line_idx).and_then(|l| {
                        let col_idx = (*col as usize).saturating_sub(1);
                        let before = &l[..col_idx.min(l.len())];
                        before
                            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .map(|s| s.to_string())
                    })
                })
                .unwrap_or_default();
            let locs = index.definition_locations(&word);
            let signature = locs.first().map(|loc| {
                let uri_str = loc.uri.to_string();
                index
                    .files
                    .get(&uri_str)
                    .and_then(|f| {
                        f.symbols
                            .iter()
                            .find(|s| s.name == word)
                            .map(|s| s.detail.clone())
                    })
                    .unwrap_or_default()
            });
            QueryResult {
                query_type: "hover".to_string(),
                data: serde_json::json!({
                    "name": word,
                    "signature": signature,
                }),
            }
        }
        QuerySpec::Summarize { name } => {
            let locs = index.definition_locations(name);
            let summary: serde_json::Value = if let Some(loc) = locs.first() {
                let uri_str = loc.uri.to_string();
                if let Some(file_ref) = index.files.get(&uri_str) {
                    let sym = file_ref.symbols.iter().find(|s| s.name == *name);
                    if let Some(sym) = sym {
                        serde_json::json!({
                            "name": sym.name,
                            "kind": format!("{:?}", sym.kind).to_lowercase(),
                            "visibility": format!("{:?}", sym.visibility).to_lowercase(),
                            "signature": sym.detail,
                            "deprecated": sym.deprecated,
                        })
                    } else {
                        serde_json::json!({ "error": "symbol not found in index" })
                    }
                } else {
                    serde_json::json!({ "error": "file not indexed" })
                }
            } else {
                serde_json::json!({ "error": "symbol not found" })
            };
            QueryResult {
                query_type: "summarize".to_string(),
                data: summary,
            }
        }
        QuerySpec::Callers {
            file,
            line,
            col,
            depth,
        } => {
            let path = std::path::Path::new(file);
            let _uri = tower_lsp::lsp_types::Url::from_file_path(path)
                .unwrap_or_else(|_| tower_lsp::lsp_types::Url::parse("file:///").unwrap());
            let word = std::fs::read_to_string(path)
                .ok()
                .and_then(|src| {
                    let lines: Vec<&str> = src.lines().collect();
                    let line_idx = (*line as usize).saturating_sub(1);
                    lines.get(line_idx).and_then(|l| {
                        let col_idx = (*col as usize).saturating_sub(1);
                        let before = &l[..col_idx.min(l.len())];
                        before
                            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .map(|s| s.to_string())
                    })
                })
                .unwrap_or_default();
            let depth = depth.unwrap_or(1);
            let callers: Vec<serde_json::Value> = index
                .call_edges
                .get(&word)
                .map(|entries| {
                    entries
                        .iter()
                        .take(20)
                        .map(|(file, name)| serde_json::json!({ "name": name, "file": file }))
                        .collect()
                })
                .unwrap_or_default();
            QueryResult {
                query_type: "callers".to_string(),
                data: serde_json::json!({
                    "name": word,
                    "callers": callers,
                    "depth": depth,
                }),
            }
        }
        QuerySpec::Implementations { name } => {
            let results: Vec<serde_json::Value> =
                if let Some(locs) = index.subtypes.get(name.as_str()) {
                    locs.value()
                        .iter()
                        .take(50)
                        .map(|loc| {
                            serde_json::json!({
                                "file": loc.uri.to_string(),
                                "line": loc.range.start.line + 1,
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            QueryResult {
                query_type: "implementations".into(),
                data: serde_json::json!({ "name": name, "results": results }),
            }
        }
        QuerySpec::Subclasses { name } => {
            let results: Vec<serde_json::Value> =
                if let Some(locs) = index.subtypes.get(name.as_str()) {
                    locs.value()
                        .iter()
                        .take(50)
                        .map(|loc| {
                            serde_json::json!({
                                "file": loc.uri.to_string(),
                                "line": loc.range.start.line + 1,
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
            QueryResult {
                query_type: "subclasses".into(),
                data: serde_json::json!({ "name": name, "results": results }),
            }
        }
    }
}
