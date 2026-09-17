//! Real CLI hierarchy regressions; every child has isolated workspace, home and cache.
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().join("workspace");
        for path in [&root, &dir.path().join("home"), &dir.path().join("cwd")] {
            std::fs::create_dir_all(path).expect("mkdir");
        }
        Self { dir, root }
    }
    fn write(&self, file: &str, source: &str) {
        std::fs::write(self.root.join(file), source).expect("write fixture");
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"));
        cmd.current_dir(&self.root)
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            // Timing-dependent slow-parse logs are not CLI diagnostics.
            .env("RUST_LOG", "error");
        cmd
    }
    fn hierarchy(&self, args: &[&str]) -> Output {
        self.command()
            .args(["call", "hierarchy"])
            .args(args)
            .args(["--json", "--no-stdlib"])
            .output()
            .expect("hierarchy")
    }
}
fn success(out: &Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    assert_eq!(
        out.stdout.iter().filter(|&&c| c == b'\n').count(),
        1,
        "compact JSON: {out:?}"
    );
    serde_json::from_slice(&out.stdout).expect("JSON")
}

#[test]
fn entry_target_default_has_real_edges_not_declarations_or_text() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun target() {}\nfun entry() { target() }\n// entry() target()\nval text = \"entry() target()\"\n");
    assert_eq!(
        success(&f.hierarchy(&["entry"])),
        json!({"name":"entry", "incoming":[], "outgoing":["target"]})
    );
    assert_eq!(
        success(&f.hierarchy(&["target"])),
        json!({"name":"target", "incoming":["entry"], "outgoing":[]})
    );
}

#[test]
fn directions_default_incoming_outgoing_and_both_select_edges() {
    let f = Fixture::new();
    f.write(
        "Calls.kt",
        "fun leaf() {}\nfun middle() { leaf() }\nfun entry() { middle() }\n",
    );
    for (flags, incoming, outgoing) in [
        (vec![], json!(["entry"]), json!(["leaf"])),
        (vec!["--incoming"], json!(["entry"]), json!([])),
        (vec!["--outgoing"], json!([]), json!(["leaf"])),
        (
            vec!["--incoming", "--outgoing"],
            json!(["entry"]),
            json!(["leaf"]),
        ),
    ] {
        for mut args in [vec!["middle"], vec!["Calls.kt", "2", "5"]] {
            args.extend(flags.iter().copied());
            assert_eq!(
                success(&f.hierarchy(&args)),
                json!({"name":"middle", "incoming":incoming, "outgoing":outgoing})
            );
        }
    }
}

#[test]
fn name_and_utf16_position_equivalent_with_root_relative_and_absolute_files() {
    let f = Fixture::new();
    f.write(
        "Calls 文.kt",
        "fun target() {}\nfun entry() { val 文 = \"😀\"; target() }\n",
    );
    std::fs::write(f.dir.path().join("cwd/Calls 文.kt"), "fun decoy() {}\n").expect("decoy");
    let root = f.root.to_str().expect("root");
    let absolute = f.root.join("Calls 文.kt");
    for args in [
        vec!["target"],
        vec!["Calls 文.kt", "1", "5"],
        vec!["Calls 文.kt", "2", "29"],
        vec![absolute.to_str().expect("path"), "2", "29"],
    ] {
        let out = f
            .command()
            .current_dir(f.dir.path().join("cwd"))
            .args(["call", "hierarchy"])
            .args(args)
            .args(["--root", root, "--json", "--no-stdlib"])
            .output()
            .expect("hierarchy");
        assert_eq!(
            success(&out),
            json!({"name":"target", "incoming":["entry"], "outgoing":[]})
        );
    }
}

fn failure(out: &Output, message: &str) -> Value {
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(message),
        "{out:?}"
    );
    let value: Value = serde_json::from_slice(&out.stdout).expect("JSON error");
    assert!(
        value["error"].as_str().expect("error").contains(message),
        "{value}"
    );
    assert!(
        value.get("incoming").is_none(),
        "no success envelope on error"
    );
    value
}

