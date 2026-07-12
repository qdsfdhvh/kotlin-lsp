use std::collections::HashMap;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::*;

fn make_loc(file: &str, line: u32, col: u32) -> Location {
    Location {
        uri: Url::parse(&format!("file:///{file}")).unwrap(),
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col,
            },
        },
    }
}

#[test]
fn roundtrip_save_and_load() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("lib.bin");

    let defs = dashmap::DashMap::new();
    defs.insert("Foo".into(), vec![make_loc("com/Foo.kt", 1, 2)]);
    defs.insert(
        "Bar".into(),
        vec![make_loc("com/Foo.kt", 5, 2), make_loc("com/Bar.kt", 3, 2)],
    );

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

    let idx = SymbolIndex {
        version: SYMBOL_INDEX_VERSION - 1,
        symbols: HashMap::new(),
    };
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
