//! Workspace selection through real child processes, with isolated home and caches.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    dir: tempfile::TempDir,
    a: PathBuf,
    b: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let a = dir.path().join("A");
        let b = dir.path().join("B space % # 库");
        for (root, tag, dep) in [(&a, "A", ":old"), (&b, "B", ":core")] {
            write(
                &root.join("settings.gradle.kts"),
                "include(\":app\", \":core\", \":old\")\n",
            );
            write(
                &root.join("app/build.gradle.kts"),
                &format!("implementation(project(\"{dep}\"))\n"),
            );
            write(&root.join("app/src/main/Code.kt"), &format!("package sample\nopen class {tag}Base\nclass {tag}Activity : {tag}Base()\nfun {tag}Target() {{}}\nfun {tag}Entry() {{ {tag}Target() }}\n"));
            write(&root.join("app/src/main/AndroidManifest.xml"), &format!("<manifest>\n<application>\n<activity android:name=\"sample.{tag}Activity\" android:exported=\"false\">\n</activity>\n</application>\n</manifest>\n"));
            std::fs::create_dir_all(root.join("nested/deep")).expect("nested cwd");
            for module in ["core", "old"] {
                std::fs::create_dir(root.join(module)).expect("module");
            }
        }
        std::fs::create_dir(a.join(".git")).expect("A git marker");
        std::fs::create_dir_all(dir.path().join("home")).expect("home");
        Self { dir, a, b }
    }
    fn command(&self, cwd: &Path) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_kotlin-lsp"));
        c.current_dir(cwd)
            .env("HOME", self.dir.path().join("home"))
            .env("USERPROFILE", self.dir.path().join("home"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .env("RUST_LOG", "error");
        c
    }
    fn run(&self, args: &[&str], root: Option<&Path>, cwd: &Path, json: bool) -> Output {
        let mut c = self.command(cwd);
        c.args(args);
        if let Some(root) = root {
            c.arg("--root").arg(root);
        }
        if json {
            c.arg("--json");
        }
        c.output().expect("CLI child")
    }
}
fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("fixture write");
}
fn success(out: Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("JSON")
}
fn path(value: &Value) -> PathBuf {
    // Filesystem-valued outputs are not URI decoded, even with literal %/#/Unicode.
    PathBuf::from(value.as_str().expect("filesystem path"))
        .canonicalize()
        .unwrap_or_else(|e| panic!("existing filesystem path {value}: {e}"))
}
fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(Value::to_string);
    rows
}
fn assert_modules(value: &Value, root: &Path, tag: &str) {
    let rows = value.as_array().expect("modules array");
    assert_eq!(rows.len(), 3, "{value}");
    for name in [":app", ":core", ":old"] {
        let row = rows
            .iter()
            .find(|r| r["name"] == name)
            .expect("named module");
        assert_eq!(path(&row["path"]), root.join(&name[1..]));
        assert_eq!(row["file_count"], if name == ":app" { 2 } else { 0 });
        assert_eq!(
            row["source_sets"],
            if name == ":app" {
                json!(["main"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            row["dependencies"],
            if name == ":app" {
                json!([if tag == "B" { ":core" } else { ":old" }])
            } else {
                json!([])
            }
        );
    }
}
#[test]
fn module_list_root_selects_all_b_metadata_not_cwd_a() {
    let f = Fixture::new();
    let b = f.b.canonicalize().expect("B");
    let v = success(f.run(&["module", "list"], Some(&f.b), &f.a, true));
    assert_modules(&v["modules"], &b, "B");
    for root in [None, Some(f.a.as_path())] {
        let v = success(f.run(&["module", "list"], root, &f.a, true));
        assert_modules(&v["modules"], &f.a.canonicalize().expect("A"), "A");
    }
}
#[test]
fn module_deps_root_selects_b_edges_not_a() {
    let f = Fixture::new();
    for (root, dep) in [
        (Some(f.b.as_path()), ":core"),
        (Some(f.a.as_path()), ":old"),
        (None, ":old"),
    ] {
        let v = success(f.run(&["module", "deps", ":app"], root, &f.a, true));
        assert_eq!(
            v,
            json!({"module":":app", "direction":"both", "dependencies":[dep], "dependents":[]})
        );
        let v = success(f.run(&["module", "deps", dep], root, &f.a, true));
        assert_eq!(
            v,
            json!({"module":dep, "direction":"both", "dependencies":[], "dependents":[":app"]})
        );
    }
}
#[test]
fn module_files_root_selects_b_full_paths_not_a() {
    let f = Fixture::new();
    for (root, expected) in [
        (Some(f.b.as_path()), &f.b),
        (Some(f.a.as_path()), &f.a),
        (None, &f.a),
    ] {
        let v = success(f.run(&["module", "files", ":app"], root, &f.a, true));
        let actual = v
            .as_array()
            .expect("files")
            .iter()
            .map(|v| json!(path(v)))
            .collect();
        assert_eq!(
            sorted(actual),
            sorted(vec![
                json!(expected
                    .join("app/build.gradle.kts")
                    .canonicalize()
                    .expect("build")),
                json!(expected
                    .join("app/src/main/Code.kt")
                    .canonicalize()
                    .expect("code"))
            ])
        );
    }
}
fn assert_graph(v: &Value, root: &Path, tag: &str) {
    assert_modules(&v["module"], &root.canonicalize().expect("root"), tag);
    assert_eq!(
        sorted(v["symbols"].as_array().expect("symbols").clone()),
        sorted(vec![
            json!(format!("{tag}Base")),
            json!(format!("{tag}Activity")),
            json!(format!("{tag}Entry")),
            json!(format!("{tag}Target"))
        ])
    );
    let calls = v["edges"]["calls"].as_array().expect("calls");
    assert_eq!(calls.len(), 1, "{v}");
    assert_eq!(calls[0]["caller"], format!("{tag}Entry"));
    assert_eq!(calls[0]["callee"], format!("{tag}Target"));
    assert_eq!(
        uri_path(&calls[0]["caller_file"]),
        root.join("app/src/main/Code.kt")
            .canonicalize()
            .expect("code")
    );
    assert_eq!(
        v["edges"]["inheritance"],
        json!([{"subtype":format!("{tag}Activity"),"supertype":format!("{tag}Base")} ])
    );
    assert_eq!(v["edges"]["imports"], json!([]));
    assert_eq!(v["edges"]["overrides"], json!([]));
}
#[test]
fn tool_graph_root_selects_b_symbols_edges_and_modules_not_a() {
    let f = Fixture::new();
    for (root, expected, tag) in [
        (Some(f.b.as_path()), &f.b, "B"),
        (Some(f.a.as_path()), &f.a, "A"),
        (None, &f.a, "A"),
    ] {
        for _ in 0..2 {
            let v = success(f.run(&["tool", "graph"], root, &f.a, true));
            assert_graph(&v, expected, tag);
        }
    }
}

fn uri_path(value: &Value) -> PathBuf {
    tower_lsp::lsp_types::Url::parse(value.as_str().expect("URI"))
        .expect("valid URI")
        .to_file_path()
        .expect("file URI")
        .canonicalize()
        .expect("existing URI path")
}
fn assert_workspace(v: &Value, root: &Path, tag: &str) {
    let root = root.canonicalize().expect("root");
    assert_eq!(path(&v["project_root"]), root);
    assert_modules(&v["modules"], &root, tag);
    assert_eq!(v["total_files"], 2);
    assert_eq!(v["total_symbols"], 3); // existing line-based overview excludes `open class`.
    let syms = v["symbols"].as_array().expect("symbols");
    assert_eq!(
        sorted(syms.iter().map(|s| s["name"].clone()).collect()),
        sorted(vec![
            json!(format!("{tag}Activity")),
            json!(format!("{tag}Target")),
            json!(format!("{tag}Entry"))
        ])
    );
    for s in syms {
        assert_eq!(path(&s["file"]), root.join("app/src/main/Code.kt"));
    }
    assert_eq!(v["entry_points"].as_array().expect("entry points").len(), 1);
    assert_eq!(v["entry_points"][0]["name"], format!("{tag}Activity"));
    assert_eq!(v["entry_points"][0]["kind"], "android");
    assert_eq!(
        path(&v["entry_points"][0]["file"]),
        root.join("app/src/main/Code.kt")
    );
}
#[test]
fn tool_workspace_root_selects_b_project_modules_symbols_and_entry_points() {
    let f = Fixture::new();
    for (root, expected, tag) in [
        (Some(f.b.as_path()), &f.b, "B"),
        (Some(f.a.as_path()), &f.a, "A"),
        (None, &f.a, "A"),
    ] {
        assert_workspace(
            &success(f.run(&["tool", "workspace"], root, &f.a, true)),
            expected,
            tag,
        );
    }
}
fn snapshot_file(root: &Path) -> String {
    // Snapshot's legacy file field is an escaped URI path without the scheme.
    tower_lsp::lsp_types::Url::from_file_path(
        root.join("app/src/main/Code.kt")
            .canonicalize()
            .expect("file"),
    )
    .expect("URI")
    .as_str()
    .trim_start_matches("file://")
    .to_string()
}
fn assert_snapshot_metadata(v: &Value, root: &Path, tag: &str) {
    assert_eq!(
        path(&v["project"]["root"]),
        root.canonicalize().expect("root")
    );
    assert_modules(&v["modules"], &root.canonicalize().expect("root"), tag);
    let syms = v["symbols"].as_array().expect("symbols");
    assert_eq!(
        sorted(syms.iter().map(|s| s["name"].clone()).collect()),
        sorted(vec![
            json!(format!("{tag}Base")),
            json!(format!("{tag}Activity")),
            json!(format!("{tag}Target")),
            json!(format!("{tag}Entry"))
        ])
    );
    for s in syms {
        assert_eq!(s["file"], snapshot_file(root));
        assert_eq!(
            s["fq_name"],
            format!("sample.{}", s["name"].as_str().expect("name"))
        );
        assert_eq!(s["visibility"], "public");
        assert_eq!(s["deprecated"], false);
    }
    assert_eq!(
        v["entry_points"],
        json!([{"kind":"class","name":format!("{tag}Activity"),"file":snapshot_file(root)}])
    );
}
fn assert_snapshot(v: &Value, root: &Path, tag: &str) {
    assert_snapshot_metadata(v, root, tag);
    assert_eq!(
        v["relationships"],
        json!({"calls":[[format!("{tag}Entry"), format!("{tag}Target")]],"extends":[[format!("{tag}Activity"),format!("{tag}Base")]],"overrides":[],"imports":[]})
    );
}
#[test]
fn tool_snapshot_root_selects_b_project_modules_symbols_relationships() {
    let f = Fixture::new();
    for (root, expected, tag) in [
        (Some(f.b.as_path()), &f.b, "B"),
        (Some(f.a.as_path()), &f.a, "A"),
        (None, &f.a, "A"),
    ] {
        for _ in 0..2 {
            assert_snapshot(
                &success(f.run(&["tool", "snapshot"], root, &f.a, true)),
                expected,
                tag,
            );
        }
    }
}
#[test]
fn android_activities_root_selects_b_manifest_not_a() {
    let f = Fixture::new();
    for (root, tag) in [
        (Some(f.b.as_path()), "B"),
        (Some(f.a.as_path()), "A"),
        (None, "A"),
    ] {
        assert_eq!(
            success(f.run(&["android", "activities"], root, &f.a, true)),
            json!([{"name":format!("sample.{tag}Activity"),"exported":false,"intent_filters":[]} ])
        );
    }
}
const WORKSPACE_COMMANDS: &[&[&str]] = &[
    &["module", "list"],
    &["module", "deps", ":app"],
    &["module", "files", ":app"],
    &["tool", "graph"],
    &["tool", "workspace"],
    &["tool", "snapshot"],
    &["android", "activities"],
];
#[test]
fn all_workspace_commands_reject_missing_and_file_roots_before_results() {
    let f = Fixture::new();
    for args in WORKSPACE_COMMANDS {
        for root in [f.b.join("missing"), f.b.join("settings.gradle.kts")] {
            for json in [false, true] {
                let out = f.run(args, Some(&root), &f.a, json);
                assert!(!out.status.success(), "{args:?}: {out:?}");
                assert!(out.stdout.is_empty(), "no false results: {out:?}");
                let err = String::from_utf8_lossy(&out.stderr);
                assert!(err.contains("--root") && err.contains("directory"), "{err}");
                assert!(err.contains(root.to_str().expect("path")), "{err}");
            }
        }
    }
}
#[test]
fn snapshot_default_and_include_libraries_cold_warm_and_default_after_include() {
    let f = Fixture::new();
    let library = f.dir.path().join("home/.kotlin-lsp/sources/Unique.kt");
    write(&library, "package homelib\nclass HomeOnlyLibrary\n");
    assert!(!f.b.join(".cache/kotlin-lsp/index.bin").exists());
    assert!(!f.dir.path().join("cache/kotlin-lsp").exists());
    for (iteration, include) in [false, false, true, true, false].into_iter().enumerate() {
        let args = if include {
            vec!["tool", "snapshot", "--include-libraries"]
        } else {
            vec!["tool", "snapshot"]
        };
        let out = f.run(&args, Some(&f.b), &f.a, true);
        assert!(out.status.success(), "{out:?}");
        if include {
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("[WARN] tool snapshot --include-libraries"),
                "{out:?}"
            );
        } else {
            assert!(out.stderr.is_empty(), "{out:?}");
        }
        let mut v: Value = serde_json::from_slice(&out.stdout).expect("snapshot JSON");
        let syms = v["symbols"].as_array_mut().expect("symbols");
        let home: Vec<_> = syms
            .iter()
            .filter(|s| s["name"] == "HomeOnlyLibrary")
            .cloned()
            .collect();
        assert_eq!(
            home.len(),
            usize::from(include),
            "iteration={iteration} include={include}: {v}"
        );
        if include {
            assert_eq!(home[0]["fq_name"], "homelib.HomeOnlyLibrary");
            assert_eq!(home[0]["kind"], "class");
            assert_eq!(home[0]["line"], 2);
            assert_eq!(home[0]["signature"], "class HomeOnlyLibrary");
            assert_eq!(home[0]["visibility"], "public");
            assert_eq!(home[0]["deprecated"], false);
            let uri =
                tower_lsp::lsp_types::Url::from_file_path(library.canonicalize().expect("library"))
                    .expect("URI");
            assert_eq!(home[0]["file"], uri.as_str().trim_start_matches("file://"));
        }
        v["symbols"]
            .as_array_mut()
            .expect("symbols")
            .retain(|s| s["name"] != "HomeOnlyLibrary");
        assert_snapshot(&v, &f.b, "B");
        assert!(f.b.join(".cache/kotlin-lsp/index.bin").is_file());
        let cache = f.dir.path().join("cache/kotlin-lsp");
        let library_cache_count = std::fs::read_dir(cache)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        name.starts_with("library-") && name.ends_with(".bin")
                    })
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(
            library_cache_count > 0,
            iteration >= 2,
            "only explicit inclusion creates home cache"
        );
    }
}
#[test]
fn snapshot_configured_external_included_but_home_excluded_by_default_cold_warm() {
    let f = Fixture::new();
    let external = f.dir.path().join("external space % # 库");
    let home = f.dir.path().join("home/.kotlin-lsp/sources");
    write(
        &external.join("External.kt"),
        "package external\nclass ConfiguredExternal\n",
    );
    write(
        &home.join("Home.kt"),
        "package homelib\nclass HomeOnlyLibrary\n",
    );
    write(
        &f.b.join("workspace.json"),
        &json!({"sourcePaths":[external,home]}).to_string(),
    );
    for include in [false, false, true, true, false] {
        let args = if include {
            vec!["tool", "snapshot", "--include-libraries"]
        } else {
            vec!["tool", "snapshot"]
        };
        let out = f.run(&args, Some(&f.b), &f.a, true);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(!out.stderr.is_empty(), include, "{out:?}");
        let mut v: Value = serde_json::from_slice(&out.stdout).expect("JSON");
        let syms = v["symbols"].as_array_mut().expect("symbols");
        let ext: Vec<_> = syms
            .iter()
            .filter(|s| s["name"] == "ConfiguredExternal")
            .collect();
        assert_eq!(ext.len(), 1, "{syms:?}");
        assert_eq!(ext[0]["fq_name"], "external.ConfiguredExternal");
        let uri = tower_lsp::lsp_types::Url::from_file_path(
            external
                .join("External.kt")
                .canonicalize()
                .expect("external"),
        )
        .expect("URI");
        assert_eq!(ext[0]["file"], uri.as_str().trim_start_matches("file://"));
        assert_eq!(
            syms.iter()
                .filter(|s| s["name"] == "HomeOnlyLibrary")
                .count(),
            usize::from(include)
        );
        syms.retain(|s| s["name"] != "ConfiguredExternal" && s["name"] != "HomeOnlyLibrary");
        assert_snapshot(&v, &f.b, "B");
    }
}
#[test]
fn snapshot_selected_external_relationships_survive_warm_cache_with_workspace_and_home_controls() {
    let f = Fixture::new();
    let code = f.b.join("app/src/main/Code.kt");
    write(&code, "package workspace\nimport sample.Visible\nopen class WorkspaceBase {\n open fun work() {}\n}\nclass WorkspaceChild : WorkspaceBase() {\n override fun work() {}\n}\nfun workspaceTarget() {}\nfun workspaceEntry() { workspaceTarget() }\n");
    let external = f.dir.path().join("external space % # 库");
    let external_file = external.join("External.kt");
    write(&external_file, "package external\nimport sample.ExternalImport\nopen class ExternalBase {\n open fun action() {}\n}\nclass ExternalChild : ExternalBase() {\n override fun action() {}\n}\n/** External documentation. */\nfun externalTarget() {}\nfun externalEntry() { externalTarget(); externalTarget() }\n");
    let home = f.dir.path().join("home/.kotlin-lsp/sources");
    let home_file = home.join("Home.kt");
    write(&home_file, "package homelib\nimport sample.HomeImport\nopen class HomeBase {\n open fun homeAction() {}\n}\nclass HomeChild : HomeBase() {\n override fun homeAction() {}\n}\nfun homeTarget() {}\nfun homeEntry() { homeTarget() }\n");
    write(
        &f.b.join("workspace.json"),
        &json!({"sourcePaths":[external, home]}).to_string(),
    );
    let check = success(f.run(
        &[
            "check",
            code.to_str().expect("code"),
            external_file.to_str().expect("external"),
            home_file.to_str().expect("home"),
        ],
        None,
        &f.a,
        true,
    ));
    assert_eq!(check["errors"], json!([]));
    assert_eq!(check["files_ok"], 3);
    let workspace_uri =
        tower_lsp::lsp_types::Url::from_file_path(code.canonicalize().expect("code")).expect("URI");
    let external_uri =
        tower_lsp::lsp_types::Url::from_file_path(external_file.canonicalize().expect("external"))
            .expect("URI");
    let home_uri =
        tower_lsp::lsp_types::Url::from_file_path(home_file.canonicalize().expect("home"))
            .expect("URI");
    let expected = json!({
        "calls":[["externalEntry","externalTarget"],["workspaceEntry","workspaceTarget"]],
        "extends":[["ExternalChild","ExternalBase"],["WorkspaceChild","WorkspaceBase"]],
        "overrides":[["external.ExternalChild.action","action"],["workspace.WorkspaceChild.work","work"]],
        "imports":[[workspace_uri.as_str(),"sample.Visible"],[external_uri.as_str(),"sample.ExternalImport"]]
    });
    for (iteration, (include, exclude)) in [
        (false, false),
        (false, false),
        (true, false),
        (true, false),
        (true, true),
        (false, false),
        (false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut args = vec!["tool", "snapshot"];
        if include {
            args.push("--include-libraries");
        }
        if exclude {
            args.push("--exclude-relationships");
        }
        let out = f.run(&args, Some(&f.b), &f.a, true);
        assert!(out.status.success(), "{out:?}");
        if include {
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("[WARN] tool snapshot --include-libraries"),
                "{out:?}"
            );
        } else {
            assert!(out.stderr.is_empty(), "{out:?}");
        }
        let v: Value = serde_json::from_slice(&out.stdout).expect("JSON");
        assert_eq!(path(&v["project"]["root"]), f.b.canonicalize().expect("B"));
        assert_modules(&v["modules"], &f.b.canonicalize().expect("B"), "B");
        assert_eq!(v["entry_points"], json!([]));
        let syms = v["symbols"].as_array().expect("symbols");
        for (uri, names) in [
            (
                &workspace_uri,
                vec![
                    "WorkspaceBase",
                    "WorkspaceChild",
                    "work",
                    "work",
                    "workspaceEntry",
                    "workspaceTarget",
                ],
            ),
            (
                &external_uri,
                vec![
                    "ExternalBase",
                    "ExternalChild",
                    "action",
                    "action",
                    "externalEntry",
                    "externalTarget",
                ],
            ),
            (
                &home_uri,
                if include {
                    vec![
                        "HomeBase",
                        "HomeChild",
                        "homeAction",
                        "homeAction",
                        "homeEntry",
                        "homeTarget",
                    ]
                } else {
                    vec![]
                },
            ),
        ] {
            assert_eq!(
                sorted(
                    syms.iter()
                        .filter(|s| s["file"] == uri.as_str().trim_start_matches("file://"))
                        .map(|s| s["name"].clone())
                        .collect()
                ),
                sorted(names.into_iter().map(|name| json!(name)).collect())
            );
        }
        assert_eq!(syms.len(), if include { 18 } else { 12 });
        let target = syms
            .iter()
            .find(|s| s["name"] == "externalTarget")
            .expect("external target");
        assert_eq!(target["fq_name"], "external.externalTarget");
        assert_eq!(target["signature"], "fun externalTarget()");
        assert_eq!(target["doc"], "External documentation.");
        assert_eq!(target["line"], 10);
        if exclude {
            assert!(v.get("relationships").is_none(), "{v}");
        } else {
            for kind in ["calls", "extends", "overrides", "imports"] {
                assert_eq!(
                    sorted(v["relationships"][kind].as_array().expect("edges").clone()),
                    sorted(expected[kind].as_array().expect("expected edges").clone()),
                    "iteration={iteration} include={include} kind={kind}: {v}"
                );
            }
        }
    }
}
#[test]
fn snapshot_selected_java_swift_external_calls_and_supertypes_are_cache_stable() {
    let f = Fixture::new();
    let external = f.dir.path().join("external space % # 库");
    let java = external.join("JavaChild.java");
    let swift = external.join("SwiftChild.swift");
    write(&java, "public class JavaChild extends JavaBase {\n public void javaTarget() {}\n public void javaEntry() { javaTarget(); }\n}\n");
    write(&swift, "public class SwiftChild: SwiftBase {}\npublic func swiftTarget() {}\npublic func swiftEntry() { swiftTarget() }\n");
    write(
        &f.b.join("workspace.json"),
        &json!({"sourcePaths":[external]}).to_string(),
    );
    let expected = json!({
        "calls":[["BEntry","BTarget"],["JavaChild.javaEntry","javaTarget"],["swiftEntry","swiftTarget"]],
        "extends":[["BActivity","BBase"],["JavaChild","JavaBase"],["SwiftChild","SwiftBase"]],
        "overrides":[], "imports":[]
    });
    for _ in 0..2 {
        let v = success(f.run(&["tool", "snapshot"], Some(&f.b), &f.a, true));
        for kind in ["calls", "extends", "overrides", "imports"] {
            assert_eq!(
                sorted(v["relationships"][kind].as_array().expect("edges").clone()),
                sorted(expected[kind].as_array().expect("expected edges").clone()),
                "{kind}: {v}"
            );
        }
        let mut workspace = v.clone();
        let syms = workspace["symbols"].as_array_mut().expect("symbols");
        for (file, names) in [
            (&java, ["JavaChild", "javaTarget", "javaEntry"]),
            (&swift, ["SwiftChild", "swiftTarget", "swiftEntry"]),
        ] {
            let uri = tower_lsp::lsp_types::Url::from_file_path(
                file.canonicalize().expect("external file"),
            )
            .expect("URI");
            let escaped = uri.as_str().trim_start_matches("file://");
            assert_eq!(
                sorted(
                    syms.iter()
                        .filter(|s| s["file"] == escaped)
                        .map(|s| s["name"].clone())
                        .collect()
                ),
                sorted(names.into_iter().map(|name| json!(name)).collect())
            );
            syms.retain(|s| s["file"] != escaped);
        }
        assert_snapshot_metadata(&workspace, &f.b, "B");
    }
}
#[test]
fn explicit_relative_roots_from_nested_cwd_govern_all_seven_operations() {
    let f = Fixture::new();
    let cwd = f.a.join("nested/deep");
    let root = PathBuf::from("../../..").join(f.b.file_name().expect("B name"));
    assert_modules(
        &success(f.run(WORKSPACE_COMMANDS[0], Some(&root), &cwd, true))["modules"],
        &f.b.canonicalize().expect("B"),
        "B",
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[1], Some(&root), &cwd, true)),
        json!({"module":":app","direction":"both","dependencies":[":core"],"dependents":[]})
    );
    let files = success(f.run(WORKSPACE_COMMANDS[2], Some(&root), &cwd, true));
    let actual = files
        .as_array()
        .expect("files")
        .iter()
        .map(|p| json!(path(p)))
        .collect();
    assert_eq!(
        sorted(actual),
        sorted(vec![
            json!(f
                .b
                .join("app/build.gradle.kts")
                .canonicalize()
                .expect("build")),
            json!(f
                .b
                .join("app/src/main/Code.kt")
                .canonicalize()
                .expect("code"))
        ])
    );
    assert_graph(
        &success(f.run(WORKSPACE_COMMANDS[3], Some(&root), &cwd, true)),
        &f.b,
        "B",
    );
    assert_workspace(
        &success(f.run(WORKSPACE_COMMANDS[4], Some(&root), &cwd, true)),
        &f.b,
        "B",
    );
    assert_snapshot(
        &success(f.run(WORKSPACE_COMMANDS[5], Some(&root), &cwd, true)),
        &f.b,
        "B",
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[6], Some(&root), &cwd, true)),
        json!([{"name":"sample.BActivity","exported":false,"intent_filters":[]} ])
    );
}
#[test]
fn rootless_nested_discovery_preserves_gradle_ancestors_and_git_workspace_defaults() {
    let f = Fixture::new();
    // B has Gradle settings but deliberately no .git: module discovery still ascends.
    let cwd = f.b.join("nested/deep");
    assert_modules(
        &success(f.run(WORKSPACE_COMMANDS[0], None, &cwd, true))["modules"],
        &f.b.canonicalize().expect("B"),
        "B",
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[1], None, &cwd, true))["dependencies"],
        json!([":core"])
    );
    let files = success(f.run(WORKSPACE_COMMANDS[2], None, &cwd, true));
    assert_eq!(files.as_array().expect("files").len(), 2);
    for file in files.as_array().expect("files") {
        assert!(path(file).starts_with(f.b.canonicalize().expect("B")));
    }
    let cwd = f.a.join("nested/deep");
    assert_graph(
        &success(f.run(WORKSPACE_COMMANDS[3], None, &cwd, true)),
        &f.a,
        "A",
    );
    assert_workspace(
        &success(f.run(WORKSPACE_COMMANDS[4], None, &cwd, true)),
        &f.a,
        "A",
    );
    assert_snapshot(
        &success(f.run(WORKSPACE_COMMANDS[5], None, &cwd, true)),
        &f.a,
        "A",
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[6], None, &cwd, true)),
        json!([{"name":"sample.AActivity","exported":false,"intent_filters":[]} ])
    );
}
#[test]
fn explicit_empty_directory_never_falls_back_to_cwd_or_gradle_ancestor() {
    let f = Fixture::new();
    let root = f.b.join("nested/deep");
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[0], Some(&root), &f.a, true)),
        json!({"modules":[]})
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[1], Some(&root), &f.a, true)),
        json!({"module":":app","direction":"both","dependencies":[],"dependents":[]})
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[2], Some(&root), &f.a, true)),
        json!([])
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[3], Some(&root), &f.a, true)),
        json!({"symbols":[],"module":[],"edges":{"calls":[],"inheritance":[],"imports":[],"overrides":[]}})
    );
    let root = root.canonicalize().expect("empty root");
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[4], Some(&root), &f.a, true)),
        json!({"project_root":root,"modules":[],"total_files":0,"total_symbols":0})
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[5], Some(&root), &f.a, true)),
        json!({"project":{"root":root},"modules":[],"symbols":[],"entry_points":[],"relationships":{"calls":[],"extends":[],"imports":[],"overrides":[]}})
    );
    assert_eq!(
        success(f.run(WORKSPACE_COMMANDS[6], Some(&root), &f.a, true)),
        json!([])
    );
}
#[test]
fn selected_workspace_graph_and_snapshot_keep_kotlin_java_swift_symbols() {
    let f = Fixture::new();
    for (root, tag) in [(&f.a, "A"), (&f.b, "B")] {
        write(
            &root.join("app/src/main/JavaType.java"),
            &format!("class {tag}Java {{}}\n"),
        );
        write(
            &root.join("app/src/main/SwiftType.swift"),
            &format!("class {tag}Swift {{}}\n"),
        );
    }
    for _ in 0..2 {
        let graph = success(f.run(&["tool", "graph"], Some(&f.b), &f.a, true));
        assert_eq!(
            sorted(graph["symbols"].as_array().expect("symbols").clone()),
            sorted(vec![
                json!("BBase"),
                json!("BActivity"),
                json!("BEntry"),
                json!("BTarget"),
                json!("BJava"),
                json!("BSwift")
            ])
        );
        assert_eq!(graph["module"][0]["file_count"], 3); // existing module scope excludes Swift.
        let v = success(f.run(&["tool", "snapshot"], Some(&f.b), &f.a, true));
        let symbols = v["symbols"].as_array().expect("symbols");
        assert_eq!(
            sorted(symbols.iter().map(|s| s["name"].clone()).collect()),
            sorted(vec![
                json!("BBase"),
                json!("BActivity"),
                json!("BEntry"),
                json!("BTarget"),
                json!("BJava"),
                json!("BSwift")
            ])
        );
        for (name, file) in [("BJava", "JavaType.java"), ("BSwift", "SwiftType.swift")] {
            let s = symbols
                .iter()
                .find(|s| s["name"] == name)
                .expect("language symbol");
            assert_eq!(s["kind"], "class");
            assert_eq!(s["line"], 1);
            let uri = tower_lsp::lsp_types::Url::from_file_path(
                f.b.join("app/src/main")
                    .join(file)
                    .canonicalize()
                    .expect("language file"),
            )
            .expect("URI");
            assert_eq!(s["file"], uri.as_str().trim_start_matches("file://"));
        }
        assert_eq!(v["relationships"]["calls"], json!([["BEntry", "BTarget"]]));
    }
    let files = success(f.run(&["module", "files", ":app"], Some(&f.b), &f.a, true));
    assert_eq!(
        sorted(
            files
                .as_array()
                .expect("files")
                .iter()
                .map(|s| json!(path(s)))
                .collect()
        ),
        sorted(
            [
                "build.gradle.kts",
                "src/main/Code.kt",
                "src/main/JavaType.java"
            ]
            .iter()
            .map(|p| json!(f.b.join("app").join(p).canonicalize().expect("file")))
            .collect()
        )
    );
}
#[test]
fn workspace_text_shapes_and_seven_command_smokes_are_semantic() {
    let f = Fixture::new();
    for (i, args) in WORKSPACE_COMMANDS.iter().enumerate() {
        let out = f.run(args, Some(&f.b), &f.a, false);
        assert!(
            out.status.success() && out.stderr.is_empty(),
            "{args:?}: {out:?}"
        );
        let text = String::from_utf8(out.stdout).expect("text");
        match i {
            0 => { assert!(text.contains(":app (2 files) [:core] @"), "{text}"); assert!(!text.contains("[:old]")); assert!(text.contains(f.b.file_name().expect("name").to_str().expect("text"))); }
            1 => assert_eq!(text, ":app depends on:\n  - :core\nDependents of :app:\n"),
            2 => {
                let paths: Vec<_> = text.lines().map(|s| json!(path(&json!(s.trim())))).collect();
                assert_eq!(sorted(paths), sorted(vec![json!(f.b.join("app/build.gradle.kts").canonicalize().expect("build")),json!(f.b.join("app/src/main/Code.kt").canonicalize().expect("code"))]));
            }
            3 => assert_eq!(text, "Symbol Graph: 4 symbols\n  calls: 1 edges\n  inheritance: 1 edges\n  imports: 0 edges\n"),
            4 => { assert!(text.starts_with("Workspace: 3 modules, 2 files, 3 symbols\n")); assert!(text.contains("entry: android 'BActivity'")); assert!(!text.contains("AActivity")); assert!(text.contains(f.b.file_name().expect("name").to_str().expect("text"))); }
            5 => assert_snapshot(&serde_json::from_str(&text).expect("snapshot is always JSON"), &f.b, "B"),
            6 => assert_eq!(text, "  sample.BActivity (not)\n"),
            _ => unreachable!("seven operations"),
        }
    }
}
#[test]
fn snapshot_exclude_relationships_preserves_selected_symbol_metadata_cold_warm() {
    let f = Fixture::new();
    write(&f.dir.path().join("home/.kotlin-lsp/sources/Library.kt"), "package homelib\n/** A documented library operation. */\nfun libraryValue(value: String): String = value\n");
    for (include, exclude) in [
        (false, false),
        (true, false),
        (true, false),
        (true, true),
        (true, true),
        (false, false),
        (false, true),
    ] {
        let mut args = vec!["tool", "snapshot"];
        if exclude {
            args.push("--exclude-relationships");
        }
        if include {
            args.push("--include-libraries");
        }
        let out = f.run(&args, Some(&f.b), &f.a, true);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(!out.stderr.is_empty(), include, "{out:?}");
        let mut v: Value = serde_json::from_slice(&out.stdout).expect("JSON");
        if exclude {
            assert!(v.get("relationships").is_none(), "{v}");
        }
        let symbols = v["symbols"].as_array_mut().expect("symbols");
        let lib: Vec<_> = symbols
            .iter()
            .filter(|s| s["name"] == "libraryValue")
            .collect();
        assert_eq!(lib.len(), usize::from(include));
        if include {
            assert_eq!(lib[0]["kind"], "function");
            assert_eq!(lib[0]["fq_name"], "homelib.libraryValue");
            assert_eq!(lib[0]["line"], 3);
            assert_eq!(lib[0]["return_type"], "String");
            assert_eq!(lib[0]["parameters"], json!([["value", "String"]]));
            assert_eq!(
                lib[0]["signature"],
                "fun libraryValue(value: String): String"
            );
            assert_eq!(lib[0]["doc"], "A documented library operation.");
        }
        symbols.retain(|s| s["name"] != "libraryValue");
        if exclude {
            assert_snapshot_metadata(&v, &f.b, "B");
        } else {
            assert_snapshot(&v, &f.b, "B");
        }
    }
}
#[test]
fn file_only_tree_composables_check_and_insert_keep_cwd_operands() {
    let f = Fixture::new();
    let cwd = f.a.join("nested/deep");
    let source = "@Composable\nfun AWidget() {}\n";
    write(&cwd.join("Same.kt"), source);
    write(&f.b.join("Same.kt"), "@Composable\nfun BWidget() {}\n");
    for root in [f.b.clone(), f.b.join("missing-root-with-no-file-operation")] {
        let out = f.run(&["tool", "tree", "Same.kt"], Some(&root), &cwd, false);
        assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
        let tree = String::from_utf8(out.stdout).expect("tree");
        assert!(
            tree.contains("\"AWidget\"") && !tree.contains("BWidget"),
            "{tree}"
        );
        let v = success(f.run(
            &["android", "composables", "Same.kt"],
            Some(&root),
            &cwd,
            true,
        ));
        assert_eq!(v, json!([{"name":"AWidget","line":1,"params":[]}]));
        let v = success(f.run(&["check", "Same.kt"], Some(&root), &cwd, true));
        assert_eq!(v["files_ok"], 1);
        assert_eq!(v["files_with_errors"], 0);
        assert_eq!(v["errors"], json!([]));
        let out = f.run(
            &[
                "edit",
                "insert",
                "Same.kt",
                "2",
                "--before",
                "--content",
                "// preview",
                "--dry-run",
            ],
            Some(&root),
            &cwd,
            false,
        );
        assert!(out.status.success() && out.stderr.is_empty(), "{out:?}");
        assert_eq!(
            String::from_utf8(out.stdout).expect("preview"),
            "@Composable\n// preview\nfun AWidget() {}\n\n"
        );
        assert_eq!(
            std::fs::read_to_string(cwd.join("Same.kt")).expect("cwd file"),
            source
        );
        assert_eq!(
            std::fs::read_to_string(f.b.join("Same.kt")).expect("B file"),
            "@Composable\nfun BWidget() {}\n"
        );
    }
}
#[test]
fn format_input_expansion_remains_cwd_relative_without_launching_ktlint() {
    let f = Fixture::new();
    write(&f.b.join("OnlyB.kt"), "class OnlyB\n");
    for mode in ["check", "apply"] {
        let out = f
            .command(&f.a)
            .args(["format", mode, "OnlyB.kt", "--dry-run", "--root"])
            .arg(&f.b)
            // No external executable can be launched even if expansion regresses.
            .env("PATH", f.dir.path().join("empty-path"))
            .output()
            .expect("format child");
        assert!(!out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("OnlyB.kt") && err.contains("no .kt or .kts files"),
            "{err}"
        );
        assert!(
            !err.contains("Install:"),
            "must fail expansion, not run ktlint: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(f.b.join("OnlyB.kt")).expect("unchanged"),
            "class OnlyB\n"
        );
    }
}
#[test]
fn help_capabilities_and_skills_truthfully_bound_workspace_options_without_workspace() {
    let f = Fixture::new();
    let cwd = f.dir.path().join("home");
    let help = f.command(&cwd).arg("--help").output().expect("help");
    assert!(help.status.success() && help.stderr.is_empty(), "{help:?}");
    let help = String::from_utf8(help.stdout).expect("help text");
    assert!(help.contains("module list/deps/files"));
    assert!(help.contains("Gradle-settings ancestors"));
    assert!(help.contains("Flags are command-specific"));
    let v = success(f.run(&["capabilities"], Some(&cwd.join("missing")), &cwd, true));
    let commands = v["commands"].as_object().expect("commands");
    for name in ["module", "tool", "android"] {
        let c = commands.get(name).expect("command");
        assert!(c["flags"]
            .as_array()
            .expect("flags")
            .contains(&json!("--root")));
    }
    let format = commands.get("format").expect("format");
    assert!(!format["flags"]
        .as_array()
        .expect("flags")
        .contains(&json!("--root")));
    let skills = f.run(
        &["tool", "skills", "read", "kotlin-lsp"],
        Some(&cwd.join("missing")),
        &cwd,
        false,
    );
    assert!(
        skills.status.success() && skills.stderr.is_empty(),
        "{skills:?}"
    );
    let text = String::from_utf8(skills.stdout).expect("skill");
    assert!(text.contains("module list/deps/files") && text.contains("cwd-relative"));
    assert!(!text.contains("Outstanding root forwarding"));
}
#[test]
fn graph_keeps_existing_cold_source_inclusion_and_uses_selected_root_configuration() {
    let f = Fixture::new();
    let external_a = f.dir.path().join("external-a");
    let external_b = f.dir.path().join("external-b");
    write(&external_a.join("Library.kt"), "class AConfiguredLibrary\n");
    write(&external_b.join("Library.kt"), "class BConfiguredLibrary\n");
    write(
        &f.a.join("workspace.json"),
        &json!({"sourcePaths":[external_a]}).to_string(),
    );
    write(
        &f.b.join("workspace.json"),
        &json!({"sourcePaths":[external_b]}).to_string(),
    );
    write(
        &f.dir.path().join("home/.kotlin-lsp/sources/Home.kt"),
        "class HomeNotConfigured\n",
    );
    for (root, tag) in [(&f.b, "B"), (&f.a, "A")] {
        let mut graph = success(f.run(&["tool", "graph"], Some(root), &f.a, true));
        let syms = graph["symbols"].as_array_mut().expect("symbols");
        assert_eq!(
            syms.iter()
                .filter(|s| **s == json!(format!("{tag}ConfiguredLibrary")))
                .count(),
            1
        );
        syms.retain(|s| *s != json!(format!("{tag}ConfiguredLibrary")));
        assert_graph(&graph, root, tag); // rejects home and wrong configuration symbols.
    }
    // Absent sourcePaths retains graph's existing automatic home inclusion on cold load.
    std::fs::remove_file(f.b.join("workspace.json")).expect("remove fixture config");
    let mut graph = success(f.run(&["tool", "graph"], Some(&f.b), &f.a, true));
    let syms = graph["symbols"].as_array_mut().expect("symbols");
    assert_eq!(
        syms.iter()
            .filter(|s| **s == json!("HomeNotConfigured"))
            .count(),
        1
    );
    syms.retain(|s| *s != json!("HomeNotConfigured"));
    assert_graph(&graph, &f.b, "B");
}
#[test]
fn snapshot_home_relationships_are_excluded_for_all_edge_kinds_cold_and_warm() {
    let mut f = Fixture::new();
    // A similarly prefixed workspace must not be classified as the home library.
    let neighbor = f
        .dir
        .path()
        .join("home/.kotlin-lsp/sources-neighbor % # 库");
    std::fs::create_dir_all(neighbor.parent().expect("neighbor parent")).expect("mkdir");
    std::fs::rename(&f.b, &neighbor).expect("move fixture B beside home sources");
    f.b = neighbor;
    let code = f.b.join("app/src/main/Code.kt");
    write(&code, "package workspace\nimport sample.Visible\nopen class WorkspaceBase {\n open fun work() {}\n}\nclass WorkspaceChild : WorkspaceBase() {\n override fun work() {}\n}\nfun workspaceTarget() {}\nfun workspaceEntry() { workspaceTarget() }\n");
    let library = f
        .dir
        .path()
        .join("home/.kotlin-lsp/sources/Library % # 库.kt");
    write(&library, "package homelib\nimport sample.LibraryImport\nopen class HomeBase {\n open fun action() {}\n}\nclass HomeChild : HomeBase() {\n override fun action() {}\n}\n/** Library entry documentation. */\nfun libraryTarget() {}\nfun libraryEntry() { libraryTarget() }\n");
    let file_uri =
        tower_lsp::lsp_types::Url::from_file_path(code.canonicalize().expect("code")).expect("URI");
    let expected_relationships = json!({"calls":[["workspaceEntry","workspaceTarget"]],"extends":[["WorkspaceChild","WorkspaceBase"]],"overrides":[["workspace.WorkspaceChild.work","work"]],"imports":[[file_uri.as_str(),"sample.Visible"]]});
    for (include, exclude) in [
        (false, false),
        (true, false),
        (true, false),
        (true, true),
        (false, false),
        (false, true),
    ] {
        let mut args = vec!["tool", "snapshot"];
        if include {
            args.push("--include-libraries");
        }
        if exclude {
            args.push("--exclude-relationships");
        }
        let out = f.run(&args, Some(&f.b), &f.a, true);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(!out.stderr.is_empty(), include, "{out:?}");
        let v: Value = serde_json::from_slice(&out.stdout).expect("JSON");
        if exclude {
            assert!(v.get("relationships").is_none(), "{v}");
        } else {
            assert_eq!(
                v["relationships"], expected_relationships,
                "include={include}: {v}"
            );
        }
        assert_modules(&v["modules"], &f.b.canonicalize().expect("B"), "B");
        let syms = v["symbols"].as_array().expect("symbols");
        let workspace_names = sorted(vec![
            json!("WorkspaceBase"),
            json!("WorkspaceChild"),
            json!("work"),
            json!("work"),
            json!("workspaceEntry"),
            json!("workspaceTarget"),
        ]);
        assert_eq!(
            sorted(
                syms.iter()
                    .filter(|s| s["file"] == snapshot_file(&f.b))
                    .map(|s| s["name"].clone())
                    .collect()
            ),
            workspace_names
        );
        let lib_uri =
            tower_lsp::lsp_types::Url::from_file_path(library.canonicalize().expect("library"))
                .expect("URI");
        let lib: Vec<_> = syms
            .iter()
            .filter(|s| s["file"] == lib_uri.as_str().trim_start_matches("file://"))
            .collect();
        assert_eq!(lib.len(), if include { 6 } else { 0 });
        assert_eq!(syms.len(), if include { 12 } else { 6 });
        if include {
            let target = lib
                .iter()
                .find(|s| s["name"] == "libraryTarget")
                .expect("library target");
            assert_eq!(target["signature"], "fun libraryTarget()");
            assert_eq!(target["doc"], "Library entry documentation.");
            assert_eq!(target["line"], 10);
            assert_eq!(target["fq_name"], "homelib.libraryTarget");
        }
    }
}
