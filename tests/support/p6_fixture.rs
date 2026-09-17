//! Shared disposable process boundary for P6 tests (no global environment mutation).
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
