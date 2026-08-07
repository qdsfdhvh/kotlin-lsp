//! CLI argument parsing via lexopt.
//

use std::path::PathBuf;

/// Filters applied to `find` / `refs` output before printing.
#[derive(Debug, Default, Clone)]
pub(crate) struct ResultFilters {
    /// Print/serialize relative paths in addition to (or in place of, for plain
    /// text) absolute paths.
    pub relative: bool,
    /// Cap result count after filtering.
    pub limit: Option<usize>,
    /// Keep only results whose `module` contains this substring.
    pub module: Option<String>,
    /// Keep only results whose `sourceSet` is in this comma-separated list.
    pub source_sets: Vec<String>,
    /// Comma-separated list of symbol `SymbolKind` names to filter by
    /// (e.g. `class,fun,interface,object`). Empty means no filter.
    pub kinds: Vec<String>,
    /// Keep only results whose *owner* (enclosing class/interface/object) name
    /// contains this substring. Ignored when the search name has a dot prefix
    /// (e.g. `ScreenAction.Refresh` auto-sets owner=`ScreenAction`).
    pub owner: Option<String>,
    /// When true, strip import-statement matches from results.
    /// Useful for common names like `Event`, `Result`, `State` that appear
    /// in thousands of import lines.
    pub exclude_imports: bool,
    /// Reference classification filter: call, read, write, override, import, type-use.
    pub ref_kind: Option<String>,
    /// Symbol visibility filter: public, internal, protected, private.
    #[allow(dead_code)]
    pub visibility: Option<String>,
    /// Comma-separated list of modifiers to filter by (e.g. `abstract,suspend,open`).
    #[allow(dead_code)]
    pub modifiers: Vec<String>,
    /// Enable fuzzy (subsequence) matching for find/refs queries.
    pub fuzzy: bool,
}

#[derive(Debug)]
pub(crate) enum Subcommand {
    Find {
        name: String,
        filters: ResultFilters,
    },
    Refs {
        name: String,
        filters: ResultFilters,
        /// Label each match with its reference type (declaration, read, write, etc.).
        explain: bool,
    },
    Hover {
        file: PathBuf,
        line: u32,
        col: u32,
    },
    /// Show completion candidates at a file position (debug).
    Complete {
        file: PathBuf,
        line: u32,
        /// 1-based UTF-16 column. `None` when resolved from `--dot` or `--eol`.
        col: Option<u32>,
        /// Resolve column to just after the last `.` on the line.
        dot: bool,
        /// Resolve column to end of trimmed content on the line (bare-word prefix).
        eol: bool,
        /// Skip loading `~/.kotlin-lsp/sources` (extracted stdlib/libraries).
        /// Returns only workspace symbols. Much faster (~2s vs ~10s).
        no_stdlib: bool,
    },
    Index,
    /// Dump semantic tokens for a file (debug).
    Tokens {
        file: PathBuf,
        /// Use CST classification only; skip cross-file index resolution (default).
        cst_only: bool,
        /// Opt-in to Phase 2 cross-file resolution (loads full index).
        resolve: bool,
        /// Show per-phase token breakdown before dedup.
        phases: bool,
        /// Also print the tree-sitter parse tree after tokens.
        show_tree: bool,
    },
    /// Dump the tree-sitter parse tree for a file (debug).
    Tree {
        file: PathBuf,
    },
    /// List auto-discovered source roots for the workspace.
    Sources {
        /// Show detailed diagnostics about why each path was included/excluded.
        explain: bool,
    },
    /// Get code actions at a position. With `--apply`, apply the first match.
    CodeAction {
        file: PathBuf,
        line: u32,
        col: u32,
        /// Optional kind filter (e.g. `"quickfix"`, `"refactor.rewrite"`).
        kind: Option<String>,
        /// When true, apply the action and print the result.
        apply: bool,
    },
    /// Extract Gradle *-sources.jar files to a sourcePaths-ready directory.
    ExtractSources {
        gradle_home: Option<PathBuf>,
        output: Option<PathBuf>,
        dry_run: bool,
        patterns: Vec<String>,
    },
    /// Check files for syntax errors.  No index / LSP session needed.
    Check {
        files: Vec<PathBuf>,
        diagnose: bool,
        when_exhaustive: bool,
    },
    /// Show index cache statistics and health checks.
    Cache {
        /// Sub-command: `stats`, `verify`.
        sub: String,
    },
    /// Batch query: read JSON query specs from stdin and return results.
    Query,
    /// Run project diagnostics: source roots, cache health, library sources, etc.
    Doctor {
        /// Show verbose diagnostics.
        verbose: bool,
        /// Output structured JSON.
        json: bool,
    },
    OrganizeImports {
        files: Vec<PathBuf>,
    },
    /// Create a new file from a template.
    NewFile {
        template: String,
        name: String,
        package_name: Option<String>,
        directory: Option<PathBuf>,
    },
    /// Index JAR source files for library symbol resolution.
    IndexJars {
        /// Root directory to scan (default: current dir).
        root: Option<PathBuf>,
    },
    /// Run performance benchmarks.
    #[allow(dead_code)]
    Benchmark,
    /// One-stop symbol context: definition + hover + refs summary.
    Context {
        file: PathBuf,
        line: u32,
        col: u32,
        expand: usize,
    },
    /// Call hierarchy: find callers (--incoming) or callees (--outgoing).
    Call {
        sub: CallSub,
    },
    /// Type hierarchy: find subtypes or supertypes.
    Type {
        sub: TypeSub,
    },
    /// Find files that import a given symbol.
    ImportsOf {
        name: String,
    },
    /// Find symbols annotated with a given annotation.
    Annotated {
        annotation: String,
    },
    /// Full-text search over symbol signatures/KDoc.
    Docs {
        query: String,
    },
    /// Impact analysis: risk score, refs, callers for a symbol.
    Impact {
        file: PathBuf,
        line: u32,
        col: u32,
    },
    /// Module introspection: list, deps, files, packages.
    Module {
        sub: ModuleSub,
    },
    /// Summarize a symbol: kind, signature, members, KDoc.
    Summarize {
        name: String,
        expand: bool,
        /// Use cached summary instead of re-parsing source.
        cached: bool,
    },
    /// Find test files/methods for a symbol.
    FindTest {
        file: PathBuf,
        line: u32,
        col: u32,
    },
    /// Find KMP expect/actual declarations.
    ExpectActual {
        name: String,
    },
    /// Android introspection: list activities, find composables.
    Android {
        sub: AndroidSub,
    },
    /// Batch type injection for a file — resolve all referenced type signatures.
    Inject {
        file: PathBuf,
    },
    /// Resolve references at a specific cursor position, filtered by declaration context.
    /// One-stop file snapshot for agents: symbols, imports, diagnostics, edit anchors.
    Inspect {
        file: PathBuf,
        /// When set, include deeper signature chains.
        expand: usize,
    },
    RefsAt {
        /// File containing the symbol.
        file: PathBuf,
        /// 1-based line of the symbol.
        line: u32,
        /// 1-based UTF-16 column of the symbol.
        col: u32,
    },
    /// Rename a symbol with preview/apply.
    Rename {
        /// File containing the symbol to rename.
        file: PathBuf,
        /// 1-based line of the symbol.
        line: u32,
        /// 1-based UTF-16 column of the symbol.
        col: u32,
        /// New name for the symbol.
        new_name: String,
        /// When true, apply the rename instead of previewing.
        apply: bool,
    },
    #[allow(dead_code)]
    Batch {
        file: PathBuf,
        dry_run: bool,
        /// When true, batch-add missing imports instead of type injection.
        imports: bool,
        /// File to write output to (JSON format).
        output: Option<String>,
        /// When true, apply the edit directly.
        apply: bool,
    },
    #[allow(dead_code)]
    Insert {
        file: PathBuf,
        line: u32,
        before: bool,
        after: bool,
        content: String,
        in_place: bool,
        /// Semantic insertion mode: "import", "member", "function", "override".
        kind: Option<String>,
        /// Owner class/interface name.
        owner: Option<String>,
        dry_run: bool,
        apply: bool,
        /// Target method name for override insertion.
        name_arg: Option<String>,
    },
    /// Format checking (ktlint) — Spotless check/apply equivalent.
    Format {
        sub: FormatSub,
        files: Vec<PathBuf>,
        dry_run: bool,
    },

