//! Shared edit preview/apply engine.
//!
//! Used by rename, missing imports, code-action apply and the semantic-insert
//! dispatcher. Other write commands do not inherit this engine's guarantees.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tower_lsp::lsp_types::{AnnotatedTextEdit, OneOf, TextEdit, Url, WorkspaceEdit};

/// A resolved file-level edit — the result of flattening a `WorkspaceEdit`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileEdit {
    pub(crate) path: PathBuf,
    pub(crate) edits: Vec<TextEdit>,
}

/// Existing summary shape: `files_modified` counts writes, or prospective changes
/// in a dry run. Errors and byte-identical noops never contribute to that count.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct EditSummary {
    pub(crate) files_modified: usize,
    pub(crate) files: Vec<FileEditResult>,
}

impl EditSummary {
    /// CLI callers print the existing report first, then propagate any failure.
    pub(crate) fn exit_if_failed(&self) {
        if self
            .files
            .iter()
            .any(|file| matches!(file, FileEditResult::Error { .. }))
        {
            eprintln!(
                "edit failed; {} files {}",
                self.files_modified,
                if self
                    .files
                    .iter()
                    .any(|file| matches!(file, FileEditResult::Ok { dry_run: true, .. }))
                {
                    "would change"
                } else {
                    "written"
                }
            );
            std::process::exit(1);
        }
    }
}

/// Per-file edit result.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub(crate) enum FileEditResult {
    #[serde(rename = "ok")]
    Ok {
        path: PathBuf,
        edits_applied: usize,
        dry_run: bool,
    },
    #[serde(rename = "error")]
    Error { path: PathBuf, message: String },
    #[serde(rename = "noop")]
    Noop { path: PathBuf },
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn uri_to_path(uri: &Url) -> Result<PathBuf, String> {
    uri.to_file_path()
        .map_err(|_| format!("URI is not a valid file path: {uri}"))
}

fn oneof_to_textedit(oneof: &OneOf<TextEdit, AnnotatedTextEdit>) -> TextEdit {
    match oneof {
        OneOf::Left(te) => te.clone(),
        OneOf::Right(ae) => TextEdit {
            range: ae.text_edit.range,
            new_text: ae.text_edit.new_text.clone(),
        },
    }
}

/// Validate that a path is under the given workspace root.
#[cfg(test)]
pub(crate) fn path_is_under_root(path: &Path, root: &Path) -> bool {
    path.canonicalize()
        .ok()
        .and_then(|p| root.canonicalize().ok().map(|r| p.starts_with(&r)))
        .unwrap_or(false)
}

// ── Flatten ──────────────────────────────────────────────────────────────

/// Flatten a `WorkspaceEdit` into per-file `FileEdit` entries.
pub(crate) fn flatten_workspace_edit(edit: &WorkspaceEdit) -> Result<Vec<FileEdit>, String> {
    let mut file_edits: Vec<FileEdit> = Vec::new();

    if let Some(changes) = &edit.changes {
        for (uri, text_edits) in changes {
            let path = uri_to_path(uri)?;
            file_edits.push(FileEdit {
                path,
                edits: text_edits.clone(),
            });
        }
    }

    if let Some(doc_changes) = &edit.document_changes {
        match doc_changes {
            tower_lsp::lsp_types::DocumentChanges::Edits(versioned) => {
                for ve in versioned {
                    let path = uri_to_path(&ve.text_document.uri)?;
                    file_edits.push(FileEdit {
                        path,
                        edits: ve.edits.iter().map(oneof_to_textedit).collect(),
                    });
                }
            }
            tower_lsp::lsp_types::DocumentChanges::Operations(ops) => {
                for op in ops {
                    if let tower_lsp::lsp_types::DocumentChangeOperation::Edit(ve) = op {
                        let path = uri_to_path(&ve.text_document.uri)?;
                        file_edits.push(FileEdit {
                            path,
                            edits: ve.edits.iter().map(oneof_to_textedit).collect(),
                        });
                    }
                }
            }
        }
    }

    Ok(file_edits)
}

