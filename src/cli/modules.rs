//! Gradle module dependency graph — parse settings.gradle.kts and
//! build.gradle.kts to discover module structure without Gradle execution.

use std::path::{Path, PathBuf};

use serde::Serialize;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModuleInfo {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) source_sets: Vec<String>,
    pub(crate) file_count: usize,
    pub(crate) dependencies: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ModulesOutput {
    modules: Vec<ModuleInfo>,
}

#[derive(Debug, Serialize)]
struct DepsOutput {
    module: String,
    direction: String,
    dependencies: Vec<String>,
    dependents: Vec<String>,
}

// ── Entry points ────────────────────────────────────────────────────────────

pub(crate) fn run_modules(json: bool) {
    let modules = discover_modules();
    if json {
        let output = ModulesOutput { modules };
        println!(
            "{}",
            serde_json::to_string_pretty(&output).expect("serialize JSON")
        );
    } else {
        for m in &modules {
            println!(
                "  {name} ({count} files) [{deps}] @ {path}",
                name = m.name,
                count = m.file_count,
                deps = m.dependencies.join(", "),
                path = m.path,
            );
        }
    }
}

pub(crate) fn run_module_deps(module: &str, direction: &str, json: bool) {
    let modules = discover_modules();

    let deps = modules
        .iter()
        .find(|m| m.name == module)
        .map(|m| m.dependencies.clone())
        .unwrap_or_default();

    let dependents: Vec<String> = modules
        .iter()
        .filter(|m| m.dependencies.contains(&module.to_string()))
        .map(|m| m.name.clone())
        .collect();

    if json {
        let output = DepsOutput {
            module: module.to_string(),
            direction: direction.to_string(),
            dependencies: deps,
            dependents,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&output).expect("serialize JSON")
        );
    } else {
        if direction == "depends-on" || direction == "both" {
            println!("{module} depends on:");
            for d in &deps {
                println!("  - {d}");
            }
        }
        if direction == "depended-by" || direction == "both" {
            println!("Dependents of {module}:");
            for d in &dependents {
                println!("  - {d}");
            }
        }
    }
}

pub(crate) fn run_module_files(module: &str, json: bool) {
    let root = find_project_root();
    let modules = discover_modules_in_root(&root);

    let module = modules.iter().find(|m| m.name == module);
    let files = module
        .map(|m| {
            let path = Path::new(&m.path);
            find_kotlin_files(path)
        })
        .unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&files).expect("serialize JSON")
        );
    } else {
        for f in &files {
            println!("  {}", f.display());
        }
    }
}

// ── Module discovery ────────────────────────────────────────────────────────

pub(crate) fn discover_modules() -> Vec<ModuleInfo> {
    let root = find_project_root();
    discover_modules_in_root(&root)
}

fn find_project_root() -> PathBuf {
    // Walk up from cwd to find settings.gradle.kts or settings.gradle.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cur = cwd.as_path();
    while let Some(parent) = cur.parent() {
        if cur.join("settings.gradle.kts").exists() || cur.join("settings.gradle").exists() {
            return cur.to_path_buf();
        }
        if parent == cur {
            break;
        }
        cur = parent;
    }
    cwd
}

fn discover_modules_in_root(root: &Path) -> Vec<ModuleInfo> {
    let mut modules = Vec::new();

    // Parse settings.gradle.kts for include() calls. Dedupe so a module listed
    // twice (e.g. via a plugin helper repeated per block) is reported once
    // (issue #233).
    let mut seen = std::collections::HashSet::new();
    let included: Vec<String> = parse_settings(root)
        .into_iter()
        .filter(|m| seen.insert(m.clone()))
        .collect();

    // For each included module, find its directory and parse build.gradle.kts.
    for module_name in &included {
        let module_dir = find_module_dir(root, module_name);

        let deps = if module_dir.exists() {
            parse_build_deps(&module_dir)
        } else {
            vec![]
        };

        let file_count = if module_dir.exists() {
            find_kotlin_files(&module_dir).len()
        } else {
            0
        };

        let source_sets = detect_source_sets(&module_dir);

        modules.push(ModuleInfo {
            name: module_name.clone(),
            path: module_dir.display().to_string(),
            source_sets,
            file_count,
            dependencies: deps,
        });
    }

    // If no modules found, add root as a single default module.
    if modules.is_empty() && root.join("src").exists() {
        let deps = parse_build_deps(root);
        let file_count = find_kotlin_files(root).len();
        let source_sets = detect_source_sets(root);
        modules.push(ModuleInfo {
            name: ":".to_string(),
            path: root.display().to_string(),
            source_sets,
            file_count,
            dependencies: deps,
        });
    }

    modules
}

// ── Settings parsing ─────────────────────────────────────────────────────────

fn parse_settings(root: &Path) -> Vec<String> {
    let mut modules = Vec::new();

    for filename in &["settings.gradle.kts", "settings.gradle"] {
        let path = root.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Match: include(":module") or include(":module1", ":module2")
            for line in content.lines() {
                let trimmed = line.trim();
                // Only real `include(...)` calls declare modules. Lines that
                // merely start with "include" — includeBuild(...) and plugin
                // helpers like includeGroupAndSubgroups("androidx") — are NOT
                // modules; matching them leaks dependency group ids into the
                // module list with non-existent paths (issue #233).
                if trimmed.starts_with("include(") {
                    modules.extend(extract_module_names(trimmed));
                }
            }
        }
    }

    modules
}

