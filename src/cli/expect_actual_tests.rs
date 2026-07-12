use super::{detect_ss, first_id, parse_kotlin};
use std::path::Path;

#[test]
fn parse_smoke() {
    assert!(parse_kotlin("fun main() {}").is_some());
}

#[test]
fn empty_no_panic() {
    let _ = parse_kotlin("");
}

#[test]
fn detect_ss_smoke() {
    let ss = detect_ss(Path::new("src/commonMain/kotlin/Foo.kt"));
    assert!(!ss.is_empty());
    let ss2 = detect_ss(Path::new("src/main/kotlin/Bar.kt"));
    assert!(!ss2.is_empty());
}

#[test]
fn first_id_no_panic() {
    let src = "class Foo";
    let tree = parse_kotlin(src).unwrap();
    let root = tree.root_node();
    if let Some(decl) = root.child(0 as u32) {
        let _id = first_id(&decl, src);
    }
}