    /// List or read agent skills bundled with the CLI.
    Skills {
        /// Sub-args: `list` or `read <name>`.
        args: Vec<String>,
    },
    /// Workspace overview: modules, packages, symbol counts.
    Workspace,
    /// Export full symbol graph (calls, inheritance, imports).
    SymbolGraph,
    /// Export complete workspace snapshot as JSON (symbols + relationships + modules).
    Snapshot {
        /// Filter by symbol kind (comma-separated: class,fun,interface...)
        filter_kind: Option<String>,
        /// Exclude relationship graph from output (symbols only).
        exclude_relationships: bool,
    },
    /// Semantic search: natural language query over symbol index.
    Search {
        query: String,
        /// Max results to return.
        limit: usize,
    },
    /// Show AI summary cache statistics.
    SummaryCacheStats,
    /// Expose machine-readable CLI capability manifest.
    Capabilities,
    /// Show Gradle dependencies parsed from build.gradle.kts / libs.versions.toml.
    GradleDeps,
}

/// Sub-command within the `module` parent command.
#[derive(Debug, Clone)]
pub(crate) enum ModuleSub {
    List,
    Deps { module: String, direction: String },
    Files { module: String },
    Packages { package: String },
}

/// Sub-command within the `android` parent command.
#[derive(Debug, Clone)]
pub(crate) enum AndroidSub {
    Activities,
    Composables {
        file: PathBuf,
        call_graph: bool,
        state: bool,
        preview: bool,
    },
}

/// Sub-command within the `type` parent command.
#[derive(Debug, Clone)]
pub(crate) enum TypeSub {
    Hierarchy {
        name: String,
        subtypes: bool,
        supertypes: bool,
        graph: bool,
        depth: u32,
    },
    Sealed {
        name: String,
    },
}

/// Sub-command within the `call` parent command.
#[derive(Debug, Clone)]
pub(crate) enum CallSub {
    Hierarchy {
        /// When Some, resolve this symbol name to a location before computing hierarchy.
        name: Option<String>,
        file: PathBuf,
        line: u32,
        col: u32,
        incoming: bool,
        outgoing: bool,
        depth: u32,
    },
}

/// Format sub-subcommand: check (lint-only, like spotlessCheck) or apply (in-place, like spotlessApply).
#[derive(Debug, Clone)]
pub(crate) enum FormatSub {
    /// Check for formatting violations without modifying files (like spotlessCheck).
    Check,
    /// Apply formatting changes in-place (like spotlessApply).
    Apply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Load cache when available; fall back to rg/fd otherwise.
    Auto,
    /// Always use rg/fd; never load index.
    Fast,
    /// Require a warm cache; exit with error if missing.
    Smart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFmt {
    Text,
    Json,
}

#[derive(Debug)]
pub(crate) struct CliArgs {
    pub subcommand: Subcommand,
    pub mode: Mode,
    pub fmt: OutputFmt,
    pub root: Option<PathBuf>,
    pub verbose: bool,
    /// Explicit `--absolute`. Forces absolute paths even when stdout isn't a
    /// TTY (where we'd otherwise auto-enable `--relative`). Has no effect when
    /// `--relative` is also set.
    pub absolute: bool,
    /// `--flat`: emit the legacy grep-style `<path>:<line>:<col>: [<kind>] <name>`
    /// format for find/refs text output. Default is grouped (rg-style) so the
    /// path isn't repeated per match.
    pub flat: bool,
    /// `--gradle`: enable Gradle dependency resolution.
    /// When set, the indexer will parse build.gradle.kts / libs.versions.toml
    /// and index source JARs for external dependencies.
    pub gradle: bool,
}

impl CliArgs {
    pub(crate) fn parse() -> Result<Option<Self>, String> {
        Self::parse_from(lexopt::Parser::from_env())
    }

