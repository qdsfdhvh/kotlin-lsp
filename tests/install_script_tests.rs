//! Hermetic shell control-flow tests: dummy archive only, no network or real installation.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

fn executable(path: &Path, source: &str) {
    std::fs::write(path, source).expect("write shim");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod shim");
}

fn run_installer(os: &str, arch: &str, version: &str) -> (tempfile::TempDir, Output) {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path();
    for name in ["bin", "home", "prefix", "archive"] {
        std::fs::create_dir(root.join(name)).expect("mkdir");
    }
    executable(
        &root.join("archive/kotlin-lsp"),
        "#!/bin/sh\nprintf '%s\\n' \"$0 $*\" >> \"$RECEIPT/verified\"\necho dummy-release\n",
    );
    let tar = Command::new("/usr/bin/tar")
        .arg("-czf")
        .arg(root.join("dummy.tar.gz"))
        .arg("-C")
        .arg(root.join("archive"))
        .arg("kotlin-lsp")
        .output()
        .expect("create dummy archive");
    assert!(tar.status.success(), "{tar:?}");
    // A closed PATH makes accidental cargo, sudo, wget or network execution impossible.
    for (name, path) in [
        ("mkdir", "/bin/mkdir"),
        ("chmod", "/bin/chmod"),
        ("rm", "/bin/rm"),
        ("cat", "/bin/cat"),
        ("ls", "/bin/ls"),
        ("head", "/usr/bin/head"),
        ("tar", "/usr/bin/tar"),
        ("install", "/usr/bin/install"),
        ("mktemp", "/usr/bin/mktemp"),
        ("dirname", "/usr/bin/dirname"),
    ] {
        std::os::unix::fs::symlink(path, root.join("bin").join(name)).expect("tool link");
    }
    executable(&root.join("bin/uname"), "#!/bin/sh\ncase \"$1\" in -s) echo \"$TEST_OS\";; -m) echo \"$TEST_ARCH\";; *) exit 99;; esac\n");
    executable(&root.join("bin/curl"), "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$RECEIPT/download\"\nwhile [ \"$#\" -gt 0 ]; do\n if [ \"$1\" = -o ]; then shift; /bin/cp \"$RECEIPT/dummy.tar.gz\" \"$1\"; fi\n shift\ndone\n");
    executable(
        &root.join("bin/cargo"),
        "#!/bin/sh\necho forbidden >> \"$RECEIPT/cargo\"\nexit 99\n",
    );
    executable(
        &root.join("bin/kotlin-lsp"),
        "#!/bin/sh\necho stale >> \"$RECEIPT/stale\"\nexit 0\n",
    );
    let out = Command::new("/bin/bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/install.sh"))
        .current_dir(root)
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("USERPROFILE", root.join("home"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("TMPDIR", root)
        .env("KOTLIN_LSP_PREFIX", root.join("prefix"))
        .env("KOTLIN_LSP_VERSION", version)
        .env("TEST_OS", os)
        .env("TEST_ARCH", arch)
        .env("RECEIPT", root)
        .output()
        .expect("run hermetic installer");
    (dir, out)
}

#[test]
fn release_only_installer_verifies_exact_destination_not_stale_path() {
    let (dir, out) = run_installer("Darwin", "arm64", "latest");
    assert!(out.status.success(), "{out:?}");
    let root = dir.path();
    assert!(!root.join("cargo").exists(), "must never invoke cargo");
    assert!(
        !root.join("stale").exists(),
        "must not verify PATH's old binary"
    );
    assert!(std::fs::read_to_string(root.join("download")).expect("URL").contains(
        "https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/kotlin-lsp-darwin-aarch64.tar.gz"));
    let receipt = std::fs::read_to_string(root.join("verified")).expect("verification receipt");
    assert!(receipt
        .lines()
        .all(|line| line == format!("{} --version", root.join("prefix/kotlin-lsp").display())));
    assert!(!receipt.is_empty());
}

#[test]
fn unsupported_darwin_x86_environment_stops_before_download() {
    let (dir, out) = run_installer("Darwin", "x86_64", "latest");
    assert!(!out.status.success(), "{out:?}");
    assert!(!dir.path().join("download").exists());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("native arm64 shell"),
        "{out:?}"
    );
}

#[test]
fn pinned_linux_release_uses_pipeline_asset_name() {
    let (dir, out) = run_installer("Linux", "x86_64", "v0.32.3");
    assert!(out.status.success(), "{out:?}");
    assert!(std::fs::read_to_string(dir.path().join("download"))
        .expect("URL")
        .contains("/releases/download/v0.32.3/kotlin-lsp-linux-x86_64.tar.gz"));
}
