//! Standalone CLI interface for kotlin-lsp.

mod android;
mod args;
mod batch;
mod batch_query;
mod call_graph;
mod check;
mod complete;
mod diagnose;
mod doctor;
mod edit;
mod expect_actual;
mod extract_sources;
mod find_test;
mod format;
mod fuzzy;
mod hover;
mod impact;
mod inheritance;
mod inject;
mod insert;
mod modules;
mod organize_imports;
mod output;
mod path_meta;
mod query_engine;
mod ref_kind;
mod run;
mod skills;
mod snapshot;
mod sources;
mod summarize;
mod symbol_graph;
mod symbol_queries;
pub(crate) mod templates;
mod tokens;
mod workspace;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod format_tests;
pub(crate) use args::CliArgs;
pub(crate) use run::run;
