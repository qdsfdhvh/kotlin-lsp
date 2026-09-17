use super::*;
use tower_lsp::lsp_types::{Position, Range};

fn te(line: u32, col: u32, end_line: u32, end_col: u32, new_text: &str) -> TextEdit {
    TextEdit {
        range: Range {
            start: Position::new(line, col),
            end: Position::new(end_line, end_col),
        },
        new_text: new_text.to_string(),
    }
}

#[test]
fn single_line_replacement() {
    let lines = vec!["hello world".to_string()];
    let edits = vec![te(0, 0, 0, 5, "goodbye")];
    assert_eq!(
        apply_text_edits_to_lines(&lines, &edits).expect("valid edit ranges"),
        vec!["goodbye world"]
    );
}

#[test]
fn multi_line_insertion() {
    let lines = vec!["line1".to_string(), "line3".to_string()];
    let edits = vec![te(1, 0, 1, 5, "line2")];
    let result = apply_text_edits_to_lines(&lines, &edits).expect("valid edit ranges");
    assert_eq!(result, vec!["line1", "line2"]);
}

#[test]
fn reverse_order_edits() {
    let lines: Vec<String> = vec!["aaa", "bbb", "ccc"]
        .into_iter()
        .map(String::from)
        .collect();
    let edits = vec![te(0, 0, 0, 3, "AAA"), te(2, 0, 2, 3, "CCC")];
    let result = apply_text_edits_to_lines(&lines, &edits).expect("valid edit ranges");
    assert_eq!(result, vec!["AAA", "bbb", "CCC"]);
}

#[test]
fn path_under_root_valid() {
    let dir = tempfile::TempDir::new().expect("temporary fixture operation");
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).expect("temporary fixture operation");
    assert!(path_is_under_root(&sub, dir.path()));
}

#[test]
fn path_outside_root_invalid() {
    assert!(!path_is_under_root(
        Path::new("/other/main.kt"),
        Path::new("/workspace"),
    ));
}

#[test]
fn preflight_missing_second_file_never_writes_first() {
    let dir = tempfile::tempdir().expect("fixture");
    let first = dir.path().join("First.kt");
    std::fs::write(&first, "class First\n").expect("source");
    let edits = vec![
        FileEdit {
            path: first.clone(),
            edits: vec![te(0, 6, 0, 11, "Changed")],
        },
        FileEdit {
            path: dir.path().join("Missing.kt"),
            edits: vec![te(0, 0, 0, 0, "x")],
        },
    ];
    let summary = apply_file_edits(&edits, Some(dir.path()), false);
    // No root for second assertion: read errors, not only containment, must preflight.
    let without_root = apply_file_edits(&edits, None, false);
    assert_eq!(std::fs::read(&first).expect("read"), b"class First\n");
    assert_eq!(summary.files_modified, 0);
    assert_eq!(without_root.files_modified, 0);
    assert_eq!(without_root.files.len(), 2);
}

#[test]
fn utf16_replacement_preserves_untouched_bytes_and_verbatim_new_text() {
    let dir = tempfile::tempdir().expect("fixture");
    for extension in ["kt", "java", "swift"] {
        let path = dir.path().join(format!("Unicode.{extension}"));
        std::fs::write(&path, "// 😀漢x\r\nkeep\nlast").expect("source");
        let summary = apply_file_edits(
            &[FileEdit {
                path: path.clone(),
                edits: vec![te(0, 6, 0, 7, "é\r\nnew\r")],
            }],
            Some(dir.path()),
            false,
        );
        assert_eq!(summary.files_modified, 1, "{summary:?}");
        assert_eq!(
            std::fs::read(&path).expect("bytes"),
            "// 😀漢é\r\nnew\r\r\nkeep\nlast".as_bytes()
        );
    }
}

#[test]
fn duplicate_canonical_targets_rejected_before_writes() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("File.kt");
    std::fs::write(&path, "abc").expect("source");
    let edits = [
        FileEdit {
            path: path.clone(),
            edits: vec![te(0, 0, 0, 1, "A")],
        },
        FileEdit {
            path: dir.path().join("./File.kt"),
            edits: vec![te(0, 1, 0, 2, "B")],
        },
    ];
    let summary = apply_file_edits(&edits, Some(dir.path()), false);
    assert_eq!(summary.files_modified, 0, "{summary:?}");
    assert_eq!(std::fs::read(&path).expect("bytes"), b"abc");
    assert!(preview_file_edits(&edits).is_err());
}

