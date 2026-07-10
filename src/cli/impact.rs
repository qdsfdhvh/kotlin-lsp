//! Impact analysis CLI — `impact <file> <line> <col>`.
//!
//! Given a symbol at a specific position, returns an impact report:
//! what references it, who calls it, and a heuristic risk score.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tower_lsp::lsp_types::Url;

use crate::cli::ref_kind::RefKind;
use crate::indexer::Indexer;

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ImpactReport {
    symbol: String,
    kind: String,
    file: String,
    line: u32,
    col: u32,
    direct_refs: usize,
    /// Ref counts broken down by kind.
    ref_breakdown: RefBreakdown,
    direct_callers: Vec<CallerInfo>,
    risk: String,
    risk_rationale: String,
}

#[derive(Debug, Serialize)]
struct RefBreakdown {
    call: usize,
    read: usize,
    write: usize,
    import: usize,
    type_use: usize,
    other: usize,
}

#[derive(Debug, Serialize)]
struct CallerInfo {
    name: String,
    file: String,
    line: u32,
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub(crate) async fn run_impact(file: &Path, line: u32, col: u32, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, file);
    let index = crate::cli::run::build_index(&root, false).await;
    let uri = Url::from_file_path(file).expect("valid file path");

    let word = extract_word_at_position(&index, &uri, line, col);
    if word.is_empty() {
        eprintln!("No symbol at cursor");
        std::process::exit(1);
    }

    // Find all references to this symbol.
    let (refs, ref_breakdown) = find_and_classify_refs(&word, &index, &root);

    // Find direct callers.
    let callers = find_direct_callers(&word, &index, &root);

    // Compute risk score.
    let (risk, rationale) = compute_risk(refs.len(), &ref_breakdown, callers.len());

    let line = if line == 0 { 1 } else { line };
    let report = ImpactReport {
        symbol: word,
        kind: guess_symbol_kind(&index, &uri),
        file: file.display().to_string(),
        line,
        col,
        direct_refs: refs.len(),
        ref_breakdown,
        direct_callers: callers,
        risk,
        risk_rationale: rationale,
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serialize JSON")
        );
    } else {
        print_report(&report);
    }
}

// ── Reference finding & classification ─────────────────────────────────────

fn find_and_classify_refs(
    name: &str,
    _index: &Arc<Indexer>,
    project_root: &Path,
) -> (Vec<String>, RefBreakdown) {
    let mut files = HashSet::new();
    let mut breakdown = RefBreakdown {
        call: 0,
        read: 0,
        write: 0,
        import: 0,
        type_use: 0,
        other: 0,
    };

    let escaped = crate::rg::regex_escape(name);
    let candidates = find_files_containing_name(&escaped, project_root);

    for candidate_file in &candidates {
        if let Ok(content) = std::fs::read_to_string(candidate_file) {
            let lang = crate::Language::from_path(candidate_file.to_str().unwrap_or(""));
            let mut parser = tree_sitter::Parser::new();
            let ts_lang = match lang {
                crate::Language::Kotlin => tree_sitter_kotlin::LANGUAGE.into(),
                crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
                crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
            };
            if parser.set_language(&ts_lang).is_err() {
                continue;
            }
            if let Some(tree) = parser.parse(&content, None) {
                classify_refs_in_tree(
                    name,
                    candidate_file,
                    &tree.root_node(),
                    &content,
                    &mut files,
                    &mut breakdown,
                );
            }
        }
    }

    let file_list: Vec<String> = files.into_iter().collect();
    (file_list, breakdown)
}

