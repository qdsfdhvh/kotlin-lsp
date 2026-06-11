//! Tests for `cli::skills` — `kotlin-lsp skills list|read`.

use super::skills;

// ── format_list ───────────────────────────────────────────────────────────────

#[test]
fn list_includes_kotlin_lsp_skill() {
    let out = skills::format_list();
    assert!(
        out.contains("kotlin-lsp"),
        "expected 'kotlin-lsp' in list output, got:\n{out}"
    );
}

#[test]
fn list_has_header() {
    let out = skills::format_list();
    assert!(out.starts_with("── bundled skills"));
}

#[test]
fn list_has_footer_hint() {
    let out = skills::format_list();
    assert!(out.contains("skills read <name>"));
}

// ── format_read ──────────────────────────────────────────────────────────────

#[test]
fn read_kotlin_lsp_skill_exists() {
    let result = skills::format_read("kotlin-lsp");
    assert!(result.is_ok(), "expected Ok, got Err: {:?}", result);
}

#[test]
fn read_kotlin_lsp_is_valid_markdown() {
    let content = skills::format_read("kotlin-lsp").unwrap();
    // Must have YAML frontmatter and body.
    assert!(
        content.starts_with("---\n"),
        "expected YAML frontmatter, got:\n{content}"
    );
    assert!(content.contains("name: kotlin-lsp"));
    assert!(content.contains("# kotlin-lsp"));
    // Must be long enough to be a meaningful skill document (≥ 2 KB).
    assert!(
        content.len() > 2000,
        "skill content too short: {} bytes",
        content.len()
    );
}

#[test]
fn read_kotlin_lsp_describes_cli_commands() {
    let content = skills::format_read("kotlin-lsp").unwrap();
    // The skill should document the major CLI commands.
    for cmd in &["find", "refs", "hover", "check", "code-action"] {
        assert!(content.contains(cmd), "expected '{cmd}' in skill content");
    }
}

// ── format_read error cases ──────────────────────────────────────────────────

#[test]
fn read_nonexistent_skill_returns_error() {
    let result = skills::format_read("nonexistent");
    match result {
        Err(msg) => {
            assert!(
                msg.contains("unknown skill"),
                "expected 'unknown skill' in error, got: {msg}"
            );
        }
        Ok(content) => panic!("expected error, got content:\n{content}"),
    }
}

#[test]
fn read_nonexistent_lists_available() {
    let result = skills::format_read("nonexistent");
    match result {
        Err(msg) => {
            assert!(
                msg.contains("kotlin-lsp"),
                "expected 'kotlin-lsp' in available-skills hint, got: {msg}"
            );
        }
        Ok(content) => panic!("expected error, got content:\n{content}"),
    }
}

// ── YAML frontmatter sanity ──────────────────────────────────────────────────

#[test]
fn all_skills_have_valid_frontmatter() {
    // Use builtin_skills() via format_read since builtin_skills is module-private.
    // Read kotlin-lsp and parse frontmatter lines.
    let content = skills::format_read("kotlin-lsp").unwrap();
    // Extract frontmatter: everything between first --- and second ---.
    let end = content
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n"))
        .expect("expected closing --- in frontmatter");
    let frontmatter = &content[..end];

    // Check common fields exist (by line prefix).
    assert!(
        frontmatter.contains("name:"),
        "missing name: in frontmatter"
    );
    assert!(
        frontmatter.contains("description:"),
        "missing description: in frontmatter"
    );
}

// ── Built-in skill invariants ────────────────────────────────────────────────

#[test]
fn read_output_is_utf8_and_printable() {
    let content = skills::format_read("kotlin-lsp").unwrap();
    // The content must be valid UTF-8 (already guaranteed by String).
    // Check it doesn't contain null bytes or control characters (aside from \n).
    for ch in content.chars() {
        if ch.is_control() && ch != '\n' {
            panic!(
                "unexpected control character U+{:04X} at position ~{}",
                ch as u32,
                content.find(ch).unwrap_or(0)
            );
        }
    }
}
