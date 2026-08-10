//! Function-body step extraction (Kotlin) — mirrors calldiff's
//! `languages/kotlin.ts` CallStep model: each function yields an ordered list
//! of body steps, where a step is either a *call* (`call_expression`) or a
//! *branch* (`if`/`else if`/`else`, `try`/`catch`/`finally`, `when` entries)
//! with nested steps. Nested function/lambda bodies are NOT attributed to the
//! outer caller (calldiff CONTRACT rule), so calls inside a lambda belong to
//! no named function and are dropped.
//!
//! Used by `call diff` to build branch-aware call trees. This is a separate
//! lightweight extractor on purpose: it does not touch the workspace
//! `call_edges` index (function-level edges for `call hierarchy` / `reach`).

use tree_sitter::Node;

use crate::queries::{
    KIND_ANON_FUN, KIND_ANON_INIT, KIND_CALL_EXPR, KIND_CATCH_BLOCK, KIND_CLASS_BODY,
    KIND_CLASS_DECL, KIND_COMPANION_OBJ, KIND_CONTROL_STRUCTURE_BODY, KIND_FINALLY_BLOCK,
    KIND_FUN_BODY, KIND_FUN_DECL, KIND_FUN_VALUE_PARAMS, KIND_IF_EXPR, KIND_LAMBDA_LIT,
    KIND_MODIFIERS, KIND_NAV_EXPR, KIND_NAV_SUFFIX, KIND_OBJECT_DECL, KIND_PARAMETER,
    KIND_SECONDARY_CTOR, KIND_SIMPLE_IDENT, KIND_STATEMENTS, KIND_THIS_EXPR, KIND_TRY_EXPR,
    KIND_TYPE_IDENT, KIND_WHEN_CONDITION, KIND_WHEN_ENTRY, KIND_WHEN_EXPR,
};

// ── data model ───────────────────────────────────────────────────────────────

/// One ordered body step: a call, or a branch with nested steps.
#[derive(Debug, Clone)]
pub(crate) enum CallStep {
    Call {
        /// Callee identity key: `name`, `Class.method`, `new X`, or `obj.prop`.
        key: String,
        /// 1-based call-site line.
        line: u32,
    },
    Branch {
        /// Display label + identity for diffing, e.g. `if x > 0`, `else`,
        /// `catch E`, `try` (the label embeds the branch condition, so it is
        /// stable enough for the LCS tree diff).
        label: String,
        /// 1-based branch-keyword/condition line.
        line: u32,
        children: Vec<CallStep>,
    },
}

/// A named callable with its ordered body steps.
#[derive(Debug, Clone)]
pub(crate) struct FuncInfo {
    /// `name` (top-level) or `ClassName.method`.
    pub(crate) key: String,
    /// File the function was extracted from (filled by the caller; the
    /// extractor itself is single-file and leaves it empty).
    pub(crate) file: String,
    /// Display label with params, e.g. `foo(a, b)`.
    pub(crate) label: String,
    pub(crate) exported: bool,
    /// 1-based definition line.
    pub(crate) line: u32,
    pub(crate) steps: Vec<CallStep>,
}

// ── CST helpers ──────────────────────────────────────────────────────────────

fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            out.push(child);
        }
    }
    out
}

fn child_by_type<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    named_children(node).into_iter().find(|c| c.kind() == kind)
}

/// Collapse runs of whitespace (incl. newlines) to a single space.
fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut ws_run = false;
    for c in text.chars() {
        if c.is_whitespace() {
            ws_run = true;
        } else {
            if ws_run && !out.is_empty() {
                out.push(' ');
            }
            ws_run = false;
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn source_text<'a>(node: &Node<'a>, source: &'a str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_string()
}

fn line_of(node: &Node) -> u32 {
    u32::try_from(node.start_position().row)
        .unwrap_or(0)
        .saturating_add(1)
}

fn is_private(node: &Node, source: &str) -> bool {
    let Some(mods) = child_by_type(node, KIND_MODIFIERS) else {
        return false;
    };
    source_text(&mods, source)
        .split_whitespace()
        .any(|w| w == "private")
}

fn is_likely_type_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    first.is_uppercase()
}

