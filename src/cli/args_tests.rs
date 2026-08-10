//! Unit tests for `cli::args`.

use super::*;

fn parse(argv: &[&str]) -> Result<Option<CliArgs>, String> {
    // lexopt expects argv[0] to be the binary name; prepend it.
    let owned: Vec<std::ffi::OsString> = std::iter::once("kotlin-lsp".into())
        .chain(argv.iter().map(|s| (*s).into()))
        .collect();
    CliArgs::parse_from(lexopt::Parser::from_args(
        owned.iter().skip(1).map(|s| s.as_os_str()),
    ))
}

fn find_filters(args: &CliArgs) -> &ResultFilters {
    match &args.subcommand {
        Subcommand::Find { filters, .. } | Subcommand::Refs { filters, .. } => filters,
        other => panic!("expected find/refs, got {other:?}"),
    }
}

#[test]
fn find_with_no_filter_flags_yields_default_filters() {
    let args = parse(&["find", "Foo"]).unwrap().unwrap();
    let filters = find_filters(&args);
    assert!(!filters.relative);
    assert!(filters.limit.is_none());
    assert!(filters.module.is_none());
    assert!(filters.source_sets.is_empty());
}

#[test]
fn find_parses_relative_flag() {
    let args = parse(&["find", "Foo", "--relative"]).unwrap().unwrap();
    assert!(find_filters(&args).relative);
}

#[test]
fn find_parses_absolute_flag() {
    let args = parse(&["find", "Foo", "--absolute"]).unwrap().unwrap();
    // `--absolute` doesn't go through ResultFilters; it lives on CliArgs so the
    // run-time TTY-default resolver can see it. The filter stays relative=false.
    assert!(args.absolute);
    assert!(!find_filters(&args).relative);
}

#[test]
fn absolute_defaults_to_false() {
    let args = parse(&["find", "Foo"]).unwrap().unwrap();
    assert!(!args.absolute);
}

#[test]
fn find_parses_limit_flag() {
    let args = parse(&["find", "Foo", "--limit", "20"]).unwrap().unwrap();
    assert_eq!(find_filters(&args).limit, Some(20));
}

#[test]
fn find_rejects_non_numeric_limit() {
    let err = parse(&["find", "Foo", "--limit", "abc"]).unwrap_err();
    assert!(err.contains("--limit"), "got: {err}");
}

#[test]
fn find_parses_module_filter() {
    let args = parse(&["find", "Foo", "--module", "features/play"])
        .unwrap()
        .unwrap();
    assert_eq!(find_filters(&args).module.as_deref(), Some("features/play"));
}

#[test]
fn refs_parses_single_source_set() {
    let args = parse(&["refs", "Foo", "--source-set", "commonMain"])
        .unwrap()
        .unwrap();
    assert_eq!(find_filters(&args).source_sets, vec!["commonMain"]);
}