// ── Text coordinates ─────────────────────────────────────────────────────

/// LSP lines exclude their CR, LF or CRLF terminator, including the empty EOF line.
fn text_lines(content: &str) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let bytes = content.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            lines.push((start, i));
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    lines.push((start, content.len()));
    lines
}

pub(crate) fn text_end_position(content: &str) -> tower_lsp::lsp_types::Position {
    let lines = text_lines(content);
    let (start, end) = lines.last().expect("text always has a line");
    tower_lsp::lsp_types::Position::new(
        (lines.len() - 1) as u32,
        content[*start..*end].encode_utf16().count() as u32,
    )
}

/// Generated line insertions may follow an unterminated last line. Only that
/// producer-specific EOF case adds a separator; TextEdit.new_text stays verbatim.
pub(crate) fn line_insertion_edit(
    content: &str,
    line: u32,
    mut new_text: String,
) -> Result<TextEdit, String> {
    let end = text_end_position(content);
    let position = if line <= end.line {
        tower_lsp::lsp_types::Position::new(line, 0)
    } else if line == end.line + 1 && end.character > 0 {
        new_text.insert(0, '\n');
        end
    } else {
        return Err("insertion line is out of range".to_string());
    };
    Ok(TextEdit {
        range: tower_lsp::lsp_types::Range::new(position, position),
        new_text,
    })
}

fn position_offset(
    content: &str,
    lines: &[(usize, usize)],
    pos: tower_lsp::lsp_types::Position,
) -> Result<usize, String> {
    let &(start, end) = lines
        .get(pos.line as usize)
        .ok_or_else(|| format!("line {} is out of range", pos.line))?;
    let mut units = 0;
    for (byte, ch) in content[start..end].char_indices() {
        if units == pos.character {
            return Ok(start + byte);
        }
        units += ch.len_utf16() as u32;
        if units > pos.character {
            return Err("position is inside a UTF-16 surrogate pair".to_string());
        }
    }
    if units == pos.character {
        Ok(end)
    } else {
        Err(format!(
            "character {} is out of range on line {}",
            pos.character, pos.line
        ))
    }
}

/// Strict UTF-16 edits against the original text. Equal-position inserts retain input
/// order, before any replacement starting there. Inserts inside a replacement conflict.
pub(crate) fn apply_text_edits(content: &str, edits: &[TextEdit]) -> Result<String, String> {
    let lines = text_lines(content);
    let mut ranges = Vec::with_capacity(edits.len());
    for edit in edits {
        let start = position_offset(content, &lines, edit.range.start)?;
        let end = position_offset(content, &lines, edit.range.end)?;
        if start > end {
            return Err("reversed edit range".to_string());
        }
        ranges.push((start, end, &edit.new_text));
    }
    // Stable sorting preserves the request order of equal inserts.
    ranges.sort_by_key(|&(start, end, _)| (start, end));
    let mut cursor = 0;
    let mut result = String::new();
    for (start, end, new_text) in ranges {
        if start < cursor {
            return Err("overlapping edit ranges".to_string());
        }
        result.push_str(&content[cursor..start]);
        result.push_str(new_text);
        cursor = end;
    }
    result.push_str(&content[cursor..]);
    Ok(result)
}

// Line-only compatibility interface for existing in-memory tests; disk consumers use
// raw text, because a Vec of lines cannot represent the original line terminators.
#[cfg(test)]
pub(crate) fn apply_text_edits_to_lines(
    lines: &[String],
    edits: &[TextEdit],
) -> Result<Vec<String>, String> {
    apply_text_edits(&lines.join("\n"), edits)
        .map(|text| text.lines().map(str::to_string).collect())
}

type FilePreview = HashMap<PathBuf, (Vec<String>, Vec<String>)>;