    /// Parse from a pre-built `lexopt::Parser`. Used by `parse()` and by unit
    /// tests that want to feed a fixed argv without touching `std::env`.
    fn parse_from(mut args: lexopt::Parser) -> Result<Option<Self>, String> {
        let Some(first) = parse_first_argument(&mut args)? else {
            return Ok(None);
        };
        let Some(subcommand) = parse_subcommand_name(first.clone())? else {
            return Err(format!(
                "unknown subcommand '{}'\nRun `kotlin-lsp --help` for usage.",
                first.to_string_lossy()
            ));
        };
        let parsed = parse_cli_flags(&mut args)?;
        let mode = parsed.mode;
        let fmt = parsed.fmt;
        let root = parsed.root.clone();
        let verbose = parsed.verbose;
        let absolute = parsed.absolute;
        let flat = parsed.flat;
        let gradle = parsed.gradle;
        let subcommand = build_subcommand(&subcommand, parsed)?;
        Ok(Some(Self {
            subcommand,
            mode,
            fmt,
            root,
            verbose,
            absolute,
            flat,
            gradle,
        }))
    }
}

struct ParsedCliFlags {
    mode: Mode,
    fmt: OutputFmt,
    root: Option<PathBuf>,
    positionals: Vec<String>,
    cst_only: bool,
    resolve: bool,
    phases: bool,
    show_tree: bool,
    verbose: bool,
    gradle_home: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    dry_run: bool,
    before: bool,
    after: bool,
    content: Option<String>,
    in_place: bool,
    dot: bool,
    eol: bool,
    no_stdlib: bool,
    relative: bool,
    absolute: bool,
    flat: bool,
    limit: Option<usize>,
    module_filter: Option<String>,
    package_filter: Option<String>,
    dir_filter: Option<PathBuf>,
    apply_action: bool,
    type_subtypes: bool,
    type_supertypes: bool,
    type_graph: bool,
    source_set_filter: Vec<String>,
    kind_filter: Option<String>,
    owner_filter: Option<String>,
    expand: usize,
    exclude_imports: bool,
    fuzzy_filter: bool,
    visibility_filter: Option<String>,
    modifier_filter: Option<String>,
    ref_kind: Option<String>,
    diagnose: bool,
    when_exhaustive: bool,
    name_arg: Option<String>,
    exclude_relationships: bool,
    cached: bool,
    gradle: bool,
}

fn parse_first_argument(args: &mut lexopt::Parser) -> Result<Option<std::ffi::OsString>, String> {
    match args.next().map_err(|e| e.to_string())? {
        None => Ok(None),
        Some(lexopt::Arg::Value(value)) => Ok(Some(value)),
        Some(lexopt::Arg::Short('h') | lexopt::Arg::Long("help")) => {
            print_help();
            std::process::exit(0);
        }
        Some(lexopt::Arg::Short('V') | lexopt::Arg::Long("version")) => {
            print_version();
            std::process::exit(0);
        }
        Some(lexopt::Arg::Long(flag)) if is_subcommand(flag) => Err(format!(
            "'{flag}' is a subcommand, not a flag — use `kotlin-lsp {flag}` (without --)"
        )),
        Some(lexopt::Arg::Short(_) | lexopt::Arg::Long(_)) => Ok(None),
    }
}

fn parse_subcommand_name(first: std::ffi::OsString) -> Result<Option<String>, String> {
    let subcommand = first.to_string_lossy().into_owned();
    if is_subcommand(&subcommand) {
        Ok(Some(subcommand))
    } else {
        Ok(None)
    }
}

fn parse_cli_flags(args: &mut lexopt::Parser) -> Result<ParsedCliFlags, String> {
    let mut parsed = ParsedCliFlags {
        mode: Mode::Auto,
        fmt: OutputFmt::Text,
        root: None,
        positionals: Vec::new(),
        cst_only: false,
        resolve: false,
        phases: false,
        show_tree: false,
        verbose: false,
        gradle_home: None,
        output_dir: None,
        dry_run: false,
        before: false,
        after: false,
        content: None,
        in_place: false,
        dot: false,
        eol: false,
        no_stdlib: false,
        relative: false,
        absolute: false,
        flat: false,
        limit: None,
        module_filter: None,
        apply_action: false,
        kind_filter: None,
        owner_filter: None,
        package_filter: None,
        dir_filter: None,
        source_set_filter: Vec::new(),
        type_subtypes: false,
        type_supertypes: false,
        type_graph: false,
        expand: 0,

        exclude_imports: false,
        fuzzy_filter: false,
        visibility_filter: None,
        modifier_filter: None,
        ref_kind: None,
        diagnose: false,
        when_exhaustive: false,
        exclude_relationships: false,
        name_arg: None,
        cached: false,
        gradle: false,
    };

    loop {
        match args.next().map_err(|e| e.to_string())? {
            None => return Ok(parsed),
            Some(lexopt::Arg::Long("fast")) => parsed.mode = Mode::Fast,
            Some(lexopt::Arg::Long("smart")) => parsed.mode = Mode::Smart,
            Some(lexopt::Arg::Long("json")) => parsed.fmt = OutputFmt::Json,
            Some(lexopt::Arg::Long("cst-only")) => parsed.cst_only = true,
            Some(lexopt::Arg::Long("resolve")) => parsed.resolve = true,
            Some(lexopt::Arg::Long("phases")) => parsed.phases = true,
            Some(lexopt::Arg::Long("tree")) => parsed.show_tree = true,
            Some(lexopt::Arg::Short('v') | lexopt::Arg::Long("verbose")) => parsed.verbose = true,
            Some(lexopt::Arg::Long("root")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.root = Some(PathBuf::from(value.to_string_lossy().as_ref()));
            }
            Some(lexopt::Arg::Long("gradle-home")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.gradle_home = Some(PathBuf::from(value.to_string_lossy().as_ref()));
            }
            Some(lexopt::Arg::Long("output")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.output_dir = Some(PathBuf::from(value.to_string_lossy().as_ref()));
            }
            Some(lexopt::Arg::Long("dry-run")) => parsed.dry_run = true,
            Some(lexopt::Arg::Long("before")) => parsed.before = true,
            Some(lexopt::Arg::Long("after")) => parsed.after = true,
            Some(lexopt::Arg::Long("content")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.content = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("in-place")) => parsed.in_place = true,
            Some(lexopt::Arg::Short('d') | lexopt::Arg::Long("dot")) => parsed.dot = true,
            Some(lexopt::Arg::Short('e') | lexopt::Arg::Long("eol")) => parsed.eol = true,
            Some(lexopt::Arg::Long("no-stdlib")) => parsed.no_stdlib = true,
            Some(lexopt::Arg::Long("relative")) => parsed.relative = true,
            Some(lexopt::Arg::Long("apply")) => parsed.apply_action = true,
            Some(lexopt::Arg::Long("subtypes")) => parsed.type_subtypes = true,
            Some(lexopt::Arg::Long("supertypes")) => parsed.type_supertypes = true,
            Some(lexopt::Arg::Long("graph")) => parsed.type_graph = true,
            Some(lexopt::Arg::Long("absolute")) => parsed.absolute = true,
            Some(lexopt::Arg::Long("flat")) => parsed.flat = true,
            Some(lexopt::Arg::Long("exclude-imports")) => parsed.exclude_imports = true,
            Some(lexopt::Arg::Long("expand")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.expand = value.to_string_lossy().parse().unwrap_or(0);
            }
            Some(lexopt::Arg::Long("cached")) => parsed.cached = true,
            Some(lexopt::Arg::Long("name")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.name_arg = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("limit")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                let raw = value.to_string_lossy();
                let n: usize = raw
                    .parse()
                    .map_err(|_| format!("--limit expects a non-negative integer, got '{raw}'"))?;
                parsed.limit = Some(n);
            }
            Some(lexopt::Arg::Long("owner")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.owner_filter = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("kind")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.kind_filter = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("package")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.package_filter = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("dir")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.dir_filter = Some(PathBuf::from(value.to_string_lossy().as_ref()));
            }
            Some(lexopt::Arg::Long("module")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.module_filter = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("ref-kind")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                parsed.ref_kind = Some(value.to_string_lossy().into_owned());
            }
            Some(lexopt::Arg::Long("exclude-relationships")) => {
                parsed.exclude_relationships = true;
            }
            Some(lexopt::Arg::Long("gradle")) => parsed.gradle = true,
            Some(lexopt::Arg::Long("when-exhaustive")) => {
                parsed.when_exhaustive = true;
            }
            Some(lexopt::Arg::Long("diagnose")) => {
                parsed.diagnose = true;
            }
            Some(lexopt::Arg::Long("source-set")) => {
                let value = args.value().map_err(|e| e.to_string())?;
                // Comma-separated → OR over source sets so callers can write
                // `--source-set commonMain,androidMain`.
                for s in value.to_string_lossy().split(',') {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        parsed.source_set_filter.push(trimmed.to_owned());
                    }
                }
            }
            Some(lexopt::Arg::Short('h') | lexopt::Arg::Long("help")) => {
                print_help();
                std::process::exit(0);
            }
            Some(lexopt::Arg::Short('V') | lexopt::Arg::Long("version")) => {
                print_version();
                std::process::exit(0);
            }
            Some(lexopt::Arg::Value(value)) => parsed
                .positionals
                .push(value.to_string_lossy().into_owned()),
            Some(lexopt::Arg::Short(flag)) => return Err(format!("Unknown short flag: -{flag}")),
            Some(lexopt::Arg::Long(flag)) => return Err(format!("Unknown flag: --{flag}")),
        }
    }
}

