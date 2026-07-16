//! Symbol summarization — `summarize <name>` returns structured info about a symbol.
//!
//! Unlike `find` (which returns locations), `summarize` returns signature, members,
//! KDoc, and dependencies so agents can decide next steps without reading source.


use serde::Serialize;

#[derive(Debug, Serialize)]
struct SymbolSummary {
    name: String,
    kind: String,
    visibility: String,
    modifiers: Vec<String>,
    signature: Option<String>,
    members: Vec<MemberSummary>,
    doc: Option<String>,
    dependencies: Vec<String>,
    file: String,
    line: u32,
    col: u32,
}

#[derive(Debug, Serialize)]
struct MemberSummary {
    name: String,
    kind: String,
    signature: Option<String>,
}

pub(crate) async fn run_summarize(name: &str, _expand: bool, json: bool) {
    let root = crate::cli::run::resolve_root_for_file(None, &std::path::PathBuf::from("."));
    let index = crate::cli::run::build_index(&root, false).await;
    let engine = crate::query::engine::WorkspaceQueryEngine::new(index);

    match engine.build_summary(name) {
        Some(summary) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&summary).expect("json"));
            } else {
                println!("{} ({})", summary.name, summary.kind);
                if let Some(ref pkg) = summary.package {
                    println!("  package: {pkg}");
                }
                println!("  file: {}:{}", summary.file, summary.line);
                if let Some(ref sig) = summary.signature {
                    println!("  signature: {sig}");
                }
                if let Some(ref doc) = summary.doc {
                    println!("  doc: {doc}");
                }
                if !summary.supertypes.is_empty() {
                    println!("  supertypes: {}", summary.supertypes.join(", "));
                }
                if !summary.callers.is_empty() {
                    println!("  callers ({}): {}", summary.callers.len(), summary.callers.join(", "));
                }
                if !summary.callees.is_empty() {
                    println!("  callees ({}): {}", summary.callees.len(), summary.callees.join(", "));
                }
            }
        }
        None => {
            eprintln!("Symbol not found: {name}");
            std::process::exit(1);
        }
    }
}
