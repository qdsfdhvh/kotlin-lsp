//! Tests for src/gradle/ parser and cache modules.

use std::path::Path;

use super::parser;
use super::GradleDeps;

// ── TOML version catalog ─────────────────────────────────────────────────

#[test]
fn parse_version_catalog_basic() {
    let toml = r#"
[versions]
kotlin = "1.9.0"
compose = "1.5.0"

[libraries]
kotlin-stdlib = "org.jetbrains.kotlin:kotlin-stdlib:1.9.0"
compose-ui = { module = "androidx.compose.ui:ui", version.ref = "compose" }
"#;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("libs.versions.toml");
    std::fs::write(&path, toml).expect("write");

    let versions = parser::parse_version_catalog(&path).expect("parse");
    assert_eq!(versions.get("kotlin").unwrap(), "1.9.0");
    assert_eq!(versions.get("compose").unwrap(), "1.5.0");
}

#[test]
fn parse_version_catalog_missing_file() {
    let result = parser::parse_version_catalog(Path::new("/nonexistent/libs.versions.toml"));
    assert!(result.is_err());
}

// ── parse_project integration ───────────────────────────────────────────

#[test]
fn parse_project_resolves_catalog_gav() {
    let dir = tempfile::tempdir().expect("tempdir");
    let gradle_dir = dir.path().join("gradle");
    std::fs::create_dir(&gradle_dir).expect("mkdir");

    std::fs::write(
        gradle_dir.join("libs.versions.toml"),
        "[libraries]\nstdlib = \"org.jetbrains.kotlin:kotlin-stdlib:1.9.0\"\n",
    )
    .expect("write toml");

    std::fs::write(
        dir.path().join("build.gradle.kts"),
        "dependencies { implementation(libs.stdlib) }\n",
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.external.len(), 1, "expected 1 external dep");
    assert_eq!(deps.external[0].alias, "libs.stdlib");
    assert_eq!(deps.external[0].group, "org.jetbrains.kotlin");
    assert_eq!(deps.external[0].artifact, "kotlin-stdlib");
    assert_eq!(deps.external[0].version, "1.9.0");
}

// ── build.gradle.kts literal deps ────────────────────────────────────────

#[test]
fn parse_project_literal_deps() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.0")
    api("androidx.core:core-ktx:1.10.0")
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.external.len(), 2);
    assert_eq!(deps.external[0].group, "org.jetbrains.kotlinx");
    assert_eq!(deps.external[0].artifact, "kotlinx-serialization-json");
    assert_eq!(deps.external[1].group, "androidx.core");
}

// ── settings.gradle.kts inclusion ────────────────────────────────────────

#[test]
fn parse_settings_for_projects() {
    let settings = r#"
include(":core:utils")
include(":feature:login")
include(":data:remote")
"#;
    let mut deps = GradleDeps {
        project_root: Default::default(),
        ..Default::default()
    };
    parser::extract_projects_from_settings(settings, &mut deps);
    assert_eq!(deps.projects.len(), 3);
    let names: Vec<&str> = deps
        .projects
        .iter()
        .map(|p| p.module_path.as_str())
        .collect();
    assert!(names.contains(&":core:utils"));
    assert!(names.contains(&":feature:login"));
    assert!(names.contains(&":data:remote"));
}

// ── parse_project ────────────────────────────────────────────────────────

#[test]
fn parse_project_complete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let gradle_dir = dir.path().join("gradle");
    std::fs::create_dir(&gradle_dir).expect("mkdir");

    std::fs::write(
        gradle_dir.join("libs.versions.toml"),
        "[versions]\nkotlin = \"1.9.0\"\n[libraries]\nstdlib = \"org.jetbrains.kotlin:kotlin-stdlib:1.9.0\"\n",
    )
    .expect("write toml");

    std::fs::write(
        dir.path().join("build.gradle.kts"),
        "dependencies { implementation(libs.stdlib) }\n",
    )
    .expect("write build");

    std::fs::write(
        dir.path().join("settings.gradle.kts"),
        "include(\":app\")\n",
    )
    .expect("write settings");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.external.len(), 1);
    assert_eq!(deps.external[0].alias, "libs.stdlib");
    assert_eq!(deps.projects.len(), 1);
    assert_eq!(deps.projects[0].module_path, ":app");
}

#[test]
fn parse_project_not_gradle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = super::parse_project(dir.path());
    assert!(result.is_none(), "no build.gradle.kts → None");
}

// ── cache module ─────────────────────────────────────────────────────────

#[test]
fn find_source_jar_home_dir_access() {
    let result = super::cache::find_source_jar("com.example", "test", "1.0.0");
    let _ = result;
}
