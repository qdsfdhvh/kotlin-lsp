//! Resolve Gradle dependency coordinates to source JAR paths.
//!
//! Searches `~/.gradle/caches/modules-2/files-2.1/` for `*-sources.jar`
//! matching a given `group:artifact:version`.

use std::path::PathBuf;

/// Locate the `-sources.jar` for a Maven coordinate in the Gradle cache.
///
/// Pattern: `~/.gradle/caches/modules-2/files-2.1/{group}/{artifact}/{version}/{hash}/{artifact}-{version}-sources.jar`
///
/// Returns `None` if the cache directory doesn't exist or the JAR is not found.
pub(crate) fn find_source_jar(group: &str, artifact: &str, version: &str) -> Option<PathBuf> {
    let gradle_cache = home_gradle_cache()?;
    let version_dir = gradle_cache.join(group).join(artifact).join(version);

    if !version_dir.is_dir() {
        return None;
    }

    // The version directory contains hash subdirectories. Each hash dir may
    // contain the actual JARs. We look for `{artifact}-{version}-sources.jar`.
    let sources_name = format!("{artifact}-{version}-sources.jar");

    if let Ok(entries) = std::fs::read_dir(&version_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let hash_dir = entry.path();
            if hash_dir.is_dir() {
                let candidate = hash_dir.join(&sources_name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn home_gradle_cache() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(
        home.join(".gradle")
            .join("caches")
            .join("modules-2")
            .join("files-2.1"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gradle_cache_path_exists() {
        // Just verify the path construction doesn't panic on macOS/Linux
        let path = home_gradle_cache();
        // Not asserting existence — Gradle might not be installed
        let _ = path;
    }

    #[test]
    fn test_find_nonexistent_jar() {
        let result = find_source_jar("com.nonexistent", "no-such-artifact", "99.99.99");
        assert!(result.is_none());
    }
}
