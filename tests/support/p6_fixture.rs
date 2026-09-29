//! Shared disposable process boundary for P6 tests (no global environment mutation).
// Every including test binary compiles this module wholesale, but each uses a
// different subset of the shared fixture/path helpers; the rest stay here so
// all binaries assert the same canonical-identity contract.
#![allow(dead_code)]
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("workspace");
        for path in [&root, &dir.path().join("home"), &dir.path().join("cwd")] {
            std::fs::create_dir_all(path).expect("mkdir");
        }
        Self { dir, root }
    }
    pub fn write(&self, file: &str, source: &str) {
        let path = self.root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, source).expect("write source");
    }
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"));
        cmd.current_dir(self.dir.path().join("cwd"))
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("XDG_CONFIG_HOME", self.dir.path().join("config"))
            .env("XDG_DATA_HOME", self.dir.path().join("data"));
        cmd
    }
    pub fn run(&self, args: &[&str], input: Option<&Value>) -> Output {
        let mut cmd = self.command();
        cmd.args(args).args(["--root"]).arg(&self.root);
        execute(cmd, input).0
    }
}

pub fn execute(mut cmd: Command, input: Option<&Value>) -> (Output, Duration) {
    let input = input.map(Value::to_string);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn().expect("spawn CLI");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
    } else {
        drop(child.stdin.take());
    }
    let out = child.wait_with_output().expect("CLI output");
    (out, start.elapsed())
}

pub fn success(out: &Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|error| panic!("JSON: {error}: {out:?}"))
}

pub fn expected_path(path: &Path) -> String {
    tower_lsp::lsp_types::Url::from_file_path(path.canonicalize().expect("canonical source"))
        .expect("source URI")
        .to_file_path()
        .expect("filesystem path")
        .to_str()
        .expect("UTF8 fixture")
        .to_owned()
}

/// Test-only decoding of CLI-reported file values. Producers report different
/// representations (native path, full `file://` URI, or the URL path tail
/// `/C:/…` on Windows); decode each to its canonical filesystem identity so
/// comparisons stay full-path — never basename or prefix-stripped.
pub fn canonical_reported_file(reported: &str) -> PathBuf {
    let decoded = if windows_drive_uri_tail(reported) {
        tower_lsp::lsp_types::Url::parse(&format!("file://{}", reported))
            .expect("reported URI tail")
            .to_file_path()
            .expect("reported URI tail path")
    } else if reported.starts_with("file:") {
        tower_lsp::lsp_types::Url::parse(reported)
            .expect("reported file URI")
            .to_file_path()
            .expect("reported URI path")
    } else {
        PathBuf::from(reported)
    };
    decoded.canonicalize().expect("canonical reported file")
}

/// `/C:/…` URL path tail (the `Url::path()` spelling), not a native path.
fn windows_drive_uri_tail(reported: &str) -> bool {
    let bytes = reported.as_bytes();
    bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes.len() == 3 || bytes[3] == b'/')
}

/// Test-only canonical identity of a fixture path, matching the normalization
/// `canonical_reported_file` applies to CLI-reported values.
pub fn canonical_fixture_file(path: &Path) -> PathBuf {
    path.canonicalize().expect("canonical fixture file")
}
