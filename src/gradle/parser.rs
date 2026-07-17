//! Parser for Gradle configuration files — zero external deps beyond `toml`.
//!
//! Three file types are parsed:
//!
//! 1. **libs.versions.toml** — TOML; two library formats:
//!    - Rich: `lib = { module = "g:a", version.ref = "v" }`
//!    - Simple: `lib = "g:a:v"`
//!
//! 2. **build.gradle.kts** — Kotlin DSL; dependency references:
//!    - `implementation(libs.xxx.yyy)` — catalog deps
//!    - `implementation(projects.xxx)` — project deps
//!    - `implementation("g:a:v")` — literal deps
//!
//! 3. **settings.gradle.kts** — Kotlin DSL; module includes:
//!    - `include(":core:analytics")` — maps to `projects.core.analytics`

use std::collections::HashMap;
use std::path::Path;

use super::{ExternalDep, GradleDeps, ProjectDep};

// ── TOML version catalog ────────────────────────────────────────────────

/// Parse `libs.versions.toml` and return the `[versions]` section as a
/// name → value map. Used to resolve `version.ref` in library definitions.
pub(crate) fn parse_version_catalog(path: &Path) -> Result<HashMap<String, String>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let doc: toml::Table = toml::from_str(&content).map_err(|e| format!("parse toml: {e}"))?;

    let mut versions = HashMap::new();

    if let Some(t) = doc.get("versions").and_then(|v| v.as_table()) {
        for (k, v) in t {
            if let Some(s) = v.as_str() {
                versions.insert(k.clone(), s.to_string());
            }
        }
    }

    Ok(versions)
}

/// Parse `libs.versions.toml` libraries section. Returns `(alias → ExternalDep)` map.
///
/// Supports two library formats:
/// - Rich: `lib = { module = "g:a", version.ref = "v" }`
/// - Simple: `lib = "g:a:v"`
pub(crate) fn parse_libraries_toml(path: &Path) -> Result<HashMap<String, ExternalDep>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let doc: toml::Table = toml::from_str(&content).map_err(|e| format!("parse toml: {e}"))?;

    let versions: HashMap<String, String> = doc
        .get("versions")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let mut libs = HashMap::new();

    if let Some(libraries) = doc.get("libraries").and_then(|v| v.as_table()) {
        for (alias, value) in libraries {
            let dep = match value {
                toml::Value::Table(t) => {
                    let module = t.get("module").and_then(|v| v.as_str()).unwrap_or("");
                    let version = t
                        .get("version")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            t.get("version.ref")
                                .and_then(|v| v.as_str())
                                .and_then(|ref_key| versions.get(ref_key).map(|s| s.as_str()))
                        })
                        .unwrap_or("");
                    parse_gav(module, version).map(|d| ExternalDep {
                        alias: alias.clone(),
                        ..d
                    })
                }
                toml::Value::String(s) => {
                    split_gav(s).map(|(group, artifact, version)| ExternalDep {
                        alias: alias.clone(),
                        group: group.to_string(),
                        artifact: artifact.to_string(),
                        version: version.to_string(),
                    })
                }
                _ => None,
            };

            if let Some(dep) = dep {
                libs.insert(alias.clone(), dep);
            }
        }
    }

    Ok(libs)
}

fn split_gav(s: &str) -> Option<(&str, &str, &str)> {
    let mut parts = s.splitn(3, ':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        None
    } else {
        Some((group, artifact, version))
    }
}

fn parse_gav(module: &str, version: &str) -> Option<ExternalDep> {
    let mut parts = module.splitn(2, ':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        None
    } else {
        Some(ExternalDep {
            alias: String::new(),
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
        })
    }
}

// ── build.gradle.kts extraction (string-scan, no regex) ────────────────

const DEP_CONFIGS: &[&str] = &[
    "implementation(",
    "api(",
    "testImplementation(",
    "compileOnly(",
    "runtimeOnly(",
    "debugImplementation(",
];

