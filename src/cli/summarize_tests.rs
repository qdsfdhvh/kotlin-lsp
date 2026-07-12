use std::path::Path;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::build_summary;

fn loc(file: &str, line: u32, col: u32) -> Location {
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
fn class() {
    let src = "package com.test\n\nclass Greeter(val name: String)\n";
    let s = build_summary(
        "Greeter",
        Path::new("/t.kt"),
        src,
        &loc("t.kt", 2, 0),
        false,
    );
    assert_eq!(s.kind, "class");
}

#[test]
fn function() {
    let src = "package com.test\n\nfun greet(name: String): String = \"Hello\"\n";
    let s = build_summary("greet", Path::new("/t.kt"), src, &loc("t.kt", 2, 0), false);
    assert_eq!(s.kind, "function");
}

#[test]
fn unknown() {
    let src = "package com.test\n\n// nothing here\n";
    let s = build_summary("NoSuch", Path::new("/t.kt"), src, &loc("t.kt", 2, 1), false);
    assert!(!s.name.is_empty());
}