#[test]
fn atomic_replace_does_not_truncate_existing_open_identity() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("File.kt");
    let alias = dir.path().join("HardLink.kt");
    std::fs::write(&path, "abc").expect("source");
    std::fs::hard_link(&path, &alias).expect("hard link");
    let summary = apply_file_edits(
        &[FileEdit {
            path: path.clone(),
            edits: vec![te(0, 0, 0, 1, "A")],
        }],
        Some(dir.path()),
        false,
    );
    assert_eq!(summary.files_modified, 1);
    assert_eq!(std::fs::read(&path).expect("new bytes"), b"Abc");
    assert_eq!(std::fs::read(&alias).expect("old identity bytes"), b"abc");
    assert_eq!(std::fs::read_dir(dir.path()).expect("listing").count(), 2);
}

#[test]
fn content_conflict_before_first_commit_is_not_overwritten() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("File.kt");
    std::fs::write(&path, "abc").expect("source");
    let summary = apply_file_edits_with_hook(
        &[FileEdit {
            path: path.clone(),
            edits: vec![te(0, 0, 0, 1, "A")],
        }],
        Some(dir.path()),
        false,
        |_, stage, _| {
            if matches!(stage, CommitStage::BeforeCommit) {
                std::fs::write(&path, "concurrent").expect("external write");
            }
        },
    );
    assert_eq!(std::fs::read(&path).expect("bytes"), b"concurrent");
    assert_eq!(summary.files_modified, 0, "{summary:?}");
}

#[test]
fn replaced_temporary_path_never_commits_foreign_sentinel() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("File.kt");
    std::fs::write(&path, "abc").expect("source");
    let mut sentinel = None;
    let summary = apply_file_edits_with_hook(
        &[FileEdit {
            path: path.clone(),
            edits: vec![te(0, 0, 0, 1, "A")],
        }],
        Some(dir.path()),
        false,
        |_, stage, temp| {
            if matches!(stage, CommitStage::BeforeReplace) {
                let temp = temp.expect("created temporary file");
                std::fs::remove_file(temp).expect("external unlink");
                std::fs::write(temp, "foreign sentinel").expect("external replacement");
                sentinel = Some(temp.to_path_buf());
            }
        },
    );
    assert_eq!(summary.files_modified, 0, "{summary:?}");
    assert_eq!(std::fs::read(&path).expect("target"), b"abc");
    assert_eq!(
        std::fs::read(sentinel.expect("sentinel path")).expect("sentinel"),
        b"foreign sentinel"
    );
}

fn batch_fixture() -> (tempfile::TempDir, Vec<FileEdit>) {
    let dir = tempfile::tempdir().expect("fixture");
    let edits = ["A.kt", "B.java", "C.swift"]
        .into_iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, "abc\r\n").expect("source");
            FileEdit {
                path,
                edits: vec![te(0, 0, 0, 1, "A")],
            }
        })
        .collect();
    (dir, edits)
}
fn assert_untouched(edits: &[FileEdit]) {
    for fe in edits {
        assert_eq!(std::fs::read(&fe.path).expect("bytes"), b"abc\r\n");
    }
}
fn has_error(summary: &EditSummary, needle: &str) -> bool {
    summary.files.iter().any(|result| matches!(result, FileEditResult::Error { message, .. } if message.contains(needle)))
}

#[test]
fn invalid_ranges_preflight_entire_batch() {
    for bad in [
        te(0, 2, 0, 1, "x"),
        te(3, 0, 3, 0, "x"),
        te(0, 4, 0, 4, "x"),
        te(1, 1, 1, 1, "x"),
        te(u32::MAX, 0, u32::MAX, 0, "x"),
    ] {
        let (dir, mut edits) = batch_fixture();
        edits[1].edits = vec![bad];
        let summary = apply_file_edits(&edits, Some(dir.path()), false);
        assert_eq!(summary.files_modified, 0, "{summary:?}");
        assert_eq!(summary.files.len(), 3);
        assert_untouched(&edits);
    }
}

