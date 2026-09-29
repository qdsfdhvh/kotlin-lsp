//! Public indexed-hover reference controls; isolated source selection and caches.
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

struct Fixture {
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        for subdir in ["workspace", "home"] {
            std::fs::create_dir(dir.path().join(subdir)).expect("mkdir");
        }
        Self { dir }
    }
    fn write(&self, name: &str, source: &str) {
        std::fs::write(self.dir.path().join("workspace").join(name), source).expect("source");
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"))
            .current_dir(self.dir.path().join("workspace"))
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("RUST_LOG", "error")
            .args(args)
            .args(["--root", ".", "--no-stdlib"])
            .output()
            .expect("CLI")
    }
    fn index(&self) {
        let output = self.run(&["index", "--json"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"", "{output:?}");
        assert_eq!(output.stderr, b"", "{output:?}");
        let cache = self
            .dir
            .path()
            .join("workspace/.cache/kotlin-lsp/index.bin");
        assert!(
            std::fs::metadata(cache)
                .expect("persisted workspace index")
                .len()
                > 0
        );
    }
    fn check(&self, file: &str) {
        let output = self.run(&["check", file, "--json"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stderr, b"", "{output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).expect("check JSON");
        assert_eq!(
            value,
            serde_json::json!({
                "files_ok": 1, "files_with_errors": 0, "errors": [], "empty_dirs": []
            })
        );
    }
    fn hover(&self, file: &str, line: u32, col: u32) -> Output {
        self.run(&[
            "hover",
            file,
            &line.to_string(),
            &col.to_string(),
            "--smart",
            "--json",
        ])
    }
}
fn signature(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(value.as_object().expect("hover object").len(), 1, "{value}");
    value["signature"].as_str().expect("signature").to_owned()
}
fn missing(output: Output) {
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("No symbol found at "),
        "{output:?}"
    );
}

#[test]
fn indexed_hover_workspace_reference_matches_declaration_signature_doc_and_utf16() {
    let f = Fixture::new();
    f.write("Target.kt", "/** Target documentation. */\nclass Target\n");
    let use_line = "  /* 😀 中文 */ Target()";
    f.write("Use.kt", &format!("fun use() {{\n{use_line}\n}}\n"));
    f.index();
    let declaration = signature(f.hover("Target.kt", 2, 7));
    assert_eq!(declaration, "class Target\n\nTarget documentation.");
    let col = use_line[..use_line.find("Target").expect("target")]
        .encode_utf16()
        .count() as u32
        + 1;
    for _ in 0..2 {
        assert_eq!(signature(f.hover("Use.kt", 2, col)), declaration);
    }
    assert!(Path::new(&f.dir.path().join("workspace/Use.kt")).is_file());
}

fn rejects_member_selection(expression: &str) {
    let f = Fixture::new();
    f.write("Target.kt", "class Target\n");
    f.write("Use.kt", &format!("fun use() {{\n  {expression}\n}}\n"));
    f.index();
    let col = expression.find("Target").expect("selected member") as u32 + 3;
    for _ in 0..2 {
        exact_missing(f.hover("Use.kt", 2, col), "Use.kt", 2, col);
    }
    assert_eq!(signature(f.hover("Target.kt", 1, 7)), "class Target");
}

#[test]
fn indexed_hover_reference_rejects_safe_navigation_global_decoy() {
    rejects_member_selection("receiver?.Target()");
}

#[test]
fn indexed_hover_reference_rejects_call_receiver_global_decoy() {
    rejects_member_selection("factory().Target()");
}

#[test]
fn indexed_hover_reference_rejects_spaced_member_global_decoy() {
    rejects_member_selection("receiver. Target()");
}

#[test]
fn indexed_hover_reference_keeps_unqualified_argument_in_member_call() {
    let f = Fixture::new();
    f.write("Target.kt", "class Target\n");
    let expression = "  receiver.consume(Target())";
    f.write("Use.kt", &format!("fun use() {{\n{expression}\n}}\n"));
    f.index();
    let col = expression.find("Target").expect("argument") as u32 + 1;
    for _ in 0..2 {
        assert_eq!(signature(f.hover("Use.kt", 2, col)), "class Target");
    }
}

#[test]
fn indexed_hover_reference_rejects_ambiguity_qualifiers_and_non_identifiers() {
    let f = Fixture::new();
    f.write("One.kt", "package one\nclass Ambiguous\nclass Target\n");
    f.write("Two.kt", "package two\nclass Ambiguous\n");
    let lines = [
        "  Ambiguous()",
        "  receiver.Target()",
        "  Missing()",
        "  // Target",
        "  \"Target\"",
        "  Target()",
        "",
        "  /* Target */",
    ];
    f.write(
        "Use.kt",
        &format!("fun use() {{\n{}\n}}\n", lines.join("\n")),
    );
    f.index();
    for (row, word) in [
        (0, "Ambiguous"),
        (1, "Target"),
        (2, "Missing"),
        (3, "Target"),
        (4, "Target"),
        (7, "Target"),
    ] {
        let col = lines[row].find(word).expect("word") as u32 + 1;
        missing(f.hover("Use.kt", row as u32 + 2, col));
    }
    missing(f.hover("Use.kt", 7, 9)); // '(' immediately after Target, not an identifier
    missing(f.hover("Use.kt", 8, 1)); // empty line
    missing(f.hover("Use.kt", 999, 1));
    missing(f.hover("Use.kt", 7, 999));
    assert_eq!(signature(f.hover("Use.kt", 7, 3)), "class Target");
    assert_eq!(signature(f.hover("One.kt", 2, 7)), "class Ambiguous");
}

// The known declaration signature is independent of the member's spelling.
// Every query is a fresh CLI process against real indexed files and caches.
fn exact_signature(output: Output, expected: &str) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stderr, b"", "{output:?}");
    assert_eq!(
        output.stdout,
        format!(
            "{{\"signature\":{}}}\n",
            serde_json::to_string(expected).expect("JSON string")
        )
        .as_bytes(),
        "{output:?}"
    );
}

