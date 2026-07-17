//! Parser for Gradle configuration files.

use std::collections::HashMap;
use std::path::Path;

use super::{ExternalDep, GradleDeps, ProjectDep};

// ── TOML version catalog ────────────────────────────────────────────────

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

/// Map catalog alias (e.g. "libs.kotlinx.coroutines") → (group, artifact, version).
pub(crate) fn resolve_lib_map(
    libs: &toml::Table,
    doc: &toml::Table,
) -> HashMap<String, (String, String, String)> {
    let versions: HashMap<&str, &str> = doc
        .get("versions")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.as_str(), s)))
                .collect()
        })
        .unwrap_or_default();
    let mut map = HashMap::new();
    for (alias, value) in libs {
        let full_alias = format!("libs.{alias}");
        match value {
            toml::Value::String(s) => {
                if let Some((g, a, v)) = split_gav(s) {
                    map.insert(full_alias, (g.to_string(), a.to_string(), v.to_string()));
                }
            }
            toml::Value::Table(t) => {
                let module = t.get("module").and_then(|v| v.as_str()).unwrap_or("");
                let version = t
                    .get("version")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        // version.ref creates a nested { ref = "..." } table in TOML
                        t.get("version")
                            .and_then(|v| v.as_table())
                            .and_then(|vt| vt.get("ref"))
                            .and_then(|v| v.as_str())
                            .or_else(|| t.get("version.ref").and_then(|v| v.as_str()))
                    })
                    .and_then(|r| versions.get(r).copied());
                if let (Some(v), Some((g, a))) = (version, parse_module(module)) {
                    map.insert(full_alias, (g.to_string(), a.to_string(), v.to_string()));
                }
            }
            _ => {}
        }
    }
    map
}

fn parse_module(module: &str) -> Option<(&str, &str)> {
    let mut parts = module.splitn(2, ':');
    Some((parts.next()?, parts.next()?)).filter(|(g, a)| !g.is_empty() && !a.is_empty())
}

fn split_gav(s: &str) -> Option<(&str, &str, &str)> {
    let mut parts = s.splitn(3, ':');
    let g = parts.next()?;
    let a = parts.next()?;
    let v = parts.next()?;
    if g.is_empty() || a.is_empty() || v.is_empty() {
        None
    } else {
        Some((g, a, v))
    }
}

// ── build.gradle.kts extraction ────────────────────────────────────────

const DEP_CONFIGS: &[&str] = &[
    "implementation(",
    "api(",
    "testImplementation(",
    "compileOnly(",
    "runtimeOnly(",
    "debugImplementation(",
];

fn scan_catalog_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("libs.") {
        let abs = search_from + pos;
        // Look for a dep config within 80 chars before the ref
        let window = abs.saturating_sub(80)..abs;
        let preceding = &content[window];
        if DEP_CONFIGS.iter().any(|c| preceding.contains(c)) {
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
        search_from = abs + 1;
    }
    refs
}

fn scan_project_refs(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("projects.") {
        let abs = search_from + pos;
        let window = abs.saturating_sub(80)..abs;
        let preceding = &content[window];
        if DEP_CONFIGS.iter().any(|c| preceding.contains(c)) {
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
        search_from = abs + 1;
    }
    refs
}

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
    for alias in scan_catalog_refs(content) {
        deps.external.push(ExternalDep {
            alias,
            group: String::new(),
            artifact: String::new(),
            version: String::new(),
        });
    }
    for alias in scan_project_refs(content) {
        deps.projects.push(ProjectDep {
            alias,
            module_path: String::new(),
        });
    }
    for alias in scan_literal_deps(content) {
        let gav_str = alias.strip_prefix("literal:").unwrap_or(&alias);
        if let Some((group, artifact, version)) = split_gav(gav_str) {
            deps.external.push(ExternalDep {
                alias: alias.clone(),
                group: group.to_string(),
                artifact: artifact.to_string(),
                version: version.to_string(),
            });
        }
    }
}

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
