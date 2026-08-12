use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tower_lsp::lsp_types::{Range, SymbolKind};

/// File language, derived from path extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Language {
    Kotlin,
    Java,
    Swift,
}

impl Language {
    pub(crate) fn from_path(path: &str) -> Self {
        if path.ends_with(".java") {
            Language::Java
        } else if path.ends_with(".swift") {
            Language::Swift
        } else {
            Language::Kotlin
        }
    }

    pub(crate) fn code_fence(self) -> &'static str {
        match self {
            Language::Kotlin => "kotlin",
            Language::Java => "java",
            Language::Swift => "swift",
        }
    }

    pub(crate) fn needs_semicolons(self) -> bool {
        matches!(self, Language::Java)
    }

    pub(crate) fn val_keyword(self) -> &'static str {
        match self {
            Language::Swift => "let",
            _ => "val",
        }
    }
}

/// A position within a document used by infer functions.
///
/// `utf16_col` is a UTF-16 code unit offset, matching the LSP `Position.character` field.
/// Using a named struct (rather than a bare `(usize, usize)` pair) prevents silent
/// transposition of line and column arguments at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorPos {
    pub line: usize,
    pub utf16_col: usize,
}

/// The caller's position context, used for visibility filtering and type-param substitution.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CallerContext<'a> {
    pub uri: Option<&'a str>,
    pub cursor_line: Option<u32>,
}

/// Supertype relationship kind: extends (class) vs implements (interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum SuperKind {
    /// Class inheritance (`class Foo : Bar(...)`).
    #[default]
    Extends,
    /// Interface implementation (`class Foo : Bar` where Bar is an interface).
    Implements,
}
/// Kotlin/Java visibility of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) enum Visibility {
    #[default]
    Public,
    Internal,
    Protected,
    Private,
}

/// Single symbol definition entry stored in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    /// Span of the entire declaration node.
    pub range: Range,
    /// Span of only the identifier — used for `selectionRange` in DocumentSymbol.
    pub selection_range: Range,
    /// Short signature shown in hover/symbol lists.
    /// e.g. `"fun addBiometryToPowerAuth(isAllowedForActiveOp: Boolean)"`,
    ///      `"class CreatePinViewModel"`, `"val isChecked: Boolean"`.
    /// Empty string when not computed.
    #[serde(default)]
    pub detail: String,
    /// Generic type parameter names extracted from the CST at parse time.
    /// e.g. `class Foo<T, U>` → `["T", "U"]`.
    /// Empty for non-generic symbols.
    #[serde(default)]
    pub type_params: Vec<String>,
    /// For extension functions: the receiver type name (without generics).
    /// e.g. `fun MyType.foo()` → `"MyType"`, `fun <T> List<T>.bar()` → `"List"`.
    /// Empty string for non-extension symbols.
    #[serde(default)]
    pub extension_receiver: String,
    /// Whether the symbol is annotated with `@Deprecated` or `@deprecated`.
    /// Detected during parsing; surfaced as `CompletionItemTag::DEPRECATED`
    /// in completion responses.
    #[serde(default)]
    pub deprecated: bool,
    /// Enclosing class/interface/object FQ name, if any.
    #[serde(default)]
    pub parent_fq_name: Option<String>,
    /// Return type extracted from signature, if any.
    #[serde(default)]
    pub return_type: Option<String>,
    /// Parameter list: (name, type_name).
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    /// KDoc summary line, if any.
    #[serde(default)]
    pub documentation: Option<String>,
    /// Whether the class/interface is declared with the `sealed` modifier.
    #[serde(default)]
    pub is_sealed: bool,
    /// Whether the symbol is a `typealias` declaration. lsp-types has no
    /// TYPE_ALIAS kind, so `--kind typealias` needs an explicit marker
    /// (issue #269); the detail field holds source text, not this flag.
    #[serde(default)]
    pub is_typealias: bool,
}

