//! Gradle dependency resolution — parse build.gradle.kts, settings.gradle.kts,
//! and libs.versions.toml to resolve library coordinates, then locate source
//! JARs in the Gradle cache for indexing.
//!
//! # Design
//!
//! - **Zero Gradle daemon** — shallow AST-based parsing only; no `gradle` CLI calls.
//! - **Lazy by default** — `index` without `--gradle` does not touch Gradle at all.
//! - **On-demand resolution** — when `--gradle` is passed and a symbol is not found
//!   in the workspace index, we parse Gradle configs on first miss and index the
//!   relevant source JARs.

pub(crate) mod cache;
pub(crate) mod parser;

use std::collections::HashMap;
use std::path::PathBuf;

/// Parsed Gradle dependency information for a workspace.
#[derive(Debug, Clone, Default)]
pub(crate) struct GradleDeps {
    /// Resolved external dependencies (group:artifact:version).
    pub external: Vec<ExternalDep>,
    /// Local project dependencies mapped to their module paths.
    pub projects: Vec<ProjectDep>,
    /// Root directory of the project (where settings.gradle.kts lives).
    pub project_root: PathBuf,
}

/// An external Maven/Gradle dependency resolved from libs.versions.toml.
#[derive(Debug, Clone)]
pub(crate) struct ExternalDep {
    /// Catalog alias, e.g. `libs.kotlinx.coroutines.core`.
    pub alias: String,
    /// Maven group, e.g. `org.jetbrains.kotlinx`.
    pub group: String,
    /// Maven artifact, e.g. `kotlinx-coroutines-core`.
    pub artifact: String,
    /// Resolved version string.
    pub version: String,
}

/// A local project dependency resolved from settings.gradle.kts.
#[derive(Debug, Clone)]
pub(crate) struct ProjectDep {
    /// Catalog alias, e.g. `projects.core.analytics`.
    pub alias: String,
    /// Module path, e.g. `:core:analytics`.
    pub module_path: String,
}

/// Parse Gradle configuration for a project rooted at `project_root`.
///
/// Returns `None` if no `build.gradle.kts` or `settings.gradle.kts` is found
/// (not a Gradle project), or if parsing fails.
pub(crate) fn parse_project(project_root: &std::path::Path) -> Option<GradleDeps> {
    let build_file = project_root.join("build.gradle.kts");
    if !build_file.exists() {
        // Also check build.gradle (Groovy) — not supported yet.
        return None;
    }

    let mut deps = GradleDeps {
        project_root: project_root.to_path_buf(),
        ..Default::default()
    };

    // 1. Parse libs.versions.toml (may be in gradle/ or root).
    let toml_paths = [
        project_root.join("gradle").join("libs.versions.toml"),
        project_root.join("libs.versions.toml"),
    ];
    let version_map: HashMap<String, String> = toml_paths
        .iter()
        .find(|p| p.exists())
        .and_then(|p| parser::parse_version_catalog(p).ok())
        .unwrap_or_default();

    // 2. Parse build.gradle.kts for dependency references.
    if let Ok(content) = std::fs::read_to_string(&build_file) {
        parser::extract_deps_from_build(&content, &version_map, &mut deps);
    }

    // 3. Parse settings.gradle.kts for project module paths.
    let settings_file = project_root.join("settings.gradle.kts");
    if let Ok(content) = std::fs::read_to_string(&settings_file) {
        parser::extract_projects_from_settings(&content, &mut deps);
    }

    if deps.external.is_empty() && deps.projects.is_empty() {
        None
    } else {
        Some(deps)
    }
}