/// Extract all `libs.xxx.yyy` references from dependency calls in content.
fn scan_catalog_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("libs.") {
        let abs = search_from + pos;
        // Check this "libs." appears inside a dep call
        if let Some(before) = content[..abs].rfind('(') {
            let slice = &content[before..abs];
            if DEP_CONFIGS.iter().any(|c| slice.contains(c)) {
                // Extract "libs.xxx.yyy" up to next `)` or non-word char
                let start = abs + "libs.".len();
                let end = content[start..]
                    .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                    .map(|p| start + p)
                    .unwrap_or(content.len());
                let alias = format!("libs.{}", &content[start..end]);
                if alias.len() > "libs.".len() {
                    refs.push(alias);
                }
            }
        }
        search_from = abs + 1;
    }
    refs
}

/// Extract all `projects.xxx.yyy` references from dependency calls.
fn scan_project_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("projects.") {
        let abs = search_from + pos;
        // Only match when "projects." is immediately after a "(" and preceded by a dep config
        if let Some(before) = content[..abs].rfind('(') {
            let slice = &content[before..abs];
            if DEP_CONFIGS.iter().any(|c| slice.contains(c)) {
                let start = abs + "projects.".len();
                let end = content[start..]
                    .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                    .map(|p| start + p)
                    .unwrap_or(content.len());
                let alias = format!("projects.{}", &content[start..end]);
                if alias.len() > "projects.".len() {
                    refs.push(alias);
                }
            }
        }
        search_from = abs + 1;
    }
    refs
}

/// Extract literal `implementation("g:a:v")` strings.
fn scan_literal_deps(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for config in DEP_CONFIGS {
        let mut search_from = 0;
        while let Some(pos) = content[search_from..].find(*config) {
            let abs = search_from + pos + config.len();
            if abs < content.len() && content.as_bytes().get(abs) == Some(&b'"') {
                let start = abs + 1;
                if let Some(end) = content[start..].find('"') {
                    let gav = &content[start..start + end];
                    if gav.contains(':') {
                        refs.push(format!("literal:{}", gav));
                    }
                }
            }
            search_from = abs;
        }
    }
    refs
}

pub(crate) fn extract_deps_from_build(
    content: &str,
    _catalog: &HashMap<String, String>,
    deps: &mut GradleDeps,
) {
    // Catalog deps
    for alias in scan_catalog_refs(content) {
        deps.external.push(ExternalDep {
            alias,
            group: String::new(),
            artifact: String::new(),
            version: String::new(),
        });
    }

    // Project deps
    for alias in scan_project_refs(content) {
        deps.projects.push(ProjectDep {
            alias,
            module_path: String::new(),
        });
    }

    // Literal deps
    for alias in scan_literal_deps(content) {
        let gav_str = alias.strip_prefix("literal:").unwrap_or(&alias);
        let alias_clone = alias.clone();
        if let Some((group, artifact, version)) = split_gav(gav_str) {
            deps.external.push(ExternalDep {
                alias: alias_clone,
                group: group.to_string(),
                artifact: artifact.to_string(),
                version: version.to_string(),
            });
        }
    }
}

// ── settings.gradle.kts extraction ─────────────────────────────────────

pub(crate) fn extract_projects_from_settings(content: &str, deps: &mut GradleDeps) {
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("include(\"") {
        let abs = search_from + pos + "include(\"".len();
        if let Some(end) = content[abs..].find('"') {
            let module_path = &content[abs..abs + end];
            let alias = format!(
                "projects.{}",
                module_path.trim_start_matches(':').replace(':', ".")
            );

            let found = deps.projects.iter_mut().any(|p| {
                if p.alias == alias {
                    p.module_path = module_path.to_string();
                    true
                } else {
                    false
                }
            });
            if !found {
                deps.projects.push(ProjectDep {
                    alias,
                    module_path: module_path.to_string(),
                });
            }
        }
        search_from = abs;
    }
}