#[test]
fn overlaps_and_surrogate_interiors_are_errors_in_preview_and_apply() {
    for bad in [
        vec![te(0, 0, 0, 2, "x"), te(0, 1, 0, 3, "y")],
        vec![te(0, 0, 0, 3, "x"), te(0, 1, 0, 1, "y")],
    ] {
        let (dir, mut edits) = batch_fixture();
        edits[1].edits = bad;
        assert!(preview_file_edits(&edits).is_err());
        assert!(has_error(
            &apply_file_edits(&edits, Some(dir.path()), false),
            "overlap"
        ));
        assert_untouched(&edits);
    }
    let (dir, mut edits) = batch_fixture();
    std::fs::write(&edits[1].path, "😀x").expect("unicode");
    edits[1].edits = vec![te(0, 1, 0, 2, "X")];
    assert!(preview_file_edits(&edits).is_err());
    assert!(has_error(
        &apply_file_edits(&edits, Some(dir.path()), false),
        "surrogate"
    ));
    assert_eq!(std::fs::read(&edits[0].path).expect("first"), b"abc\r\n");
    assert_eq!(
        std::fs::read(&edits[1].path).expect("second"),
        "😀x".as_bytes()
    );
}

#[test]
fn invalid_utf8_and_directory_second_file_preflight_no_writes() {
    for directory in [false, true] {
        let (dir, edits) = batch_fixture();
        if directory {
            std::fs::remove_file(&edits[1].path).expect("remove");
            std::fs::create_dir(&edits[1].path).expect("directory target");
        } else {
            std::fs::write(&edits[1].path, [0xff]).expect("invalid UTF8");
        }
        let summary = apply_file_edits(&edits, Some(dir.path()), false);
        assert_eq!(summary.files_modified, 0, "{summary:?}");
        assert_eq!(std::fs::read(&edits[0].path).expect("first"), b"abc\r\n");
        assert_eq!(std::fs::read(&edits[2].path).expect("third"), b"abc\r\n");
    }
}

#[test]
fn outside_root_and_invalid_root_preflight_no_writes() {
    let (dir, mut edits) = batch_fixture();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("Outside.kt");
    std::fs::write(&target, "abc\r\n").expect("outside source");
    edits[1].path = target;
    assert!(has_error(
        &apply_file_edits(&edits, Some(dir.path()), false),
        "workspace root"
    ));
    assert_untouched(&edits);
    for root in [dir.path().join("missing"), edits[0].path.clone()] {
        assert!(has_error(
            &apply_file_edits(&edits, Some(&root), false),
            "workspace root"
        ));
        assert_untouched(&edits);
    }
}

#[test]
fn valid_batch_preview_apply_and_dry_run_counts_match_bytes() {
    let (dir, edits) = batch_fixture();
    let preview = preview_file_edits(&edits).expect("preview");
    let dry = apply_file_edits(&edits, Some(dir.path()), true);
    assert_eq!(dry.files_modified, 3);
    assert!(dry
        .files
        .iter()
        .all(|r| matches!(r, FileEditResult::Ok { dry_run: true, .. })));
    assert_untouched(&edits);
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
    let applied = apply_file_edits(&edits, Some(dir.path()), false);
    assert_eq!(applied.files_modified, 3, "{applied:?}");
    for fe in &edits {
        assert_eq!(preview[&fe.path].0.concat(), "abc\r\n");
        assert_eq!(preview[&fe.path].1.concat(), "Abc\r\n");
        assert_eq!(std::fs::read(&fe.path).expect("bytes"), b"Abc\r\n");
    }
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
}

