//! CLI `format` subcommand — ktlint check/apply, matching Spotless semantics.
//!
//! # Modes
//!
//! - **check**: like `spotlessCheck` — runs ktlint in lint-only mode, reports
//!   violations with diff context, exits non-zero on any violation.
//! - **apply**: like `spotlessApply` — runs ktlint `--format` in-place.
//!   With `--dry-run` it previews changes without writing.
//!
//! Both modes require `ktlint` to be installed on PATH.  A friendly error is
//! printed if the tool is missing.

use std::path::PathBuf;

use serde::Serialize;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Check whether `ktlint` (or any named binary) is available on PATH.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(name);
            if full.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::metadata(&full).ok().and_then(|m| {
                        if m.permissions().mode() & 0o111 != 0 {
                            Some(full)
                        } else {
                            None
                        }
                    })
                }
                #[cfg(not(unix))]
                Some(full)
            } else {
                None
            }
        })
    })
}

/// Installation hint for the current platform.
fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "Install: brew install ktlint"
    } else if cfg!(target_os = "linux") {
        "Install: https://ktlint.github.io/ktlint/latest/install/cli/"
    } else {
        "Install: https://ktlint.github.io/ktlint/latest/install/cli/"
    }
}

// ─── Data types ──────────────────────────────────────────────────────────────

/// A single ktlint violation, as reported on stderr/stdout.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Violation {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) col: u32,
    pub(crate) rule_id: String,
    pub(crate) message: String,
}

/// Per-file check result.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
enum CheckFileResult {
    #[serde(rename = "ok")]
    Ok { file: String },
    #[serde(rename = "violations")]
    Violations {
        file: String,
        violations: Vec<Violation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
    },
    #[serde(rename = "error")]
    Error { file: String, message: String },
}

/// Top-level JSON output for check.
#[derive(Debug, Clone, Serialize)]
struct CheckSummary {
    total_files: usize,
    files_with_violations: usize,
    files_with_errors: usize,
    total_violations: usize,
    results: Vec<CheckFileResult>,
}

/// Per-file apply result.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
enum ApplyFileResult {
    #[serde(rename = "ok")]
    Ok {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
    },
    #[serde(rename = "noop")]
    Noop { file: String },
    #[serde(rename = "error")]
    Error { file: String, message: String },
}

/// Top-level JSON output for apply.
#[derive(Debug, Clone, Serialize)]
struct ApplySummary {
    total_files: usize,
    files_formatted: usize,
    files_noop: usize,
    files_errored: usize,
    results: Vec<ApplyFileResult>,
}

// ─── Tool runner ─────────────────────────────────────────────────────────────

/// Run `ktlint <file>` in lint-only mode (no `--format`).
///
/// Returns a vector of parsed violations.
fn run_ktlint_lint(file: &str) -> Result<Vec<Violation>, String> {
    let output = std::process::Command::new("ktlint")
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run ktlint: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut violations = parse_ktlint_output(stdout.as_ref());
    if violations.is_empty() {
        violations = parse_ktlint_output(stderr.as_ref());
    }

    Ok(violations)
}

/// Run `ktlint --format --stdin --stdin-path <path>` and return formatted content.
fn run_ktlint_format_stdin(file: &str, input: &str) -> Result<String, String> {
    let mut child = std::process::Command::new("ktlint")
        .args(["--format", "--stdin", "--stdin-path", file])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to run ktlint: {e}"))?;

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| format!("stdin write error: {e}"))?;
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("ktlint wait error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ktlint exited with error: {stderr}"));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("invalid utf-8 output: {e}"))
}

/// Run `ktlint --format <file>` in-place.
fn run_ktlint_format_inplace(file: &str) -> Result<bool, String> {
    let output = std::process::Command::new("ktlint")
        .args(["--format", file])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("failed to run ktlint: {e}"))?;

    Ok(output.status.success())
}

// ─── Output parsing ──────────────────────────────────────────────────────────

/// Parse ktlint output lines in the format `file:line:col:ruleId - message`.
fn parse_ktlint_output(output: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(v) = try_parse_standard(line) {
            violations.push(v);
        }
    }
    violations
}

fn try_parse_standard(line: &str) -> Option<Violation> {
    // Format: /path/to/File.kt:42:5:chain-wrapping - Chain should be wrapped
    let parts: Vec<&str> = line.splitn(4, ':').collect();
    if parts.len() < 4 {
        return None;
    }
    let file = parts[0].to_string();
    let line_num: u32 = parts[1].parse().ok()?;
    let col: u32 = parts[2].parse().ok()?;

    // parts[3] = "ruleId - message" or "ruleId message"
    let rest = parts[3];
    let dash_pos = rest.find(" - ");
    let (rule_id, message) = if let Some(pos) = dash_pos {
        (rest[..pos].to_string(), rest[pos + 3..].to_string())
    } else {
        (String::new(), rest.to_string())
    };

    Some(Violation {
        file,
        line: line_num,
        col,
        rule_id,
        message,
    })
}