fn build_subcommand(subcommand: &str, parsed: ParsedCliFlags) -> Result<Subcommand, String> {
    let ParsedCliFlags {
        positionals,
        cst_only,
        resolve,
        phases,
        show_tree,
        gradle_home,
        output_dir,
        dry_run,
        before,
        after,
        content,
        in_place,
        dot,
        eol,
        no_stdlib,
        relative,
        limit,
        module_filter,
        source_set_filter,
        kind_filter,
        apply_action,
        package_filter,
        dir_filter,
        type_subtypes,
        type_supertypes,
        type_graph,
        exclude_imports,
        fuzzy_filter,
        visibility_filter,
        modifier_filter,
        expand,
        owner_filter,
        ref_kind,
        ..
    } = parsed;
    let filters = ResultFilters {
        relative,
        kinds: kind_filter
            .as_ref()
            .map(|k| k.split(',').map(str::to_owned).collect())
            .unwrap_or_default(),
        limit,
        module: module_filter,
        source_sets: source_set_filter,
        owner: owner_filter,
        exclude_imports,
        fuzzy: fuzzy_filter,
        ref_kind,
        visibility: visibility_filter,
        modifiers: modifier_filter
            .as_ref()
            .map(|m| m.split(',').map(str::to_owned).collect())
            .unwrap_or_default(),
    };
    match subcommand {
        "find" => Ok(Subcommand::Find {
            name: first_positional(positionals, "find requires a NAME argument")?,
            filters,
        }),
        "refs" => {
            let explain_refs = positionals.get(1).map(|s| s.as_str()) == Some("explain");
            Ok(Subcommand::Refs {
                name: first_positional(positionals, "refs requires a NAME argument")?,
                filters,
                explain: explain_refs,
            })
        }
        "hover" => build_hover_subcommand(positionals),
        "complete" => build_complete_subcommand(positionals, dot, eol, no_stdlib),
        "index" => Ok(Subcommand::Index),
        "index-jars" => {
            let root = positionals.first().map(PathBuf::from);
            Ok(Subcommand::IndexJars { root })
        }
        "search" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            let rest = positionals.get(1..).unwrap_or(&[]);
            match sub.as_str() {
                "docs" => {
                    let query = rest.first().cloned().ok_or("search docs requires QUERY")?;
                    Ok(Subcommand::Docs { query })
                }
                "semantic" => {
                    let query = rest.first().cloned().ok_or("search requires QUERY")?;
                    let limit = parsed.limit.unwrap_or(20);
                    Ok(Subcommand::Search { query, limit })
                }
                "summarize" => {
                    let name = rest.first().cloned().unwrap_or_default();
                    if name.is_empty() || name == "help" {
                        return Err("search summarize requires a symbol name".to_string());
                    }
                    Ok(Subcommand::Summarize {
                        name,
                        expand: parsed.expand > 0,
                        cached: parsed.cached,
                    })
                }
                "cache-stats" => Ok(Subcommand::SummaryCacheStats),
                "imports" => {
                    let name = rest
                        .first()
                        .cloned()
                        .ok_or("search imports requires a NAME argument")?;
                    Ok(Subcommand::ImportsOf { name })
                }
                "annotated" => {
                    let annotation = rest
                        .first()
                        .cloned()
                        .ok_or("search annotated requires an ANNOTATION argument")?;
                    Ok(Subcommand::Annotated { annotation })
                }
                "find-test" => {
                    let (file, line, col) = parse_file_line_col(rest.to_vec(), "search find-test")?;
                    Ok(Subcommand::FindTest { file, line, col })
                }
                "expect-actual" => {
                    let name = rest
                        .first()
                        .cloned()
                        .ok_or("search expect-actual requires a NAME argument")?;
                    Ok(Subcommand::ExpectActual { name })
                }
                "" => Err("search requires QUERY".into()),
                // Treat unrecognized subcommand as a semantic search query
                // so `kotlin-lsp search <query>` works as documented in help text and SKILL.md
                o => {
                    let limit = parsed.limit.unwrap_or(20);
                    Ok(Subcommand::Search {
                        query: o.to_string(),
                        limit,
                    })
                }
            }
        }
        "edit" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            let rem = positionals[1..].to_vec();
            match sub.as_str() {
                "rename" => {
                    let (f, l, c) = parse_file_line_col(rem, "edit rename")?;
                    let nn = positionals.get(4).ok_or("edit rename: <file> <line> <col> <newName>")?.to_string();
                    Ok(Subcommand::Rename { file: f, line: l, col: c, new_name: nn, apply: apply_action })
                }
                "batch" => {
                    let f = PathBuf::from(rem.first().cloned().unwrap_or_default());
                    if f.as_os_str().is_empty() { return Err("edit batch requires FILE".into()); }
                    Ok(Subcommand::Batch { file: f, dry_run, imports: false, apply: apply_action, output: None })
                }
                "imports" => {
                    let f = PathBuf::from(rem.first().cloned().unwrap_or_default());
                    if f.as_os_str().is_empty() { return Err("edit imports requires FILE".into()); }
                    Ok(Subcommand::Batch { file: f, dry_run, imports: true, apply: apply_action, output: None })
                }
                "inject" => {
                    let f = PathBuf::from(rem.first().cloned().unwrap_or_default());
                    if f.as_os_str().is_empty() { return Err("edit inject requires FILE".into()); }
                    Ok(Subcommand::Inject { file: f })
                }
                "insert" => build_insert_subcommand(rem, before, after, content, in_place),
                "new" => {
                    let tpl = rem.first().cloned().unwrap_or_default();
                    let n = rem.get(1).cloned().unwrap_or_default();
                    if tpl.is_empty() || n.is_empty() { return Err("edit new: <template> <Name>".into()); }
                    Ok(Subcommand::NewFile { template: tpl, name: n, package_name: package_filter.clone(), directory: dir_filter.clone() })
                }
                "organize" => {
                    let files: Vec<PathBuf> = rem.iter().map(PathBuf::from).collect();
                    Ok(Subcommand::OrganizeImports { files })
                }
                o => Err(format!("unknown edit subcommand '{o}'. Available: rename, batch, imports, inject, insert, new, organize")),
            }
        }
        "tool" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            let rem = positionals[1..].to_vec();
            match sub.as_str() {
                "tokens" => {
                    let f = PathBuf::from(rem.first().cloned().ok_or("tool tokens requires FILE")?);
                    Ok(Subcommand::Tokens { file: f, cst_only, resolve, phases, show_tree })
                }
                "tree" => {
                    let f = PathBuf::from(rem.first().cloned().ok_or("tool tree requires FILE")?);
                    Ok(Subcommand::Tree { file: f })
                }
                "inspect" => {
                    let f = PathBuf::from(rem.first().cloned().ok_or("tool inspect requires FILE")?);
                    Ok(Subcommand::Inspect { file: f, expand })
                }
                "graph" => Ok(Subcommand::SymbolGraph),
                "snapshot" => Ok(Subcommand::Snapshot { filter_kind: None, exclude_relationships: parsed.exclude_relationships }),
                "bench" => Ok(Subcommand::Benchmark),
                "doctor" => Ok(Subcommand::Doctor { verbose: parsed.verbose, json: parsed.fmt == OutputFmt::Json }),
                "workspace" => Ok(Subcommand::Workspace),
                "query" => Ok(Subcommand::Query),
                "skills" => Ok(Subcommand::Skills { args: rem }),
                "code-action" => {
                    let (f, l, c) = parse_file_line_col(rem, "tool code-action")?;
                    Ok(Subcommand::CodeAction { file: f, line: l, col: c, kind: kind_filter.clone(), apply: apply_action })
                }
                o => Err(format!("unknown tool subcommand '{o}'. Available: tokens, tree, inspect, graph, snapshot, bench, doctor, workspace, query, skills, code-action")),
            }
        }

        "gradle-deps" => Ok(Subcommand::GradleDeps),
        "benchmark" => Ok(Subcommand::Benchmark),
        "tokens" => Ok(Subcommand::Tokens {
            file: PathBuf::from(first_positional(
                positionals,
                "tokens requires a FILE argument",
            )?),
            cst_only,
            resolve,
            phases,
            show_tree,
        }),
        "tree" => Ok(Subcommand::Tree {
            file: PathBuf::from(first_positional(
                positionals,
                "tree requires a FILE argument",
            )?),
        }),
        "callers" => {
            eprintln!("[WARN] 'callers' is deprecated; use 'call hierarchy --incoming'");
            let depth = positionals
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|d| *d > 0)
                .unwrap_or(1);
            let (file, line, col) = parse_file_line_col(positionals, "callers")?;
            Ok(Subcommand::Call {
                sub: CallSub::Hierarchy {
                    name: None,
                    file,
                    line,
                    col,
                    incoming: true,
                    outgoing: false,
                    depth,
                },
            })
        }
        "callees" => {
            eprintln!("[WARN] 'callees' is deprecated; use 'call hierarchy --outgoing'");
            let depth = positionals
                .first()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|d| *d > 0)
                .unwrap_or(1);
            let (file, line, col) = parse_file_line_col(positionals, "callees")?;
            Ok(Subcommand::Call {
                sub: CallSub::Hierarchy {
                    name: None,
                    file,
                    line,
                    col,
                    incoming: false,
                    outgoing: true,
                    depth,
                },
            })
        }
        "impact" => {
            let (file, line, col) = parse_file_line_col(positionals, "impact")?;
            Ok(Subcommand::Impact { file, line, col })
        }
        "modules" => {
            eprintln!("[WARN] 'modules' is deprecated; use 'module list'");
            Ok(Subcommand::Module {
                sub: ModuleSub::List,
            })
        }
        "module-deps" => {
            eprintln!("[WARN] 'module-deps' is deprecated; use 'module deps <name>'");
            let module = positionals.first().cloned().unwrap_or_default();
            if module.is_empty() || module == "help" {
                return Err("module-deps requires a module name".to_string());
            }
            let direction = positionals
                .get(1)
                .cloned()
                .unwrap_or_else(|| "both".to_string());
            Ok(Subcommand::Module {
                sub: ModuleSub::Deps { module, direction },
            })
        }
        "module-files" => {
            eprintln!("[WARN] 'module-files' is deprecated; use 'module files <name>'");
            let module = positionals.first().cloned().unwrap_or_default();
            if module.is_empty() || module == "help" {
                return Err("module-files requires a module name".to_string());
            }
            Ok(Subcommand::Module {
                sub: ModuleSub::Files { module },
            })
        }
        "module" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            match sub.as_str() {
                "" | "list" => Ok(Subcommand::Module {
                    sub: ModuleSub::List,
                }),
                "deps" => {
                    let module = positionals.get(1).cloned().unwrap_or_default();
                    if module.is_empty() {
                        return Err("module deps requires a module name".to_string());
                    }
                    let direction = positionals
                        .get(2)
                        .cloned()
                        .unwrap_or_else(|| "both".to_string());
                    Ok(Subcommand::Module {
                        sub: ModuleSub::Deps { module, direction },
                    })
                }
                "files" => {
                    let module = positionals.get(1).cloned().unwrap_or_default();
                    if module.is_empty() {
                        return Err("module files requires a module name".to_string());
                    }
                    Ok(Subcommand::Module {
                        sub: ModuleSub::Files { module },
                    })
                }
                "packages" => {
                    let package = positionals.get(1).cloned().unwrap_or_default();
                    Ok(Subcommand::Module {
                        sub: ModuleSub::Packages { package },
                    })
                }
                other => Err(format!(
                    "unknown module subcommand '{}'. Available: list, deps, files, packages",
                    other
                )),
            }
        }
        "summarize" => {
            let name = positionals.first().cloned().unwrap_or_default();
            if name.is_empty() || name == "help" {
                return Err("summarize requires a symbol name".to_string());
            }
            let expand = positionals.contains(&"--expand".to_string())
                || positionals.contains(&"-E".to_string());
            let cached = parsed.cached;
            Ok(Subcommand::Summarize {
                name,
                expand,
                cached,
            })
        }
        "find-test" => {
            let (file, line, col) = parse_file_line_col(positionals, "find-test")?;
            Ok(Subcommand::FindTest { file, line, col })
        }
        "expect-actual" => {
            let name = positionals.first().cloned().unwrap_or_default();
            if name.is_empty() || name == "help" {
                return Err("expect-actual requires a symbol name".to_string());
            }
            Ok(Subcommand::ExpectActual { name })
        }
        "android-activities" => {
            eprintln!("[WARN] 'android-activities' is deprecated; use 'android activities'");
            Ok(Subcommand::Android {
                sub: AndroidSub::Activities,
            })
        }
        "android-composables" => {
            eprintln!(
                "[WARN] 'android-composables' is deprecated; use 'android composables <file>'"
            );
            let file = positionals
                .first()
                .cloned()
                .map(PathBuf::from)
                .ok_or("android composables requires a FILE argument".to_string())?;
            Ok(Subcommand::Android {
                sub: AndroidSub::Composables {
                    file,
                    call_graph: false,
                    state: false,
                    preview: false,
                },
            })
        }
        "android" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            match sub.as_str() {
                "" | "activities" => Ok(Subcommand::Android {
                    sub: AndroidSub::Activities,
                }),
                "composables" => {
                    let file = positionals
                        .get(1)
                        .cloned()
                        .map(PathBuf::from)
                        .ok_or("android composables requires a FILE argument".to_string())?;
                    Ok(Subcommand::Android {
                        sub: AndroidSub::Composables {
                            file,
                            call_graph: false,
                            state: false,
                            preview: false,
                        },
                    })
                }
                other => Err(format!(
                    "unknown android subcommand '{}'. Available: activities, composables",
                    other
                )),
            }
        }
        "workspace" => Ok(Subcommand::Workspace),
        "symbol-graph" => Ok(Subcommand::SymbolGraph),
        "snapshot" => {
            let filter_kind: Option<String> = None;
            let exclude_relationships = parsed.exclude_relationships;
            Ok(Subcommand::Snapshot {
                filter_kind,
                exclude_relationships,
            })
        }
        "sources" => Ok(Subcommand::Sources {
            explain: positionals.first().map(|s| s.as_str()) == Some("explain"),
        }),
        "extract-sources" => Ok(Subcommand::ExtractSources {
            gradle_home,
            output: output_dir,
            dry_run,
            patterns: positionals,
        }),
        "check" => Ok(Subcommand::Check {
            files: positionals.into_iter().map(PathBuf::from).collect(),
            diagnose: parsed.diagnose,
            when_exhaustive: parsed.when_exhaustive,
        }),
        "cache" => Ok(Subcommand::Cache {
            sub: positionals.first().cloned().unwrap_or_default(),
        }),
        "rename" => {
            let (file, line, col) = parse_file_line_col(positionals.clone(), "rename")?;
            let new_name = positionals
                .get(3)
                .ok_or("rename requires: <file> <line> <col> <newName>")?
                .to_string();
            Ok(Subcommand::Rename {
                file,
                line,
                col,
                new_name,
                apply: apply_action,
            })
        }
        "code-action" | "code_action" => {
            let (file, line, col) = parse_file_line_col(positionals, "code-action")?;
            Ok(Subcommand::CodeAction {
                file,
                line,
                col,
                kind: kind_filter.clone(),
                apply: apply_action,
            })
        }
        "batch-imports" => {
            let file_path = PathBuf::from(first_positional(
                positionals,
                "batch-imports requires a FILE argument",
            )?);
            Ok(Subcommand::Batch {
                file: file_path,
                dry_run,
                imports: true,
                output: output_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
                apply: apply_action,
            })
        }
        "new-file" => {
            let template = positionals.first().cloned().unwrap_or_default();
            let name = positionals.get(1).cloned().unwrap_or_default();
            if template.is_empty() || name.is_empty() {
                return Err(
                    "Usage: kotlin-lsp new-file <template> <Name> [--package <pkg>] [--dir <dir>]"
                        .into(),
                );
            }
            Ok(Subcommand::NewFile {
                template,
                name,
                package_name: package_filter.clone(),
                directory: dir_filter.clone(),
            })
        }
        "inject" => Ok(Subcommand::Inject {
            file: PathBuf::from(first_positional(
                positionals,
                "inject requires a FILE argument",
            )?),
        }),
        "insert" => build_insert_subcommand(positionals, before, after, content, in_place),
        "insert-import" => build_semantic_insert(
            positionals.clone(),
            "import",
            content,
            dry_run,
            apply_action,
            parsed.name_arg.clone(),
        ),
        "insert-member" => build_semantic_insert(
            positionals.clone(),
            "member",
            content,
            dry_run,
            apply_action,
            parsed.name_arg.clone(),
        ),
        "insert-function" => build_semantic_insert(
            positionals.clone(),
            "function",
            content,
            dry_run,
            apply_action,
            parsed.name_arg.clone(),
        ),
        "insert-override" => build_semantic_insert(
            positionals.clone(),
            "override",
            content,
            dry_run,
            apply_action,
            parsed.name_arg.clone(),
        ),
        "batch" => Ok(Subcommand::Batch {
            file: PathBuf::from(first_positional(
                positionals,
                "batch requires a RULE_JSON argument",
            )?),
            dry_run,
            imports: false,
            output: None,
            apply: false,
        }),
        "organize-imports" => Ok(Subcommand::OrganizeImports {
            files: positionals.into_iter().map(PathBuf::from).collect(),
        }),
        "inspect" => {
            let file = PathBuf::from(first_positional(
                positionals,
                "inspect requires a FILE argument",
            )?);
            Ok(Subcommand::Inspect { file, expand })
        }
        "refs-at" => {
            let (file, line, col) = parse_file_line_col(positionals, "refs-at")?;
            Ok(Subcommand::RefsAt { file, line, col })
        }
        "context" => {
            let (file, line, col) = parse_file_line_col(positionals, "context")?;
            Ok(Subcommand::Context {
                file,
                line,
                col,
                expand: parsed.expand,
            })
        }
        "call-hierarchy" => {
            eprintln!("[WARN] 'call-hierarchy' is deprecated; use 'call hierarchy'");
            let (file, line, col) = parse_file_line_col(positionals, "call-hierarchy")?;
            Ok(Subcommand::Call {
                sub: CallSub::Hierarchy {
                    name: None,
                    file,
                    line,
                    col,
                    incoming: true,
                    outgoing: true,
                    depth: 1,
                },
            })
        }
        "call" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            match sub.as_str() {
                "sealed" => {
                    let n = positionals.get(1).cloned().unwrap_or_default();
                    if n.is_empty() {
                        return Err("type sealed requires NAME".into());
                    }
                    Ok(Subcommand::Type {
                        sub: TypeSub::Sealed { name: n },
                    })
                }
                "hierarchy" => {
                    let pos = &positionals[1..];
                    let (name, file, line, col) = match pos.len() {
                        1 => (Some(pos[0].clone()), PathBuf::new(), 0u32, 0u32),
                        _ => {
                            let (f, l, c) = parse_file_line_col(pos.to_vec(), "call hierarchy")?;
                            (None, f, l, c)
                        }
                    };
                    Ok(Subcommand::Call {
                        sub: CallSub::Hierarchy {
                            name,
                            file,
                            line,
                            col,
                            incoming: true,
                            outgoing: true,
                            depth: 1,
                        },
                    })
                }
                other => Err(format!(
                    "unknown call subcommand '{}'. Available: hierarchy, sealed",
                    other
                )),
            }
        }
        "implementations" => {
            eprintln!("[WARN] 'implementations' is deprecated; use 'type hierarchy --subtypes'");
            let name = positionals
                .first()
                .cloned()
                .ok_or("implementations requires a NAME argument")?;
            let depth = positionals.get(1).and_then(|d| d.parse().ok()).unwrap_or(1);
            Ok(Subcommand::Type {
                sub: TypeSub::Hierarchy {
                    name,
                    subtypes: true,
                    supertypes: false,
                    graph: false,
                    depth,
                },
            })
        }
        "subclasses" => {
            eprintln!("[WARN] 'subclasses' is deprecated; use 'type hierarchy --subtypes'");
            let name = positionals
                .first()
                .cloned()
                .ok_or("subclasses requires a NAME argument")?;
            let depth = positionals.get(1).and_then(|d| d.parse().ok()).unwrap_or(1);
            Ok(Subcommand::Type {
                sub: TypeSub::Hierarchy {
                    name,
                    subtypes: true,
                    supertypes: false,
                    graph: false,
                    depth,
                },
            })
        }
        "type" => {
            let sub = positionals.first().cloned().unwrap_or_default();
            match sub.as_str() {
                "sealed" => {
                    let n = positionals.get(1).cloned().unwrap_or_default();
                    if n.is_empty() {
                        return Err("type sealed requires NAME".into());
                    }
                    Ok(Subcommand::Type {
                        sub: TypeSub::Sealed { name: n },
                    })
                }
                "hierarchy" => {
                    let remaining = positionals[1..].to_vec();
                    build_type_subcommand(remaining, type_subtypes, type_supertypes, type_graph)
                }
                other => Err(format!(
                    "unknown type subcommand '{}'. Available: hierarchy, sealed",
                    other
                )),
            }
        }
        "imports-of" => {
            let name = positionals
                .first()
                .cloned()
                .ok_or("imports-of requires a NAME argument")?;
            Ok(Subcommand::ImportsOf { name })
        }
        "annotated" => {
            let annotation = positionals
                .first()
                .cloned()
                .ok_or("annotated requires an ANNOTATION argument")?;
            Ok(Subcommand::Annotated { annotation })
        }
        "package-deps" => {
            eprintln!("[WARN] 'package-deps' is deprecated; use 'module packages <name>'");
            let package = positionals.first().cloned().unwrap_or_default();
            Ok(Subcommand::Module {
                sub: ModuleSub::Packages { package },
            })
        }
        "docs" => {
            let query = positionals
                .first()
                .cloned()
                .ok_or("docs requires a QUERY argument")?;
            Ok(Subcommand::Docs { query })
        }
        "capabilities" => Ok(Subcommand::Capabilities),
        "summary-cache" => Ok(Subcommand::SummaryCacheStats),
        "type-hierarchy" => {
            eprintln!("[WARN] 'type-hierarchy' is deprecated; use 'type hierarchy'");
            build_type_subcommand(positionals, type_subtypes, type_supertypes, type_graph)
        }
        "format" => {
            if positionals.is_empty() {
                return Err("format requires a subcommand: 'check' or 'apply'".to_string());
            }
            let sub = positionals[0].as_str();
            let sub = match sub {
                "check" => FormatSub::Check,
                "apply" => FormatSub::Apply,
                other => {
                    return Err(format!(
                        "unknown format subcommand '{other}'; expected 'check' or 'apply'"
                    ));
                }
            };
            let files: Vec<PathBuf> = positionals[1..].iter().map(PathBuf::from).collect();
            if files.is_empty() {
                return Err(
                    "format requires at least one FILE or DIRECTORY argument after 'check'/'apply'"
                        .to_string(),
                );
            }
            Ok(Subcommand::Format {
                sub,
                files,
                dry_run: parsed.dry_run,
            })
        }
        "skills" => {
            let args = positionals; // positional args after 'skills'
            Ok(Subcommand::Skills { args })
        }
        "query" => Ok(Subcommand::Query),
        "doctor" => Ok(Subcommand::Doctor {
            verbose: parsed.verbose,
            json: parsed.fmt == OutputFmt::Json,
        }),
        _ => unreachable!(),
    }
}