#[test]
fn adjacent_replacements_equal_inserts_and_multiline_order() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("File.kt");
    std::fs::write(&path, "abcd\r\nlast").expect("source");
    let edits = [FileEdit {
        path: path.clone(),
        edits: vec![
            te(0, 1, 0, 1, "I"),
            te(0, 1, 0, 1, "J"),
            te(0, 1, 0, 2, "B"),
            te(0, 0, 0, 1, "A"),
            te(0, 2, 1, 2, "\nx\r\ny"),
        ],
    }];
    let preview = preview_file_edits(&edits).expect("preview");
    assert_eq!(preview[&path].1.concat(), "AIJB\nx\r\nyst");
    assert_eq!(apply_file_edits(&edits, None, false).files_modified, 1);
    assert_eq!(std::fs::read(&path).expect("bytes"), b"AIJB\nx\r\nyst");
}

#[test]
fn empty_eof_insert_and_final_newline_are_requested_only() {
    for (source, edit, expected) in [
        ("", te(0, 0, 0, 0, "x"), "x"),
        ("a", te(0, 1, 0, 1, "\r\n"), "a\r\n"),
        ("a\r\n", te(1, 0, 1, 0, "b"), "a\r\nb"),
        ("a\n", te(0, 1, 1, 0, ""), "a"),
        ("a\rb", te(1, 1, 1, 1, "c"), "a\rbc"),
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("File.kt");
        std::fs::write(&path, source).expect("source");
        let summary = apply_file_edits(
            &[FileEdit {
                path: path.clone(),
                edits: vec![edit],
            }],
            None,
            false,
        );
        assert_eq!(summary.files_modified, 1, "{summary:?}");
        assert_eq!(std::fs::read(&path).expect("bytes"), expected.as_bytes());
    }
}

#[test]
fn noop_and_dry_run_preserve_identity_permissions_and_no_temporaries() {
    let (dir, mut edits) = batch_fixture();
    let handle = same_file::Handle::from_path(&edits[0].path).expect("identity");
    let permissions = std::fs::metadata(&edits[0].path)
        .expect("metadata")
        .permissions();
    let modified = std::fs::metadata(&edits[0].path)
        .expect("metadata")
        .modified()
        .expect("mtime");
    assert_eq!(apply_file_edits(&edits, None, true).files_modified, 3);
    for fe in &mut edits {
        fe.edits = vec![te(0, 0, 0, 1, "a")];
    }
    assert_eq!(apply_file_edits(&edits, None, false).files_modified, 0);
    for fe in &mut edits {
        fe.edits.clear();
    }
    assert_eq!(apply_file_edits(&edits, None, false).files_modified, 0);
    assert_eq!(
        handle,
        same_file::Handle::from_path(&edits[0].path).expect("identity")
    );
    let metadata = std::fs::metadata(&edits[0].path).expect("metadata");
    assert_eq!(metadata.permissions(), permissions);
    assert_eq!(metadata.modified().expect("mtime"), modified);
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
    assert_untouched(&edits);
}

#[test]
fn second_file_conflict_before_first_commit_is_globally_rechecked() {
    let (dir, edits) = batch_fixture();
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |i, stage, _| {
        if i == 0 && matches!(stage, CommitStage::BeforeCommit) {
            std::fs::write(&edits[1].path, "concurrent").expect("external write");
        }
    });
    assert_eq!(summary.files_modified, 0);
    assert!(has_error(&summary, "content changed"));
    assert_eq!(std::fs::read(&edits[0].path).expect("first"), b"abc\r\n");
}

#[test]
fn content_conflict_between_files_reports_partial_and_unattempted() {
    let (dir, edits) = batch_fixture();
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |i, stage, _| {
        if i == 1 && matches!(stage, CommitStage::BeforeCommit) {
            std::fs::write(&edits[1].path, "concurrent").expect("external write");
        }
    });
    assert_eq!(summary.files_modified, 1, "{summary:?}");
    assert!(has_error(&summary, "content changed"));
    assert!(has_error(&summary, "not attempted"));
    assert_eq!(std::fs::read(&edits[0].path).expect("first"), b"Abc\r\n");
    assert_eq!(
        std::fs::read(&edits[1].path).expect("second"),
        b"concurrent"
    );
    assert_eq!(std::fs::read(&edits[2].path).expect("third"), b"abc\r\n");
}

