//! Direct, name-based call relationships from the existing workspace graph.
use crate::query::engine::WorkspaceQueryEngine;
use crate::StrExt;
use std::path::Path;
use tower_lsp::lsp_types::{Location, Position, Range, SymbolKind, Url};

pub(crate) enum Query {
    Name(String),
    Position {
        name: String,
        location: Location,
        declaration: bool,
    },
}

pub(crate) fn query_at(base: &Path, file: &Path, line: u32, col: u32) -> Result<Query, String> {
    if line == 0 || col == 0 {
        return Err("line and col must be 1-based".into());
    }
    let path = base
        .join(file)
        .canonicalize()
        .map_err(|error| format!("Cannot read file {}: {error}", file.display()))?;
    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("Cannot read file {}: {error}", file.display()))?;
    let text = source
        .lines()
        .nth(line as usize - 1)
        .ok_or("line is outside the file")?;
    if col as usize > text.encode_utf16().count() + 1 {
        return Err("col is outside the line".into());
    }
    let name = text.word_at_utf16_col(col as usize - 1);
    if name.is_empty() {
        return Err("No callable at cursor".into());
    }
    let byte = crate::indexer::live_tree::utf16_col_to_byte(text, col as usize - 1);
    let start = text[..byte].trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    let position = Position::new(line - 1, start.encode_utf16().count() as u32);
    let location = Location::new(
        Url::from_file_path(&path).map_err(|_| "Invalid file path")?,
        Range::new(position, position),
    );
    let kind = super::ref_kind::classify_reference(&location, &name);
    let declaration = matches!(
        kind,
        super::ref_kind::RefKind::Declaration | super::ref_kind::RefKind::Override
    );
    if kind != super::ref_kind::RefKind::Call && !declaration {
        return Err("No callable at cursor (use a function declaration or call)".into());
    }
    Ok(Query::Position {
        name,
        location,
        declaration,
    })
}

pub(crate) fn fail(error: &str, json: bool) -> ! {
    if json {
        println!("{}", serde_json::json!({"error":error}));
    }
    eprintln!("{error}");
    std::process::exit(1);
}

pub(crate) fn run(
    engine: &WorkspaceQueryEngine,
    query: &Query,
    incoming: bool,
    outgoing: bool,
    json: bool,
) {
    let name = match query {
        Query::Name(name) | Query::Position { name, .. } => name,
    };
    let catalog = catalog(engine, name).unwrap_or_else(|error| fail(&error, json));
    let mut candidates: Vec<_> = catalog
        .iter()
        .filter(|symbol| symbol.name == *name || symbol.key == *name)
        .collect();
    if let Query::Position {
        location,
        declaration,
        ..
    } = query
    {
        let cached_library = location.uri.to_file_path().ok().is_some_and(|path| {
            engine
                .index
                .library_cache_entries
                .read()
                .expect("library cache lock")
                .as_ref()
                .is_some_and(|entries| entries.contains_key(path.to_string_lossy().as_ref()))
        });
        if !engine.index.files.contains_key(location.uri.as_str()) && !cached_library {
            fail(
                "Cursor file is not indexed under the selected root/source paths",
                json,
            );
        }
        let declarations: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|symbol| {
                symbol.location.uri == location.uri
                    && symbol.location.range.start == location.range.start
            })
            .collect();
        if *declaration {
            candidates = declarations;
        }
    }
    if candidates.len() != 1 {
        if candidates.is_empty() {
            fail(&format!("No callable found for '{name}'"), json);
        }
        ambiguous(name, &candidates, json);
    }
    let target = candidates[0];
    let name = &target.name;
    let key = &target.key;
    let mut callers = Vec::new();
    if incoming {
        let mut keys = vec![key.as_str()];
        if name != key {
            keys.push(name);
        }
        for edge_key in keys {
            let edges = engine.callers_of(edge_key);
            if edges.is_empty() {
                continue;
            }
            let matches: Vec<_> = catalog
                .iter()
                .filter(|symbol| {
                    if edge_key.contains('.') {
                        symbol.key == edge_key
                    } else {
                        symbol.name == edge_key
                    }
                })
                .collect();
            if matches.len() > 1 {
                ambiguous(edge_key, &matches, json);
            }
            callers.extend(edges.into_iter().map(|(_, caller)| caller));
        }
    }
    let mut callees = if outgoing {
        // Edges retain caller file/key, but not a declaration range. Refuse
        // same-file overloads rather than merging their bodies.
        let matches: Vec<_> = catalog
            .iter()
            .filter(|symbol| symbol.key == *key && symbol.location.uri == target.location.uri)
            .collect();
        if matches.len() > 1 {
            ambiguous(key, &matches, json);
        }
        engine
            .callees_of(key)
            .into_iter()
            .filter(|(uri, _)| *uri == target.location.uri.as_str())
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    callers.sort();
    callers.dedup();
    callees.sort();
    callees.dedup();
    if json {
        println!(
            "{}",
            serde_json::json!({"name":name,"incoming":callers,"outgoing":callees})
        );
    } else {
        println!("## Call hierarchy for `{name}`");
        for (enabled, title, items) in [
            (incoming, "Incoming calls", callers),
            (outgoing, "Outgoing calls", callees),
        ] {
            if enabled {
                println!("### {title}");
                if items.is_empty() {
                    println!("  (none)");
                }
                for item in items {
                    println!("  - {item}");
                }
            }
        }
    }
}

