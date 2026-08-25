use super::Backend;
use tower_lsp::lsp_types::{InitializeParams, Url};

#[test]
fn client_workspace_root_is_not_widened_to_git_ancestor_issue_325() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let git_root = temp.path().join("repository");
    let client_root = git_root.join("nested-project");
    std::fs::create_dir_all(git_root.join(".git")).expect("create git marker");
    std::fs::create_dir_all(&client_root).expect("create client workspace");

    #[allow(deprecated)]
    let params = InitializeParams {
        root_uri: Some(Url::from_directory_path(&client_root).expect("client root URI")),
        ..InitializeParams::default()
    };

    assert_eq!(Backend::resolve_workspace_root(&params), Some(client_root));
}