#[test]
fn content_change_after_temporary_write_is_rechecked_and_cleaned() {
    let (dir, edits) = batch_fixture();
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |_, stage, _| {
        if matches!(stage, CommitStage::BeforeReplace) {
            std::fs::write(&edits[0].path, "concurrent").expect("external write");
        }
    });
    assert_eq!(summary.files_modified, 0);
    assert!(has_error(&summary, "content changed"));
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
}

#[test]
fn same_bytes_target_replacement_detects_new_identity() {
    let (dir, edits) = batch_fixture();
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |_, stage, _| {
        if matches!(stage, CommitStage::BeforeReplace) {
            std::fs::remove_file(&edits[0].path).expect("external unlink");
            std::fs::write(&edits[0].path, "abc\r\n").expect("external replacement");
        }
    });
    assert_eq!(summary.files_modified, 0);
    assert!(has_error(&summary, "identity changed"));
    assert_untouched(&edits);
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
}

#[test]
fn unique_temporary_names_preserve_preexisting_sentinels() {
    let (dir, edits) = batch_fixture();
    for name in [
        ".kotlin-lsp-edit-",
        "File.kt.tmp",
        ".kotlin-lsp-edit-sentinel",
    ] {
        std::fs::write(dir.path().join(name), "sentinel").expect("sentinel");
    }
    let mut names = std::collections::HashSet::new();
    let summary = apply_file_edits_with_hook(&edits, None, false, |_, stage, temp| {
        if matches!(stage, CommitStage::BeforeReplace) {
            let path = temp.expect("temp");
            assert_eq!(
                path.parent(),
                Some(
                    dir.path()
                        .canonicalize()
                        .expect("canonical directory")
                        .as_path()
                )
            );
            assert!(names.insert(path.to_path_buf()));
            assert!(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .is_err(),
                "create-new must not replace occupied name"
            );
        }
    });
    assert_eq!(summary.files_modified, 3);
    assert_eq!(names.len(), 3);
    for name in [
        ".kotlin-lsp-edit-",
        "File.kt.tmp",
        ".kotlin-lsp-edit-sentinel",
    ] {
        assert_eq!(
            std::fs::read(dir.path().join(name)).expect("sentinel"),
            b"sentinel"
        );
    }
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 6);
}

#[cfg(unix)]
#[test]
fn symlink_inside_target_preserves_link_and_outside_is_blocked() {
    use std::os::unix::fs::{symlink, MetadataExt};
    let (dir, edits) = batch_fixture();
    let link = dir.path().join("Link.kt");
    symlink(&edits[0].path, &link).expect("link");
    let original = std::fs::symlink_metadata(&link).expect("link metadata");
    let request = [FileEdit {
        path: link.clone(),
        edits: edits[0].edits.clone(),
    }];
    assert_eq!(
        apply_file_edits(&request, Some(dir.path()), false).files_modified,
        1
    );
    assert_eq!(
        std::fs::symlink_metadata(&link).expect("link").ino(),
        original.ino()
    );
    assert_eq!(std::fs::read(&edits[0].path).expect("target"), b"Abc\r\n");
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("Outside.kt");
    std::fs::write(&target, "abc").expect("outside");
    std::fs::remove_file(&link).expect("unlink");
    symlink(&target, &link).expect("outside link");
    assert!(has_error(
        &apply_file_edits(&request, Some(dir.path()), false),
        "workspace root"
    ));
    assert_eq!(std::fs::read(target).expect("outside"), b"abc");
}

#[cfg(unix)]
#[test]
fn duplicate_symlink_alias_preflight_rejects_without_editing_target() {
    let (dir, mut edits) = batch_fixture();
    let link = dir.path().join("Alias.kt");
    std::os::unix::fs::symlink(&edits[0].path, &link).expect("link");
    edits[1].path = link;
    assert!(has_error(
        &apply_file_edits(&edits, Some(dir.path()), false),
        "duplicate canonical"
    ));
    assert_untouched(&edits);
}

