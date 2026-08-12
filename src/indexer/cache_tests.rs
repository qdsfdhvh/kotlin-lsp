//! Unit tests for `indexer::cache`.

use tower_lsp::lsp_types::{SymbolKind, Url};

use super::*;
use crate::types::{FileData, LazyLines, SymbolEntry, Visibility};

fn uri(path: &str) -> Url {
    Url::parse(&format!("file:///test{path}")).unwrap()
}

use crate::indexer::test_helpers::with_xdg_cache;

/// `cache_entry_to_file_result` must reconstruct supertypes from `FileData.lines`
/// even when the `FileCacheEntry` was loaded from disk (lines are always cached).
#[test]
fn cache_entry_to_file_result_supertypes_extracted() {
    let u = uri("/Cat.kt");
    let mut data = FileData {
        lines: LazyLines::from_vec(vec![
            "class Cat : IAnimal {".into(),
            "    fun meow() {}".into(),
            "}".into(),
        ]),
        ..FileData::default()
    };
    data.symbols.push(SymbolEntry {
        name: "Cat".into(),
        kind: SymbolKind::CLASS,
        visibility: Visibility::Public,
        range: Default::default(),
        selection_range: Default::default(),
        detail: String::new(),
        type_params: Vec::new(),
        extension_receiver: String::new(),
        deprecated: false,
        parent_fq_name: None,
        return_type: None,
        parameters: Vec::new(),
        documentation: None,

        is_sealed: false,
        is_typealias: false,
    });
    data.supers.push((
        0,
        "IAnimal".into(),
        vec![],
        crate::types::SuperKind::Extends,
    ));

    let entry = FileCacheEntry {
        mtime_secs: 100,
        file_size: 0,
        content_hash: 42,
        file_data: data,
    };

    let result = cache_entry_to_file_result(&u, &entry);
    let super_names: Vec<&str> = result
        .supertypes
        .iter()
        .map(|(n, _, _)| n.as_str())
        .collect();
    assert!(
        super_names.contains(&"IAnimal"),
        "IAnimal missing from supertypes: {super_names:?}",
    );
}

/// `cache_entry_to_file_result` must copy `content_hash` through unchanged.
#[test]
fn cache_entry_to_file_result_preserves_hash() {
    let u = uri("/Foo.kt");
    let mut data = FileData {
        lines: LazyLines::from_vec(vec!["class Foo".into()]),
        ..FileData::default()
    };
    data.symbols.push(SymbolEntry {
        name: "Foo".into(),
        kind: SymbolKind::CLASS,
        visibility: Visibility::Public,
        range: Default::default(),
        selection_range: Default::default(),
        detail: String::new(),
        type_params: Vec::new(),
        extension_receiver: String::new(),
        deprecated: false,
        parent_fq_name: None,
        return_type: None,
        parameters: Vec::new(),
        documentation: None,

        is_sealed: false,
        is_typealias: false,
    });

    let entry = FileCacheEntry {
        mtime_secs: 0,
        file_size: 0,
        content_hash: 0xdeadbeef,
        file_data: data,
    };

    let result = cache_entry_to_file_result(&u, &entry);
    assert_eq!(result.content_hash, 0xdeadbeef);
}

/// `workspace_cache_path` must be stable: same root → same path.
#[test]
fn workspace_cache_path_stable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("my_project");
    with_xdg_cache(tmp.path(), || {
        let p1 = workspace_cache_path(&root);
        let p2 = workspace_cache_path(&root);
        assert_eq!(p1, p2);
    });
}

/// Different roots must produce different cache paths.
#[test]
fn workspace_cache_path_differs_for_different_roots() {
    let tmp = tempfile::tempdir().expect("tempdir");
    with_xdg_cache(tmp.path(), || {
        let p1 = workspace_cache_path(&tmp.path().join("project_a"));
        let p2 = workspace_cache_path(&tmp.path().join("project_b"));
        assert_ne!(p1, p2);
    });
}

/// `try_load_cache` must return `None` for a non-existent root (no panic).
#[test]
fn try_load_cache_missing_returns_none() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("workspace");
    std::fs::create_dir(&root).expect("create workspace dir");

    with_xdg_cache(tmp.path(), || {
        let result = try_load_cache(&root);
        assert!(result.is_none());
    });
}