// ─── Diff generation ─────────────────────────────────────────────────────────

/// Produce a simple unified-diff between original and formatted content.
///
/// Returns `None` when the content is identical (no diff).
fn generate_diff(original: &str, formatted: &str, file_label: &str) -> Option<String> {
    if original == formatted {
        return None;
    }

    use std::fmt::Write;
    let mut diff = String::new();
    let _ = writeln!(diff, "--- a/{file_label}");
    let _ = writeln!(diff, "+++ b/{file_label}");

    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();

    let mut hunk_orig_start = 0usize;
    let mut hunk_fmt_start = 0usize;
    let mut hunk_lines: Vec<(char, String)> = Vec::new();

    let mut i = 0usize;
    let mut j = 0usize;

    while i < orig_lines.len() || j < fmt_lines.len() {
        if i < orig_lines.len() && j < fmt_lines.len() && orig_lines[i] == fmt_lines[j] {
            if !hunk_lines.is_empty() {
                let _ = writeln!(
                    diff,
                    "@@ -{},{} +{},{} @@",
                    hunk_orig_start + 1,
                    hunk_lines
                        .iter()
                        .filter(|(c, _)| *c == '-' || *c == ' ')
                        .count(),
                    hunk_fmt_start + 1,
                    hunk_lines
                        .iter()
                        .filter(|(c, _)| *c == '+' || *c == ' ')
                        .count(),
                );
                for (c, line) in &hunk_lines {
                    let _ = writeln!(diff, "{c}{line}");
                }
                hunk_lines.clear();
            }
            i += 1;
            j += 1;
            continue;
        }

        if hunk_lines.is_empty() {
            hunk_orig_start = i;
            hunk_fmt_start = j;
        }

        if i < orig_lines.len()
            && (j >= fmt_lines.len()
                || (i + 1 < orig_lines.len()
                    && j < fmt_lines.len()
                    && orig_lines[i + 1] == fmt_lines[j]))
        {
            hunk_lines.push(('-', orig_lines[i].to_string()));
            i += 1;
        } else if j < fmt_lines.len() {
            hunk_lines.push(('+', fmt_lines[j].to_string()));
            j += 1;
        } else {
            break;
        }
    }

    if !hunk_lines.is_empty() {
        let _ = writeln!(
            diff,
            "@@ -{},{} +{},{} @@",
            hunk_orig_start + 1,
            hunk_lines
                .iter()
                .filter(|(c, _)| *c == '-' || *c == ' ')
                .count(),
            hunk_fmt_start + 1,
            hunk_lines
                .iter()
                .filter(|(c, _)| *c == '+' || *c == ' ')
                .count(),
        );
        for (c, line) in &hunk_lines {
            let _ = writeln!(diff, "{c}{line}");
        }
    }

    Some(diff)
}

// ─── Check (spotlessCheck equivalent) ────────────────────────────────────────

/// Run `format check` — validates formatting without modifying files.
///
/// For each file, runs ktlint in lint-only mode, then optionally formats via
/// pipe to generate a diff.  Reports violations and exits non-zero if any found.
pub(crate) fn run_format_check(files: &[PathBuf], json: bool) {
    if which("ktlint").is_none() {
        eprintln!("error: ktlint not found on PATH");
        eprintln!("       {}", install_hint());
        std::process::exit(1);
    }

    let mut results: Vec<CheckFileResult> = Vec::with_capacity(files.len());
    let mut total_kotlin_files = 0usize;
    let mut total_violations = 0usize;
    let mut files_with_violations = 0usize;
    let mut files_with_errors = 0usize;

    for file in files {
        let path_str = file.to_string_lossy();
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext != "kt" && ext != "kts" {
            continue;
        }
        total_kotlin_files += 1;

        let violations = match run_ktlint_lint(&path_str) {
            Ok(v) => v,
            Err(e) => {
                results.push(CheckFileResult::Error {
                    file: path_str.to_string(),
                    message: e,
                });
                files_with_errors += 1;
                continue;
            }
        };

        if violations.is_empty() {
            results.push(CheckFileResult::Ok {
                file: path_str.to_string(),
            });
            continue;
        }

        let diff = match std::fs::read_to_string(file) {
            Ok(original) => match run_ktlint_format_stdin(&path_str, &original) {
                Ok(formatted) => generate_diff(&original, &formatted, &path_str),
                Err(_) => None,
            },
            Err(_) => None,
        };

        total_violations += violations.len();
        files_with_violations += 1;
        results.push(CheckFileResult::Violations {
            file: path_str.to_string(),
            violations,
            diff,
        });
    }

    if json {
        let summary = CheckSummary {
            total_files: total_kotlin_files,
            files_with_violations,
            files_with_errors,
            total_violations,
            results,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).expect("serialize JSON")
        );
    } else {
        for result in &results {
            match result {
                CheckFileResult::Ok { file } => {
                    println!("✓ {file}");
                }
                CheckFileResult::Violations {
                    file,
                    violations,
                    diff,
                } => {
                    println!("\n✗ {file}: {} violation(s)", violations.len());
                    for v in violations {
                        println!(
                            "  {}:{}:{}: {} — {}",
                            v.file, v.line, v.col, v.rule_id, v.message
                        );
                    }
                    if let Some(d) = diff {
                        println!("{d}");
                    }
                }
                CheckFileResult::Error { file, message } => {
                    eprintln!("⚠ {file}: {message}");
                }
            }
        }

        let summary = format!(
            "\n{total_violations} violation(s) in {files_with_violations} file(s), \
             {files_with_errors} error(s)",
        );
        if files_with_violations > 0 || files_with_errors > 0 {
            eprintln!("{summary}");
            std::process::exit(1);
        } else {
            println!("{summary}");
            println!("All files OK.");
        }
    }
}

