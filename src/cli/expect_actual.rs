//! KMP expect/actual resolution.
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct ExpectActual {
    expect_name: String,
    expect_file: String,
    expect_line: u32,
    actuals: Vec<ActualInfo>,
}
#[derive(Debug, Serialize)]
struct ActualInfo {
    file: String,
    line: u32,
    source_set: String,
    signature: String,
}

pub(crate) async fn run_expect_actual(name: &str, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let _index = crate::cli::run::build_index(&root, false).await;
    let candidates = find_files(name, &root);
    let mut expect: Option<(String, u32)> = None;
    let mut actuals = Vec::new();
    for file in &candidates {
        let ss = detect_ss(file);
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        if let Some(tree) = parse_kotlin(&content) {
            let r = tree.root_node();
            let mut s = vec![r];
            while let Some(n) = s.pop() {
                let is_decl = matches!(
                    n.kind(),
                    "function_declaration" | "class_declaration" | "property_declaration"
                );
                if is_decl {
                    if has_mod(&n, &content, "expect") {
                        let nm = first_id(&n, &content);
                        if nm == name {
                            expect = Some((
                                file.display().to_string(),
                                n.start_position().row as u32 + 1,
                            ));
                        }
                    }
                    if has_mod(&n, &content, "actual") {
                        let nm = first_id(&n, &content);
                        if nm == name {
                            actuals.push(ActualInfo {
                                file: file.display().to_string(),
                                line: n.start_position().row as u32 + 1,
                                source_set: ss.clone(),
                                signature: n
                                    .utf8_text(content.as_bytes())
                                    .unwrap_or("")
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .to_string(),
                            });
                        }
                    }
                }
                for ch in children(&n) {
                    s.push(ch);
                }
            }
        }
    }
    if json {
        let r = ExpectActual {
            expect_name: name.to_string(),
            expect_file: expect.as_ref().map(|(f, _)| f.clone()).unwrap_or_default(),
            expect_line: expect.as_ref().map(|(_, l)| *l).unwrap_or(0),
            actuals,
        };
        println!("{}", serde_json::to_string_pretty(&r).unwrap());
    } else {
        if let Some((f, l)) = &expect {
            println!("expect `{name}` at {f}:{l}");
        } else {
            println!("No expect for `{name}`");
        }
        for a in &actuals {
            println!(
                "  actual @ {}:{} [{}] {}",
                a.file, a.line, a.source_set, a.signature
            );
        }
    }
}

fn parse_kotlin(s: &str) -> Option<tree_sitter::Tree> {
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_kotlin_sg::LANGUAGE.into()).ok();
    p.parse(s, None)
}
fn has_mod(n: &tree_sitter::Node, s: &str, m: &str) -> bool {
    for c in children(n) {
        if c.kind() == "modifiers" && s[c.start_byte()..c.end_byte()].contains(m) {
            return true;
        }
    }
    false
}
fn first_id(n: &tree_sitter::Node, s: &str) -> String {
    for c in children(n) {
        if c.kind() == "simple_identifier" {
            return c.utf8_text(s.as_bytes()).unwrap_or("").into();
        }
    }
    String::new()
}
fn children<'a>(n: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut c = n.walk();
    n.children(&mut c).collect()
}
fn detect_ss(f: &std::path::Path) -> String {
    let p = f.to_string_lossy();
    if p.contains("/commonMain/") {
        "commonMain"
    } else if p.contains("/androidMain/") {
        "androidMain"
    } else if p.contains("/iosMain/") {
        "iosMain"
    } else {
        "unknown"
    }
    .into()
}
fn find_files(name: &str, root: &std::path::Path) -> Vec<PathBuf> {
    use std::process::Command;
    let mut cmd = Command::new("rg");
    cmd.args(["--files-with-matches", "-e", name]);
    cmd.arg(root);
    match cmd.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(PathBuf::from)
            .collect(),
        Err(_) => vec![],
    }
}
