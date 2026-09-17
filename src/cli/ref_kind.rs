//! Reference classification — classify references by their usage (call, read, write, etc.).
//!
//! Uses tree-sitter to examine the CST context of each reference location to determine
//! whether it's a function call, field read, field write, override, import, or type use.

use crate::queries::*;
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
/// Locations use zero-based UTF-16 columns.
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
        crate::Language::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
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

    // A zero-width lookup at a Swift statement boundary can select the enclosing
    // statements node. Include the first character to identify this occurrence.
    let width = line_text
        .get(byte_col..)
        .and_then(|text| text.chars().next())
        .map_or(0, char::len_utf8);
    let end = tree_sitter::Point::new(line, byte_col + width);
    let Some(start_node) = tree.root_node().descendant_for_point_range(point, end) else {
        return RefKind::Reference;
    };

    classify_node(&start_node, name, &source)
}

/// Given a tree-sitter node at the reference position, determine the reference kind.
fn classify_node(node: &tree_sitter::Node<'_>, _name: &str, source: &str) -> RefKind {
    // Kotlin soft keywords (e.g. `field`) have an anonymous token inside the
    // identifier. Point lookup reaches that token; identity checks need its wrapper.
    let identifier = node.parent().filter(|parent| {
        !node.is_named() && matches!(parent.kind(), KIND_SIMPLE_IDENT | KIND_IDENTIFIER)
    });
    let node = identifier.as_ref().unwrap_or(node);

    // Import aliases can be type_identifier leaves; the enclosing import takes
    // precedence over both type-use and other identifier contexts.
    let mut ancestor = Some(*node);
    while let Some(candidate) = ancestor {
        if matches!(candidate.kind(), KIND_IMPORT_HEADER | KIND_IMPORT_DECL) {
            return RefKind::Import;
        }
        ancestor = candidate.parent();
    }

    // Kotlin class names are type_identifier nodes too; declaration identity
    // takes precedence over the generic type-use context.
    if let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            KIND_CLASS_DECL | KIND_INTERFACE_DECL | KIND_OBJECT_DECL | KIND_ENUM_DECL
        ) && declaration_name(&parent).is_some_and(|name| name.id() == node.id())
        {
            return RefKind::Declaration;
        }
    }
    let mut cur = *node;

    // Compare node identity, not just spelling: a receiver/argument with the
    // same name as the callee is still a read.
    let mut ancestor = Some(*node);
    while let Some(candidate) = ancestor {
        if matches!(candidate.kind(), KIND_CALL_EXPR | KIND_METHOD_INVOCATION) {
            let callee = if candidate.kind() == KIND_METHOD_INVOCATION {
                candidate.child_by_field_name("name")
            } else {
                candidate.named_child(0).and_then(last_identifier)
            };
            if callee.is_some_and(|callee| callee.id() == node.id()) {
                return RefKind::Call;
            }
            break;
        }
        if matches!(candidate.kind(), KIND_FUN_DECL | KIND_FUN_BODY | KIND_BLOCK) {
            break;
        }
        ancestor = candidate.parent();
    }

    // Check for prefix/postfix increment/decrement → write
    if let Some(parent) = node.parent() {
        if parent.kind() == KIND_POSTFIX_EXPR || parent.kind() == KIND_PREFIX_EXPR {
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

    // Walk up to find the usage context
    loop {
        match cur.kind() {
            // Inside import → import reference
            KIND_IMPORT_HEADER | KIND_IMPORT_DECL => return RefKind::Import,

            // Inside type annotation / supertype list → type use
            KIND_USER_TYPE
            | KIND_TYPE_IDENT
            | KIND_SUPERCLASS
            | KIND_SUPER_INTERFACES
            | KIND_TYPE_ARGS
            | KIND_TYPE_PROJECTION
            | KIND_FUNCTION_TYPE
            | KIND_NULLABLE_TYPE
            | KIND_TYPE_PARAM => {
                return RefKind::TypeUse;
            }

            // Inside an assignment expression where this is the target → write
            KIND_ASSIGNMENT | KIND_ASSIGNMENT_EXPR => {
                if is_assignment_target(&cur, node) {
                    return RefKind::Write;
                }
                return RefKind::Read;
            }

            // Only the declaration's name is a declaration. Walking up from
            // an initializer or function body must not label its reads as one.
            KIND_FUN_DECL | KIND_METHOD_DECL | KIND_PROP_DECL | KIND_CLASS_DECL
            | KIND_INTERFACE_DECL | KIND_OBJECT_DECL | KIND_ENUM_DECL | KIND_PARAMETER
            | KIND_CLASS_PARAM | KIND_FORMAL_PARAM | KIND_VAR_DECLARATOR => {
                let declared = declaration_name(&cur);
                if declared.is_some_and(|declared| declared.id() == node.id()) {
                    return if has_modifier(&cur, source, "override") {
                        RefKind::Override
                    } else {
                        RefKind::Declaration
                    };
                }
                return RefKind::Read;
            }

            KIND_SOURCE_FILE | KIND_PROGRAM => break,
            _ => {}
        }

        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }

    RefKind::Read
}

fn declaration_name<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    node.child_by_field_name("name")
        .map(|name| {
            // Swift property names are patterns wrapping the actual bound identifier.
            name.child_by_field_name("bound_identifier").unwrap_or(name)
        })
        .or_else(|| {
            children(node).into_iter().find_map(|child| {
                if matches!(
                    child.kind(),
                    KIND_SIMPLE_IDENT | KIND_IDENTIFIER | KIND_TYPE_IDENT
                ) {
                    Some(child)
                } else if matches!(child.kind(), KIND_VAR_DECL | KIND_VAR_DECLARATOR) {
                    child.named_child(0)
                } else {
                    None
                }
            })
        })
}

