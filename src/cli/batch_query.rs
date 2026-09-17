//! Batch query CLI — `kotlin-lsp tool query` accepts a JSON array of query specs
//! via stdin and returns results in order. Loads the index only once.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::query_engine::{IndexQueryEngine, QueryEngine};
use crate::indexer::Indexer;
use crate::query::references::sort_locations;
use crate::StrExt;

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

pub(crate) async fn run_query(json: bool, explicit_root: Option<&Path>, no_stdlib: bool) {
    let mut input = String::new();
    if let Err(error) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input) {
        eprintln!("Failed to read query stdin: {error}");
        std::process::exit(1);
    }
    let specs: Vec<serde_json::Value> = serde_json::from_str(&input).unwrap_or_else(|e| {
        eprintln!("Invalid query JSON: {e}");
        std::process::exit(1);
    });
    let root = crate::cli::run::resolve_root(explicit_root);
    let root = root.canonicalize().unwrap_or_else(|error| {
        eprintln!("Invalid query root {}: {error}", root.display());
        std::process::exit(1);
    });
    // Without --root, retain ordinary cwd-relative file operands even when
    // workspace discovery finds a .git ancestor.
    let file_base = explicit_root.unwrap_or_else(|| Path::new("."));
    let index = crate::cli::run::build_index(&root, no_stdlib).await;
    let engine = IndexQueryEngine::new(Arc::clone(&index));
    let mut results = Vec::with_capacity(specs.len());
    for value in specs {
        let query_type = value
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let result = serde_json::from_value::<QuerySpec>(value)
            .map_err(|error| error.to_string())
            .and_then(|spec| execute_query(&spec, &index, &engine, file_base))
            .unwrap_or_else(|error| QueryResult {
                query_type,
                data: serde_json::json!({ "error": error }),
            });
        results.push(result);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string(&results).expect("serialize JSON")
        );
    } else {
        for r in &results {
            println!("[{}] {}", r.query_type, r.data);
        }
    }
    if results
        .iter()
        .any(|result| result.data.get("error").is_some())
    {
        std::process::exit(1);
    }
}

/// Resolve file operands against the batch root and validate human UTF-16 positions.
fn word_at(root: &Path, file: &str, line: u32, col: u32) -> Result<(PathBuf, String), String> {
    if line == 0 || col == 0 {
        return Err("line and col must be 1-based".into());
    }
    let path = root
        .join(file)
        .canonicalize()
        .map_err(|error| format!("{file}: {error}"))?;
    let source = std::fs::read_to_string(&path).map_err(|error| format!("{file}: {error}"))?;
    let text = source
        .lines()
        .nth(line as usize - 1)
        .ok_or("line is outside the file")?;
    if col as usize > text.encode_utf16().count() + 1 {
        return Err("col is outside the line".into());
    }
    let word = text.word_at_utf16_col(col as usize - 1);
    if word.is_empty() {
        return Err("no identifier at position".into());
    }
    Ok((path, word))
}

fn location_results(mut locations: Vec<tower_lsp::lsp_types::Location>) -> Vec<serde_json::Value> {
    sort_locations(&mut locations);
    locations
        .iter()
        .map(|loc| {
            serde_json::json!({
                "file": loc.uri.to_file_path().map(|p| p.display().to_string()).unwrap_or_default(),
                "line": loc.range.start.line + 1,
                "col": loc.range.start.character + 1,
            })
        })
        .collect()
}

fn execute_query(
    spec: &QuerySpec,
    index: &Arc<Indexer>,
    engine: &IndexQueryEngine,
    root: &Path,
) -> Result<QueryResult, String> {
    Ok(match spec {
        QuerySpec::Definition { name } => {
            let results = location_results(engine.definitions(name));
            QueryResult {
                query_type: "definition".to_string(),
                data: serde_json::json!({ "results": results }),
            }
        }
        QuerySpec::References { name, ref_kind } => {
            let results = location_results(engine.references_with_kind(name, ref_kind.as_deref())?);
            QueryResult {
                query_type: "references".to_string(),
                data: serde_json::json!({
                    "results": results,
                    "filter_applied": ref_kind,
                }),
            }
        }
        QuerySpec::Hover { file, line, col } => {
            let (_path, word) = word_at(root, file, *line, *col)?;
            let mut locations = engine.definitions(&word);
            sort_locations(&mut locations);
            // Reuse signature enrichment to load cached source lines. Empty detail
            // is not a signature; ambiguous name-only matches remain honest misses.
            let signature = if locations.len() == 1 {
                use crate::indexer::resolution::{
                    enrich_at_location, ResolveOptions, SubstitutionContext,
                };
                enrich_at_location(
                    index.as_ref(),
                    &locations[0],
                    &word,
                    SubstitutionContext::None,
                    &ResolveOptions::hover(),
                )
                .map(|symbol| symbol.signature)
                .filter(|signature| !signature.is_empty())
            } else {
                None
            };
            QueryResult {
                query_type: "hover".to_string(),
                data: serde_json::json!({
                    "name": word,
                    "signature": signature,
                }),
            }
        }
        QuerySpec::Summarize { name } => {
            let mut locs = engine.definitions(name);
            sort_locations(&mut locs);
            let summary: serde_json::Value = if let Some(loc) = locs.first() {
                let uri_str = loc.uri.to_string();
                if let Some(file_ref) = index.get_file(&uri_str) {
                    let sym = file_ref.symbols.iter().find(|s| s.name == *name);
                    if let Some(sym) = sym {
                        serde_json::json!({
                            "name": sym.name,
                            "kind": sym.kind_label(),
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
            let depth = depth.unwrap_or(1);
            if depth != 1 {
                return Err("callers depth supports only 1 (or omitted)".into());
            }
            let (_path, word) = word_at(root, file, *line, *col)?;
            let mut entries = engine.callers_of(&word);
            entries.sort();
            entries.dedup();
            let callers: Vec<serde_json::Value> = entries
                .iter()
                .take(20)
                .map(|(file, name)| serde_json::json!({ "name": name, "file": file }))
                .collect();
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
                    let mut locations = locs.value().clone();
                    sort_locations(&mut locations);
                    locations
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
                    let mut locations = locs.value().clone();
                    sort_locations(&mut locations);
                    locations
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
    })
}