fn params_label(params: Option<&Node>, source: &str) -> String {
    let Some(params) = params else {
        return "()".to_string();
    };
    let mut names: Vec<String> = Vec::new();
    for p in named_children(params) {
        if p.kind() == KIND_PARAMETER {
            let id = child_by_type(&p, KIND_SIMPLE_IDENT);
            names.push(
                id.map(|n| source_text(&n, source))
                    .unwrap_or_else(|| "_".to_string()),
            );
        }
    }
    if names.is_empty() {
        "()".to_string()
    } else {
        format!("({})", names.join(", "))
    }
}

fn navigation_prop(nav: &Node, source: &str) -> Option<String> {
    let suffix = child_by_type(nav, KIND_NAV_SUFFIX)?;
    let id = child_by_type(&suffix, KIND_SIMPLE_IDENT)?;
    Some(source_text(&id, source))
}

/// Callee identity for a call target node (calldiff `calleeKey`).
fn callee_key(node: &Node, class_name: Option<&str>, source: &str) -> Option<String> {
    match node.kind() {
        KIND_SIMPLE_IDENT => {
            let name = source_text(node, source);
            if is_likely_type_name(&name) {
                Some(format!("new {name}"))
            } else {
                Some(name)
            }
        }
        KIND_NAV_EXPR => {
            let kids = named_children(node);
            let object = kids.first()?;
            let prop = navigation_prop(node, source)?;
            if object.kind() == KIND_THIS_EXPR {
                return class_name.map(|c| format!("{c}.{prop}"));
            }
            if object.kind() == KIND_SIMPLE_IDENT {
                return Some(format!("{}.{prop}", source_text(object, source)));
            }
            if let Some(c) = class_name {
                return Some(format!("{c}.{prop}"));
            }
            Some(prop)
        }
        _ => None,
    }
}

// ── step collection ──────────────────────────────────────────────────────────

fn statements_of<'a>(body: Option<&Node<'a>>, _source: &'a str) -> Vec<Node<'a>> {
    let Some(body) = body else {
        return Vec::new();
    };
    let kinds = [KIND_FUN_BODY, KIND_CONTROL_STRUCTURE_BODY];
    if kinds.contains(&body.kind()) {
        if let Some(stmts) = child_by_type(body, KIND_STATEMENTS) {
            return named_children(&stmts);
        }
        // empty `{}` body — or a bare nested if_expression
        return named_children(body)
            .into_iter()
            .filter(|c| c.kind() != KIND_STATEMENTS)
            .collect();
    }
    if body.kind() == KIND_STATEMENTS {
        return named_children(body);
    }
    vec![*body]
}