#[test]
fn invalid_files_cursors_and_noncallable_text_fail_honestly() {
    let f = Fixture::new();
    f.write(
        "Calls.kt",
        "fun target() {}\n// target()\nval text = \"target()\"\n\nval number = 1\n",
    );
    for (args, message) in [
        (vec!["missing.kt", "1", "1"], "Cannot read file"),
        (vec!["Calls.kt", "99", "5"], "outside"),
        (vec!["Calls.kt", "1", "99"], "outside"),
        (vec!["Calls.kt", "4", "1"], "No callable"),
        (vec!["Calls.kt", "2", "4"], "No callable"),
        (vec!["Calls.kt", "3", "13"], "No callable"),
        (vec!["number"], "No callable"),
        (vec!["missing"], "No callable"),
    ] {
        failure(&f.hierarchy(&args), message);
    }
    for args in [["Calls.kt", "0", "5"], ["Calls.kt", "1", "0"]] {
        let out = f.hierarchy(&args);
        assert!(!out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty(), "parser errors use stderr: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("1-based"),
            "{out:?}"
        );
    }
    assert_eq!(
        success(&f.hierarchy(&["Calls.kt", "1", "5"])),
        json!({"name":"target","incoming":[],"outgoing":[]})
    );
}

#[test]
fn same_named_methods_are_not_merged_and_ambiguous_names_list_locations() {
    let f = Fixture::new();
    f.write("Calls.kt", "class Left { fun target() {} }\nclass Right { fun target() {} }\nfun entry() { Left().target() }\n");
    let out = failure(&f.hierarchy(&["target"]), "Ambiguous callable");
    let file = tower_lsp::lsp_types::Url::from_file_path(
        f.root.join("Calls.kt").canonicalize().expect("file"),
    )
    .expect("URI")
    .to_file_path()
    .expect("path");
    assert_eq!(
        out["candidates"],
        json!([
            {"name":"Left.target", "file":file, "line":1, "col":18},
            {"name":"Right.target", "file":file, "line":2, "col":19}
        ])
    );
    for args in [vec!["Left.target"], vec!["Calls.kt", "1", "18"]] {
        assert_eq!(
            success(&f.hierarchy(&args)),
            json!({"name":"target", "incoming":["entry"], "outgoing":[]})
        );
    }
    assert_eq!(
        success(&f.hierarchy(&["Right.target"])),
        json!({"name":"target", "incoming":[], "outgoing":[]})
    );
}

#[test]
fn kotlin_java_swift_direct_calls_have_positive_and_empty_controls() {
    for (file, source, caller, target) in [
        (
            "Calls.kt",
            "fun target() {}\nfun entry() { target() }\n",
            "entry",
            "target",
        ),
        (
            "Calls.java",
            "class Calls {\n void target() {}\n void entry() { target(); }\n}\n",
            "Calls.entry",
            "Calls.target",
        ),
        (
            "Calls.swift",
            "func target() {}\nfunc entry() { target() }\n",
            "entry",
            "target",
        ),
    ] {
        let f = Fixture::new();
        f.write(file, source);
        assert_eq!(
            success(&f.hierarchy(&[target])),
            json!({"name":"target","incoming":[caller],"outgoing":[]})
        );
        assert_eq!(
            success(&f.hierarchy(&[caller])),
            json!({"name":"entry","incoming":[],"outgoing":["target"]})
        );
    }
}

#[test]
fn same_name_files_filter_outgoing_and_unresolvable_overloads_fail() {
    let f = Fixture::new();
    f.write("Left.kt", "fun shared() { left() }\nfun left() {}\n");
    f.write("Right.kt", "fun shared() { right() }\nfun right() {}\n");
    failure(&f.hierarchy(&["shared"]), "Ambiguous callable");
    assert_eq!(
        success(&f.hierarchy(&["Left.kt", "1", "5", "--outgoing"])),
        json!({"name":"shared","incoming":[],"outgoing":["left"]})
    );
    assert_eq!(
        success(&f.hierarchy(&["Right.kt", "1", "5", "--outgoing"])),
        json!({"name":"shared","incoming":[],"outgoing":["right"]})
    );
    f.write("Caller.kt", "fun entry() { shared() }\n");
    failure(
        &f.hierarchy(&["Left.kt", "1", "5", "--incoming"]),
        "Ambiguous callable",
    );
    f.write(
        "Overloads.kt",
        "fun overloaded() { left() }\nfun overloaded(x: Int) { right() }\n",
    );
    failure(
        &f.hierarchy(&["Overloads.kt", "1", "5", "--outgoing"]),
        "Ambiguous callable",
    );
}