fn classify_refs_in_tree(
    name: &str,
    file: &Path,
    root: &tree_sitter::Node,
    source: &str,
    files: &mut HashSet<String>,
    breakdown: &mut RefBreakdown,
) {
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "simple_identifier" {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                if text == name {
                    let kind = classify_usage(&node, source);
                    match kind {
                        RefKind::Call => breakdown.call += 1,
                        RefKind::Read => breakdown.read += 1,
                        RefKind::Write => breakdown.write += 1,
                        RefKind::Import => breakdown.import += 1,
                        RefKind::TypeUse => breakdown.type_use += 1,
                        _ => breakdown.other += 1,
                    }
                    files.insert(file.display().to_string());
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Lightweight classification for impact analysis (doesn't read full files
/// a second time — operates on already-parsed tree).
fn classify_usage(node: &tree_sitter::Node, source: &str) -> RefKind {
    let mut cur = *node;

    // Check parent for call expression.
    if let Some(parent) = cur.parent() {
        if parent.kind() == "call_expression" {
            let callee_name = first_child_simple_id(&parent, source);
            if callee_name == node.utf8_text(source.as_bytes()).unwrap_or("") {
                return RefKind::Call;
            }
        }
        if parent.kind() == "navigation_expression" {
            if let Some(gp) = parent.parent() {
                if gp.kind() == "call_expression" {
                    return RefKind::Call;
                }
            }
        }
    }

    // Walk up for context.
    loop {
        match cur.kind() {
            "import_header" | "import_declaration" => return RefKind::Import,
            "user_type" | "type_identifier" | "superclass" | "super_interfaces"
            | "type_arguments" => return RefKind::TypeUse,
            "assignment" => {
                if is_on_lhs(&cur, node) {
                    return RefKind::Write;
                }
                return RefKind::Read;
            }
            "property_declaration"
            | "function_declaration"
            | "method_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "object_declaration" => {
                return RefKind::Declaration;
            }
            "source_file" | "program" => break,
            _ => {}
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }

    RefKind::Reference
}

fn first_child_simple_id(node: &tree_sitter::Node, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

fn is_on_lhs(assignment: &tree_sitter::Node, inner: &tree_sitter::Node) -> bool {
    let mut cursor = assignment.walk();
    for child in assignment.children(&mut cursor) {
        if child.kind() == "eq" || child.kind() == "EQ" {
            return inner.end_position().column <= child.start_position().column;
        }
    }
    false
}

// ── Caller finding ──────────────────────────────────────────────────────────

fn find_direct_callers(name: &str, _index: &Arc<Indexer>, project_root: &Path) -> Vec<CallerInfo> {
    let mut callers = Vec::new();
    let escaped = crate::rg::regex_escape(name);
    let candidates = find_files_containing_name(&escaped, project_root);

    for candidate_file in &candidates {
        if let Ok(content) = std::fs::read_to_string(candidate_file) {
            let lang = crate::Language::from_path(candidate_file.to_str().unwrap_or(""));
            let mut parser = tree_sitter::Parser::new();
            let ts_lang = match lang {
                crate::Language::Kotlin => tree_sitter_kotlin::LANGUAGE.into(),
                crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
                crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
            };
            if parser.set_language(&ts_lang).is_err() {
                continue;
            }
            if let Some(tree) = parser.parse(&content, None) {
                find_caller_funcs(
                    name,
                    &tree.root_node(),
                    candidate_file,
                    &content,
                    &mut callers,
                );
            }
        }
    }

    callers
}

fn find_caller_funcs(
    name: &str,
    root: &tree_sitter::Node,
    file: &Path,
    source: &str,
    callers: &mut Vec<CallerInfo>,
) {
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            let callee = first_child_simple_id(&node, source);
            if callee == name {
                // Walk up to find the enclosing function.
                if let Some((fn_name, fn_line)) = find_containing_function(&node, source) {
                    if !callers
                        .iter()
                        .any(|c| c.name == fn_name && c.line == fn_line)
                    {
                        callers.push(CallerInfo {
                            name: fn_name,
                            file: file.display().to_string(),
                            line: fn_line,
                        });
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn find_containing_function(node: &tree_sitter::Node, source: &str) -> Option<(String, u32)> {
    let mut cur = *node;
    loop {
        match cur.kind() {
            "function_declaration" | "method_declaration" | "constructor_declaration" => {
                let start = cur.start_position();
                let name = first_child_simple_id(&cur, source);
                return Some((name, start.row as u32 + 1));
            }
            "source_file" | "program" => return None,
            _ => {
                let p = cur.parent()?;
                cur = p
            }
        }
    }
}

// ── Risk computation ────────────────────────────────────────────────────────

fn compute_risk(
    total_refs: usize,
    breakdown: &RefBreakdown,
    caller_count: usize,
) -> (String, String) {
    // Heuristic:
    // - 0 refs → low (unused / only declaration)
    // - 1-5 refs → low
    // - 6-50 refs → medium
    // - >50 refs → high
    // - >10 callers → bumps one level
    let mut parts = Vec::new();

    let base_risk = match total_refs {
        0..=1 => "low",
        2..=5 => "low",
        6..=50 => "medium",
        _ => "high",
    };

    let effective_risk = if caller_count > 10 && base_risk != "high" {
        if base_risk == "low" {
            "medium"
        } else {
            "high"
        }
    } else {
        base_risk
    };

    parts.push(format!("{total_refs} total references"));
    parts.push(format!("{caller_count} direct callers"));

    if breakdown.call > 0 {
        parts.push(format!("{} call sites", breakdown.call));
    }
    if breakdown.write > 0 {
        parts.push(format!("{} write sites", breakdown.write));
    }
    if breakdown.import > 0 {
        parts.push(format!("{} imports", breakdown.import));
    }

    (effective_risk.to_string(), parts.join(", "))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn extract_word_at_position(index: &Arc<Indexer>, uri: &Url, line: u32, col: u32) -> String {
    let lines = index.mem_lines_for(uri.as_str());
    lines
        .as_ref()
        .and_then(|l| {
            let li = line.saturating_sub(1) as usize;
            l.get(li).map(|ln| {
                crate::StrExt::word_at_utf16_col(ln.as_str(), col.saturating_sub(1) as usize)
            })
        })
        .unwrap_or_default()
}

fn guess_symbol_kind(index: &Arc<Indexer>, uri: &Url) -> String {
    // Just a rough guess from the file content.
    let lines = index.mem_lines_for(uri.as_str());
    lines
        .as_ref()
        .and_then(|lines| {
            lines.iter().find_map(|line| {
                if line.trim().starts_with("class ") {
                    Some("class".to_string())
                } else if line.trim().starts_with("interface ") {
                    Some("interface".to_string())
                } else if line.trim().starts_with("object ") {
                    Some("object".to_string())
                } else if line.trim().starts_with("fun ") || line.trim().starts_with("suspend fun ")
                {
                    Some("function".to_string())
                } else if line.trim().starts_with("val ") || line.trim().starts_with("var ") {
                    Some("property".to_string())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn find_files_containing_name(escaped_pattern: &str, root: &Path) -> Vec<PathBuf> {
    use std::process::Command;

    let mut cmd = Command::new("rg");
    cmd.args(["--files-with-matches", "--max-count", "50"]);
    for ext in crate::rg::SOURCE_EXTENSIONS {
        cmd.args(["--glob", &format!("*.{ext}")]);
    }
    cmd.args(["-e", escaped_pattern]);
    cmd.arg(root);

    let out = match cmd.output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(PathBuf::from)
        .collect()
}

// ── Output ──────────────────────────────────────────────────────────────────

fn print_report(report: &ImpactReport) {
    println!("Impact Report for `{}`", report.symbol);
    println!("  Kind: {}", report.kind);
    println!("  Location: {}:{}:{}", report.file, report.line, report.col);
    println!("  Risk: {} ({})", report.risk, report.risk_rationale);
    println!();
    println!("  References: {}", report.direct_refs);
    println!("    Calls:      {}", report.ref_breakdown.call);
    println!("    Reads:      {}", report.ref_breakdown.read);
    println!("    Writes:     {}", report.ref_breakdown.write);
    println!("    Imports:    {}", report.ref_breakdown.import);
    println!("    Type uses:  {}", report.ref_breakdown.type_use);
    println!("    Other:      {}", report.ref_breakdown.other);
    if !report.direct_callers.is_empty() {
        println!();
        println!("  Direct Callers:");
        for caller in &report.direct_callers {
            println!("    - {} @ {}:{}", caller.name, caller.file, caller.line);
        }
    }
}