/// Preview retains terminators in line strings so joining them reproduces exact bytes.
pub(crate) fn preview_file_edits(edits: &[FileEdit]) -> Result<FilePreview, String> {
    let mut result = HashMap::new();
    let mut targets = std::collections::HashSet::new();
    for fe in edits {
        let target = fe.path.canonicalize().map_err(|e| e.to_string())?;
        if !targets.insert(target) {
            return Err("duplicate canonical edit target".to_string());
        }
        let content =
            std::fs::read_to_string(&fe.path).map_err(|e| format!("{}: {e}", fe.path.display()))?;
        let new_content = apply_text_edits(&content, &fe.edits)?;
        result.insert(
            fe.path.clone(),
            (
                content.split_inclusive('\n').map(str::to_string).collect(),
                new_content
                    .split_inclusive('\n')
                    .map(str::to_string)
                    .collect(),
            ),
        );
    }
    Ok(result)
}

// ── Apply ────────────────────────────────────────────────────────────────

/// Apply file edits to disk.
pub(crate) fn apply_file_edits(
    edits: &[FileEdit],
    root: Option<&Path>,
    dry_run: bool,
) -> EditSummary {
    apply_file_edits_with_hook(edits, root, dry_run, |_, _, _| {})
}

// Internal interleaving seam: tests mutate real files at commit boundaries. The CLI
// supplies a zero-cost no-op; there are no CLI flags/environment test switches.
#[derive(Clone, Copy)]
enum CommitStage {
    BeforeCommit,
    BeforeReplace,
}

struct PathIdentity {
    path: PathBuf,
    canonical: PathBuf,
    handle: same_file::Handle,
    link: Option<(PathBuf, std::fs::Metadata)>,
}

impl PathIdentity {
    fn capture(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| format!("identity: {e}"))?;
        let link = if metadata.is_symlink() {
            Some((
                std::fs::read_link(path).map_err(|e| e.to_string())?,
                metadata,
            ))
        } else {
            None
        };
        Ok(Self {
            path: path.to_path_buf(),
            canonical: path.canonicalize().map_err(|e| e.to_string())?,
            handle: same_file::Handle::from_path(path).map_err(|e| format!("identity: {e}"))?,
            link,
        })
    }

    fn recheck(&self) -> Result<(), String> {
        let current = Self::capture(&self.path)?;
        let same_link = match (&self.link, &current.link) {
            (None, None) => true,
            (Some((old_path, old)), Some((new_path, new))) => {
                let same = old_path == new_path
                    && old.created().ok() == new.created().ok()
                    && old.modified().ok() == new.modified().ok();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    same && old.dev() == new.dev() && old.ino() == new.ino()
                }
                #[cfg(not(unix))]
                {
                    same
                }
            }
            _ => false,
        };
        if self.canonical != current.canonical || self.handle != current.handle || !same_link {
            return Err(format!("path identity changed: {}", self.path.display()));
        }
        Ok(())
    }
}

fn path_identities(path: &Path) -> Result<Vec<PathIdentity>, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    absolute.ancestors().map(PathIdentity::capture).collect()
}

struct PreparedEdit {
    target: PathBuf,
    identities: Vec<PathIdentity>,
    content: String,
    replacement: String,
    permissions: std::fs::Permissions,
}

impl PreparedEdit {
    fn recheck(&self, root: &[PathIdentity]) -> Result<(), String> {
        for identity in root.iter().chain(&self.identities) {
            identity.recheck()?;
        }
        let metadata = std::fs::metadata(&self.target).map_err(|e| e.to_string())?;
        if metadata.permissions() != self.permissions {
            return Err("target permissions changed".to_string());
        }
        let current = std::fs::read(&self.target).map_err(|e| format!("recheck read: {e}"))?;
        if current != self.content.as_bytes() {
            return Err("file content changed since preflight".to_string());
        }
        Ok(())
    }
}