fn build_hover_subcommand(positionals: Vec<String>) -> Result<Subcommand, String> {
    let (file, line, col) = parse_file_line_col(positionals, "hover")?;
    Ok(Subcommand::Hover { file, line, col })
}

fn build_complete_subcommand(
    positionals: Vec<String>,
    dot: bool,
    eol: bool,
    no_stdlib: bool,
) -> Result<Subcommand, String> {
    let mut iter = positionals.into_iter();
    let file = PathBuf::from(iter.next().ok_or("complete requires a FILE argument")?);
    let line = iter
        .next()
        .ok_or("complete requires a LINE argument")?
        .parse::<u32>()
        .map_err(|_| "LINE must be a positive integer".to_string())?;
    if line == 0 {
        return Err("LINE must be >= 1 (positions are 1-based)".to_string());
    }
    if dot && eol {
        return Err("--dot and --eol are mutually exclusive".to_string());
    }
    // col is optional when --dot or --eol is given
    let col = match iter.next() {
        Some(s) => {
            let c = s
                .parse::<u32>()
                .map_err(|_| "COL must be a positive integer".to_string())?;
            if c == 0 {
                return Err("COL must be >= 1 (positions are 1-based)".to_string());
            }
            Some(c)
        }
        None => {
            if !dot && !eol {
                return Err("complete requires a COL argument (or use --dot / --eol)".to_string());
            }
            None
        }
    };
    Ok(Subcommand::Complete {
        file,
        line,
        col,
        dot,
        eol,
        no_stdlib,
    })
}

