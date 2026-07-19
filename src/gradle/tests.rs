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
    assert_eq!(deps.external[0].config, "implementation");
    assert_eq!(deps.projects.len(), 1);
    assert_eq!(deps.projects[0].module_path, ":app");
}

#[test]
fn parse_project_not_gradle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let result = super::parse_project(dir.path());
    assert!(result.is_none(), "no build.gradle.kts → None");
}

// ── Plugin scanning ─────────────────────────────────────────────────────

#[test]
fn scan_plugins_id_basic() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"plugins {
    id("org.jetbrains.kotlin.jvm") version "1.9.0"
    id("com.android.application") version "8.2.0" apply false
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.plugins.len(), 2, "expected 2 plugins");
    assert_eq!(deps.plugins[0].id, "com.android.application"); // sorted
    assert_eq!(deps.plugins[0].version.as_deref(), Some("8.2.0"));
    assert!(deps.plugins[0].apply_false);
    assert_eq!(deps.plugins[1].id, "org.jetbrains.kotlin.jvm");
    assert_eq!(deps.plugins[1].version.as_deref(), Some("1.9.0"));
    assert!(!deps.plugins[1].apply_false);
}

#[test]
fn scan_plugins_kotlin_shorthand() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"plugins {
    kotlin("jvm") version "1.9.0"
    kotlin("plugin.serialization") version "1.9.0"
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.plugins.len(), 2);
    // kotlin("jvm") → "kotlin-jvm"
    assert_eq!(deps.plugins[0].id, "kotlin-jvm");
    assert_eq!(deps.plugins[1].id, "kotlin-plugin.serialization");
}

#[test]
fn scan_plugins_no_plugins() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        "dependencies { implementation(\"com.example:lib:1.0\") }\n",
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert!(deps.plugins.is_empty());
}
// ── cache module ─────────────────────────────────────────────────────────

#[test]
fn config_detection() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"dependencies {
    implementation("com.example:app:1.0")
    api("com.example:api:2.0")
    testImplementation("org.test:junit:4.13")
    kapt("com.google.dagger:dagger-compiler:2.50")
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.external.len(), 4);
    let impl_dep = deps
        .external
        .iter()
        .find(|d| d.artifact == "app")
        .expect("app dep");
    assert_eq!(impl_dep.config, "implementation");
    let api_dep = deps
        .external
        .iter()
        .find(|d| d.artifact == "api")
        .expect("api dep");
    assert_eq!(api_dep.config, "api");
    let test_dep = deps
        .external
        .iter()
        .find(|d| d.artifact == "junit")
        .expect("junit dep");
    assert_eq!(test_dep.config, "testImplementation");
    let kapt_dep = deps
        .external
        .iter()
        .find(|d| d.artifact == "dagger-compiler")
        .expect("kapt dep");
    assert_eq!(kapt_dep.config, "kapt");
}

#[test]
fn config_detection_with_catalog() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("libs.versions.toml"),
        r#"[versions]
compose-bom = "2024.01.00"
[libraries]
compose-ui = { module = "androidx.compose.ui:ui", version.ref = "compose-bom" }
compose-material = { module = "androidx.compose.material:material", version.ref = "compose-bom" }
"#,
    )
    .expect("write toml");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"dependencies {
    implementation(libs.compose.ui)
    testImplementation(libs.compose.material)
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert!(deps.external.len() >= 2);
    let ui_dep = deps
        .external
        .iter()
        .find(|d| d.alias == "libs.compose.ui")
        .expect("ui dep");
    assert_eq!(ui_dep.config, "implementation");
    let mat_dep = deps
        .external
        .iter()
        .find(|d| d.alias == "libs.compose.material")
        .expect("mat dep");
    assert_eq!(mat_dep.config, "testImplementation");
}

#[test]
fn android_block_basic() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"android {
    namespace = "com.example.app"
    compileSdk = 34
    defaultConfig {
        applicationId = "com.example.app"
        minSdk = 24
        targetSdk = 34
    }
}"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert_eq!(deps.android.namespace.as_deref(), Some("com.example.app"));
    assert_eq!(deps.android.compile_sdk, Some(34));
    assert_eq!(
        deps.android.application_id.as_deref(),
        Some("com.example.app")
    );
    assert_eq!(deps.android.min_sdk, Some(24));
    assert_eq!(deps.android.target_sdk, Some(34));
}

#[test]
fn android_block_no_android() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("build.gradle.kts"),
        r#"plugins { id("org.jetbrains.kotlin.jvm") }"#,
    )
    .expect("write build");

    let deps = super::parse_project(dir.path()).expect("should parse");
    assert!(deps.android.namespace.is_none());
    assert!(deps.android.compile_sdk.is_none());
}
#[test]
fn find_source_jar_home_dir_access() {
    let result = super::cache::find_source_jar("com.example", "test", "1.0.0");
    let _ = result;
}