impl SymbolEntry {
    /// Return the line number where the symbol's identifier starts.
    ///
    /// This is a convenience accessor for `.selection_range.start.line` (the identifier line),
    /// distinguishing it from `.range.start.line` (the full declaration start, which may differ on
    /// multiline declarations). Reduces coupling and avoids repeated deep field access.
    pub(crate) fn selection_start(&self) -> u32 {
        self.selection_range.start.line
    }

    /// CLI-facing kind label (`{:?}` lowercased), except typealiases get their
    /// own `typealias` kind (issue #269): lsp-types has no TYPE_ALIAS kind, and
    /// `--kind typealias` must match the declarations `--kind class` should not.
    pub(crate) fn kind_label(&self) -> String {
        if self.is_typealias {
            "typealias".to_string()
        } else {
            format!("{:?}", self.kind).to_lowercase()
        }
    }
}

/// One import statement parsed from a Kotlin file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ImportEntry {
    /// Fully-qualified path without the trailing `.*`.
    /// e.g. `"com.example.Foo"` or `"com.example"` for star imports.
    pub full_path: String,
    /// The name usable locally: last segment, alias, or `"*"` for star.
    pub local_name: String,
    /// True for `import com.example.*`.
    pub is_star: bool,
}

/// A structural syntax error detected by tree-sitter.
///
/// These are zero-false-positive issues: missing brackets, unclosed strings,
/// garbled syntax from a bad edit.  They are NOT serialized to the disk cache
/// (cheap to recompute on every parse).
#[derive(Debug, Clone)]
pub(crate) struct SyntaxError {
    pub range: Range,
    pub message: String,
}

/// Lazily-loaded raw source lines.
///
/// Parse-time files carry their lines eagerly (built from content already in
/// memory). Cache hits deserialize with empty lines — `#[serde(skip)]` keeps
/// raw source out of index.bin, which is the bulk of its payload — and
/// [`fill`](LazyLines::fill) re-reads them from disk only when a caller
/// actually needs them (hover/complete/find). Iter-based commands
/// (search/docs/imports) never touch the disk. Sharing is `Arc`-based so
/// `clone()` stays a refcount bump, same as the old `Arc<Vec<String>>`.
#[derive(Debug, Default)]
pub(crate) struct LazyLines {
    inner: Arc<OnceLock<Arc<Vec<String>>>>,
}

fn empty_lines() -> &'static Vec<String> {
    static EMPTY: Vec<String> = Vec::new();
    &EMPTY
}

impl LazyLines {
    /// Eagerly fill from source text already in memory (the parse path).
    pub(crate) fn from_content(content: &str) -> Self {
        let inner = Arc::new(OnceLock::new());
        let _ = inner.set(Arc::new(content.lines().map(str::to_owned).collect()));
        Self { inner }
    }

    /// Eagerly fill from pre-split lines (test fixtures).
    #[cfg(test)]
    pub(crate) fn from_vec(lines: Vec<String>) -> Self {
        let inner = Arc::new(OnceLock::new());
        let _ = inner.set(Arc::new(lines));
        Self { inner }
    }

    /// Fill from disk if not already filled. No-op when already filled or the
    /// file cannot be read (deleted files keep empty lines).
    pub(crate) fn fill(&self, path: &Path) {
        self.inner.get_or_init(|| {
            std::fs::read_to_string(path)
                .map(|c| Arc::new(c.lines().map(str::to_owned).collect()))
                .unwrap_or_default()
        });
    }

    pub(crate) fn is_filled(&self) -> bool {
        self.inner.get().is_some()
    }

    /// The shared lines `Arc` (empty when never filled) — for call sites whose
    /// API expects `Arc<Vec<String>>`.
    pub(crate) fn filled_arc(&self) -> Arc<Vec<String>> {
        self.inner.get().cloned().unwrap_or_default()
    }
}

