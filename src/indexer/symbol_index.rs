//! Compact symbol index — enables sub-200ms cold start by loading only
//! symbol names + locations, deferring full FileData loads to first use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

pub(super) const SYMBOL_INDEX_VERSION: u32 = 11;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CompactLoc {
    pub(super) uri: String,
    start_line: u32,
    start_col: u32,
    end_line: u32,
    end_col: u32,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SymbolIndex {
    pub(super) version: u32,
    pub(super) symbols: HashMap<String, Vec<CompactLoc>>,
}

pub(super) fn symbol_index_path(library_cache_path: &Path) -> PathBuf {
    let mut p = library_cache_path.to_path_buf();
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("symbols");
    p.set_file_name(format!("{stem}-symbols.bin"));
    p
}

pub(super) fn save_symbol_index(
    definitions: &DashMap<String, Vec<Location>>,
    library_uris: &DashSet<String>,
    lib_cache_path: &Path,
) {
    let mut symbols: HashMap<String, Vec<CompactLoc>> = HashMap::new();

    for def in definitions.iter() {
        let name = def.key();
        let locs = def.value();
        let mut compact_locs: Vec<CompactLoc> = Vec::new();

        for loc in locs.iter() {
            let uri_str = loc.uri.to_string();
            if !library_uris.contains(&uri_str) {
                continue;
            }
            compact_locs.push(CompactLoc {
                uri: uri_str,
                start_line: loc.range.start.line,
                start_col: loc.range.start.character,
                end_line: loc.range.end.line,
                end_col: loc.range.end.character,
            });
        }

        if !compact_locs.is_empty() {
            symbols.insert(name.clone(), compact_locs);
        }
    }

    let index = SymbolIndex {
        version: SYMBOL_INDEX_VERSION,
        symbols,
    };

    let path = symbol_index_path(lib_cache_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match bincode::serialize(&index) {
        Ok(bytes) => {
            let compressed = crate::indexer::cache::zstd_compress(&bytes);
            let tmp = path.with_extension("bin.tmp");
            if std::fs::write(&tmp, &compressed)
                .and_then(|()| std::fs::rename(&tmp, &path))
                .is_ok()
            {
                log::info!(
                    "Symbol index saved ({} symbols, {} KB → {} KB zstd)",
                    index.symbols.len(),
                    bytes.len() / 1024,
                    compressed.len() / 1024,
                );
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => log::warn!("Symbol index serialize failed: {e}"),
    }
}

pub(super) fn try_load_symbol_index(lib_cache_path: &Path) -> Option<SymbolIndex> {
    let path = symbol_index_path(lib_cache_path);
    let bytes = std::fs::read(&path).ok()?;
    let decompressed = crate::indexer::cache::zstd_decompress(&bytes).ok()?;
    let index: SymbolIndex = bincode::deserialize(&decompressed).ok()?;
    if index.version != SYMBOL_INDEX_VERSION {
        return None;
    }
    Some(index)
}

pub(super) fn populate_from_symbol_index(
    index: &SymbolIndex,
    definitions: &DashMap<String, Vec<Location>>,
) -> DashSet<String> {
    let need_load: DashSet<String> = DashSet::new();

    for (name, locs) in &index.symbols {
        let tower_locs: Vec<Location> = locs
            .iter()
            .map(|cl| {
                need_load.insert(cl.uri.clone());
                Location {
                    uri: Url::parse(&cl.uri)
                        .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                    range: Range {
                        start: Position {
                            line: cl.start_line,
                            character: cl.start_col,
                        },
                        end: Position {
                            line: cl.end_line,
                            character: cl.end_col,
                        },
                    },
                }
            })
            .collect();
        definitions.insert(name.clone(), tower_locs);
    }

    need_load
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    use super::*;

    fn make_loc(file: &str, line: u32, col: u32) -> Location {
        Location {
            uri: Url::parse(&format!("file:///{file}")).unwrap(),
            range: Range {
                start: Position { line, character: col },
                end: Position { line, character: col },
            },
        }
    }

    #[test]
    fn roundtrip_save_and_load() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("lib.bin");

        let defs = dashmap::DashMap::new();
        defs.insert("Foo".into(), vec![make_loc("com/Foo.kt", 1, 2)]);
        defs.insert("Bar".into(), vec![
            make_loc("com/Foo.kt", 5, 2),
            make_loc("com/Bar.kt", 3, 2),
        ]);

        let uris = dashmap::DashSet::new();
        uris.insert("file:///com/Foo.kt".into());
        uris.insert("file:///com/Bar.kt".into());

        save_symbol_index(&defs, &uris, &path);
        assert!(symbol_index_path(&path).exists());

        let idx = try_load_symbol_index(&path).expect("load");
        assert_eq!(idx.symbols.len(), 2);
        assert_eq!(idx.symbols["Foo"].len(), 1);
        assert_eq!(idx.symbols["Bar"].len(), 2);

        let new_defs = dashmap::DashMap::new();
        let needed = populate_from_symbol_index(&idx, &new_defs);
        assert_eq!(needed.len(), 2);
        assert_eq!(new_defs.len(), 2);
    }

    #[test]
    fn version_mismatch_rejected() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("lib.bin");

        let idx = SymbolIndex { version: SYMBOL_INDEX_VERSION - 1, symbols: HashMap::new() };
        let bytes = bincode::serialize(&idx).unwrap();
        let compressed = crate::indexer::cache::zstd_compress(&bytes);
        std::fs::write(symbol_index_path(&path), &compressed).unwrap();

        assert!(try_load_symbol_index(&path).is_none());
    }

    #[test]
    fn empty_index_works() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("lib.bin");

        let defs = dashmap::DashMap::new();
        let uris = dashmap::DashSet::new();
        save_symbol_index(&defs, &uris, &path);

        let idx = try_load_symbol_index(&path).expect("load");
        assert_eq!(idx.symbols.len(), 0);

        let new_defs = dashmap::DashMap::new();
        assert_eq!(populate_from_symbol_index(&idx, &new_defs).len(), 0);
    }
}