fn exact_missing(output: Output, file: &str, line: u32, col: u32) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(output.stdout, b"", "{output:?}");
    assert_eq!(
        output.stderr,
        format!("No symbol found at {file}:{line}:{col}\n").as_bytes(),
        "{output:?}"
    );
}

fn java_role(statement: &str, col: u32, selected: bool) {
    let f = Fixture::new();
    f.write("Target.java", "class Target {}\n");
    f.write(
        "Use.java",
        &format!("class Use {{\n  void use() {{\n    {statement}\n  }}\n}}\n"),
    );
    // The bundled grammar must accept the form; parser recovery is not coverage.
    f.check("Use.java");
    f.index();
    for _ in 0..2 {
        if selected {
            exact_missing(f.hover("Use.java", 3, col), "Use.java", 3, col);
        } else {
            exact_signature(f.hover("Use.java", 3, col), "class Target {}");
        }
    }
    exact_signature(f.hover("Target.java", 1, 7), "class Target {}");
}

#[test]
fn java_hover_rejects_call_receiver_method_global_decoy() {
    java_role("factory().Target();", 15, true);
}

#[test]
fn java_hover_rejects_spaced_method_global_decoy() {
    java_role("receiver. Target();", 15, true);
}

#[test]
fn java_hover_rejects_call_receiver_field_global_decoy() {
    java_role("consume(factory().Target);", 23, true);
}

#[test]
fn java_hover_rejects_spaced_field_global_decoy() {
    java_role("consume(receiver. Target);", 23, true);
}

#[test]
fn java_hover_rejects_generic_method_global_decoy() {
    java_role("receiver.<String>Target();", 22, true);
}

#[test]
fn java_hover_rejects_spaced_qualified_type_global_decoy() {
    java_role("outer. Target value;", 12, true);
}

#[test]
fn java_hover_rejects_annotated_qualified_type_global_decoy() {
    java_role("outer.@Mark Target value;", 17, true);
}

#[test]
fn java_hover_rejects_method_reference_global_decoy() {
    java_role("consume(receiver::Target);", 23, true);
}

#[test]
fn java_hover_rejects_generic_method_reference_global_decoy() {
    java_role("consume(receiver::<String>Target);", 31, true);
}

#[test]
fn java_hover_keeps_unqualified_argument_type_in_member_call() {
    java_role("receiver.consume(new Target());", 26, false);
}

#[test]
fn java_hover_keeps_unqualified_type() {
    java_role("Target value;", 5, false);
}

#[test]
fn java_hover_keeps_method_receiver_identifier() {
    java_role("Target.consume();", 5, false);
}

#[test]
fn java_hover_keeps_field_receiver_identifier() {
    java_role("consume(Target.value);", 13, false);
}

#[test]
fn java_hover_keeps_method_reference_receiver_identifier() {
    java_role("consume(Target::consume);", 13, false);
}

#[test]
fn java_hover_keeps_constructor_reference_type() {
    java_role("consume(Target::new);", 13, false);
}

#[test]
fn java_hover_keeps_generic_member_type_argument() {
    java_role("receiver.<Target>consume();", 15, false);
}

#[test]
fn swift_hover_rejects_selected_member_global_decoy() {
    let f = Fixture::new();
    f.write("Target.swift", "class Target {}\n");
    f.write(
        "Use.swift",
        "func use() {\n  factory().Target()\n  receiver.Target()\n}\n",
    );
    f.check("Use.swift");
    f.index();
    for _ in 0..2 {
        exact_missing(f.hover("Use.swift", 2, 13), "Use.swift", 2, 13);
        exact_missing(f.hover("Use.swift", 3, 12), "Use.swift", 3, 12);
    }
    exact_signature(f.hover("Target.swift", 1, 7), "class Target {}");
}

#[test]
fn swift_hover_keeps_unqualified_argument_and_declaration() {
    let f = Fixture::new();
    f.write("Target.swift", "class Target {}\n");
    f.write(
        "Use.swift",
        "func use() {\n  receiver.consume(Target())\n}\n",
    );
    f.check("Use.swift");
    f.index();
    for _ in 0..2 {
        exact_signature(f.hover("Use.swift", 2, 20), "class Target {}");
    }
    exact_signature(f.hover("Target.swift", 1, 7), "class Target {}");
}

#[test]
fn java_hover_rejects_scoped_annotation_global_decoy() {
    java_role("@outer. Target String value;", 13, true);
}

#[test]
fn java_hover_keeps_qualified_type_receiver_identifier() {
    java_role("Target.Inner value;", 5, false);
}

#[test]
fn java_hover_keeps_method_reference_type_argument() {
    java_role("consume(receiver::<Target>consume);", 24, false);
}

#[test]
fn java_hover_rejects_qualified_creation_type_global_decoy() {
    java_role("outer.new Target();", 15, true);
}

#[test]
fn java_hover_rejects_qualified_generic_creation_type_global_decoy() {
    java_role("outer.new Target<String>();", 15, true);
}

#[test]
fn java_hover_keeps_unqualified_annotated_creation_type() {
    java_role("consume(new @Mark Target());", 23, false);
}

#[test]
fn java_hover_keeps_qualified_creation_argument_type() {
    java_role("outer.new Inner(new Target());", 25, false);
}
