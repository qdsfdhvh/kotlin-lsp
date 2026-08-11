//! Symbol summarization — `summarize <name>` returns structured info about a symbol.
//!
//! Unlike `find` (which returns locations), `summarize` returns signature, members,
//! KDoc, and dependencies so agents can decide next steps without reading source.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Serialize)]
struct SymbolSummary {
    name: String,
    kind: String,
    visibility: String,
    modifiers: Vec<String>,
    signature: Option<String>,
    members: Vec<MemberSummary>,
    doc: Option<String>,
    dependencies: Vec<String>,
    file: String,
    line: u32,
    col: u32,
}

#[derive(Debug, Serialize)]
struct MemberSummary {
    name: String,
    kind: String,
    signature: Option<String>,
}

pub(crate) async fn run_summarize(name: &str, expand: bool, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;

    let locs = index.definition_locations(name);
    if locs.is_empty() {
        eprintln!("Symbol not found: {name}");
        std::process::exit(1);
    }

    // Use the first definition location.
    let loc = &locs[0];
    let file_path = loc
        .uri
        .to_file_path()
        .ok()
        .or_else(|| {
            // Fallback: try relative path against workspace root
            let path_str = loc.uri.path().trim_start_matches('/');
            let candidate = root.join(path_str);
            if candidate.exists() {
                Some(candidate)
            } else {
                None
            }
        })
        .unwrap_or_else(|| PathBuf::from("."));

    // Try fast path: read symbol metadata from indexed FileData.
    // Only re-parse source for KDoc (and members in --expand).
    let uri_str = loc.uri.to_string();
    let indexed_sym = index.files.get(&uri_str).and_then(|f| {
        f.symbols
            .iter()
            .find(|s| s.name == name && s.selection_range.start.line == loc.range.start.line)
            .cloned()
    });

    let summary = if let Some(sym) = indexed_sym {
        build_summary_from_index(name, &sym, &file_path, loc, &index)
    } else {
        let source = std::fs::read_to_string(&file_path).unwrap_or_default();
        build_summary(name, &file_path, &source, loc, expand)
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).expect("serialize JSON")
        );
    } else {
        print_summary(&summary, expand);
    }
}

fn build_summary(
    name: &str,
    file_path: &std::path::Path,
    source: &str,
    loc: &tower_lsp::lsp_types::Location,
    expand: bool,
) -> SymbolSummary {
    let lang = crate::Language::from_path(file_path.to_str().unwrap_or(""));
    let mut parser = tree_sitter::Parser::new();
    let ts_lang = match lang {
        crate::Language::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
        crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
        crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
    };
    parser.set_language(&ts_lang).ok();
    let tree = parser.parse(source, None);

    let root = tree.as_ref().map(|t| t.root_node());

    // Find the declaration node near the definition location.
    let line = loc.range.start.line as usize;
    let byte_col = source
        .lines()
        .nth(line)
        .map(|lt| {
            crate::indexer::live_tree::utf16_col_to_byte(lt, loc.range.start.character as usize)
        })
        .unwrap_or(0);
    let point = tree_sitter::Point::new(line, byte_col);

    let decl = root
        .and_then(|r| r.descendant_for_point_range(point, point))
        .and_then(|n| find_enclosing_declaration(&n, root.as_ref().unwrap()));

    let (kind, visibility, modifiers, signature, members, dependencies) = match &decl {
        Some(d) => extract_decl_info(d, source, expand),
        None => (
            "unknown".to_string(),
            "public".to_string(),
            vec![],
            None,
            vec![],
            vec![],
        ),
    };

    let doc = decl.as_ref().and_then(|d| extract_kdoc(d, source));

    SymbolSummary {
        name: name.to_string(),
        kind,
        visibility,
        modifiers,
        signature,
        members,
        doc,
        dependencies,
        file: file_path.display().to_string(),
        line: loc.range.start.line + 1,
        col: loc.range.start.character + 1,
    }
}

fn find_enclosing_declaration<'a>(
    node: &tree_sitter::Node<'a>,
    _root: &tree_sitter::Node<'_>,
) -> Option<tree_sitter::Node<'a>> {
    let mut cur = *node;
    loop {
        match cur.kind() {
            "function_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "object_declaration"
            | "enum_declaration"
            | "property_declaration" => return Some(cur),
            "source_file" | "program" => return None,
            _ => {
                let p = cur.parent()?;
                cur = p
            }
        }
    }
}