#[test]
fn hierarchy_rejects_depth_and_extra_operands_instead_of_ignoring_them() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun entry() {}\n");
    for args in [
        vec!["entry", "--max-depth", "2"],
        vec!["entry", "--depth", "2"],
        vec!["Calls.kt", "1", "5", "2"],
    ] {
        let out = f.hierarchy(&args);
        assert!(!out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("depth") || error.contains("requires NAME or FILE LINE COL"),
            "{out:?}"
        );
    }
    assert_eq!(
        success(&f.hierarchy(&["entry"])),
        json!({"name":"entry","incoming":[],"outgoing":[]})
    );
}

#[test]
fn repeated_calls_cycles_and_actual_cold_warm_cache_are_deterministic() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun entry() { target(); target(); entry() }\nfun target() { entry() }\nfun isolated() {}\n");
    let cache = f.root.join(".cache/kotlin-lsp/index.bin");
    assert!(!cache.exists(), "fresh workspace is cold");
    let expected =
        json!({"name":"entry", "incoming":["entry","target"], "outgoing":["entry","target"]});
    let cold = f.hierarchy(&["entry"]);
    assert_eq!(success(&cold), expected);
    assert!(std::fs::metadata(&cache).expect("persisted cache").len() > 0);
    let warm = f.hierarchy(&["entry"]);
    assert_eq!(success(&warm), expected);
    assert_eq!(cold.stdout, warm.stdout, "byte-stable warm result");
    assert_eq!(success(&f.hierarchy(&["Calls.kt", "1", "5"])), expected);
    assert_eq!(
        success(&f.hierarchy(&["isolated"])),
        json!({"name":"isolated","incoming":[],"outgoing":[]})
    );
}

#[test]
fn no_stdlib_excludes_fake_home_callables_and_edges_with_positive_control() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun target() {}\nfun entry() { target() }\n");
    let library = f.dir.path().join("home/.kotlin-lsp/sources/lib/Library.kt");
    std::fs::create_dir_all(library.parent().expect("parent")).expect("mkdir");
    std::fs::write(library, "fun libraryEntry() { target() }\n").expect("library");
    failure(&f.hierarchy(&["libraryEntry"]), "No callable");
    assert_eq!(
        success(&f.hierarchy(&["target"])),
        json!({"name":"target","incoming":["entry"],"outgoing":[]})
    );
    let cache_dir = f.dir.path().join("cache/kotlin-lsp");
    assert!(
        std::fs::read_dir(&cache_dir)
            .expect("cache directory")
            .all(|entry| !entry
                .expect("cache entry")
                .file_name()
                .to_string_lossy()
                .starts_with("library-")),
        "disabled queries did not populate the library cache"
    );
    let enabled = f
        .command()
        .args(["call", "hierarchy", "target", "--json"])
        .output()
        .expect("enabled");
    assert_eq!(
        success(&enabled),
        json!({"name":"target","incoming":["entry","libraryEntry"],"outgoing":[]})
    );
    let library_cache_files: Vec<_> = std::fs::read_dir(&cache_dir)
        .expect("library cache directory")
        .map(|entry| entry.expect("cache entry").path())
        .filter(|path| {
            path.file_name()
                .expect("filename")
                .to_string_lossy()
                .starts_with("library-")
                && path.extension().is_some_and(|ext| ext == "bin")
        })
        .collect();
    assert!(
        library_cache_files.len() >= 2,
        "full cache and compact symbol index persisted"
    );
    for file in &library_cache_files {
        assert!(std::fs::metadata(file).expect("cache metadata").len() > 0);
    }
    let warm = f
        .command()
        .args(["call", "hierarchy", "target", "--json"])
        .output()
        .expect("warm enabled");
    assert_eq!(
        success(&warm),
        json!({"name":"target","incoming":["entry","libraryEntry"],"outgoing":[]})
    );
    assert_eq!(enabled.stdout, warm.stdout);
    let position = f
        .command()
        .args(["call", "hierarchy"])
        .arg(f.dir.path().join("home/.kotlin-lsp/sources/lib/Library.kt"))
        .args(["1", "22", "--json"])
        .output()
        .expect("warm library call position");
    assert_eq!(
        success(&position),
        json!({"name":"target","incoming":["entry","libraryEntry"],"outgoing":[]})
    );
    let enabled = f
        .command()
        .args(["call", "hierarchy", "libraryEntry", "--json"])
        .output()
        .expect("enabled");
    assert_eq!(
        success(&enabled),
        json!({"name":"libraryEntry","incoming":[],"outgoing":["target"]})
    );
    assert_eq!(
        success(&f.hierarchy(&["target"])),
        json!({"name":"target","incoming":["entry"],"outgoing":[]})
    );
}

