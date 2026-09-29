//! Real source-path indexing → persisted native cache → fresh lazy accessor.
use std::sync::Arc;

use tower_lsp::lsp_types::Url;

use super::{test_helpers::with_xdg_cache, Indexer};

#[test]
fn get_file_native_cache_first_access_lines_and_identity() {
    let dir = tempfile::tempdir().expect("fixture");
    with_xdg_cache(&dir.path().join("cache"), || {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let root = dir.path().join("workspace");
            let library = dir.path().join("external space % # 中文");
            std::fs::create_dir_all(&root).expect("workspace");
            std::fs::create_dir_all(&library).expect("library");
            let root = root.canonicalize().expect("canonical workspace");
            let library = library.canonicalize().expect("canonical library");
            let requested = library.join("Requested % # 中文.kt");
            let other = library.join("Other.kt");
            let removed = library.join("Removed.kt");
            let workspace = root.join("Requested % # 中文.kt");
            let source = "package external\nclass Requested\n// exact 中文 % # lines\n";
            for (path, text) in [
                (&requested, source),
                (&other, "class Other\n"),
                (&removed, "class Removed\n"),
                (&workspace, "class WorkspaceOnly\n"),
            ] {
                std::fs::write(path, text).expect("source");
            }
            let uri = Url::from_file_path(&requested).expect("requested URI");
            let requested_native = uri.to_file_path().expect("decoded URI");
            // Windows canonicalization adds a verbatim prefix; file URLs decode
            // to ordinary native paths. Compare filesystem identity, not spelling.
            assert_eq!(
                requested_native.canonicalize().expect("decoded identity"),
                requested.canonicalize().expect("requested identity")
            );
            assert!(uri.as_str().contains("%25"));
            assert!(uri.as_str().contains("%23"));
            assert!(uri.as_str().contains("%20"));
            let other_uri = Url::from_file_path(&other).expect("other URI");
            let other_native = other_uri.to_file_path().expect("decoded other URI");
            let removed_uri = Url::from_file_path(&removed).expect("removed URI");
            let workspace_uri = Url::from_file_path(&workspace).expect("workspace URI");
            let cold = Arc::new(Indexer::new());
            *cold.source_paths_raw.write().expect("source paths") =
                vec![library.to_string_lossy().into_owned()];
            cold.ensure_indexed(&workspace_uri);
            Arc::clone(&cold).index_source_paths(root.clone()).await;
            let cold_data = cold.get_file(uri.as_str()).expect("cold library");
            let expected_lines: Vec<String> = source.lines().map(str::to_owned).collect();
            assert_eq!(&**cold_data.lines, &expected_lines);
            assert_eq!(cold_data.symbols[0].name, "Requested");
            let cache_path =
                super::cache::library_cache_path(&[library.to_string_lossy().into_owned()]);
            assert!(std::fs::metadata(&cache_path).expect("full cache").len() > 0);
            assert!(
                std::fs::metadata(super::symbol_index::symbol_index_path(&cache_path))
                    .expect("compact cache")
                    .len()
                    > 0
            );
            // Read the real writer's output: no fabricated cache entries.
            let persisted = super::cache::try_load_library_cache_from(&cache_path)
                .expect("persisted native cache");
            // Persisted keys use URI-decoded native paths, just like the writer.
            assert!(persisted.contains_key(requested_native.to_string_lossy().as_ref()));
            assert!(!persisted.contains_key(uri.as_str()));
            assert!(!persisted[requested_native.to_string_lossy().as_ref()]
                .file_data
                .lines
                .is_filled());
            drop(persisted);
            drop(cold);

            let warm = Arc::new(Indexer::new());
            *warm.source_paths_raw.write().expect("source paths") =
                vec![library.to_string_lossy().into_owned()];
            warm.ensure_indexed(&workspace_uri);
            Arc::clone(&warm).index_source_paths(root).await;
            assert!(!warm.files.contains_key(uri.as_str()), "fast start is lazy");
            assert!(!warm.files.contains_key(other_uri.as_str()));
            let first = warm
                .get_file(uri.as_str())
                .expect("first warm library access");
            assert_eq!(first.symbols[0].name, "Requested");
            assert_eq!(
                &**first.lines, &expected_lines,
                "first return must already hydrate lines"
            );
            let repeated = warm.get_file(uri.as_str()).expect("repeated access");
            assert!(Arc::ptr_eq(&first, &repeated));
            assert_eq!(&**repeated.lines, &expected_lines);
            assert!(
                !warm.files.contains_key(other_uri.as_str()),
                "only requested file materialized"
            );
            let entries = warm.library_cache_entries.read().expect("cache entries");
            assert!(
                !entries.as_ref().expect("deserialized cache")
                    [other_native.to_string_lossy().as_ref()]
                .file_data
                .lines
                .is_filled(),
                "unrequested cached source stays unfilled"
            );
            drop(entries);
            assert_eq!(
                warm.get_file(workspace_uri.as_str())
                    .expect("workspace retained")
                    .symbols[0]
                    .name,
                "WorkspaceOnly"
            );
            assert!(warm.get_file("not a URI").is_none());
            assert!(warm
                .get_file("https://example.invalid/Requested.kt")
                .is_none());
            let missing = Url::from_file_path(library.join("Missing.kt")).expect("missing URI");
            assert!(warm.get_file(missing.as_str()).is_none());
            std::fs::remove_file(&removed).expect("remove cached source");
            let missing_source = warm
                .get_file(removed_uri.as_str())
                .expect("cached metadata survives missing source");
            assert_eq!(missing_source.symbols[0].name, "Removed");
            assert!(missing_source.lines.is_empty());
            assert!(warm
                .get_file(removed_uri.as_str())
                .expect("repeat missing source")
                .lines
                .is_empty());
            // Already-filled source data must not be overwritten on subsequent access.
            std::fs::write(&requested, "class ChangedOnDisk\n").expect("change source");
            assert_eq!(
                &**warm
                    .get_file(uri.as_str())
                    .expect("stable filled data")
                    .lines,
                &expected_lines
            );
        });
    });
}