#[cfg(unix)]
#[test]
fn symlink_retarget_and_same_target_link_replacement_are_detected() {
    for same_target in [true, false] {
        let (dir, edits) = batch_fixture();
        let link = dir.path().join("Link.kt");
        std::os::unix::fs::symlink(&edits[0].path, &link).expect("link");
        let summary = apply_file_edits_with_hook(
            &[FileEdit {
                path: link.clone(),
                edits: edits[0].edits.clone(),
            }],
            Some(dir.path()),
            false,
            |_, stage, _| {
                if matches!(stage, CommitStage::BeforeReplace) {
                    std::fs::remove_file(&link).expect("unlink");
                    std::os::unix::fs::symlink(&edits[usize::from(!same_target)].path, &link)
                        .expect("retarget link");
                }
            },
        );
        assert_eq!(summary.files_modified, 0, "{summary:?}");
        assert!(has_error(&summary, "identity changed"));
        assert_untouched(&edits);
        assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 4);
    }
}

#[cfg(unix)]
#[test]
fn parent_and_root_symlink_retargets_before_commit_are_detected() {
    for root_link in [true, false] {
        let (dir, edits) = batch_fixture();
        let other = tempfile::tempdir().expect("other");
        std::fs::write(other.path().join("A.kt"), "abc\r\n").expect("other source");
        let holder = tempfile::tempdir().expect("link holder");
        let link = holder.path().join("Workspace");
        std::os::unix::fs::symlink(dir.path(), &link).expect("directory symlink");
        let request = [FileEdit {
            path: if root_link {
                edits[0].path.clone()
            } else {
                link.join("A.kt")
            },
            edits: edits[0].edits.clone(),
        }];
        let summary = apply_file_edits_with_hook(
            &request,
            Some(if root_link { &link } else { dir.path() }),
            false,
            |_, stage, _| {
                if matches!(stage, CommitStage::BeforeCommit) {
                    std::fs::remove_file(&link).expect("unlink");
                    std::os::unix::fs::symlink(other.path(), &link).expect("retarget");
                }
            },
        );
        assert_eq!(summary.files_modified, 0);
        assert!(has_error(&summary, "identity changed"));
        assert_untouched(&edits);
        assert_eq!(
            std::fs::read(other.path().join("A.kt")).expect("outside"),
            b"abc\r\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn late_directory_move_never_deletes_foreign_sentinel_reports_retained_temp() {
    let dir = tempfile::tempdir().expect("fixture");
    let parent = dir.path().join("parent");
    let moved = dir.path().join("moved");
    std::fs::create_dir(&parent).expect("parent");
    let target = parent.join("File.kt");
    std::fs::write(&target, "abc").expect("source");
    let mut name = None;
    let summary = apply_file_edits_with_hook(
        &[FileEdit {
            path: target.clone(),
            edits: vec![te(0, 0, 0, 1, "A")],
        }],
        Some(dir.path()),
        false,
        |_, stage, temp| {
            if matches!(stage, CommitStage::BeforeReplace) {
                let filename = temp.expect("temp").file_name().expect("name").to_owned();
                std::fs::rename(&parent, &moved).expect("move directory");
                std::fs::create_dir(&parent).expect("foreign directory");
                std::fs::write(parent.join(&filename), "foreign sentinel").expect("sentinel");
                std::fs::write(&target, "foreign target").expect("foreign target");
                name = Some(filename);
            }
        },
    );
    assert_eq!(summary.files_modified, 0);
    assert!(has_error(&summary, "temporary cleanup skipped"));
    assert!(has_error(&summary, "may remain"));
    let name = name.expect("temp name");
    assert_eq!(
        std::fs::read(parent.join(&name)).expect("sentinel"),
        b"foreign sentinel"
    );
    assert_eq!(
        std::fs::read(target).expect("foreign target"),
        b"foreign target"
    );
    assert_eq!(
        std::fs::read(moved.join("File.kt")).expect("original target"),
        b"abc"
    );
    assert_eq!(
        std::fs::read(moved.join(name)).expect("retained own temp"),
        b"Abc"
    );
}

#[cfg(unix)]
#[test]
fn permissions_preserved_and_unreadable_second_file_preflights() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, edits) = batch_fixture();
    std::fs::set_permissions(&edits[0].path, std::fs::Permissions::from_mode(0o751)).expect("mode");
    assert_eq!(apply_file_edits(&edits[..1], None, false).files_modified, 1);
    assert_eq!(
        std::fs::metadata(&edits[0].path)
            .expect("mode")
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    std::fs::write(&edits[0].path, "abc\r\n").expect("reset");
    std::fs::set_permissions(&edits[1].path, std::fs::Permissions::from_mode(0o000))
        .expect("unreadable");
    let denied = std::fs::read(&edits[1].path).is_err();
    let summary = apply_file_edits(&edits, Some(dir.path()), false);
    std::fs::set_permissions(&edits[1].path, std::fs::Permissions::from_mode(0o644))
        .expect("restore");
    // A privileged runner may bypass read denial, but read-only validation still rejects it.
    assert_eq!(summary.files_modified, 0);
    if denied {
        assert!(has_error(&summary, "error") || has_error(&summary, "denied"));
    }
    assert_untouched(&edits);
}

#[cfg(unix)]
#[test]
fn temporary_creation_failure_after_first_write_reports_partial() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, edits) = batch_fixture();
    let permissions = std::fs::metadata(dir.path())
        .expect("metadata")
        .permissions();
    // Verify this runner actually enforces directory write denial; root runners skip this OS condition.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).expect("deny");
    let denied = tempfile::NamedTempFile::new_in(dir.path()).is_err();
    std::fs::set_permissions(dir.path(), permissions.clone()).expect("restore");
    if !denied {
        eprintln!("directory permission test not applicable to privileged runner");
        return;
    }
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |i, stage, _| {
        if i == 1 && matches!(stage, CommitStage::BeforeCommit) {
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
                .expect("deny");
        }
    });
    std::fs::set_permissions(dir.path(), permissions).expect("restore");
    assert_eq!(summary.files_modified, 1);
    assert!(has_error(&summary, "temporary file:"));
    assert!(has_error(&summary, "not attempted"));
    assert_eq!(std::fs::read(&edits[0].path).expect("first"), b"Abc\r\n");
    assert_eq!(std::fs::read(&edits[1].path).expect("second"), b"abc\r\n");
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 3);
}