fn extract_module_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    // Match quoted strings like ":module" or "project(':module')"
    let rest = line;
    let mut i = 0;
    let chars: Vec<char> = rest.chars().collect();
    while i < chars.len() {
        if chars[i] == '"' || chars[i] == '\'' {
            i += 1;
            let mut name = String::new();
            while i < chars.len() && chars[i] != '"' && chars[i] != '\'' {
                name.push(chars[i]);
                i += 1;
            }
            if !name.is_empty() && (name.starts_with(':') || !name.contains(' ')) {
                names.push(name);
            }
        }
        i += 1;
    }
    names
}

// ── Build dependency parsing ─────────────────────────────────────────────────

fn parse_build_deps(module_dir: &Path) -> Vec<String> {
    let mut deps = Vec::new();

    for filename in &["build.gradle.kts", "build.gradle"] {
        let path = module_dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let trimmed = line.trim();
                // Match: implementation(project(":lib")) or api(project(":lib"))
                if (trimmed.starts_with("implementation")
                    || trimmed.starts_with("api")
                    || trimmed.starts_with("compileOnly")
                    || trimmed.starts_with("testImplementation")
                    || trimmed.starts_with("androidTestImplementation"))
                    && trimmed.contains("project")
                {
                    deps.extend(extract_module_names(trimmed));
                }
            }
        }
    }

    deps
}

// ── File discovery ───────────────────────────────────────────────────────────

fn find_kotlin_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip build, .git, etc.
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "build" || name == ".git" || name == "node_modules" {
                    continue;
                }
                files.extend(find_kotlin_files(&path));
            } else if let Some(ext) = path.extension() {
                if ext == "kt" || ext == "kts" || ext == "java" {
                    files.push(path);
                }
            }
        }
    }
    files
}

fn find_module_dir(root: &Path, module_name: &str) -> PathBuf {
    // Module name like ":feature:login" → feature/login
    let rel = module_name.trim_start_matches(':').replace(':', "/");
    let candidate = root.join(&rel);
    if candidate.exists() {
        return candidate;
    }
    // Fallback: try without leading colon
    root.join(module_name.trim_start_matches(':'))
}

fn detect_source_sets(module_dir: &Path) -> Vec<String> {
    let mut sets = Vec::new();
    let src_dir = module_dir.join("src");
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    sets.push(name.to_string());
                }
            }
        }
    }
    sets
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_include_single() {
        let modules = extract_module_names(r#"include(":app")"#);
        assert!(modules.contains(&":app".to_string()));
    }

    #[test]
    fn parse_include_multiple() {
        let modules = extract_module_names(r#"include(":app", ":core")"#);
        assert!(modules.contains(&":app".to_string()));
        assert!(modules.contains(&":core".to_string()));
    }

    #[test]
    fn parse_implementation_project() {
        let modules = extract_module_names(r#"implementation(project(":core:network"))"#);
        assert!(modules.contains(&":core:network".to_string()));
    }

    #[test]
    fn detect_source_sets_from_temp_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/main/kotlin")).unwrap();
        std::fs::create_dir_all(dir.path().join("src/test/kotlin")).unwrap();
        let sets = detect_source_sets(dir.path());
        assert!(sets.contains(&"main".to_string()));
        assert!(sets.contains(&"test".to_string()));
    }

    // ── module list must not leak dependency group ids (#233) ──────────────

    #[test]
    fn parse_settings_ignores_include_group_and_subgroups() {
        // Regression for #233: plugin helpers like
        // `includeGroupAndSubgroups("androidx")` start with "include" but are
        // dependency group ids, not modules. Only real `include(...)` calls
        // declare modules.
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("settings.gradle.kts"),
            r#"rootProject.name = "sample"
include(":app")
include(":core:data")
pluginManagement {
    repositories {
        includeGroupAndSubgroups("androidx")
        includeGroupAndSubgroups("com.android")
        includeGroupAndSubgroups("com.google")
        includeGroupAndSubgroups("org.chromium")
    }
}
includeBuild("build-logic")
"#,
        )
        .unwrap();
        let modules = parse_settings(dir.path());
        assert_eq!(
            modules,
            vec![":app".to_string(), ":core:data".to_string()],
            "group ids from includeGroupAndSubgroups and includeBuild must not appear as modules, got: {modules:?}"
        );
    }

    #[test]
    fn discover_modules_dedupes_repeated_includes() {
        // Regression for #233: a module listed more than once (e.g. the same
        // helper block repeated per build type) must be reported once.
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("app/src/main/kotlin")).unwrap();
        std::fs::write(
            dir.path().join("settings.gradle.kts"),
            "include(\":app\")\ninclude(\":app\")\n",
        )
        .unwrap();
        let modules = discover_modules_in_root(dir.path());
        let names: Vec<&str> = modules.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            vec![":app"],
            "duplicate includes must be deduped, got: {names:?}"
        );
    }
}