/// Java exposes a `left` field; Kotlin's first named child is its target.
/// Neither depends on the spelling of the assignment operator (`=`, `+=`, ...).
fn is_assignment_target(assignment: &tree_sitter::Node<'_>, inner: &tree_sitter::Node<'_>) -> bool {
    assignment
        .child_by_field_name("left")
        .or_else(|| assignment.named_child(0))
        .and_then(assigned_identifier)
        .is_some_and(|target| target.id() == inner.id())
}

/// Select only the assigned identifier, never identifiers evaluated to reach it.
/// Kotlin's assignable wrapper can contain a flat sequence of navigation/index
/// suffixes: only its final suffix matters. An index target assigns an element,
/// not the array or any identifier inside the index, so it yields no identifier.
fn assigned_identifier(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        KIND_SIMPLE_IDENT | KIND_IDENTIFIER => Some(node),
        KIND_FIELD_ACCESS => node.child_by_field_name("field"),
        KIND_DIRECTLY_ASSIGNABLE_EXPR | KIND_NAV_EXPR | KIND_NAV_SUFFIX => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .last()
                .and_then(assigned_identifier)
        }
        _ => None,
    }
}

/// Check if a declaration node has a specific modifier keyword.
fn has_modifier(decl: &tree_sitter::Node<'_>, source: &str, modifier: &str) -> bool {
    for child in children(decl) {
        if child.kind() == KIND_MODIFIERS {
            let text = &source[child.start_byte()..child.end_byte()];
            return text.contains(modifier);
        }
    }
    false
}

/// The terminal identifier in a callee, including a navigation suffix.
fn last_identifier(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if matches!(
        node.kind(),
        KIND_SIMPLE_IDENT | KIND_IDENTIFIER | KIND_TYPE_IDENT
    ) {
        return Some(node);
    }
    if matches!(node.kind(), KIND_NAV_EXPR | KIND_NAV_SUFFIX) {
        return children(&node).into_iter().rev().find_map(last_identifier);
    }
    None
}

/// Collect children into a Vec (borrowed).
fn children<'a>(node: &tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

#[cfg(test)]
#[path = "ref_kind_tests.rs"]
mod tests;
