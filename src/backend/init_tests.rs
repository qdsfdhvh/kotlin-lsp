use super::Backend;
use tower_lsp::lsp_types::{InitializeParams, Url, WorkspaceFolder};

#[test]
fn client_workspace_root_is_not_widened_to_git_ancestor_issue_325() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let git_root = temp.path().join("repository");
    let client_root = git_root.join("nested-project");
    std::fs::create_dir_all(git_root.join(".git")).expect("create git marker");
    std::fs::create_dir_all(&client_root).expect("create client workspace");

    // `root_uri` is deprecated upstream but is exactly what Neovim's
    // kotlin-lsp config sends (issue #325) — the point of this test is
    // exercising client rootUri handling, so the deprecation is deliberate.
    #[allow(deprecated)]
    let params = InitializeParams {
        root_uri: Some(Url::from_directory_path(&client_root).expect("client root URI")),
        ..InitializeParams::default()
    };

    // Assert the client-root function directly: resolve_workspace_root also
    // consults KOTLIN_LSP_WORKSPACE_ROOT / config overrides, which would make
    // this test environment-dependent.
    assert_eq!(
        Backend::workspace_root_from_client(&params),
        Some(client_root)
    );
}

#[test]
fn workspace_folder_root_is_not_widened_to_git_ancestor_issue_325() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let git_root = temp.path().join("repository");
    let folder_root = git_root.join("nested-project");
    std::fs::create_dir_all(git_root.join(".git")).expect("create git marker");
    std::fs::create_dir_all(&folder_root).expect("create workspace folder");

    let params = InitializeParams {
        root_uri: None,
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: Url::from_directory_path(&folder_root).expect("folder URI"),
            name: "nested-project".to_string(),
        }]),
        ..InitializeParams::default()
    };

    assert_eq!(
        Backend::workspace_root_from_client(&params),
        Some(folder_root)
    );
}
