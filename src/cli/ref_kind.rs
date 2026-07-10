//! Reference classification — classify references by their usage (call, read, write, etc.).
//!
//! Uses tree-sitter to examine the CST context of each reference location to determine
//! whether it's a function call, field read, field write, override, import, or type use.

use tower_lsp::lsp_types::Location;

/// Supported reference kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefKind {
    Call,
    Read,
    Write,
    Override,
    Import,
    TypeUse,
    Declaration,
    Reference,
}

impl RefKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RefKind::Call => "call",
            RefKind::Read => "read",
            RefKind::Write => "write",
            RefKind::Override => "override",
            RefKind::Import => "import",
            RefKind::TypeUse => "type-use",
            RefKind::Declaration => "declaration",
            RefKind::Reference => "reference",
        }
    }

    /// Parse from a CLI `--ref-kind` value. Returns None for invalid values.
    pub(crate) fn from_arg(s: &str) -> Option<Self> {
        match s {
            "call" => Some(RefKind::Call),
            "read" => Some(RefKind::Read),
            "write" => Some(RefKind::Write),
            "override" => Some(RefKind::Override),
            "import" => Some(RefKind::Import),
            "type-use" => Some(RefKind::TypeUse),
            "declaration" => Some(RefKind::Declaration),
            "all" | "reference" => None, // "all" means no filter
            _ => None,
        }
    }
}

/// Classify a reference at a given location using tree-sitter.
///
/// Returns the reference kind and also mutates `kind_out` to the classified kind string.
pub(crate) fn classify_reference(loc: &Location, name: &str) -> RefKind {
    let file_path = match loc.uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return RefKind::Reference,
    };

    let source = match std::fs::read_to_string(&file_path) {
        Ok(s) => s,
        Err(_) => return RefKind::Reference,
    };

    let lang = crate::Language::from_path(file_path.to_str().unwrap_or(""));
    let mut parser = tree_sitter::Parser::new();
    let ts_lang = match lang {
        crate::Language::Kotlin => tree_sitter_kotlin::LANGUAGE.into(),
        crate::Language::Java => tree_sitter_java::LANGUAGE.into(),
        crate::Language::Swift => tree_sitter_swift::LANGUAGE.into(),
    };
    if parser.set_language(&ts_lang).is_err() {
        return RefKind::Reference;
    }

    let tree = match parser.parse(&source, None) {
        Some(t) => t,
        None => return RefKind::Reference,
    };

    let line = loc.range.start.line as usize;
    let col = loc.range.start.character as usize;

    // Find the line's text to go from character offset to byte offset
    let line_text = match source.lines().nth(line) {
        Some(lt) => lt,
        None => return RefKind::Reference,
    };
    let byte_col = crate::indexer::live_tree::utf16_col_to_byte(line_text, col);
    let point = tree_sitter::Point::new(line, byte_col);

    let Some(start_node) = tree.root_node().descendant_for_point_range(point, point) else {
        return RefKind::Reference;
    };

    classify_node(&start_node, name, &source)
}

/// Given a tree-sitter node at the reference position, determine the reference kind.
fn classify_node(node: &tree_sitter::Node<'_>, name: &str, source: &str) -> RefKind {
    // Walk up from the node to find the enclosing context.
    let mut cur = *node;

    // Check immediate parent
    if let Some(parent) = cur.parent() {
        // call_expression → this is a call site
        if parent.kind() == "call_expression" {
            // Make sure we're the callee, not an argument
            let callee_name = first_simple_identifier(&parent, source);
            if callee_name == name {
                return RefKind::Call;
            }
        }

        // navigation_expression inside call_expression → also a call site
        if parent.kind() == "navigation_expression" {
            if let Some(grandparent) = parent.parent() {
                if grandparent.kind() == "call_expression" {
                    let callee_name = last_simple_identifier(&parent, source);
                    if callee_name == name {
                        return RefKind::Call;
                    }
                }
            }
        }
    }

    // Walk up to find the usage context
    loop {
        match cur.kind() {
            // Inside import → import reference
            "import_header" | "import_declaration" => return RefKind::Import,

            // Inside type annotation / supertype list → type use
            "user_type" | "type_identifier" | "superclass" | "super_interfaces"
            | "type_arguments" | "type_projection" | "function_type" | "nullable_type"
            | "type_parameter" => {
                return RefKind::TypeUse;
            }

            // Inside an assignment expression where this is the target → write
            "assignment" => {
                // Check if this node is on the LHS
                if is_left_of_equals(&cur, node) {
                    return RefKind::Write;
                }
                return RefKind::Read;
            }

            // Function/method declaration → check for override modifier
            "function_declaration" | "method_declaration" => {
                if has_modifier(&cur, source, "override") {
                    return RefKind::Override;
                }
                return RefKind::Declaration;
            }

            // Property declaration
            "property_declaration" => {
                if has_modifier(&cur, source, "override") {
                    return RefKind::Override;
                }
                return RefKind::Declaration;
            }

            // Class/interface/object declaration → declaration
            "class_declaration"
            | "interface_declaration"
            | "object_declaration"
            | "enum_declaration" => {
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

    // Check for prefix/postfix increment/decrement → write
    if let Some(parent) = node.parent() {
        if parent.kind() == "postfix_expression" || parent.kind() == "prefix_expression" {
            // Check for ++/--
            if source
                .get(parent.start_byte()..parent.end_byte())
                .map(|s| s.contains("++") || s.contains("--"))
                .unwrap_or(false)
            {
                return RefKind::Write;
            }
        }
    }

    RefKind::Reference
}

/// Check if `inner` is to the left of the `=` in an assignment node.
#[allow(dead_code)]
fn is_left_of_equals(assignment: &tree_sitter::Node<'_>, inner: &tree_sitter::Node<'_>) -> bool {
    for child in children(assignment) {
        if child.kind() == "eq" || child.kind() == "EQ" {
            return inner.end_position().column <= child.start_position().column;
        }
    }
    false
}

/// Check if a declaration node has a specific modifier keyword.
#[allow(dead_code)]
fn has_modifier(decl: &tree_sitter::Node<'_>, source: &str, modifier: &str) -> bool {
    for child in children(decl) {
        if child.kind() == "modifiers" {
            let text = &source[child.start_byte()..child.end_byte()];
            return text.contains(modifier);
        }
    }
    false
}

/// Get the first simple_identifier child's text.
fn first_simple_identifier(node: &tree_sitter::Node<'_>, source: &str) -> String {
    for child in children(node) {
        if child.kind() == "simple_identifier" {
            return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Get the last simple_identifier child's text (for navigation_expression).
fn last_simple_identifier(node: &tree_sitter::Node<'_>, source: &str) -> String {
    let mut last = String::new();
    for child in children(node) {
        if child.kind() == "simple_identifier" {
            last = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    last
}

/// Collect children into a Vec (borrowed).
fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

#[cfg(test)]
#[path = "ref_kind_tests.rs"]
mod tests;