#[cfg(unix)]
#[test]
fn replace_failure_reports_zero_writes_and_cleanup_failure_honestly() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, edits) = batch_fixture();
    let permissions = std::fs::metadata(dir.path())
        .expect("metadata")
        .permissions();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).expect("deny");
    let denied = tempfile::NamedTempFile::new_in(dir.path()).is_err();
    std::fs::set_permissions(dir.path(), permissions.clone()).expect("restore");
    if !denied {
        eprintln!("directory permission test not applicable to privileged runner");
        return;
    }
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |_, stage, _| {
        if matches!(stage, CommitStage::BeforeReplace) {
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
                .expect("deny replace");
        }
    });
    std::fs::set_permissions(dir.path(), permissions).expect("restore");
    assert_eq!(summary.files_modified, 0);
    assert!(has_error(&summary, "replace error:"));
    assert!(has_error(&summary, "temporary cleanup failed"));
    assert_untouched(&edits);
    assert_eq!(std::fs::read_dir(dir.path()).expect("directory").count(), 4);
}

#[cfg(unix)]
#[test]
fn temporary_symlink_to_moved_own_file_is_not_committed_or_deleted() {
    let (dir, edits) = batch_fixture();
    let moved = dir.path().join("moved-temp");
    let mut foreign = None;
    let summary = apply_file_edits_with_hook(&edits, Some(dir.path()), false, |_, stage, temp| {
        if matches!(stage, CommitStage::BeforeReplace) {
            let temp = temp.expect("temp");
            std::fs::rename(temp, &moved).expect("move temp");
            std::os::unix::fs::symlink(&moved, temp).expect("foreign symlink");
            foreign = Some(temp.to_path_buf());
        }
    });
    assert_eq!(summary.files_modified, 0, "{summary:?}");
    assert_untouched(&edits);
    assert!(std::fs::symlink_metadata(foreign.expect("foreign path"))
        .expect("retained foreign link")
        .is_symlink());
    assert_eq!(std::fs::read(moved).expect("moved own file"), b"Abc\r\n");
}
