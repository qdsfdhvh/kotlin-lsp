//! Parser for Gradle configuration files.

use std::collections::HashMap;
use std::path::Path;

use super::{ExternalDep, GradleDeps, ProjectDep};

// ── TOML version catalog ────────────────────────────────────────────────

#[allow(dead_code)] // used in tests
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

// ── Plugin scanning ────────────────────────────────────────────────────

/// Scan build.gradle.kts for plugin declarations.
fn scan_plugins(content: &str) -> Vec<super::Plugin> {
    let mut plugins = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Pattern 1: id("plugin.id") [version "x"] [apply false]
    // Pattern 2: kotlin("jvm") [version "x"] [apply false]
    for pattern in &["id(", "kotlin("] {
        let mut search_from = 0;
        while let Some(pos) = content[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            // Extract the plugin identifier between quotes
            let plugin_id = if let Some(rest) = content[start..]
                .strip_prefix('"')
                .and_then(|s| s.split('"').next())
            {
                if rest.is_empty() {
                    search_from = start + 1;
                    continue;
                }
                if *pattern == "kotlin(" {
                    format!("kotlin-{rest}")
                } else {
                    rest.to_string()
                }
            } else {
                search_from = start + 1;
                continue;
            };

            if !seen.insert(plugin_id.clone()) {
                search_from = start + plugin_id.len();
                continue;
            }

            // Look for version and apply false after the closing paren
            let after_paren_start = start + plugin_id.len() + 2; // "x")
            let after = &content[after_paren_start..];
            let end_of_line = after.find('\n').unwrap_or(after.len());
            let after_line = &after[..end_of_line];

            let version = if let Some(ver_pos) = after_line.find("version ") {
                let ver_start = ver_pos + "version ".len();
                let ver_str = &after_line[ver_start..];
                ver_str
                    .strip_prefix('"')
                    .and_then(|s| s.split('"').next())
                    .map(|s| s.to_string())
            } else {
                None
            };

            let apply_false = after_line.contains("apply false");

            plugins.push(super::Plugin {
                id: plugin_id,
                version,
                apply_false,
            });

            search_from = after_paren_start;
        }
    }

    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}
// ── build.gradle.kts extraction ────────────────────────────────────────

const DEP_CONFIGS: &[&str] = &[
    "testImplementation(",
    "androidTestImplementation(",
    "debugImplementation(",
    "implementation(",
    "api(",
    "compileOnly(",
    "runtimeOnly(",
    "kapt(",
    "ksp(",
];

fn scan_catalog_refs(content: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("libs.") {
        let abs = search_from + pos;
        let window = abs.saturating_sub(80)..abs;
        let preceding = &content[window];
        // Detect which config this libs ref is under
        let config = DEP_CONFIGS
            .iter()
            .find(|c| preceding.contains(*c))
            .map(|c| c.trim_end_matches('('))
            .unwrap_or("implementation");

        let start = abs + "libs.".len();
        let end = content[start..]
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .map(|p| start + p)
            .unwrap_or(content.len());
        let alias = format!("libs.{}", &content[start..end]);
        if alias.len() > "libs.".len() {
            refs.push((alias, config.to_string()));
        }
        search_from = abs + 1;
    }
    refs
}

fn scan_project_refs(content: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find("projects.") {
        let abs = search_from + pos;
        let window = abs.saturating_sub(80)..abs;
        let preceding = &content[window];
        if let Some(config) = DEP_CONFIGS.iter().find(|c| preceding.contains(*c)) {
            let config_name = config.trim_end_matches('(');
            let start = abs + "projects.".len();
            let end = content[start..]
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                .map(|p| start + p)
                .unwrap_or(content.len());
            let alias = format!("projects.{}", &content[start..end]);
            if alias.len() > "projects.".len() {
                refs.push((alias, config_name.to_string()));
            }
        }
        search_from = abs + 1;
    }
    refs
}

fn scan_literal_deps(content: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    for config in DEP_CONFIGS {
        let config_name = config.trim_end_matches('(');
        let mut search_from = 0;
        while let Some(pos) = content[search_from..].find(*config) {
            let abs = search_from + pos + config.len();
            if abs < content.len() && content.as_bytes().get(abs) == Some(&b'"') {
                let start = abs + 1;
                if let Some(end) = content[start..].find('"') {
                    let gav = &content[start..start + end];
                    if gav.contains(':') {
                        refs.push((format!("literal:{}", gav), config_name.to_string()));
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
    for (alias, config) in scan_catalog_refs(content) {
        deps.external.push(ExternalDep {
            alias,
            group: String::new(),
            artifact: String::new(),
            version: String::new(),
            config,
        });
    }
    for (alias, _config) in scan_project_refs(content) {
        deps.projects.push(ProjectDep {
            alias,
            module_path: String::new(),
        });
    }
    for (alias, config) in scan_literal_deps(content) {
        let gav_str = alias.strip_prefix("literal:").unwrap_or(&alias);
        if let Some((group, artifact, version)) = split_gav(gav_str) {
            deps.external.push(ExternalDep {
                alias: alias.clone(),
                group: group.to_string(),
                artifact: artifact.to_string(),
                version: version.to_string(),
                config,
            });
        }
    }
    // Scan plugin declarations
    let plugins = scan_plugins(content);
    deps.plugins.extend(plugins);
    // Scan android block
    deps.android = scan_android_block(content);
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

// ── Android block scanning ──────────────────────────────────────────────

/// Scan build.gradle.kts for android { ... } block settings.
fn scan_android_block(content: &str) -> super::AndroidBlock {
    let mut block = super::AndroidBlock::default();

    // Simple line-based extraction from the android block
    let android_start = if let Some(pos) = content.find("android {") {
        pos
    } else {
        // Also try "android{" without space
        content.find("android{").unwrap_or(usize::MAX)
    };
    if android_start == usize::MAX {
        return block;
    }

    let inside = &content[android_start..];
    // Find the matching closing brace by tracking brace depth
    let mut depth = 0;
    let mut end = 0;
    let bytes = inside.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let block_text = if end > 0 { &inside[..end] } else { inside };

    for line in block_text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("namespace =") {
            block.namespace = Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        } else if let Some(value) = trimmed.strip_prefix("compileSdk =") {
            block.compile_sdk = value.trim().parse().ok();
        } else if let Some(value) = trimmed.strip_prefix("applicationId =") {
            block.application_id = Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        } else if let Some(value) = trimmed.strip_prefix("minSdk =") {
            block.min_sdk = value.trim().parse().ok();
        } else if let Some(value) = trimmed.strip_prefix("targetSdk =") {
            block.target_sdk = value.trim().parse().ok();
        }
    }

    block
}
