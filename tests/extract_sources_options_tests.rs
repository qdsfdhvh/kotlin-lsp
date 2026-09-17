//! Extraction options through a real CLI and a disposable Gradle source JAR.
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Fixture {
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        for path in [
            "home",
            "workspace",
            "gradle/caches/modules-2/files-2.1/example/library/1.0/hash",
        ] {
            std::fs::create_dir_all(dir.path().join(path)).expect("mkdir");
        }
        let jar = std::fs::File::create(dir.path().join(
            "gradle/caches/modules-2/files-2.1/example/library/1.0/hash/library-1.0-sources.jar",
        ))
        .expect("JAR");
        let mut zip = zip::ZipWriter::new(jar);
        zip.start_file(
            "example/Library.kt",
            zip::write::SimpleFileOptions::default(),
        )
        .expect("source entry");
        zip.write_all(b"package example\nclass Library\n")
            .expect("source bytes");
        zip.start_file("README.txt", zip::write::SimpleFileOptions::default())
            .expect("non-source entry");
        zip.write_all(b"not a source file")
            .expect("non-source bytes");
        zip.finish().expect("finish JAR");
        Self { dir }
    }
    fn run(&self, extra: &[&str]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"))
            .current_dir(self.dir.path().join("workspace"))
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("GRADLE_USER_HOME", self.dir.path().join("unused-gradle"))
            .env("RUST_LOG", "error")
            .args(["extract-sources", "--gradle-home"])
            .arg(self.dir.path().join("gradle"))
            .arg("--output")
            .arg(self.dir.path().join("output"))
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("CLI");
        let start = Instant::now();
        loop {
            if child.try_wait().expect("poll").is_some() {
                return child.wait_with_output().expect("output");
            }
            if start.elapsed() > Duration::from_secs(15) {
                child.kill().expect("kill timeout");
                panic!(
                    "extract-sources timeout: {:?}",
                    child.wait_with_output().expect("timeout output")
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn extract_sources_rejects_root_before_scanning_or_writing_with_actionable_options() {
    for dry_run in [false, true] {
        let f = Fixture::new();
        let mut args = vec!["--root", "."];
        if dry_run {
            args.push("--dry-run");
        }
        let out = f.run(&args);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        assert!(
            out.stdout.is_empty(),
            "no scan/output announcements: {out:?}"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("extract-sources does not support --root"),
            "{stderr}"
        );
        assert!(
            stderr.contains("--gradle-home") && stderr.contains("--output"),
            "{stderr}"
        );
        assert!(!f.dir.path().join("output").exists());
        assert!(!f.dir.path().join("workspace/.cache").exists());
        assert!(!f.dir.path().join("home/.kotlin-lsp").exists());
    }
}

#[test]
fn extract_sources_isolated_dry_run_discovers_filtered_jar_without_writes() {
    let f = Fixture::new();
    let out = f.run(&["example.library", "--dry-run"]);
    // Gradle paths use separate group/artifact directories, so use a group filter.
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Found 1 *-sources.jar file(s) total."),
        "{text}"
    );
    assert!(
        text.contains("After filtering: 0 jar(s) match pattern(s)."),
        "{text}"
    );
    assert!(text.contains("Nothing to extract."), "{text}");
    assert!(!f.dir.path().join("output").exists());
    let out = f.run(&["example", "--dry-run"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Dry run — no files will be written."),
        "{text}"
    );
    assert!(
        text.contains("After filtering: 1 jar(s) match pattern(s)."),
        "{text}"
    );
    assert!(text.contains("example.library-1.0"), "{text}");
    assert!(text.contains("would extract 1 file(s)"), "{text}");
    assert!(!f.dir.path().join("output").exists());
    assert!(!f.dir.path().join("home/.kotlin-lsp").exists());
}

#[test]
fn extract_sources_valid_options_extract_only_source_bytes_to_explicit_output() {
    let f = Fixture::new();
    let out = f.run(&["example"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("extracted 1 file(s)"), "{text}");
    let dest = f.dir.path().join("output/example.library-1.0");
    assert_eq!(
        std::fs::read(dest.join("example/Library.kt")).expect("extracted source"),
        b"package example\nclass Library\n"
    );
    assert!(!dest.join("README.txt").exists());
    assert!(!f.dir.path().join("home/.kotlin-lsp").exists());
}
