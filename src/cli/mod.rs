//! Standalone CLI interface for kotlin-lsp.

mod android;
mod args;
mod batch;
mod call_graph;
mod check;
mod complete;
mod doctor;
mod edit;
mod expect_actual;
mod extract_sources;
mod find_test;
mod format;
mod hover;
mod impact;
mod inject;
mod insert;
mod modules;
mod organize_imports;
mod output;
mod path_meta;
mod ref_kind;
mod run;
mod skills;
mod sources;
mod summarize;
pub(crate) mod templates;
mod tokens;
mod workspace;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod format_tests;
pub(crate) use args::CliArgs;
pub(crate) use run::run;