// ─── Apply (spotlessApply equivalent) ────────────────────────────────────────

/// Run `format apply` — applies ktlint formatting in-place (or dry-run).
///
/// When `dry_run` is true, the formatter runs via stdin pipe and the diff is
/// reported but no files are modified.
pub(crate) fn run_format_apply(files: &[PathBuf], json: bool, dry_run: bool) {
    if which("ktlint").is_none() {
        eprintln!("error: ktlint not found on PATH");
        eprintln!("       {}", install_hint());
        std::process::exit(1);
    }

    let mut results: Vec<ApplyFileResult> = Vec::with_capacity(files.len());
    let mut total_kotlin_files = 0usize;
    let mut files_formatted = 0usize;
    let mut files_noop = 0usize;
    let mut files_errored = 0usize;

    for file in files {
        let path_str = file.to_string_lossy();
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext != "kt" && ext != "kts" {
            continue;
        }
        total_kotlin_files += 1;

        let original = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                results.push(ApplyFileResult::Error {
                    file: path_str.to_string(),
                    message: format!("read error: {e}"),
                });
                files_errored += 1;
                continue;
            }
        };

        if dry_run {
            let formatted = match run_ktlint_format_stdin(&path_str, &original) {
                Ok(f) => f,
                Err(e) => {
                    results.push(ApplyFileResult::Error {
                        file: path_str.to_string(),
                        message: e,
                    });
                    files_errored += 1;
                    continue;
                }
            };

            if original == formatted {
                results.push(ApplyFileResult::Noop {
                    file: path_str.to_string(),
                });
                files_noop += 1;
            } else {
                let diff = generate_diff(&original, &formatted, &path_str);
                results.push(ApplyFileResult::Ok {
                    file: path_str.to_string(),
                    diff,
                });
                files_formatted += 1;
            }
            continue;
        }

        match run_ktlint_format_inplace(&path_str) {
            Ok(_) => {
                let new_content = match std::fs::read_to_string(file) {
                    Ok(c) => c,
                    Err(_) => String::new(),
                };

                if original == new_content {
                    results.push(ApplyFileResult::Noop {
                        file: path_str.to_string(),
                    });
                    files_noop += 1;
                } else {
                    let diff = generate_diff(&original, &new_content, &path_str);
                    results.push(ApplyFileResult::Ok {
                        file: path_str.to_string(),
                        diff,
                    });
                    files_formatted += 1;
                }
            }
            Err(e) => {
                results.push(ApplyFileResult::Error {
                    file: path_str.to_string(),
                    message: e,
                });
                files_errored += 1;
            }
        }
    }

    if json {
        let summary = ApplySummary {
            total_files: total_kotlin_files,
            files_formatted,
            files_noop,
            files_errored,
            results,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).expect("serialize JSON")
        );
    } else {
        for result in &results {
            match result {
                ApplyFileResult::Ok { file, diff } => {
                    println!("✓ {file}: formatted");
                    if let Some(d) = diff {
                        println!("{d}");
                    }
                }
                ApplyFileResult::Noop { file } => {
                    println!("· {file}: already formatted");
                }
                ApplyFileResult::Error { file, message } => {
                    eprintln!("⚠ {file}: {message}");
                }
            }
        }

        let summary = format!(
            "\n{files_formatted} file(s) formatted, {files_noop} already clean, \
             {files_errored} error(s).",
        );
        if files_errored > 0 {
            eprintln!("{summary}");
            std::process::exit(1);
        } else {
            println!("{summary}");
        }
    }
}

