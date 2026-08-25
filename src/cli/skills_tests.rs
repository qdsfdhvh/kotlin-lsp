//! Tests for `cli::skills` — `kotlin-lsp skills list|read`.

use super::skills;

/// Normalize CRLF to LF so content assertions hold regardless of how the
/// runner checked out the repo (Windows CI converts LF→CRLF on checkout,
/// and `include_str!` embeds the on-disk bytes).
fn lf(content: &str) -> String {
    content.replace("\r\n", "\n")
}

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
    let content = lf(&skills::format_read("kotlin-lsp").unwrap());
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
    let content = lf(&skills::format_read("kotlin-lsp").unwrap());
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

// ── Bundled references consistency ──────────────────────────────────────────

#[test]
fn every_references_file_is_cited_by_skill() {
    // Every `references/*.md` shipped next to SKILL.md must be cited by
    // SKILL.md itself. An uncited reference file is invisible to agents and
    // rots silently (#326: references/indexing.md was unreachable while
    // SKILL.md linked out to a GitHub URL instead of the local file).
    let skill_dir = std::path::Path::new("skills/kotlin-lsp");
    let references_dir = skill_dir.join("references");
    let skill = std::fs::read_to_string(skill_dir.join("SKILL.md"))
        .expect("read skills/kotlin-lsp/SKILL.md (tests run from crate root)");

    let mut checked = 0;
    for entry in std::fs::read_dir(&references_dir)
        .unwrap_or_else(|e| panic!("read skills/kotlin-lsp/references: {e}"))
    {
        let path = entry.expect("read_dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        checked += 1;
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let cite = format!("references/{file_name}");
        assert!(
            skill.contains(&cite),
            "SKILL.md must cite '{cite}' — the file at \
             skills/kotlin-lsp/references/{file_name} is unreachable to agents"
        );
    }
    assert!(
        checked > 0,
        "expected at least one references/*.md to check"
    );
}

#[test]
fn read_output_is_utf8_and_printable() {
    // Normalize CRLF first: `\r` is a control character, so the check below
    // would flag every line of a CRLF checkout (Windows CI).
    let content = lf(&skills::format_read("kotlin-lsp").unwrap());
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
