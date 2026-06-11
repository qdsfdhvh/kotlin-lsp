//! `kotlin-lsp skills` — list or read agent skills bundled with the CLI.
//!
//! Skills are agent-facing documents that teach AI coding agents how to use
//! kotlin-lsp efficiently. They ship with the CLI binary (embedded at compile
//! time via `include_str!`) so the agent always reads docs that match the
//! installed version exactly.
//!
//! Usage:
//!   kotlin-lsp skills list          — list available skill names + descriptions
//!   kotlin-lsp skills read <name>   — print the full SKILL.md for <name>

use std::collections::BTreeMap;

// ── Embedded skill manifests ──────────────────────────────────────────────────
//
// Each entry: the name used in `skills read <name>`, the display description,
// and the raw SKILL.md content embedded at compile-time.
//
// To add a new skill:
//   1. Create `skills/<name>/SKILL.md` in the repo root.
//   2. Add an entry below with `include_str!("../../skills/<name>/SKILL.md")`.

fn builtin_skills() -> BTreeMap<&'static str, SkillEntry> {
    BTreeMap::from([
        (
            "kotlin-lsp",
            SkillEntry {
                description: "Use the `kotlin-lsp` CLI for precise symbol lookup in Kotlin/Java/Swift projects — faster than grep/rg and returns typed answers (declarations, refs, signatures) instead of raw text matches. Saves tokens because results are scoped and structured.",
                content: include_str!("../../skills/kotlin-lsp/SKILL.md"),
            },
        ),
    ])
}

struct SkillEntry {
    /// Short description shown in `skills list`.
    description: &'static str,
    /// Full raw SKILL.md content shown in `skills read <name>`.
    content: &'static str,
}

/// Run `kotlin-lsp skills list` — print available skills.
fn run_list() {
    print!("{}", format_list());
}

/// Build the text that `skills list` prints.
pub(crate) fn format_list() -> String {
    let skills = builtin_skills();
    if skills.is_empty() {
        return "No skills bundled with this version.\n".to_string();
    }

    let max_w = skills.keys().map(|k| k.len()).max().unwrap_or(8);
    let mut out = String::from("── bundled skills ──────────────────────────────\n");
    for (name, entry) in &skills {
        let desc = if entry.description.len() > 72 {
            format!("{}…", &entry.description[..69])
        } else {
            entry.description.to_string()
        };
        out.push_str(&format!("  {name:>max_w$}  {desc}\n"));
    }
    out.push_str(&format!(
        "\nUse `kotlin-lsp skills read <name>` for the full document.\n"
    ));
    out
}

/// Run `kotlin-lsp skills read <name>` — print the full SKILL.md.
fn run_read(name: &str) {
    match format_read(name) {
        Ok(text) => print!("{}", text),
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    }
}

/// Build the text that `skills read <name>` prints, or an error message.
pub(crate) fn format_read(name: &str) -> Result<String, String> {
    let skills = builtin_skills();
    match skills.get(name) {
        Some(entry) => Ok(entry.content.to_string()),
        None => {
            let mut msg = format!("error: unknown skill '{}'\n", name);
            let known: Vec<&str> = skills.keys().copied().collect();
            if known.is_empty() {
                msg.push_str("No skills bundled with this version.\n");
            } else {
                msg.push_str(&format!("Available: {}\n", known.join(", ")));
            }
            Err(msg)
        }
    }
}

/// Public entry point: `kotlin-lsp skills <list|read> [name]`.
pub(crate) fn run_skills(args: Vec<String>) {
    let action = args.first().map(|s| s.as_str()).unwrap_or("list");
    match action {
        "list" => run_list(),
        "read" => {
            let name = args.get(1).expect("skills read requires a NAME argument");
            run_read(name);
        }
        other => {
            eprintln!("error: unknown skills subcommand '{other}'");
            eprintln!("Usage: kotlin-lsp skills list|read <name>");
            std::process::exit(1);
        }
    }
}
