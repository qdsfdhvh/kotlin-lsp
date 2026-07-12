use super::diagnose_call_args;
use crate::indexer::Indexer;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn smoke_main_no_args() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("Main.kt");
    std::fs::write(&file, "fun main() {}\n").unwrap();
    let idx = Arc::new(Indexer::new());
    let uri = tower_lsp::lsp_types::Url::from_file_path(&file).unwrap();
    idx.index_content(&uri, &std::fs::read_to_string(&file).unwrap());
    let diags = diagnose_call_args(&file, &idx);
    assert!(diags.is_empty(), "unexpected: {:?}", diags);
}

#[test]
fn smoke_run_diagnose() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("D.kt");
    std::fs::write(&file, "fun main() {}\n").unwrap();
    let files = vec![PathBuf::from(&file)];
    let idx = Arc::new(Indexer::new());
    super::run_diagnose(&files, &idx, false);
}