fn extract_decl_info(
    decl: &tree_sitter::Node,
    source: &str,
    expand: bool,
) -> (
    String,             // kind
    String,             // visibility
    Vec<String>,        // modifiers
    Option<String>,     // signature
    Vec<MemberSummary>, // members
    Vec<String>,        // dependencies (types used)
) {
    let kind = kind_label(decl.kind());
    let (visibility, modifiers) = extract_modifiers(decl, source);
    let signature = extract_signature(decl, source);
    let members = if expand {
        extract_members(decl, source)
    } else {
        vec![]
    };
    let dependencies = extract_type_dependencies(decl, source);

    (
        kind,
        visibility,
        modifiers,
        signature,
        members,
        dependencies,
    )
}

fn kind_label(kind: &str) -> String {
    match kind {
        "class_declaration" => "class".to_string(),
        "interface_declaration" => "interface".to_string(),
        "object_declaration" => "object".to_string(),
        "enum_declaration" => "enum".to_string(),
        "function_declaration" => "function".to_string(),
        "method_declaration" => "method".to_string(),
        "constructor_declaration" => "constructor".to_string(),
        "property_declaration" => "property".to_string(),
        _ => kind.to_string(),
    }
}

fn extract_modifiers(decl: &tree_sitter::Node, source: &str) -> (String, Vec<String>) {
    let mut vis = "public".to_string();
    let mut mods = Vec::new();

    for child in children(decl) {
        if child.kind() == "modifiers" {
            let text = &source[child.start_byte()..child.end_byte()];
            if text.contains("private") {
                vis = "private".to_string();
            }
            if text.contains("internal") {
                vis = "internal".to_string();
            }
            if text.contains("protected") {
                vis = "protected".to_string();
            }
            if text.contains("abstract") {
                mods.push("abstract".to_string());
            }
            if text.contains("open") {
                mods.push("open".to_string());
            }
            if text.contains("data") {
                mods.push("data".to_string());
            }
            if text.contains("sealed") {
                mods.push("sealed".to_string());
            }
            if text.contains("override") {
                mods.push("override".to_string());
            }
            if text.contains("suspend") {
                mods.push("suspend".to_string());
            }
            if text.contains("inline") {
                mods.push("inline".to_string());
            }
            if text.contains("tailrec") {
                mods.push("tailrec".to_string());
            }
            if text.contains("operator") {
                mods.push("operator".to_string());
            }
            if text.contains("infix") {
                mods.push("infix".to_string());
            }
        }
    }

    (vis, mods)
}

fn extract_signature(decl: &tree_sitter::Node, source: &str) -> Option<String> {
    let text = decl.utf8_text(source.as_bytes()).ok()?;
    let sig = text.lines().next().unwrap_or(text);
    // Trim braces for single-line functions.
    if sig.contains('{') {
        let idx = sig.find('{').unwrap_or(sig.len());
        Some(sig[..idx].trim().to_string())
    } else {
        Some(sig.to_string())
    }
}

fn extract_members(decl: &tree_sitter::Node, source: &str) -> Vec<MemberSummary> {
    let mut members = Vec::new();
    let body = find_body(decl);
    if let Some(body) = body {
        for child in children(&body) {
            match child.kind() {
                "function_declaration" | "method_declaration" | "constructor_declaration" => {
                    let name = first_child_simple_id(&child, source);
                    let sig = extract_signature(&child, source);
                    members.push(MemberSummary {
                        name,
                        kind: kind_label(child.kind()),
                        signature: sig,
                    });
                }
                "property_declaration" => {
                    let name = first_child_simple_id(&child, source);
                    let sig = extract_signature(&child, source);
                    members.push(MemberSummary {
                        name,
                        kind: "property".to_string(),
                        signature: sig,
                    });
                }
                _ => {}
            }
        }
    }
    members
}

fn find_body<'a>(decl: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    children(decl).into_iter().find(|&child| {
        child.kind() == "class_body"
            || child.kind() == "function_body"
            || child.kind() == "enum_class_body"
    })
}