/// `save_cache` → `try_load_cache` roundtrip: symbols survive disk persistence.
#[test]
fn save_and_load_cache_roundtrip() {
    use crate::indexer::Indexer;

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("workspace");
    std::fs::create_dir(&root).expect("create workspace dir");

    let src = "package com.example\nclass RoundtripClass";
    let kt_file = tmp.path().join("RoundtripClass.kt");
    std::fs::write(&kt_file, src).expect("write kt file");
    let u = Url::from_file_path(&kt_file).expect("valid file URL");

    let idx = Indexer::new();
    idx.index_content(&u, src);

    with_xdg_cache(tmp.path(), || {
        save_cache(
            &root,
            &idx.files,
            &idx.content_hashes,
            &idx.library_uris,
            true,
        );

        let loaded = try_load_cache(&root).expect("cache should exist after save");
        assert_eq!(loaded.version, CACHE_VERSION);
        assert!(loaded.complete_scan);

        let file_path = kt_file.to_string_lossy().to_string();
        let entry = loaded
            .entries
            .get(&file_path)
            .expect("entry should be present");
        let has_class = entry
            .file_data
            .symbols
            .iter()
            .any(|s| s.name == "RoundtripClass");
        assert!(
            has_class,
            "RoundtripClass symbol missing from cache roundtrip"
        );
    });
}

// ── Tier-1 dir freshness (issue #270) ────────────────────────────────────────

#[test]
fn dir_freshness_uses_mtime_without_payload() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::TempDir::new().expect("tempdir");
    let src = dir.path().join("sources");
    std::fs::create_dir_all(&src).expect("mkdir");
    let cache = dir.path().join("library-abc.bin");
    std::fs::write(&cache, b"x").expect("cache file");

    // Cache newer than the source dir → fresh.
    let now = SystemTime::now();
    let _ = std::fs::File::options()
        .write(true)
        .open(&cache)
        .and_then(|f| f.set_modified(now + Duration::from_secs(60)));
    assert!(
        super::library_cache_dirs_fresh(std::slice::from_ref(&src), &cache),
        "cache newer than dir is fresh"
    );

    // Source dir newer than cache → stale (and it must not deserialize — this
    // function is pure stat; it cannot fail on a corrupt payload).
    let _ = std::fs::File::options()
        .write(true)
        .open(&cache)
        .and_then(|f| f.set_modified(now - Duration::from_secs(60)));
    assert!(
        !super::library_cache_dirs_fresh(std::slice::from_ref(&src), &cache),
        "dir newer than cache is stale"
    );
}

// ── LazyLines: cache hits deserialize empty, fill() re-reads disk (issue #304) ─

#[test]
fn lazy_lines_fill_from_disk_on_cache_hit() {
    use std::io::Write;
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("Source.kt");
    std::fs::File::create(&path)
        .expect("create")
        .write_all(b"class Cat\n    fun meow()\n")
        .expect("write");

    // A cache-hit FileData has empty lines (serde skip).
    let data = FileData::default();
    assert!(!data.lines.is_filled());
    assert!(data.lines.is_empty());

    // fill() pulls them from disk.
    data.lines.fill(&path);
    assert!(data.lines.is_filled());
    assert_eq!(data.lines.len(), 2);
    assert_eq!(data.lines[0], "class Cat");
    assert_eq!(data.lines[1], "    fun meow()");

    // fill() is idempotent; from_content stays eager.
    data.lines.fill(&path);
    assert_eq!(data.lines.len(), 2);
    let eager = LazyLines::from_content("a\nb\n");
    assert!(eager.is_filled());
    assert_eq!(eager.len(), 2);
}

#[test]
fn lazy_lines_share_via_clone() {
    let lines = LazyLines::from_content("x\ny\n");
    let clone = lines.clone();
    assert_eq!(clone.len(), 2);
    assert_eq!(clone[1], "y");
}

#[test]
fn cache_roundtrip_skips_lines() {
    // bincode round-trip: lines are not serialized; everything else survives.
    let mut data = FileData {
        lines: LazyLines::from_content("class Cat\n"),
        ..Default::default()
    };
    data.symbols.push(SymbolEntry {
        name: "Cat".into(),
        kind: SymbolKind::CLASS,
        visibility: Visibility::Public,
        range: Default::default(),
        selection_range: Default::default(),
        detail: "class Cat".into(),
        type_params: Vec::new(),
        extension_receiver: String::new(),
        deprecated: false,
        parent_fq_name: None,
        return_type: None,
        parameters: Vec::new(),
        documentation: None,
        is_sealed: false,
        is_typealias: false,
    });
    let bytes = bincode::serialize(&data).expect("serialize");
    let back: FileData = bincode::deserialize(&bytes).expect("deserialize");
    assert!(!back.lines.is_filled(), "lines must not survive the cache");
    assert_eq!(back.symbols.len(), 1);
    assert_eq!(back.symbols[0].name, "Cat");
    assert_eq!(back.symbols[0].detail, "class Cat");
}