#[test]
fn position_never_substitutes_a_same_named_noncallable_or_unindexed_file() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun target() {}\nfun entry() { target() }\n");
    f.write("Other.kt", "class Other { val target = 1 }\n");
    failure(&f.hierarchy(&["Other.kt", "1", "19"]), "No callable");
    let outside = f.dir.path().join("Outside.kt");
    std::fs::write(&outside, "fun target() {}\n").expect("outside");
    failure(
        &f.hierarchy(&[outside.to_str().expect("path"), "1", "5"]),
        "not indexed",
    );
    assert_eq!(
        success(&f.hierarchy(&["Calls.kt", "2", "15"])),
        json!({"name":"target","incoming":["entry"],"outgoing":[]})
    );
}

#[test]
fn help_and_generated_capabilities_advertise_real_hierarchy_boundaries() {
    let f = Fixture::new();
    let help = f.command().arg("--help").output().expect("help");
    assert!(help.status.success() && help.stderr.is_empty(), "{help:?}");
    let text = String::from_utf8(help.stdout).expect("help UTF8");
    let hierarchy = text
        .split("CALL HIERARCHY:")
        .nth(1)
        .expect("hierarchy contract");
    for phrase in [
        "--incoming",
        "--outgoing",
        "default",
        "UTF-16",
        "--no-stdlib",
        "string",
        "depth",
    ] {
        assert!(hierarchy.contains(phrase), "{phrase}: {hierarchy}");
    }
    let out = f
        .command()
        .args(["capabilities", "--json"])
        .output()
        .expect("capabilities");
    assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
    let manifest: Value = serde_json::from_slice(&out.stdout).expect("manifest");
    let flags = manifest["commands"]["call"]["flags"]
        .as_array()
        .expect("flags");
    for flag in [
        "--incoming",
        "--outgoing",
        "--root",
        "--no-stdlib",
        "--json",
    ] {
        assert!(flags.contains(&json!(flag)), "{flag}: {flags:?}");
    }
    assert_eq!(
        manifest["commands"]["call"]["subcommands"],
        json!(["hierarchy", "diff", "reach"])
    );
}

#[test]
fn direction_flags_are_rejected_outside_hierarchy() {
    let f = Fixture::new();
    for args in [
        vec!["find", "entry", "--incoming"],
        vec!["call", "reach", "entry", "--outgoing"],
        vec!["call", "diff", "--incoming"],
    ] {
        let out = f
            .command()
            .args(args)
            .args(["--json", "--no-stdlib"])
            .output()
            .expect("CLI");
        assert!(!out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("only supported by call hierarchy"),
            "{out:?}"
        );
    }
}

#[test]
fn invalid_root_is_reported_before_lookup() {
    let f = Fixture::new();
    f.write("Calls.kt", "fun entry() {}\n");
    for root in [f.root.join("missing"), f.root.join("Calls.kt")] {
        let out = f
            .command()
            .args([
                "call",
                "hierarchy",
                "entry",
                "--json",
                "--no-stdlib",
                "--root",
            ])
            .arg(root)
            .output()
            .expect("invalid root");
        failure(&out, "Invalid hierarchy root");
    }
}

#[test]
fn text_directions_and_unicode_names_are_exact_and_missing_name_fails() {
    let f = Fixture::new();
    f.write("Calls.swift", "func 目标() {}\nfunc entry() { 目标() }\n");
    assert_eq!(
        success(&f.hierarchy(&["目标"])),
        json!({"name":"目标","incoming":["entry"],"outgoing":[]})
    );
    assert_eq!(
        success(&f.hierarchy(&["Calls.swift", "2", "16"])),
        json!({"name":"目标","incoming":["entry"],"outgoing":[]})
    );
    let out = f
        .command()
        .args(["call", "hierarchy", "entry", "--outgoing", "--no-stdlib"])
        .output()
        .expect("text");
    assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).expect("UTF8"),
        "## Call hierarchy for `entry`\n### Outgoing calls\n  - 目标\n"
    );
    let out = f
        .command()
        .args(["call", "hierarchy", "entry", "--incoming", "--no-stdlib"])
        .output()
        .expect("text");
    assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).expect("UTF8"),
        "## Call hierarchy for `entry`\n### Incoming calls\n  (none)\n"
    );
    let out = f
        .command()
        .args(["call", "hierarchy", "missing", "--no-stdlib"])
        .output()
        .expect("text error");
    assert!(!out.status.success() && out.stdout.is_empty(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stderr).expect("UTF8"),
        "No callable found for 'missing'\n"
    );
}