#[test]
fn refs_parses_comma_separated_source_sets() {
    let args = parse(&[
        "refs",
        "Foo",
        "--source-set",
        "commonMain,androidMain,iosMain",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(
        find_filters(&args).source_sets,
        vec!["commonMain", "androidMain", "iosMain"]
    );
}

#[test]
fn refs_dedupes_whitespace_in_source_set_csv() {
    let args = parse(&["refs", "Foo", "--source-set", " commonMain , androidMain "])
        .unwrap()
        .unwrap();
    assert_eq!(
        find_filters(&args).source_sets,
        vec!["commonMain", "androidMain"]
    );
}

#[test]
fn refs_accepts_repeated_source_set_flag() {
    // `--source-set commonMain --source-set androidMain` should also work as OR.
    let args = parse(&[
        "refs",
        "Foo",
        "--source-set",
        "commonMain",
        "--source-set",
        "androidMain",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(
        find_filters(&args).source_sets,
        vec!["commonMain", "androidMain"]
    );
}

#[test]
fn find_combines_all_filter_flags() {
    let args = parse(&[
        "find",
        "Foo",
        "--json",
        "--relative",
        "--limit",
        "5",
        "--module",
        "play",
        "--source-set",
        "commonMain",
    ])
    .unwrap()
    .unwrap();
    assert!(matches!(args.fmt, OutputFmt::Json));
    let f = find_filters(&args);
    assert!(f.relative);
    assert_eq!(f.limit, Some(5));
    assert_eq!(f.module.as_deref(), Some("play"));
    assert_eq!(f.source_sets, vec!["commonMain"]);
}

#[test]
fn sources_explain_parses_first_positional_arg() {
    let args = parse(&["sources", "explain"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Sources { explain } => assert!(explain),
        other => panic!("expected sources, got {other:?}"),
    }
}

#[test]
fn cache_stats_parses_first_positional_arg() {
    let args = parse(&["cache", "stats"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Cache { sub } => assert_eq!(sub, "stats"),
        other => panic!("expected cache, got {other:?}"),
    }
}

#[test]
fn code_action_subcommand_is_reachable() {
    let args = parse(&["tool", "code-action", "Foo.kt", "2", "3", "--apply"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::CodeAction {
            file,
            line,
            col,
            apply,
            ..
        } => {
            assert_eq!(file, std::path::PathBuf::from("Foo.kt"));
            assert_eq!(line, 2);
            assert_eq!(col, 3);
            assert!(apply);
        }
        other => panic!("expected code-action, got {other:?}"),
    }
}

#[test]
fn batch_imports_subcommand_is_reachable() {
    let args = parse(&[
        "edit",
        "imports",
        "Foo.kt",
        "--dry-run",
        "--output",
        "out.json",
    ])
    .unwrap()
    .unwrap();
    match args.subcommand {
        Subcommand::Batch {
            file,
            dry_run,
            imports,
            output,
            ..
        } => {
            assert_eq!(file, std::path::PathBuf::from("Foo.kt"));
            assert!(dry_run);
            assert!(imports);
            // output is None in group handler
            assert_eq!(output.as_deref(), None);
        }
        other => panic!("expected batch-imports, got {other:?}"),
    }
}

#[test]
fn new_file_parses_template_and_name_from_first_two_args() {
    let args = parse(&[
        "edit",
        "new",
        "viewmodel",
        "LoginViewModel",
        "--package",
        "com.example",
        "--dir",
        "src/main/kotlin",
    ])
    .unwrap()
    .unwrap();
    match args.subcommand {
        Subcommand::NewFile {
            template,
            name,
            package_name,
            directory,
        } => {
            assert_eq!(template, "viewmodel");
            assert_eq!(name, "LoginViewModel");
            assert_eq!(package_name.as_deref(), Some("com.example"));
            assert_eq!(directory, Some(std::path::PathBuf::from("src/main/kotlin")));
        }
        other => panic!("expected new-file, got {other:?}"),
    }
}

#[test]
fn insert_parses_direction_content_and_in_place() {
    let args = parse(&[
        "edit",
        "insert",
        "Foo.kt",
        "10",
        "--after",
        "--content",
        "println(\"hi\")",
        "--in-place",
    ])
    .unwrap()
    .unwrap();
    match args.subcommand {
        Subcommand::Insert {
            file,
            line,
            before,
            after,
            content,
            in_place,
            ..
        } => {
            assert_eq!(file, std::path::PathBuf::from("Foo.kt"));
            assert_eq!(line, 10);
            assert!(!before);
            assert!(after);
            assert_eq!(content, "println(\"hi\")");
            assert!(in_place);
        }
        other => panic!("expected insert, got {other:?}"),
    }
}

#[test]
fn insert_requires_one_direction() {
    let err = parse(&["edit", "insert", "Foo.kt", "10", "--content", "println()"]).unwrap_err();
    assert!(err.contains("exactly one"), "got: {err}");
}

#[test]
fn insert_requires_content() {
    let err = parse(&["edit", "insert", "Foo.kt", "10", "--before"]).unwrap_err();
    assert!(err.contains("--content"), "got: {err}");
}

#[test]
fn batch_parses_rule_file_and_dry_run() {
    let args = parse(&["edit", "batch", "rules.json", "--dry-run"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Batch {
            file,
            dry_run,
            imports,
            output,
            ..
        } => {
            assert_eq!(file, std::path::PathBuf::from("rules.json"));
            assert!(dry_run);
            assert!(!imports);
            assert!(output.is_none());
        }
        other => panic!("expected batch, got {other:?}"),
    }
}

#[test]
fn index_jars_subcommand_is_reachable() {
    let args = parse(&["index-jars", "build/libs"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::IndexJars { root } => {
            assert_eq!(root, Some(std::path::PathBuf::from("build/libs")));
        }
        other => panic!("expected index-jars, got {other:?}"),
    }
}

#[test]
fn benchmark_subcommand_is_reachable() {
    let args = parse(&["tool", "bench"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Benchmark => {}
        other => panic!("expected benchmark, got {other:?}"),
    }
}

#[test]
fn type_hierarchy_defaults_to_subtypes() {
    let args = parse(&["type", "hierarchy", "Base"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Type {
            sub:
                TypeSub::Hierarchy {
                    name,
                    subtypes,
                    supertypes,
                    graph: _graph,
                    ..
                },
        } => {
            assert_eq!(name, "Base");
            assert!(subtypes);
            assert!(!supertypes);
        }
        other => panic!("expected type-hierarchy, got {other:?}"),
    }
}

#[test]
fn type_hierarchy_supertypes_flag_is_reachable() {
    let args = parse(&["type", "hierarchy", "Child", "--supertypes"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Type {
            sub:
                TypeSub::Hierarchy {
                    name,
                    subtypes,
                    supertypes,
                    graph: _graph,
                    ..
                },
        } => {
            assert_eq!(name, "Child");
            assert!(!subtypes);
            assert!(supertypes);
        }
        other => panic!("expected type-hierarchy, got {other:?}"),
    }
}

#[test]
fn type_hierarchy_can_request_both_directions() {
    let args = parse(&["type", "hierarchy", "Node", "--subtypes", "--supertypes"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Type {
            sub:
                TypeSub::Hierarchy {
                    name,
                    subtypes,
                    supertypes,
                    graph: _graph,
                    ..
                },
        } => {
            assert_eq!(name, "Node");
            assert!(subtypes);
            assert!(supertypes);
        }
        other => panic!("expected type-hierarchy, got {other:?}"),
    }
}

// ── skills subcommand ────────────────────────────────────────────────────────────

#[test]
fn skills_list_parses_correctly() {
    let args = parse(&["tool", "skills", "list"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Skills { args: inner } => {
            assert_eq!(inner, vec!["list"]);
        }
        other => panic!("expected Skills, got {other:?}"),
    }
}

#[test]
fn skills_read_parses_name_arg() {
    let args = parse(&["tool", "skills", "read", "kotlin-lsp"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Skills { args: inner } => {
            assert_eq!(inner, vec!["read", "kotlin-lsp"]);
        }
        other => panic!("expected Skills, got {other:?}"),
    }
}

#[test]
fn skills_parses_as_subcommand() {
    let args = parse(&["tool", "skills"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Skills { .. } => {} // no args = defaults to list
        other => panic!("expected Skills, got {other:?}"),
    }
}

// ── search subcommand ────────────────────────────────────────────────────────────

#[test]
fn search_shorthand_parses_query() {
    let args = parse(&["search", "login view model"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Search { query, limit } => {
            assert_eq!(query, "login view model");
            assert_eq!(limit, 20); // default
        }
        other => panic!("expected Search, got {other:?}"),
    }
}

#[test]
fn search_semantic_explicit_parses_query() {
    let args = parse(&["search", "semantic", "login view model"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Search { query, limit } => {
            assert_eq!(query, "login view model");
            assert_eq!(limit, 20);
        }
        other => panic!("expected Search, got {other:?}"),
    }
}

#[test]
fn search_requires_query() {
    let err = parse(&["search"]).unwrap_err();
    assert!(err.contains("QUERY"), "got: {err}");
}

#[test]
fn search_shorthand_parses_limit() {
    let args = parse(&["search", "login", "--limit", "5"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Search { query, limit } => {
            assert_eq!(query, "login");
            assert_eq!(limit, 5);
        }
        other => panic!("expected Search, got {other:?}"),
    }
}

#[test]
fn search_semantic_explicit_parses_limit() {
    let args = parse(&["search", "semantic", "login", "--limit", "10"])
        .unwrap()
        .unwrap();
    match args.subcommand {
        Subcommand::Search { query, limit } => {
            assert_eq!(query, "login");
            assert_eq!(limit, 10);
        }
        other => panic!("expected Search, got {other:?}"),
    }
}

#[test]
fn search_docs_subcommand_parses_query() {
    let args = parse(&["search", "docs", "view model"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::Docs { query } => {
            assert_eq!(query, "view model");
        }
        other => panic!("expected Docs, got {other:?}"),
    }
}

#[test]
fn search_summarize_is_a_search_member() {
    // `search summarize <name>` is the canonical symbol-summary form
    // (docs/commands.md). It must NOT fall through to a semantic search.
    let args = parse(&["search", "summarize", "LoginViewModel"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::Summarize { name, .. } => {
            assert_eq!(name, "LoginViewModel");
        }
        other => panic!("expected Summarize, got {other:?}"),
    }
}

// ── search group members (#228) ────────────────────────────────────────────

#[test]
fn search_cache_stats_parses() {
    let args = parse(&["search", "cache-stats"]).unwrap().unwrap();
    match args.subcommand {
        Subcommand::SummaryCacheStats => {}
        other => panic!("expected SummaryCacheStats, got {other:?}"),
    }
}

#[test]
fn search_imports_parses_name() {
    let args = parse(&["search", "imports", "UserRepo"]).unwrap().unwrap();
    match &args.subcommand {
        Subcommand::ImportsOf { name } => {
            assert_eq!(name, "UserRepo");
        }
        other => panic!("expected ImportsOf, got {other:?}"),
    }
}

#[test]
fn search_imports_requires_name() {
    let err = parse(&["search", "imports"]).unwrap_err();
    assert!(err.contains("NAME"), "got: {err}");
}

#[test]
fn search_annotated_parses_annotation() {
    let args = parse(&["search", "annotated", "Composable"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::Annotated { annotation } => {
            assert_eq!(annotation, "Composable");
        }
        other => panic!("expected Annotated, got {other:?}"),
    }
}

#[test]
fn search_find_test_parses_position() {
    let args = parse(&["search", "find-test", "Foo.kt", "10", "4"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::FindTest { file, line, col } => {
            assert_eq!(file.to_string_lossy(), "Foo.kt");
            assert_eq!(*line, 10);
            assert_eq!(*col, 4);
        }
        other => panic!("expected FindTest, got {other:?}"),
    }
}

#[test]
fn search_expect_actual_parses_name() {
    let args = parse(&["search", "expect-actual", "formatTime"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::ExpectActual { name } => {
            assert_eq!(name, "formatTime");
        }
        other => panic!("expected ExpectActual, got {other:?}"),
    }
}

// ── help/parser consistency (#228) ─────────────────────────────────────────
// --help must advertise ONLY invocable commands, and every invocable command
// must be advertised. These tests fail the build when the parser gate
// (is_subcommand), the handlers (build_subcommand) and the help text drift.

/// Tokenized COMMAND parts of the SUBCOMMANDS section of --help. The
/// description column (after the first double space) is excluded so a
/// description word like `List` is never mistaken for a member.
fn help_subcommand_lines() -> Vec<Vec<String>> {
    help_command_lines()
        .iter()
        .filter_map(|l| {
            let cmd_part = l.split_once("  ").map(|(c, _)| c).unwrap_or(l.as_str());
            let tokens: Vec<String> = cmd_part.split_whitespace().map(String::from).collect();
            if tokens.is_empty() {
                None
            } else {
                Some(tokens)
            }
        })
        .collect()
}

#[test]
fn help_advertises_only_invocable_commands() {
    // Regression for #228: --help advertised 12 top-level subcommands the
    // parser rejected ("unknown subcommand"). Every advertised top-level
    // word must be present in the is_subcommand() gate.
    for line in help_subcommand_lines() {
        let cmd = line[0].as_str();
        assert!(
            is_subcommand(cmd),
            "--help advertises '{cmd}' but the parser rejects it — add it to is_subcommand() or remove it from print_help()"
        );
    }
}

#[test]
fn help_group_members_parse() {
    // Every `<group> <member> …` line in --help must parse (not error) with
    // placeholder arguments filled in. Catches advertised group members the
    // parser does not implement and members that were never registered.
    //
    // The `search` group needs a stronger check: its catch-all treats any
    // unknown member as a semantic-search query, so `search bogus` *parses*.
    // Each advertised search member must resolve to its intended variant
    // instead of silently falling through (the #228 failure mode).
    let extra_args: &[(&str, &str, &[&str])] = &[(
        "edit",
        "insert",
        &["--after", "--content", "X"], // required flags omitted from the help line
    )];
    for line in help_subcommand_lines() {
        let cmd = line[0].as_str();
        let member = line.get(1).map(|s| s.as_str()).unwrap_or("");
        // `search <query>` / `docs <query>` are top-level-arg forms, not group
        // members; covered by help_advertises_only_invocable_commands.
        if member.is_empty() || member.starts_with('<') || member.starts_with('[') {
            continue;
        }
        let mut args: Vec<String> = vec![cmd.to_string(), member.to_string()];
        for tok in &line[2..] {
            // "1" works for every placeholder: filenames, symbol names, and
            // the numeric <line>/<col> positionals ("X" would fail the parse).
            // `<file>...`/`<file/dir>...` are variadic placeholders — same dummy.
            if tok.starts_with('<') || tok.ends_with("...") {
                args.push("1".to_string());
            }
        }
        if let Some((_, _, extra)) = extra_args
            .iter()
            .find(|(c, m, _)| *c == cmd && *m == member)
        {
            args.extend(extra.iter().map(|s| s.to_string()));
        }
        let joined = args.join(" ");
        let parsed = parse(&args.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_else(|e| panic!("--help advertises '{joined}' but parsing failed: {e}"))
            .unwrap_or_else(|| panic!("--help advertises '{joined}' but the parser rejected it"));
        if cmd == "search" && member != "semantic" {
            // semantic (and the bare <query> shorthand) are the only search
            // members whose intended result IS Subcommand::Search.
            assert!(
                !matches!(parsed.subcommand, Subcommand::Search { .. }),
                "--help advertises 'search {member}' but it falls through to a semantic search — implement it in build_subcommand() or remove it from print_help()"
            );
        }
    }
}

// ── capabilities manifest == --help (#231) ───────────────────────────────────

#[test]
fn help_command_parts_are_structural() {
    // The capabilities manifest parses each help line as
    // `<cmd> [member] <placeholders>␣␣<description>`. A command part that
    // contains a bare word after the member (a token that is neither `<…>`
    // nor `[…]) means the double-space description boundary is missing, and
    // the manifest would mis-parse the line (issue #231).
    for line in help_command_lines() {
        let cmd_part = line
            .split_once("  ")
            .map(|(c, _)| c)
            .unwrap_or(line.as_str());
        let tokens: Vec<&str> = cmd_part.split_whitespace().collect();
        assert!(!tokens.is_empty(), "empty help line: {line:?}");
        let rest: &[&str] =
            if tokens.len() >= 2 && !tokens[1].starts_with('<') && !tokens[1].starts_with('[') {
                &tokens[2..] // member present
            } else {
                &tokens[1..]
            };
        for tok in rest {
            assert!(
                tok.starts_with('<') || tok.starts_with('['),
                "help line command part has a non-placeholder word '{tok}' in '{cmd_part}' — use two spaces before the description"
            );
        }
    }
}

#[test]
fn capabilities_manifest_matches_help() {
    // The machine-readable manifest must expose exactly the same command
    // surface as --help, in both directions: nothing missing, nothing the
    // parser rejects. The manifest is generated from help_command_lines(),
    // so this test fails when either side drifts.
    let caps = capabilities_manifest();
    let commands = caps["commands"]
        .as_object()
        .expect("manifest has a commands object");

    // Groups = commands whose manifest entry lists subcommands. A bare help
    // line (`search <query>`) is the same command family as its group key.
    let groups: std::collections::BTreeSet<&String> = commands
        .iter()
        .filter(|(_, v)| v.get("subcommands").is_some())
        .map(|(k, _)| k)
        .collect();

    let mut help_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in help_subcommand_lines() {
        let cmd = &line[0];
        if line
            .get(1)
            .map(|s| !s.starts_with('<') && !s.starts_with('['))
            .unwrap_or(false)
        {
            help_set.insert(format!("{cmd} {}", line[1]));
        } else if !groups.contains(cmd) {
            help_set.insert(cmd.clone());
        }
    }

    let mut caps_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (cmd, info) in commands {
        if let Some(subs) = info.get("subcommands").and_then(|s| s.as_array()) {
            for s in subs {
                caps_set.insert(format!(
                    "{cmd} {}",
                    s.as_str().expect("subcommand is a string")
                ));
            }
        } else {
            caps_set.insert(cmd.clone());
        }
    }

    assert_eq!(
        help_set, caps_set,
        "capabilities --json and --help disagree — the manifest is generated from help_command_lines(), keep them consistent"
    );
}

#[test]
fn capabilities_manifest_reports_grammar_versions() {
    // The manifest must expose the tree-sitter grammar crate versions (baked
    // in by build.rs) so users can attribute grammar behavior differences.
    let caps = capabilities_manifest();
    let grammars = caps["grammars"]
        .as_object()
        .expect("manifest has a grammars object");
    for key in ["kotlin", "java", "swift"] {
        let version = grammars
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(
            !version.is_empty() && version != "unknown",
            "grammar version for {key} must be baked in at build time, got {version:?}"
        );
    }
}

#[test]
fn version_lines_reports_grammars_line() {
    // `--version` contract: first line is the tool version, second line lists
    // the tree-sitter grammar crate versions (grammar version visibility).
    let binding = version_lines();
    let lines: Vec<&str> = binding.lines().collect();
    assert_eq!(lines.len(), 2, "version output has exactly two lines");
    assert!(
        lines[0].starts_with("kotlin-lsp "),
        "first line is the tool version, got {:?}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("grammars: kotlin "),
        "second line reports grammar versions, got {:?}",
        lines[1]
    );
}

// ── docs top-level alias (#219) ────────────────────────────────────────────────
#[test]
fn docs_top_level_parses_query() {
    let args = parse(&["docs", "StateFlow"]).unwrap().unwrap();
    match &args.subcommand {
        Subcommand::Docs { query } => {
            assert_eq!(query, "StateFlow");
        }
        other => panic!("expected Docs, got {other:?}"),
    }
}

#[test]
fn docs_requires_query() {
    let err = parse(&["docs"]).unwrap_err();
    assert!(err.contains("QUERY"), "got: {err}");
}

// ── call hierarchy by name (#218) ─────────────────────────────────────────────

#[test]
fn call_hierarchy_by_name_parses_symbol() {
    let args = parse(&["call", "hierarchy", "AuthViewModel"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::Call { sub } => match sub {
            CallSub::Hierarchy { name, .. } => {
                assert_eq!(name.as_deref(), Some("AuthViewModel"));
            }
            CallSub::Diff { .. } => panic!("expected Hierarchy, got Diff"),
            CallSub::Reach { .. } => panic!("expected Hierarchy, got Reach"),
        },
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn call_reach_parses_entry_target_depth() {
    let args = parse(&[
        "call",
        "reach",
        "runCheckout",
        "--to",
        "sendEmail",
        "--max-depth",
        "5",
    ])
    .unwrap()
    .unwrap();
    match &args.subcommand {
        Subcommand::Call { sub } => match sub {
            CallSub::Reach {
                entry,
                target,
                max_depth,
            } => {
                assert_eq!(entry, "runCheckout");
                assert_eq!(target.as_deref(), Some("sendEmail"));
                assert_eq!(*max_depth, 5);
            }
            CallSub::Hierarchy { .. } => panic!("expected Reach, got Hierarchy"),
            CallSub::Diff { .. } => panic!("expected Reach, got Diff"),
        },
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn call_reach_parses_without_target_uses_default_depth() {
    let args = parse(&["call", "reach", "boot"]).unwrap().unwrap();
    match &args.subcommand {
        Subcommand::Call { sub } => match sub {
            CallSub::Reach {
                entry,
                target,
                max_depth,
            } => {
                assert_eq!(entry, "boot");
                assert!(target.is_none());
                assert_eq!(
                    *max_depth,
                    crate::cli::reach::DEFAULT_MAX_DEPTH,
                    "default depth applies when --max-depth is omitted"
                );
            }
            CallSub::Hierarchy { .. } => panic!("expected Reach, got Hierarchy"),
            CallSub::Diff { .. } => panic!("expected Reach, got Diff"),
        },
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn call_hierarchy_positional_parses_file_line_col() {
    let args = parse(&["call", "hierarchy", "src/Foo.kt", "42", "10"])
        .unwrap()
        .unwrap();
    match &args.subcommand {
        Subcommand::Call { sub } => match sub {
            CallSub::Hierarchy {
                name,
                file,
                line,
                col,
                ..
            } => {
                assert!(name.is_none());
                assert_eq!(file, &std::path::PathBuf::from("src/Foo.kt"));
                assert_eq!(*line, 42);
                assert_eq!(*col, 10);
            }
            CallSub::Diff { .. } => panic!("expected Hierarchy, got Diff"),
            CallSub::Reach { .. } => panic!("expected Hierarchy, got Reach"),
        },
        other => panic!("expected Call, got {other:?}"),
    }
}

// ── capabilities (#220) ────────────────────────────────────────────────────────

#[test]
fn capabilities_parses_as_subcommand() {
    let args = parse(&["capabilities"]).unwrap().unwrap();
    assert!(matches!(args.subcommand, Subcommand::Capabilities));
}

#[test]
fn capabilities_json_flag_is_accepted() {
    // --json should not cause a parse error
    let args = parse(&["capabilities", "--json"]).unwrap().unwrap();
    assert!(matches!(args.subcommand, Subcommand::Capabilities));
}