fn collect_statements(
    statements: &[Node],
    class_name: Option<&str>,
    source: &str,
) -> Vec<CallStep> {
    let mut steps: Vec<CallStep> = Vec::new();
    let mut seen: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();

    // ── if / else-if / else chain ───────────────────────────────────────────
    fn push_if_chain(
        node: &Node,
        as_else_if: bool,
        class_name: Option<&str>,
        source: &str,
        steps: &mut Vec<CallStep>,
    ) {
        let consequence = node.child_by_field_name("consequence");
        let alternative = node.child_by_field_name("alternative");
        let cond = node.child_by_field_name("condition");
        let cond_text = cond
            .map(|c| collapse_ws(&source_text(&c, source)))
            .filter(|t| !t.is_empty());
        let label = match (as_else_if, cond_text) {
            (true, Some(text)) => format!("else if {text}"),
            (true, None) => "else if".to_string(),
            (false, Some(text)) => format!("if {text}"),
            (false, None) => "if".to_string(),
        };
        let line = cond.map(|c| line_of(&c)).unwrap_or_else(|| line_of(node));
        let children = consequence
            .map(|b| collect_statements(&statements_of(Some(&b), source), class_name, source))
            .unwrap_or_default();
        steps.push(CallStep::Branch {
            label,
            line,
            children,
        });

        let Some(alt) = alternative else { return };
        let alt_stmts = statements_of(Some(&alt), source);
        if alt_stmts.len() == 1 && alt_stmts[0].kind() == KIND_IF_EXPR {
            push_if_chain(&alt_stmts[0], true, class_name, source, steps);
            return;
        }
        steps.push(CallStep::Branch {
            label: "else".to_string(),
            line: line_of(&alt),
            children: collect_statements(&alt_stmts, class_name, source),
        });
    }

    // ── recursive walk ─────────────────────────────────────────────────────
    fn walk(
        node: &Node,
        class_name: Option<&str>,
        source: &str,
        steps: &mut Vec<CallStep>,
        seen: &mut std::collections::HashSet<(String, usize)>,
    ) {
        // Boundaries: do not descend into nested declarations / lambdas.
        if matches!(
            node.kind(),
            KIND_FUN_DECL
                | KIND_CLASS_DECL
                | KIND_OBJECT_DECL
                | KIND_COMPANION_OBJ
                | KIND_ANON_FUN
                | KIND_LAMBDA_LIT
                | KIND_SECONDARY_CTOR
                | KIND_ANON_INIT
        ) {
            return;
        }

        match node.kind() {
            KIND_IF_EXPR => {
                push_if_chain(node, false, class_name, source, steps);
                return;
            }
            KIND_TRY_EXPR => {
                // `try` body: statements child of the block.
                let try_stmts = child_by_type(node, KIND_STATEMENTS)
                    .map(|s| named_children(&s))
                    .unwrap_or_default();
                steps.push(CallStep::Branch {
                    label: "try".to_string(),
                    line: line_of(node),
                    children: collect_statements(&try_stmts, class_name, source),
                });
                for clause in named_children(node) {
                    if clause.kind() == KIND_CATCH_BLOCK {
                        let kids = named_children(&clause);
                        let type_text = kids
                            .iter()
                            .find(|c| {
                                c.kind() != KIND_SIMPLE_IDENT
                                    && c.kind() != KIND_STATEMENTS
                                    && c.is_named()
                            })
                            .map(|c| collapse_ws(&source_text(c, source)))
                            .filter(|t| !t.is_empty());
                        let body = child_by_type(&clause, KIND_STATEMENTS)
                            .map(|s| named_children(&s))
                            .unwrap_or_default();
                        let label = match type_text {
                            Some(t) => format!("catch {t}"),
                            None => "catch".to_string(),
                        };
                        steps.push(CallStep::Branch {
                            label,
                            line: line_of(&clause),
                            children: collect_statements(&body, class_name, source),
                        });
                    } else if clause.kind() == KIND_FINALLY_BLOCK {
                        let body = child_by_type(&clause, KIND_STATEMENTS)
                            .map(|s| named_children(&s))
                            .unwrap_or_default();
                        steps.push(CallStep::Branch {
                            label: "finally".to_string(),
                            line: line_of(&clause),
                            children: collect_statements(&body, class_name, source),
                        });
                    }
                }
                return;
            }
            KIND_WHEN_EXPR => {
                for entry in named_children(node) {
                    if entry.kind() != KIND_WHEN_ENTRY {
                        continue;
                    }
                    let cond = child_by_type(&entry, KIND_WHEN_CONDITION);
                    let body = child_by_type(&entry, KIND_CONTROL_STRUCTURE_BODY);
                    let children = body
                        .map(|b| {
                            collect_statements(&statements_of(Some(&b), source), class_name, source)
                        })
                        .unwrap_or_default();
                    match cond {
                        Some(c) => {
                            let text = collapse_ws(&source_text(&c, source));
                            steps.push(CallStep::Branch {
                                label: format!("case {text}"),
                                line: line_of(&c),
                                children,
                            });
                        }
                        None => {
                            steps.push(CallStep::Branch {
                                label: "else".to_string(),
                                line: line_of(&entry),
                                children,
                            });
                        }
                    }
                }
                return;
            }
            KIND_CALL_EXPR => {
                if let Some(callee) = node.named_child(0) {
                    if let Some(key) = callee_key(&callee, class_name, source) {
                        let mark = (key.clone(), node.start_byte());
                        if !seen.contains(&mark) {
                            seen.insert(mark);
                            steps.push(CallStep::Call {
                                key,
                                line: line_of(node),
                            });
                        }
                    }
                }
                // Walk remaining children for nested calls, e.g. a(b()).
                let kids = named_children(node);
                for child in kids.iter().skip(1) {
                    walk(child, class_name, source, steps, seen);
                }
                return;
            }
            _ => {}
        }

        for child in named_children(node) {
            walk(&child, class_name, source, steps, seen);
        }
    }

    for stmt in statements {
        walk(stmt, class_name, source, &mut steps, &mut seen);
    }
    steps
}

// ── declaration handling ─────────────────────────────────────────────────────