impl Clone for LazyLines {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl std::ops::Deref for LazyLines {
    type Target = Vec<String>;
    fn deref(&self) -> &Vec<String> {
        match self.inner.get() {
            Some(a) => a.as_ref(),
            None => empty_lines(),
        }
    }
}

/// All data we keep in memory for one source file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FileData {
    pub symbols: Vec<SymbolEntry>,
    pub imports: Vec<ImportEntry>,
    /// Package declaration, e.g. `"com.example.app"`.
    pub package: Option<String>,
    /// Raw source lines — kept for `word_at()` lookups without hitting disk.
    /// Lazily loaded: filled at parse time, or from disk on first use after a
    /// cache hit; not serialized to the disk cache (issue #304 — raw source
    /// was the bulk of index.bin).
    #[serde(skip)]
    pub lines: LazyLines,
    /// Lower-cased identifiers found before `:` on non-comment lines.
    /// Populated once at parse time; used by completion without re-scanning.
    pub declared_names: Vec<String>,
    /// Supertype relationships extracted from the CST at parse time.
    /// Each entry is `(class_name_line, supertype_name, type_args)` where:
    /// - `class_name_line` matches `SymbolEntry::selection_range.start.line` for the declaring class
    /// - `supertype_name` is the base name without type arguments (e.g. `"FlowReducer"`)
    /// - `type_args` are the concrete type arguments (e.g. `["Event", "Effect", "State"]`)
    #[serde(default)]
    pub supers: Vec<(u32, String, Vec<String>, SuperKind)>,
    /// RHS-inferred types for unannotated properties, extracted from the CST at parse time.
    /// Each entry is `(declaration_line, var_name, inferred_type)`.
    /// Used as the primary type inference path for indexed files, avoiding fragile string
    /// scanning for patterns like `inject<T>()`, `by lazy { T() }`, and `T(args)`.
    /// Call edges extracted at parse time: (caller_name, callee_name) pairs.
    /// Used for fast callers/callees graph traversal without re-parsing source.
    #[serde(default = "Vec::new")]
    pub call_edges: Vec<(String, String)>,
    /// Annotation edges extracted at parse time: (symbol_name, annotation_name) pairs.
    /// e.g. ("MyScreen", "Composable"), ("repo", "Inject").
    /// Used for fast annotation lookups without file-level text search.
    #[serde(default = "Vec::new")]
    pub annotation_edges: Vec<(String, String)>,
    pub rhs_types: Vec<(u32, String, String)>,
    /// Method-call RHS patterns for unannotated properties: `val x = receiver.method(args)`.
    /// Each entry is `(declaration_line, var_name, receiver_name, method_name)`.
    /// Used by method-return-type inference for indexed files.
    #[serde(default)]
    pub method_call_rhs: Vec<(u32, String, String, String)>,
    /// Structural syntax errors from tree-sitter (ERROR / MISSING nodes).
    /// Transient — not serialized to disk cache.
    #[serde(skip)]
    pub syntax_errors: Vec<SyntaxError>,
    /// Whether the file is tool-generated (path convention or header banner),
    /// judged once at index time. A relevance hint for ranking — generated
    /// symbols rank below real implementations with the same name; never a
    /// hard filter (codegraph #1500).
    #[serde(default)]
    pub generated: bool,
}

impl FileData {
    /// Find the name of the innermost class/interface/object/enum that contains
    /// `line` in this file's symbol list. Returns `None` if the symbol is
    /// top-level (not inside any class).
    pub(crate) fn containing_class_at(&self, line: u32) -> Option<String> {
        const CLASS_KINDS: &[SymbolKind] = &[
            SymbolKind::CLASS,
            SymbolKind::INTERFACE,
            SymbolKind::STRUCT,
            SymbolKind::ENUM,
            SymbolKind::OBJECT,
        ];
        self.symbols
            .iter()
            .filter(|s| CLASS_KINDS.contains(&s.kind))
            .filter(|s| s.range.start.line <= line && line <= s.range.end.line)
            .min_by_key(|s| s.range.end.line.saturating_sub(s.range.start.line))
            .map(|s| s.name.clone())
    }
}