fn build_insert_subcommand(
    positionals: Vec<String>,
    before: bool,
    after: bool,
    content: Option<String>,
    in_place: bool,
) -> Result<Subcommand, String> {
    let mut iter = positionals.into_iter();
    let file = PathBuf::from(iter.next().ok_or("insert requires a FILE argument")?);
    let line = iter
        .next()
        .ok_or("insert requires a LINE argument")?
        .parse::<u32>()
        .map_err(|_| "LINE must be a positive integer".to_string())?;
    if line == 0 {
        return Err("LINE must be >= 1 (positions are 1-based)".to_string());
    }
    if before == after {
        return Err("insert requires exactly one of --before or --after".to_string());
    }
    let content = content.ok_or("insert requires --content <text>")?;
    Ok(Subcommand::Insert {
        file,
        line,
        before,
        after,
        content,
        in_place,
        kind: None,
        owner: None,
        dry_run: false,
        apply: false,
        name_arg: None,
    })
}

fn parse_file_line_col(
    positionals: Vec<String>,
    name: &'static str,
) -> Result<(PathBuf, u32, u32), String> {
    let mut iter = positionals.into_iter();
    let file = PathBuf::from(
        iter.next()
            .ok_or_else(|| format!("{name} requires FILE LINE COL arguments"))?,
    );
    let line = iter
        .next()
        .ok_or_else(|| format!("{name} requires LINE argument"))?
        .parse::<u32>()
        .map_err(|_| "LINE must be a positive integer".to_string())?;
    if line == 0 {
        return Err("LINE must be >= 1 (positions are 1-based)".to_string());
    }
    let col = iter
        .next()
        .ok_or_else(|| format!("{name} requires COL argument"))?
        .parse::<u32>()
        .map_err(|_| "COL must be a positive integer".to_string())?;
    if col == 0 {
        return Err("COL must be >= 1 (positions are 1-based)".to_string());
    }
    Ok((file, line, col))
}

