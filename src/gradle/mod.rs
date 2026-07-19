//! Gradle dependency resolution — parse build.gradle.kts, settings.gradle.kts,
//! and libs.versions.toml to resolve library coordinates, then locate source
//! JARs in the Gradle cache for indexing.

use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) mod cache;
pub(crate) mod parser;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// A Gradle plugin declaration.
#[derive(Debug, Clone)]
pub(crate) struct Plugin {
    /// Plugin ID, e.g. "org.jetbrains.kotlin.jvm"
    pub id: String,
    /// Version string if specified
    pub version: Option<String>,
    /// Whether declared with `apply false`
    pub apply_false: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GradleDeps {
    pub external: Vec<ExternalDep>,
    pub projects: Vec<ProjectDep>,
    pub plugins: Vec<Plugin>,
    pub project_root: PathBuf,
    pub android: AndroidBlock,
    /// mtime of build.gradle.kts when last parsed (seconds since epoch)
    pub mtime_secs: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalDep {
    pub alias: String,
    pub group: String,
    pub artifact: String,
    pub version: String,
    /// Gradle configuration: "implementation", "api", "testImplementation", etc.
    pub config: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectDep {
    pub alias: String,
    pub module_path: String,
}

/// Android block settings extracted from build.gradle.kts
#[derive(Debug, Clone, Default)]
pub(crate) struct AndroidBlock {
    pub namespace: Option<String>,
    pub compile_sdk: Option<u32>,
    pub application_id: Option<String>,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
}

/// Parse Gradle configuration for a project rooted at `project_root`.
/// Returns `None` if no `build.gradle.kts` is found (not a Gradle project).
pub(crate) fn parse_project(project_root: &std::path::Path) -> Option<GradleDeps> {
    let build_file = project_root.join("build.gradle.kts");
    if !build_file.exists() {
        return None;
    }

    let mut deps = GradleDeps {
        project_root: project_root.to_path_buf(),
        ..Default::default()
    };
    // Record build file mtime for staleness detection
    deps.mtime_secs = std::fs::metadata(&build_file)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // 1. Parse build.gradle.kts for dependency references.
    let build_content = std::fs::read_to_string(&build_file).ok()?;
    let version_map: HashMap<String, String> = HashMap::new();
    parser::extract_deps_from_build(&build_content, &version_map, &mut deps);

    // 2. Resolve catalog aliases → GAV from libs.versions.toml.
    let toml_paths = [
        project_root.join("gradle").join("libs.versions.toml"),
        project_root.join("libs.versions.toml"),
    ];
    if let Some(lib_map) = resolve_catalog_gav(&toml_paths) {
        for dep in &mut deps.external {
            if dep.group.is_empty() {
                if let Some((g, a, v)) = lib_map.get(&dep.alias) {
                    dep.group = g.clone();
                    dep.artifact = a.clone();
                    dep.version = v.clone();
                }
            }
        }
    }

    // 3. Parse settings.gradle.kts for project module paths.
    let settings_file = project_root.join("settings.gradle.kts");
    if let Ok(content) = std::fs::read_to_string(&settings_file) {
        parser::extract_projects_from_settings(&content, &mut deps);
    }

    let has_android = deps.android.namespace.is_some()
        || deps.android.compile_sdk.is_some()
        || deps.android.application_id.is_some()
        || deps.android.min_sdk.is_some()
        || deps.android.target_sdk.is_some();
    if deps.external.is_empty()
        && deps.projects.is_empty()
        && deps.plugins.is_empty()
        && !has_android
    {
        None
    } else {
        Some(deps)
    }
}

fn resolve_catalog_gav(
    toml_paths: &[PathBuf],
) -> Option<HashMap<String, (String, String, String)>> {
    let path = toml_paths.iter().find(|p| p.exists())?;
    let content = std::fs::read_to_string(path).ok()?;
    let doc: toml::Table = toml::from_str(&content).ok()?;
    let libs = doc.get("libraries")?.as_table()?;
    Some(parser::resolve_lib_map(libs, &doc))
}