/// Result of parsing a single file. Pure data, no side effects.
/// This is what index_content will return instead of mutating DashMaps.
#[derive(Debug, Clone)]
pub(crate) struct FileIndexResult {
    /// File URI that was parsed.
    pub uri: tower_lsp::lsp_types::Url,
    /// Parsed file data (symbols, imports, package, lines).
    pub data: FileData,
    /// Supertype relationships discovered in this file.
    /// Format: (supertype_name, implementing_class_location)
    pub supertypes: Vec<(String, tower_lsp::lsp_types::Location, SuperKind)>,
    /// Content hash for cache invalidation.
    pub content_hash: u64,
    /// Parse error if tree-sitter failed.
    #[allow(dead_code)]
    pub error: Option<String>,
}

/// Statistics about an indexing run.
#[derive(Debug, Clone, Default)]
pub(crate) struct IndexStats {
    /// Total files discovered.
    #[allow(dead_code)]
    pub files_discovered: usize,
    /// Files loaded from cache (mtime unchanged).
    pub cache_hits: usize,
    /// Files actually parsed by tree-sitter.
    pub files_parsed: usize,
    /// Total symbols extracted.
    pub symbols_extracted: usize,
    /// Total packages found.
    #[allow(dead_code)]
    pub packages_found: usize,
    /// Parse errors encountered.
    #[allow(dead_code)]
    pub errors: usize,
}

/// Result of indexing an entire workspace. Pure data, no side effects.
/// This is what index_workspace will return instead of mutating state.
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceIndexResult {
    /// All successfully parsed files.
    pub files: Vec<FileIndexResult>,
    /// Statistics about the indexing run.
    pub stats: IndexStats,
    /// Workspace root that was indexed.
    #[allow(dead_code)]
    pub workspace_root: std::path::PathBuf,
    /// True if the run was aborted mid-way (e.g. root generation changed).
    /// Callers must NOT call apply_workspace_result when this is true — doing
    /// so would reset_index_state() and apply only the partial result set.
    pub aborted: bool,
    /// True when the workspace was fully scanned (not truncated by MAX_INDEX_FILES).
    /// Written into the on-disk cache so warm-manifest mode is only used when the
    /// cache is a complete snapshot of the workspace.
    pub complete_scan: bool,
}

/// Configuration toggles for inlay hints, parsed from the client's
/// `initializationOptions.inlayHints`.
///
/// All fields default to `true` (emit all hints) to preserve existing behaviour
/// when no config is provided.
#[derive(Debug, Clone)]
pub(crate) struct InlayHintConfig {
    /// Show `: Type` after implicit lambda parameter `it`.
    pub lambda_it: bool,
    /// Show `: Type` after named lambda parameters `{ item -> }`.
    pub lambda_params: bool,
    /// Show `: Type` after `this` in scope functions / class methods.
    pub this_hints: bool,
    /// Show `: InferredType` after untyped `val` / `var` declarations.
    pub untyped_vars: bool,
}

impl Default for InlayHintConfig {
    fn default() -> Self {
        Self {
            lambda_it: true,
            lambda_params: true,
            this_hints: true,
            untyped_vars: true,
        }
    }
}

impl InlayHintConfig {
    /// Parse from the client's initialization options JSON.
    ///
    /// Expected structure:
    /// ```json
    /// {
    ///   "inlayHints": {
    ///     "lambdaIt": true,
    ///     "lambdaParams": true,
    ///     "thisHints": true,
    ///     "untypedVars": true
    ///   }
    /// }
    /// ```
    pub(crate) fn from_init_opts(val: Option<&serde_json::Value>) -> Self {
        let Some(v) = val else {
            return Self::default();
        };
        let hints = v.get("inlayHints");
        let Some(hints) = hints else {
            return Self::default();
        };
        Self {
            lambda_it: hints
                .get("lambdaIt")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            lambda_params: hints
                .get("lambdaParams")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            this_hints: hints
                .get("thisHints")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            untyped_vars: hints
                .get("untypedVars")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        }
    }
}