fn prepare_file(
    fe: &FileEdit,
    root: Option<&Path>,
    targets: &mut std::collections::HashSet<PathBuf>,
) -> Result<PreparedEdit, String> {
    let mut identities = path_identities(&fe.path)?;
    let target = identities[0].canonical.clone();
    identities.extend(path_identities(&target)?);
    if !targets.insert(target.clone()) {
        return Err("duplicate canonical edit target".to_string());
    }
    if root.is_some_and(|root| !target.starts_with(root)) {
        return Err("path is not under workspace root".to_string());
    }
    let metadata = std::fs::metadata(&target).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("target is not a regular file".to_string());
    }
    let content = std::fs::read_to_string(&target).map_err(|e| format!("read error: {e}"))?;
    let replacement = apply_text_edits(&content, &fe.edits)?;
    let permissions = metadata.permissions();
    if content != replacement && permissions.readonly() {
        return Err("target is read-only".to_string());
    }
    Ok(PreparedEdit {
        target,
        identities,
        content,
        replacement,
        permissions,
    })
}

fn failed_preflight(edits: &[FileEdit], errors: Vec<Option<String>>) -> EditSummary {
    EditSummary {
        files_modified: 0,
        files: edits
            .iter()
            .zip(errors)
            .map(|(fe, error)| FileEditResult::Error {
                path: fe.path.clone(),
                message: error
                    .unwrap_or_else(|| "not attempted: batch preflight failed".to_string()),
            })
            .collect(),
    }
}

fn apply_file_edits_with_hook<F: FnMut(usize, CommitStage, Option<&Path>)>(
    edits: &[FileEdit],
    root: Option<&Path>,
    dry_run: bool,
    mut hook: F,
) -> EditSummary {
    let root_identities = match root.map(path_identities).transpose() {
        Ok(identities) => identities.unwrap_or_default(),
        Err(error) => {
            return failed_preflight(
                edits,
                edits
                    .iter()
                    .map(|_| Some(format!("workspace root: {error}")))
                    .collect(),
            )
        }
    };
    let canonical_root = root_identities
        .first()
        .map(|identity| identity.canonical.as_path());
    if canonical_root.is_some_and(|root| !root.is_dir()) {
        return failed_preflight(
            edits,
            edits
                .iter()
                .map(|_| Some("workspace root is not a directory".to_string()))
                .collect(),
        );
    }
    let mut targets = std::collections::HashSet::new();
    let prepared: Vec<_> = edits
        .iter()
        .map(|fe| prepare_file(fe, canonical_root, &mut targets))
        .collect();
    if prepared.iter().any(Result::is_err) {
        return failed_preflight(edits, prepared.into_iter().map(Result::err).collect());
    }
    let prepared: Vec<_> = prepared
        .into_iter()
        .map(|p| p.expect("all preflight results checked"))
        .collect();
    if !dry_run && !prepared.is_empty() {
        hook(0, CommitStage::BeforeCommit, None);
        let errors: Vec<_> = prepared
            .iter()
            .map(|p| p.recheck(&root_identities).err())
            .collect();
        if errors.iter().any(Option::is_some) {
            return failed_preflight(edits, errors);
        }
    }
    let mut files = Vec::with_capacity(edits.len());
    let mut stopped = false;
    for (index, (fe, prepared)) in edits.iter().zip(prepared).enumerate() {
        let result = if stopped {
            Err("not attempted: earlier file failed during commit".to_string())
        } else if prepared.content == prepared.replacement {
            Ok(FileEditResult::Noop {
                path: fe.path.clone(),
            })
        } else {
            let commit = if dry_run {
                Ok(())
            } else {
                if index != 0 {
                    hook(index, CommitStage::BeforeCommit, None);
                }
                prepared.recheck(&root_identities).and_then(|()| {
                    replace_file(&prepared, &root_identities, |temp| {
                        hook(index, CommitStage::BeforeReplace, Some(temp))
                    })
                })
            };
            commit.map(|()| FileEditResult::Ok {
                path: fe.path.clone(),
                edits_applied: fe.edits.len(),
                dry_run,
            })
        };
        files.push(result.unwrap_or_else(|message| {
            stopped = true;
            FileEditResult::Error {
                path: fe.path.clone(),
                message,
            }
        }));
    }
    EditSummary {
        files_modified: files
            .iter()
            .filter(|f| matches!(f, FileEditResult::Ok { .. }))
            .count(),
        files,
    }
}