// ─── Test helpers ────────────────────────────────────────────────────────────

/// Internal test helpers exposed for unit tests.
#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;

    pub(crate) fn parse_ktlint_output_for_test(output: &str) -> Vec<Violation> {
        parse_ktlint_output(output)
    }

    pub(crate) fn generate_diff_for_test(
        original: &str,
        formatted: &str,
        label: &str,
    ) -> Option<String> {
        generate_diff(original, formatted, label)
    }

    /// Run format check in test context — does not call `std::process::exit`.
    pub(crate) fn run_format_check_for_test(files: &[PathBuf], json: bool) {
        let mut results: Vec<CheckFileResult> = Vec::with_capacity(files.len());
        let mut total_kotlin_files = 0usize;
        let mut total_violations = 0usize;
        let mut files_with_violations = 0usize;
        let mut files_with_errors = 0usize;

        for file in files {
            let path_str = file.to_string_lossy();
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "kt" && ext != "kts" {
                continue;
            }
            total_kotlin_files += 1;

            let violations = match run_ktlint_lint(&path_str) {
                Ok(v) => v,
                Err(e) => {
                    results.push(CheckFileResult::Error {
                        file: path_str.to_string(),
                        message: e,
                    });
                    files_with_errors += 1;
                    continue;
                }
            };

            if violations.is_empty() {
                results.push(CheckFileResult::Ok {
                    file: path_str.to_string(),
                });
                continue;
            }

            let diff = match std::fs::read_to_string(file) {
                Ok(original) => match run_ktlint_format_stdin(&path_str, &original) {
                    Ok(formatted) => generate_diff(&original, &formatted, &path_str),
                    Err(_) => None,
                },
                Err(_) => None,
            };

            total_violations += violations.len();
            files_with_violations += 1;
            results.push(CheckFileResult::Violations {
                file: path_str.to_string(),
                violations,
                diff,
            });
        }

        if json {
            let summary = CheckSummary {
                total_files: total_kotlin_files,
                files_with_violations,
                files_with_errors,
                total_violations,
                results,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).expect("serialize JSON")
            );
        }
    }

    /// Run format apply in test context — without process exit.
    pub(crate) fn run_format_apply_for_test(files: &[PathBuf], json: bool, dry_run: bool) {
        let mut results: Vec<ApplyFileResult> = Vec::with_capacity(files.len());
        let mut total_kotlin_files = 0usize;
        let mut files_formatted = 0usize;
        let mut files_noop = 0usize;
        let mut files_errored = 0usize;

        for file in files {
            let path_str = file.to_string_lossy();
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "kt" && ext != "kts" {
                continue;
            }
            total_kotlin_files += 1;

            let original = match std::fs::read_to_string(file) {
                Ok(c) => c,
                Err(e) => {
                    results.push(ApplyFileResult::Error {
                        file: path_str.to_string(),
                        message: format!("read error: {e}"),
                    });
                    files_errored += 1;
                    continue;
                }
            };

            if dry_run {
                let formatted = match run_ktlint_format_stdin(&path_str, &original) {
                    Ok(f) => f,
                    Err(e) => {
                        results.push(ApplyFileResult::Error {
                            file: path_str.to_string(),
                            message: e,
                        });
                        files_errored += 1;
                        continue;
                    }
                };
                if original == formatted {
                    results.push(ApplyFileResult::Noop {
                        file: path_str.to_string(),
                    });
                    files_noop += 1;
                } else {
                    let diff = generate_diff(&original, &formatted, &path_str);
                    results.push(ApplyFileResult::Ok {
                        file: path_str.to_string(),
                        diff,
                    });
                    files_formatted += 1;
                }
                continue;
            }

            match run_ktlint_format_inplace(&path_str) {
                Ok(_) => {
                    let new_content = match std::fs::read_to_string(file) {
                        Ok(c) => c,
                        Err(_) => String::new(),
                    };

                    if original == new_content {
                        results.push(ApplyFileResult::Noop {
                            file: path_str.to_string(),
                        });
                        files_noop += 1;
                    } else {
                        let diff = generate_diff(&original, &new_content, &path_str);
                        results.push(ApplyFileResult::Ok {
                            file: path_str.to_string(),
                            diff,
                        });
                        files_formatted += 1;
                    }
                }
                Err(e) => {
                    results.push(ApplyFileResult::Error {
                        file: path_str.to_string(),
                        message: e,
                    });
                    files_errored += 1;
                }
            }
        }

        if json {
            let summary = ApplySummary {
                total_files: total_kotlin_files,
                files_formatted,
                files_noop,
                files_errored,
                results,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).expect("serialize JSON")
            );
        }
    }
}
