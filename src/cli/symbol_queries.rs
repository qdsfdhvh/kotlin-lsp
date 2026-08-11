//! Additional symbol queries: imports-of, annotated, package-deps, docs.
//! All computed from existing index data — no additional parsing needed.

use std::collections::HashMap;
use std::path::PathBuf;

// ── imports-of ──────────────────────────────────────────────────────────────

pub(crate) async fn run_imports_of(name: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut importing_files: Vec<String> = Vec::new();

    // Scan all indexed files for import statements matching `name`
    for file_entry in index.files.iter() {
        let uri_str = file_entry.key();
        let file_data = file_entry.value();
        for import in &file_data.imports {
            if import.full_path.contains(name)
                || import.full_path.ends_with(&format!(".{name}"))
                || import.full_path == name
            {
                importing_files.push(uri_str.clone());
                break;
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "name": name,
            "importing_files": importing_files,
            "count": importing_files.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Files importing {}: {} found", name, importing_files.len());
        for f in &importing_files {
            println!("  {f}");
        }
    }
}

// ── annotated ───────────────────────────────────────────────────────────────

pub(crate) async fn run_annotated(annotation: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    // Use pre-built annotation edge index (O(1) lookup) instead of
    // scanning all files' symbol details with string contains.
    let mut results: Vec<serde_json::Value> = Vec::new();

    if let Some(entries) = index.annotation_edges.get(annotation) {
        for (file, symbol_name) in entries.iter() {
            // Try to get symbol details from the file data
            let line = index
                .files
                .get(file)
                .and_then(|fd| {
                    fd.symbols
                        .iter()
                        .find(|s| &s.name == symbol_name)
                        .map(|s| s.selection_range.start.line + 1)
                })
                .unwrap_or(0);

            results.push(serde_json::json!({
                "name": symbol_name,
                "file": file,
                "line": line,
            }));
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else {
        println!("{} symbols matching @{annotation}:", results.len());
        for r in &results {
            let name = r["name"].as_str().unwrap_or("?");
            let file = r["file"].as_str().unwrap_or("?");
            let line = r["line"].as_u64().unwrap_or(0);
            println!("  {name} @ {file}:{line}");
        }
    }
}

// ── package-deps ────────────────────────────────────────────────────────────

pub(crate) async fn run_package_deps(package: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut deps: HashMap<String, Vec<String>> = HashMap::new();

    for file_entry in index.files.iter() {
        let file_data = file_entry.value();
        let pkg = file_data.package.as_deref().unwrap_or(".");
        if pkg == package || package.is_empty() {
            for import in &file_data.imports {
                if let Some(dot) = import.full_path.rfind('.') {
                    let dep_pkg = &import.full_path[..dot];
                    if dep_pkg != pkg {
                        deps.entry(dep_pkg.to_string())
                            .or_default()
                            .push(file_entry.key().clone());
                    }
                }
            }
        }
    }

    if json {
        let output: serde_json::Value = serde_json::json!({
            "package": package,
            "dependencies": deps.keys().collect::<Vec<_>>(),
            "details": deps.iter().map(|(pkg, files)| {
                serde_json::json!({ "package": pkg, "file_count": files.len() })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Package dependencies for '{package}':");
        let mut sorted: Vec<_> = deps.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (dep_pkg, files) in &sorted {
            println!("  → {dep_pkg} ({})", files.len());
        }
    }
}

// ── docs ─────────────────────────────────────────────────────────────────────

pub(crate) async fn run_docs(query: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let mut results: Vec<serde_json::Value> = Vec::new();
    let query_lower = query.to_lowercase();

    for file_entry in index.files.iter() {
        let file_data = file_entry.value();
        let path = std::path::Path::new(file_entry.key());
        for sym in &file_data.symbols {
            if sym.detail.to_lowercase().contains(&query_lower) {
                results.push(serde_json::json!({
                    "name": sym.name,
                    "kind": sym.kind_label(),
                    "file": path.display().to_string(),
                    "line": sym.selection_range.start.line + 1,
                    "signature": sym.detail,
                    "visibility": format!("{:?}", sym.visibility).to_lowercase(),
                    "deprecated": sym.deprecated,
                }));
            }
            if results.len() >= 50 {
                break;
            }
        }
        if results.len() >= 50 {
            break;
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    } else {
        println!("{} symbols matching '{}':", results.len(), query);
        for r in &results {
            let name = r["name"].as_str().unwrap_or("?");
            let sig = r["signature"].as_str().unwrap_or("");
            let file = r["file"].as_str().unwrap_or("?");
            println!("  {name} — {sig}");
            println!("    @ {file}");
        }
    }
}
