//! Tests for reference classification.

use super::*;
use tower_lsp::lsp_types::{Position, Range, Url};

#[test]
fn classify_call_expression() {
    let source = "fun test() { helper() }";
    test_classify(source, "helper", 0, 13, RefKind::Call);
}

#[test]
fn classify_declaration() {
    let source = "fun helper() { return 0 }";
    test_classify(source, "helper", 0, 4, RefKind::Declaration);
}

#[test]
fn classify_override() {
    let source = "override fun helper() { return 0 }";
    test_classify(source, "helper", 0, 13, RefKind::Override);
}

#[test]
fn classify_import() {
    let source = "import com.example.Foo";
    test_classify(source, "Foo", 0, 19, RefKind::Import);
}

#[test]
fn classify_type_use() {
    let source = "val x: Foo = Foo()";
    // "Foo" at col 8 is a type annotation
    test_classify(source, "Foo", 0, 8, RefKind::TypeUse);
}

#[test]
fn ref_kind_from_arg() {
    assert_eq!(RefKind::from_arg("call"), Some(RefKind::Call));
    assert_eq!(RefKind::from_arg("read"), Some(RefKind::Read));
    assert_eq!(RefKind::from_arg("write"), Some(RefKind::Write));
    assert_eq!(RefKind::from_arg("override"), Some(RefKind::Override));
    assert_eq!(RefKind::from_arg("import"), Some(RefKind::Import));
    assert_eq!(RefKind::from_arg("type-use"), Some(RefKind::TypeUse));
    assert_eq!(RefKind::from_arg("declaration"), Some(RefKind::Declaration));
    assert_eq!(RefKind::from_arg("all"), None);
    assert_eq!(RefKind::from_arg("bogus"), None);
}

#[test]
fn ref_kind_as_str() {
    assert_eq!(RefKind::Call.as_str(), "call");
    assert_eq!(RefKind::Read.as_str(), "read");
    assert_eq!(RefKind::Write.as_str(), "write");
    assert_eq!(RefKind::Override.as_str(), "override");
    assert_eq!(RefKind::Import.as_str(), "import");
    assert_eq!(RefKind::TypeUse.as_str(), "type-use");
    assert_eq!(RefKind::Declaration.as_str(), "declaration");
    assert_eq!(RefKind::Reference.as_str(), "reference");
}

fn test_classify(source: &str, name: &str, line: u32, col: u32, expected: RefKind) {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().expect("temp file");
    tmp.write_all(source.as_bytes()).expect("write");
    tmp.flush().expect("flush");

    let uri = Url::from_file_path(tmp.path()).expect("valid path");
    let loc = Location {
        uri,
        range: Range {
            start: Position {
                line,
                character: col,
            },
            end: Position {
                line,
                character: col + name.len() as u32,
            },
        },
    };

    let kind = classify_reference(&loc, name);
    assert_eq!(
        kind,
        expected,
        "classify_reference({name} at {line}:{col} in {source:?}) = {:?}, expected {:?}",
        kind.as_str(),
        expected.as_str()
    );
}