struct Callable {
    name: String,
    key: String,
    location: Location,
}

fn catalog(engine: &WorkspaceQueryEngine, query: &str) -> Result<Vec<Callable>, String> {
    // Warm library indexes keep declarations lazy. Materialize their existing
    // FileData on this same index so cold/warm callable lookup agrees.
    engine.index.lazy_load_library_full();
    let libraries = engine
        .index
        .library_cache_entries
        .read()
        .expect("library cache lock")
        .clone();
    if let Some(libraries) = libraries {
        // Cache keys are filesystem paths, not URIs. Fast start leaves both
        // these FileData and their call edges unloaded; no re-index is needed.
        for (path, entry) in libraries.iter() {
            let Ok(uri) = Url::from_file_path(path) else {
                continue;
            };
            if engine.index.files.contains_key(uri.as_str()) {
                continue;
            }
            let bare = query.rsplit('.').next().unwrap_or(query);
            for (caller, callee) in &entry.file_data.call_edges {
                if caller.rsplit('.').next() == Some(bare)
                    || callee.rsplit('.').next() == Some(bare)
                {
                    engine
                        .index
                        .call_edges
                        .entry(callee.clone())
                        .or_default()
                        .push((uri.to_string(), caller.clone()));
                }
            }
            // Only expand candidate files; incoming contributors need their
            // edges, not their source lines or entire declaration catalog.
            if entry
                .file_data
                .symbols
                .iter()
                .any(|symbol| symbol.name == bare)
            {
                engine.index.files.insert(
                    uri.to_string(),
                    std::sync::Arc::new(entry.file_data.clone()),
                );
            }
        }
    }
    let mut symbols = Vec::new();
    for file in engine.index.files.iter() {
        let Ok(uri) = Url::parse(file.key()) else {
            continue;
        };
        for sym in &file.symbols {
            if !matches!(sym.kind, SymbolKind::FUNCTION | SymbolKind::METHOD)
                || sym.name != query.rsplit('.').next().unwrap_or(query)
            {
                continue;
            }
            // Call extraction qualifies by the nearest enclosing type, whereas
            // parent_fq_name may name the outermost type. Derive this graph key
            // locally from indexed ranges without changing shared symbol data.
            let owner = file
                .symbols
                .iter()
                .filter(|owner| {
                    matches!(
                        owner.kind,
                        SymbolKind::CLASS
                            | SymbolKind::INTERFACE
                            | SymbolKind::STRUCT
                            | SymbolKind::ENUM
                            | SymbolKind::OBJECT
                    ) && owner.range.start <= sym.range.start
                        && sym.range.end <= owner.range.end
                })
                .max_by_key(|owner| owner.range.start);
            let key = match owner {
                Some(owner) => format!("{}.{}", owner.name, sym.name),
                None => sym.name.clone(),
            };
            // Parser SymbolEntry columns are tree-sitter byte offsets. Convert
            // only these candidate declarations at the CLI boundary; leave the
            // shared symbol/cache representation unchanged.
            crate::indexer::Indexer::fill_lines(file.value(), uri.as_str());
            let utf16 = |position: Position| -> Result<Position, String> {
                let prefix = file
                    .lines
                    .get(position.line as usize)
                    .and_then(|line| line.get(..position.character as usize))
                    .ok_or_else(|| {
                        format!(
                            "Cannot read indexed declaration {}:{}",
                            uri,
                            position.line + 1
                        )
                    })?;
                Ok(Position::new(
                    position.line,
                    prefix.encode_utf16().count() as u32,
                ))
            };
            let range = Range::new(
                utf16(sym.selection_range.start)?,
                utf16(sym.selection_range.end)?,
            );
            symbols.push(Callable {
                name: sym.name.clone(),
                key,
                location: Location::new(uri.clone(), range),
            });
        }
    }
    symbols.sort_by(|a, b| {
        a.location
            .uri
            .as_str()
            .cmp(b.location.uri.as_str())
            .then(a.location.range.start.cmp(&b.location.range.start))
    });
    Ok(symbols)
}

fn ambiguous(query: &str, candidates: &[&Callable], json: bool) -> ! {
    let error = format!("Ambiguous callable '{query}' ({} candidates); use a declaration position or a unique Class.method", candidates.len());
    let candidates: Vec<_> = candidates
        .iter()
        .map(|symbol| {
            serde_json::json!({
                "name":symbol.key,
                "file":symbol.location.uri.to_file_path().expect("indexed file URI"),
                "line":symbol.location.range.start.line + 1,
                "col":symbol.location.range.start.character + 1,
            })
        })
        .collect();
    if json {
        println!(
            "{}",
            serde_json::json!({"error":error, "candidates":candidates})
        );
    }
    eprintln!("{error}");
    if !json {
        for candidate in candidates {
            eprintln!("{candidate}");
        }
    }
    std::process::exit(1);
}
