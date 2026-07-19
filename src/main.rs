#![warn(unreachable_pub)]
mod backend;
mod cli;
mod gradle;
mod indexer;
mod inlay_hints;
mod lines_ext;
mod parser;
mod path_util;
mod queries;
mod query;
mod resolver;
mod rg;
mod semantic_tokens;
mod stdlib;
mod stdlib_tail;
mod str_ext;
mod swift_stdlib;
mod task_runner;
mod types;
mod workspace_json;

pub(crate) use lines_ext::LinesExt;
pub(crate) use str_ext::StrExt;
pub(crate) use types::Language;

use tower_lsp::{LspService, Server};

fn main() {
    // Build custom tokio runtime with larger blocking pool
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(512)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main());
}

async fn async_main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr) // keep stdout clean for LSP JSON-RPC
        .init();

    // CLI subcommands: find, refs, hover, index
    // kotlin-lsp uninstall — clean up everything
    if std::env::args().any(|a| a == "uninstall") {
        println!("kotlin-lsp uninstall");
        println!();
        println!("This will remove:");
        println!("  • Library sources (~/.kotlin-lsp/)");
        println!("  • Global cache (~/.cache/kotlin-lsp/)");
        println!("  • Current project cache (.cache/kotlin-lsp/)");
        println!();
        print!("Continue? [y/N] ");
        use std::io::Write;
        std::io::stdout().flush().ok();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        if input.trim().to_lowercase() != "y" {
            println!("Cancelled.");
            return;
        }
        println!("Removing...");
        if let Ok(home) = std::env::var("HOME") {
            let lib = std::path::PathBuf::from(&home)
                .join(".cache")
                .join("kotlin-lsp");
            if lib.exists() {
                std::fs::remove_dir_all(&lib).ok();
                println!("  ✅ {}", lib.display());
            }
            let cache = std::path::PathBuf::from(&home)
                .join(".cache")
                .join("kotlin-lsp");
            if cache.exists() {
                std::fs::remove_dir_all(&cache).ok();
                println!("  ✅ {}", cache.display());
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            let pc = cwd.join(".cache").join("kotlin-lsp");
            if pc.exists() {
                std::fs::remove_dir_all(&pc).ok();
                println!("  ✅ {}", pc.display());
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            println!("  Binary: {}", exe.display());
            println!("  To remove the binary: rm '{}'", exe.display());
        }
        println!("Done.");
        return;
    }

    match cli::CliArgs::parse() {
        Ok(Some(args)) => {
            cli::run(args).await;
            return;
        }
        Ok(None) => {} // LSP mode
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("Usage: kotlin-lsp [find|refs|hover|index] [--fast|--smart] [--json] [--root <dir>]");
            std::process::exit(1);
        }
    }

    let lsp_flags = parse_lsp_flags();
    let format_tool = lsp_flags.format_tool;
    let agent_mode = lsp_flags.agent_mode;
    let args: Vec<String> = lsp_flags.remaining;

    // --index-only <path>  — build cache and exit
    if args.first().map(|s| s == "--index-only").unwrap_or(false) {
        let path = args.get(1).cloned().unwrap_or_else(|| {
            eprintln!("Usage: kotlin-lsp --index-only <path>");
            std::process::exit(1);
        });
        let pb = std::path::PathBuf::from(path);
        if !pb.is_dir() {
            eprintln!("Path is not a directory: {}", pb.display());
            std::process::exit(1);
        }
        let idx = std::sync::Arc::new(indexer::Indexer::new());
        let root = pb.canonicalize().unwrap_or(pb);
        println!("Indexing workspace: {}", root.display());
        std::sync::Arc::clone(&idx)
            .index_workspace_full(&root, std::sync::Arc::new(indexer::NoopReporter))
            .await;
        println!(
            "Indexing complete: {} files, {} symbols",
            idx.files.len(),
            idx.definitions.len()
        );
        std::process::exit(0);
    }

    // --port <N>  — serve a single LSP client over TCP (useful for Android / Sora Editor)

    if args.first().map(|s| s == "--port").unwrap_or(false) {
        let port: u16 = args
            .get(1)
            .unwrap_or_else(|| {
                eprintln!("Usage: kotlin-lsp --port <port>");
                std::process::exit(1);
            })
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("Invalid port number");
                std::process::exit(1);
            });

        let addr = format!("127.0.0.1:{port}");
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Failed to bind {addr}: {e}");
                std::process::exit(1);
            });
        eprintln!("kotlin-lsp listening on {addr} (TCP, loopback only)");

        // Serve one client at a time; restart the loop for subsequent connections.
        loop {
            let (stream, peer) = listener.accept().await.unwrap_or_else(|e| {
                eprintln!("Accept error: {e}");
                std::process::exit(1);
            });
            eprintln!("Client connected: {peer}");
            let (reader, writer) = tokio::io::split(stream);
            let ft = format_tool.clone();
            let (service, socket) = LspService::new(move |client| {
                let mut backend = backend::Backend::new(client);
                if let Some(tool) = ft.clone() {
                    backend = backend.with_format_tool(tool);
                    if agent_mode {
                        backend = backend.with_agent_mode();
                    }
                }
                backend
            });
            Server::new(reader, writer, socket).serve(service).await;
            eprintln!("Client disconnected, waiting for next connection…");
        }
    }

    // Default: stdio transport
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let ft = format_tool.clone();
    let (service, socket) = LspService::new(move |client| {
        let mut backend = backend::Backend::new(client);
        if let Some(tool) = ft.clone() {
            backend = backend.with_format_tool(tool);
            if agent_mode {
                backend = backend.with_agent_mode();
            }
        }
        backend
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// LSP-server-level CLI flags, parsed before subcommands.
struct LspCliFlags {
    format_tool: Option<String>,
    agent_mode: bool,
    /// Remaining args after stripping recognised flags (kept in order).
    remaining: Vec<String>,
}

/// Extract LSP-level flags from `std::env::args()`.
/// Recognised flags (`--format-tool`, `--port`, `--index-only` + their values)
/// are consumed; everything else is returned in `remaining`.
///
/// This ensures `--format-tool ktlint --port 1234` and
/// `--port 1234 --format-tool ktlint` both work.
fn parse_lsp_flags() -> LspCliFlags {
    let mut format_tool: Option<String> = None;
    let mut agent_mode = false;
    let mut remaining: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1).peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--agent" => {
                agent_mode = true;
            }
            "--format-tool" => {
                let tool = args.next().unwrap_or_else(|| {
                    eprintln!("Usage: kotlin-lsp --format-tool <ktlint|ktfmt>");
                    std::process::exit(1);
                });
                if tool != "ktlint" && tool != "ktfmt" {
                    eprintln!("error: unknown format tool '{tool}'; expected 'ktlint' or 'ktfmt'");
                    std::process::exit(1);
                }
                format_tool = Some(tool);
            }
            // --port and --index-only need their values too
            "--port" | "--index-only" => {
                remaining.push(arg.clone());
                if let Some(val) = args.next() {
                    remaining.push(val);
                } else {
                    eprintln!("Usage: kotlin-lsp {arg} <value>");
                    std::process::exit(1);
                }
            }
            _ => {
                remaining.push(arg.clone());
            }
        }
    }

    LspCliFlags {
        format_tool,
        agent_mode,
        remaining,
    }
}
// docs only