fn extract_type_dependencies(decl: &tree_sitter::Node, source: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut stack = vec![*decl];
    while let Some(node) = stack.pop() {
        if node.kind() == "user_type" || node.kind() == "type_identifier" {
            if let Ok(name) = node.utf8_text(source.as_bytes()) {
                if !deps.contains(&name.to_string()) {
                    deps.push(name.to_string());
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    deps
}

fn extract_kdoc(decl: &tree_sitter::Node, source: &str) -> Option<String> {
    // Look for preceding comment nodes.
    let mut prev = decl.prev_sibling();
    while let Some(p) = prev {
        let text = p.utf8_text(source.as_bytes()).unwrap_or("");
        if text.starts_with("/**") {
            return Some(strip_kdoc_markers(text));
        }
        if p.kind() != "comment" && p.kind() != "block_comment" {
            break;
        }
        prev = p.prev_sibling();
    }
    None
}

fn strip_kdoc_markers(kdoc: &str) -> String {
    kdoc.lines()
        .map(|l| {
            l.trim()
                .trim_start_matches("/**")
                .trim_start_matches(" *")
                .trim_start_matches("*/")
                .trim()
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_child_simple_id(node: &tree_sitter::Node, source: &str) -> String {
    for child in children(node) {
        if child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// Build a SymbolSummary directly from indexed FileData without re-parsing source.
/// KDoc is not available from the index; use `--expand` + source re-parse for that.
fn build_summary_from_index(
    name: &str,
    sym: &crate::types::SymbolEntry,
    file_path: &std::path::Path,
    loc: &tower_lsp::lsp_types::Location,
    index: &std::sync::Arc<crate::indexer::Indexer>,
) -> SymbolSummary {
    let visibility = format!("{:?}", sym.visibility).to_lowercase();
    let mut modifiers = Vec::new();
    if sym.deprecated {
        modifiers.push("deprecated".to_string());
    }
    // Infer modifiers from detail/signature text
    let detail_lower = sym.detail.to_lowercase();
    for keyword in &[
        "abstract",
        "open",
        "data",
        "sealed",
        "suspend",
        "inline",
        "override",
        "tailrec",
        "operator",
        "infix",
        "expect",
        "actual",
        "external",
        "const",
        "lateinit",
        "inner",
        "annotation",
        "companion",
    ] {
        if detail_lower.contains(keyword) {
            modifiers.push(keyword.to_string());
        }
    }

    let signature = if sym.detail.is_empty() {
        None
    } else {
        Some(sym.detail.clone())
    };

    let kind = sym.kind_label();

    // Members: look up all non-private symbols in the same file.
    let members: Vec<MemberSummary> = index
        .files
        .get(&loc.uri.to_string())
        .map(|f| {
            f.symbols
                .iter()
                .filter(|s| s.name != name)
                .filter(|s| s.visibility != crate::types::Visibility::Private)
                .map(|s| MemberSummary {
                    name: s.name.clone(),
                    kind: s.kind_label(),
                    signature: if s.detail.is_empty() {
                        None
                    } else {
                        Some(s.detail.clone())
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    SymbolSummary {
        name: name.to_string(),
        kind,
        visibility,
        modifiers,
        signature,
        members,
        doc: sym.documentation.clone(),
        dependencies: vec![],
        file: file_path.display().to_string(),
        line: loc.range.start.line + 1,
        col: loc.range.start.character + 1,
    }
}

fn print_summary(summary: &SymbolSummary, expand: bool) {
    println!("{}", summary.name);
    println!("  Kind: {} {}", summary.visibility, summary.kind);
    if !summary.modifiers.is_empty() {
        println!("  Modifiers: {}", summary.modifiers.join(", "));
    }
    if let Some(ref sig) = summary.signature {
        println!("  Signature: {sig}");
    }
    if let Some(ref doc) = summary.doc {
        println!("  Doc: {doc}");
    }
    if !summary.dependencies.is_empty() {
        println!("  Dependencies: {}", summary.dependencies.join(", "));
    }
    println!(
        "  Location: {}:{}:{}",
        summary.file, summary.line, summary.col
    );
    if expand && !summary.members.is_empty() {
        println!("  Members:");
        for m in &summary.members {
            print!("    {} {} ", m.kind, m.name);
            if let Some(ref sig) = m.signature {
                print!("{sig}");
            }
            println!();
        }
    }
}
#[cfg(test)]
#[path = "summarize_tests.rs"]
mod tests;