fn build_type_subcommand(
    positionals: Vec<String>,
    type_subtypes: bool,
    type_supertypes: bool,
    type_graph: bool,
) -> Result<Subcommand, String> {
    let mut name: Option<String> = None;
    for arg in &positionals {
        if name.is_none() {
            name = Some(arg.clone());
        }
    }
    let name = name.ok_or("type hierarchy requires a NAME argument")?;
    let subtypes = type_subtypes || !type_supertypes;
    Ok(Subcommand::Type {
        sub: TypeSub::Hierarchy {
            name,
            subtypes,
            supertypes: type_supertypes,
            graph: type_graph,
            depth: 1,
        },
    })
}

fn first_positional(
    positionals: Vec<String>,
    missing_message: &'static str,
) -> Result<String, String> {
    positionals
        .into_iter()
        .next()
        .ok_or_else(|| missing_message.to_string())
}

fn is_subcommand(value: &str) -> bool {
    matches!(
        value,
        "find"
            | "refs"
            | "hover"
            | "complete"
            | "index"
            | "index-jars"
            | "sources"
            | "extract-sources"
            | "cache"
            | "docs"
            | "capabilities"
            | "check"
            | "context"
            | "call"
            | "type"
            | "format"
            | "search"
            | "edit"
            | "tool"
            | "gradle-deps"
            | "impact"
            | "module"
            | "android"
    )
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;

fn print_version() {
    println!("kotlin-lsp {}", env!("CARGO_PKG_VERSION"));
}

fn print_help() {
    println!(
        "kotlin-lsp {} — Kotlin/Java symbol navigation

USAGE:
    kotlin-lsp <SUBCOMMAND> [OPTIONS] [ARGS]
    kotlin-lsp                            # start LSP server (stdio)

Output is tuned for AI agents: text mode is minimal (one record per line,
grep-friendly), and `--json` emits compact JSON (no pretty-print). Pipe to
`jq` for human reading.

SUBCOMMANDS:
    find <name>                        Find declarations of a symbol
    refs <name> [explain]              Find references to a symbol
    hover <file> <line> <col>          Show type/doc info at a position
    complete <file> <line> [col]       Show completion candidates
    context <file> <line> <col>        Definition + signature + refs summary
    check <file>...                    Check syntax errors without LSP
    impact <file> <line> <col>         Impact analysis: what depends on this?
    index [--root <dir>]               Index workspace (auto-detect build system)
    index-jars [root]                  Index JAR/class files for navigation
    sources                            List auto-discovered source roots
    extract-sources [lib...]           Unpack *-sources.jar files
    cache stats                        Show disk cache statistics
    gradle-deps                        Show parsed Gradle dependencies
    docs <query>                       Search symbols by name or signature (alias of `search docs`)
    capabilities                       List CLI capabilities (use --json)

    call hierarchy <file> <line> <col>   Show callers/callees for symbol at position
    call hierarchy <name>                Show callers/callees for a symbol by name
    type hierarchy <name>                Show subtypes or supertypes
    type sealed <name>                   Show sealed subclasses
    module list                          List all project modules
    module deps <name>                   Show module dependencies
    module files <name>                  List files in a module
    module packages [name]               Show package-level import dependencies
    android activities                   List Android activities from AndroidManifest
    android composables <file>           Find @Composable functions
    format check <file/dir>...           Check formatting violations (like spotlessCheck)
    format apply <file/dir>...          Apply formatting in-place (like spotlessApply)

    search <query>                    Semantic search with TF-IDF ranking
    search semantic <query>           Semantic search (explicit)
    search docs <query>               Search symbols by name or signature (KDoc)
    search summarize <name>           Show rich summary for a symbol
    search cache-stats                Show AI summary cache statistics
    search imports <name>             Show files importing the given symbol
    search annotated <name>           Find symbols annotated with @name
    search find-test <file> <line> <col>  Find related test files
    search expect-actual <name>       Find KMP expect/actual declarations

    edit rename <file> <line> <col> <name>  Rename symbol across all files
    edit batch <rule-json>             Apply batch edit rules
    edit imports <file>                Batch add missing imports (from rules)
    edit inject <file>                 Batch-resolve referenced type signatures
    edit insert <file> <line>          Insert a snippet at a line
    edit new <template> <name>         Create a new file from a template
    edit organize <file>...            Sort, dedup, and remove unused imports

    tool tokens <file>                 Show per-phase token breakdown (debug)
    tool tree <file>                   Dump tree-sitter parse tree (debug)
    tool inspect <file>                Display detailed file diagnostics
    tool graph                         Export symbol graph
    tool snapshot                      Snapshot workspace symbols
    tool bench                         Run LSP operation benchmarks
    tool doctor                        System health diagnostics
    tool workspace                     Workspace overview
    tool query                         Batch symbol queries (--json)
    tool skills <list|read>            List or read bundled agent skills
    tool code-action <file> <line> <col>  List code actions at a position

OPTIONS:
    --fast              Use rg/fd only; never load index (default when no cache)
    --smart             Require a pre-built index; fails if missing
    --json              Output as compact JSON (no whitespace; pipe to `jq` for humans)
    --root <dir>        Workspace root (default: nearest .git dir or cwd)
    --resolve           (tokens) Load index for Phase 2 cross-file resolution
    --cst-only          (tokens) Force CST-only mode (default, kept for clarity)
    --phases            (tokens) Show per-phase token breakdown with dedup markers
    --tree              (tokens) Also print the parse tree after tokens
    --gradle-home <dir> (extract-sources) Gradle home (default: $GRADLE_USER_HOME or ~/.gradle)
    --output <dir>      (extract-sources) Output root (default: ~/.kotlin-lsp/sources)
    --dry-run           (extract-sources, batch, batch-imports) Preview only
    --before            (insert) Insert before the given line
    --after             (insert) Insert after the given line
    --content <text>    (insert) Content to insert
    --in-place          (insert) Write changes to the file instead of stdout
    -d, --dot           (complete) Resolve col to just after the last '.' on the line
    -e, --eol           (complete) Resolve col to end of trimmed content on the line
    --no-stdlib         (complete) Skip ~/.kotlin-lsp/sources; workspace symbols only (~2s)
    --relative          (find, refs) Print paths relative to --root. Auto-enabled
                        when stdout is not a TTY (typical AI agent invocation).
                        With --json, the `file` field carries the relative path
                        and `relativePath` is omitted to avoid duplication.
    --apply             (code-action) Apply the first matching code action
    --absolute          (find, refs) Force absolute paths even when piped.
                        Overrides the non-TTY auto-relative default.
    --flat              (find, refs) Use legacy `path:line:col: name` format
                        (one full path per line). Default groups by file
                        (path printed once per group, `name` omitted because
                        it's the query) — much cheaper for refs with many hits.
    --limit <n>         (find, refs) Cap result count after filtering
    --kind <k>          (find, refs) Filter by symbol kind (class,fun,interface,...)
    --module <fragment> (find, refs) Keep only results whose module path contains <fragment>
    --source-set <set>  (find, refs) Keep only results in the given source set(s).
                        Comma-separate for OR: --source-set commonMain,androidMain
    --owner <name>      (find, refs) Keep only results whose owner (enclosing
                        class/interface/object name) contains <name>
    --subtypes          (type-hierarchy) Include subtypes (default)
    --supertypes        (type-hierarchy) Include supertypes
    --package <pkg>     (new-file) Package name for generated file
    --dir <dir>         (new-file) Output directory
    --expand <n>        (context) Include surrounding source lines
    -v, --verbose       Show progress messages (indexing, cache status)
    -h, --help          Print this help
    -V, --version       Print version

EXAMPLES:
    kotlin-lsp find MyViewModel
    kotlin-lsp find MyViewModel --json --relative
    kotlin-lsp refs MyViewModel --json --source-set commonMain --limit 20
    kotlin-lsp refs MyViewModel --json --module features/play
    kotlin-lsp refs --fast MyViewModel --root ./android
    kotlin-lsp hover src/Foo.kt 42 10 --json
    kotlin-lsp complete src/Foo.kt 42 10
    kotlin-lsp complete src/Foo.kt 42 10 --json
    kotlin-lsp complete src/Foo.kt 42 --dot --json
    kotlin-lsp complete src/Foo.kt 42 --eol --json
    kotlin-lsp complete src/Foo.kt 42 --dot --no-stdlib --json
    kotlin-lsp context src/Foo.kt 42 10
    kotlin-lsp check src/Foo.kt
    kotlin-lsp edit organize src/Foo.kt
    kotlin-lsp edit insert src/Foo.kt 42 --after --content \"println(value)\" --in-place
    kotlin-lsp edit batch rules.json --dry-run
    kotlin-lsp index --root ./android
    kotlin-lsp index-jars ~/.gradle/caches
    kotlin-lsp sources --root ./android
    kotlin-lsp sources explain --json
    kotlin-lsp extract-sources
    kotlin-lsp extract-sources androidx.compose org.jetbrains.kotlin
    kotlin-lsp extract-sources --dry-run
    kotlin-lsp extract-sources --output ~/my-sources androidx.compose
    kotlin-lsp tool tokens src/Foo.kt
    kotlin-lsp tool tokens --resolve src/Foo.kt
    kotlin-lsp tool tokens src/Foo.kt --tree
    kotlin-lsp tool tree src/Foo.kt

Full command reference: https://github.com/qdsfdhvh/kotlin-lsp/blob/main/docs/commands.md",
        env!("CARGO_PKG_VERSION")
    );
}

fn build_semantic_insert(
    positionals: Vec<String>,
    kind: &str,
    content: Option<String>,
    dry_run: bool,
    apply: bool,
    name_arg: Option<String>,
) -> Result<Subcommand, String> {
    let mut iter = positionals.into_iter();
    let file = PathBuf::from(iter.next().ok_or("insert-* requires a FILE argument")?);
    let owner = iter.next();
    // For insert-import, content defaults to fqn if not explicitly provided.
    let content = match (kind, &owner, content) {
        ("import", Some(fqn), None) => format!("import {fqn}"),
        ("import", Some(fqn), Some(custom)) => {
            // Allow --content to override auto-generated import.
            if custom.is_empty() {
                format!("import {fqn}")
            } else {
                custom
            }
        }
        ("import", None, None) => {
            return Err("insert-import requires either FQN or --content <text>".to_owned());
        }
        (_, _, Some(c)) => c,
        (_, _, None) => {
            return Err(format!("insert-{kind} requires --content <text>"));
        }
    };
    Ok(Subcommand::Insert {
        file,
        line: 0,
        before: false,
        after: false,
        content,
        in_place: false,
        kind: Some(kind.to_string()),
        owner: owner.map(|s| s.to_string()),
        dry_run,
        apply,
        name_arg,
    })
}