fn check_temporary_path(
    temp: &tempfile::NamedTempFile,
    handle: &same_file::Handle,
    parents: &[PathIdentity],
) -> Result<(), String> {
    for parent in parents {
        parent.recheck()?;
    }
    let metadata =
        std::fs::symlink_metadata(temp.path()).map_err(|e| format!("temporary identity: {e}"))?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err("temporary path is no longer a regular file".to_string());
    }
    let current = same_file::Handle::from_path(temp.path())
        .map_err(|e| format!("temporary identity: {e}"))?;
    if &current != handle {
        return Err("temporary file identity changed".to_string());
    }
    Ok(())
}

fn cleanup_temporary(
    temp: tempfile::NamedTempFile,
    handle: &same_file::Handle,
    parents: &[PathIdentity],
    error: String,
) -> String {
    if let Err(identity_error) = check_temporary_path(&temp, handle, parents) {
        // Cleanup is disabled: an untrusted pathname must never delete a foreign file.
        return format!("{error}; temporary cleanup skipped ({identity_error}); temporary file may remain near {}", temp.path().display());
    }
    match temp.close() {
        Ok(()) => error,
        Err(cleanup_error) => format!("{error}; temporary cleanup failed: {cleanup_error}"),
    }
}

fn replace_file<F: FnMut(&Path)>(
    prepared: &PreparedEdit,
    root: &[PathIdentity],
    mut hook: F,
) -> Result<(), String> {
    use std::io::Write;
    let parent = prepared.target.parent().ok_or("target has no parent")?;
    let parents = path_identities(parent)?;
    let mut temp = tempfile::Builder::new()
        .prefix(".kotlin-lsp-edit-")
        .tempfile_in(parent)
        .map_err(|e| format!("temporary file: {e}"))?;
    // All error paths below perform identity-checked cleanup explicitly, including
    // PersistError (which otherwise drops its NamedTempFile at a possibly stale path).
    temp.disable_cleanup(true);
    let handle = temp
        .as_file()
        .try_clone()
        .and_then(same_file::Handle::from_file)
        .map_err(|e| {
            format!(
                "temporary identity: {e}; temporary file may remain at {}",
                temp.path().display()
            )
        })?;
    let ready = (|| {
        temp.write_all(prepared.replacement.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;
        temp.as_file()
            .set_permissions(prepared.permissions.clone())
            .map_err(|e| format!("permissions: {e}"))?;
        hook(temp.path());
        prepared.recheck(root)?;
        check_temporary_path(&temp, &handle, &parents)
    })();
    if let Err(error) = ready {
        return Err(cleanup_temporary(temp, &handle, &parents, error));
    }
    match temp.persist(&prepared.target) {
        Ok(_) => Ok(()),
        Err(error) => Err(cleanup_temporary(
            error.file,
            &handle,
            &parents,
            format!("replace error: {}", error.error),
        )),
    }
}

// ── Format preview ───────────────────────────────────────────────────────

// Retained text renderer for in-process callers; CLI imports uses its JSON preview.
#[allow(dead_code)]
pub(crate) fn format_preview(preview: &HashMap<PathBuf, (Vec<String>, Vec<String>)>) -> String {
    let mut out = String::new();
    for (path, (old_lines, new_lines)) in preview {
        out.push_str(&format!("--- {}\n", path.display()));
        out.push_str(&format!("+++ {}\n", path.display()));
        let max_lines = old_lines.len().max(new_lines.len());
        for i in 0..max_lines {
            let old = old_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            let new = new_lines.get(i).map(|s| s.as_str()).unwrap_or("");
            if old != new {
                for (marker, line) in [('-', old), ('+', new)] {
                    out.push(marker);
                    out.push_str(line);
                    if !line.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