#[test]
fn implicit_root_preserves_nested_cwd_relative_files() {
    let f = Fixture::new();
    std::fs::create_dir(f.root.join(".git")).expect("root marker");
    std::fs::create_dir(f.root.join("nested")).expect("nested");
    f.write("Calls.kt", "fun decoy() {}\n");
    f.write(
        "nested/Calls.kt",
        "fun target() {}\nfun entry() { target() }\n",
    );
    let out = f
        .command()
        .current_dir(f.root.join("nested"))
        .args([
            "call",
            "hierarchy",
            "Calls.kt",
            "1",
            "5",
            "--json",
            "--no-stdlib",
        ])
        .output()
        .expect("nested cwd");
    assert_eq!(
        success(&out),
        json!({"name":"target","incoming":["entry"],"outgoing":[]})
    );
}

#[test]
fn override_method_position_matches_name_without_selecting_override_property() {
    let f = Fixture::new();
    f.write(
        "Calls.kt",
        "open class Base {\n    open fun entry() {}\n}\nclass Child : Base() {\n    override fun entry() { leaf() }\n}\nfun leaf() {}\nopen class PropertyBase { open val leaf = 1 }\nclass PropertyChild : PropertyBase() {\n    override val leaf = 2\n}\n",
    );
    assert_eq!(
        success(&f.hierarchy(&["Base.entry", "--outgoing"])),
        json!({"name":"entry","incoming":[],"outgoing":[]})
    );
    for args in [
        vec!["Child.entry", "--outgoing"],
        vec!["Calls.kt", "5", "18", "--outgoing"],
    ] {
        assert_eq!(
            success(&f.hierarchy(&args)),
            json!({"name":"entry","incoming":[],"outgoing":["leaf"]})
        );
    }
    failure(&f.hierarchy(&["Calls.kt", "10", "18"]), "No callable");
    assert_eq!(
        success(&f.hierarchy(&["leaf"])),
        json!({"name":"leaf","incoming":["Child.entry"],"outgoing":[]})
    );
}

#[test]
fn nested_method_uses_nearest_owner_for_name_position_and_edges() {
    let f = Fixture::new();
    f.write(
        "Calls.kt",
        "fun leaf() {}\nclass Outer {\n    class Inner {\n        fun entry() { leaf() }\n    }\n}\n",
    );
    assert_eq!(
        success(&f.hierarchy(&["leaf", "--incoming"])),
        json!({"name":"leaf","incoming":["Inner.entry"],"outgoing":[]})
    );
    for args in [
        vec!["entry", "--outgoing"],
        vec!["Inner.entry", "--outgoing"],
        vec!["Calls.kt", "4", "13", "--outgoing"],
    ] {
        assert_eq!(
            success(&f.hierarchy(&args)),
            json!({"name":"entry","incoming":[],"outgoing":["leaf"]})
        );
    }
    assert_eq!(
        failure(&f.hierarchy(&["Outer.entry", "--outgoing"]), "No callable"),
        json!({"error":"No callable found for 'Outer.entry'"})
    );
}

#[test]
fn utf16_declaration_after_emoji_matches_name_and_candidate_columns() {
    let f = Fixture::new();
    f.write(
        "Calls.kt",
        "/* 😀 */ fun target() {}\nfun entry() { target() }\n",
    );
    let expected = json!({"name":"target","incoming":["entry"],"outgoing":[]});
    assert_eq!(success(&f.hierarchy(&["target"])), expected);
    assert_eq!(success(&f.hierarchy(&["Calls.kt", "1", "14"])), expected);
    f.write("Other.kt", "fun target() {}\n");
    let out = failure(&f.hierarchy(&["target"]), "Ambiguous callable");
    assert_eq!(out["candidates"][0]["line"], 1);
    assert_eq!(out["candidates"][0]["col"], 14);
    assert_eq!(out["candidates"][1]["col"], 5);
}