fn handle_function(
    node: &Node,
    class_name: Option<&str>,
    source: &str,
    functions: &mut Vec<FuncInfo>,
) {
    let Some(name_node) = child_by_type(node, KIND_SIMPLE_IDENT) else {
        return;
    };
    let name = source_text(&name_node, source);
    let params = child_by_type(node, KIND_FUN_VALUE_PARAMS);
    let body = child_by_type(node, KIND_FUN_BODY);
    let key = match class_name {
        Some(c) => format!("{c}.{name}"),
        None => name.clone(),
    };
    let label = format!("{key}{}", params_label(params.as_ref(), source));
    functions.push(FuncInfo {
        key,
        label: label.clone(),
        file: String::new(),
        exported: !is_private(node, source),
        line: line_of(&name_node),
        steps: collect_statements(&statements_of(body.as_ref(), source), class_name, source),
    });
}

fn handle_class(node: &Node, source: &str, functions: &mut Vec<FuncInfo>) {
    let Some(name_node) = child_by_type(node, KIND_TYPE_IDENT) else {
        return;
    };
    let class_name = source_text(&name_node, source);
    let Some(body) = child_by_type(node, KIND_CLASS_BODY) else {
        return;
    };
    for element in named_children(&body) {
        match element.kind() {
            KIND_FUN_DECL => handle_function(&element, Some(&class_name), source, functions),
            KIND_SECONDARY_CTOR => {
                let params = child_by_type(&element, KIND_FUN_VALUE_PARAMS);
                let stmts = child_by_type(&element, KIND_STATEMENTS)
                    .map(|s| named_children(&s))
                    .unwrap_or_default();
                functions.push(FuncInfo {
                    key: format!("{class_name}.constructor"),
                    label: format!("{class_name}{}", params_label(params.as_ref(), source)),
                    file: String::new(),
                    exported: !is_private(&element, source),
                    line: line_of(&element),
                    steps: collect_statements(&stmts, Some(&class_name), source),
                });
            }
            KIND_ANON_INIT => {
                let stmts = child_by_type(&element, KIND_STATEMENTS)
                    .map(|s| named_children(&s))
                    .unwrap_or_default();
                let info = FuncInfo {
                    key: format!("{class_name}.init"),
                    label: format!("{class_name}()"),
                    file: String::new(),
                    exported: true,
                    line: line_of(&element),
                    steps: collect_statements(&stmts, Some(&class_name), source),
                };
                functions.push(info);
            }
            KIND_CLASS_DECL => handle_class(&element, source, functions),
            KIND_COMPANION_OBJ | KIND_OBJECT_DECL => {
                handle_object_like(&element, Some(&class_name), source, functions)
            }
            _ => {}
        }
    }
}

fn handle_object_like(
    node: &Node,
    parent_class: Option<&str>,
    source: &str,
    functions: &mut Vec<FuncInfo>,
) {
    let object_name = child_by_type(node, KIND_TYPE_IDENT)
        .or_else(|| child_by_type(node, KIND_SIMPLE_IDENT))
        .map(|n| source_text(&n, source));
    let type_name = if node.kind() == KIND_COMPANION_OBJ {
        parent_class.or(object_name.as_deref())
    } else {
        object_name.as_deref().or(parent_class)
    };
    let Some(type_name) = type_name else {
        return;
    };
    let Some(body) = child_by_type(node, KIND_CLASS_BODY) else {
        return;
    };
    for element in named_children(&body) {
        if element.kind() == KIND_FUN_DECL {
            handle_function(&element, Some(type_name), source, functions);
        }
    }
}

// ── entry point ──────────────────────────────────────────────────────────────

/// Extract named functions (top-level, class methods, object/companion
/// methods) with their body steps from Kotlin source. Kotlin only.
pub(crate) fn extract_functions(source: &str) -> Vec<FuncInfo> {
    let mut parser = tree_sitter::Parser::new();
    let _ = parser.set_language(&tree_sitter_kotlin_sg::LANGUAGE.into());
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut functions: Vec<FuncInfo> = Vec::new();
    for stmt in named_children(&root) {
        match stmt.kind() {
            KIND_FUN_DECL => handle_function(&stmt, None, source, &mut functions),
            KIND_CLASS_DECL => handle_class(&stmt, source, &mut functions),
            KIND_OBJECT_DECL => handle_object_like(&stmt, None, source, &mut functions),
            _ => {}
        }
    }
    functions
}
