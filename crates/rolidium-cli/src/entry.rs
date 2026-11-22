// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The entry points shared by the separate small programs and by the single `rolidium` program.

use crate::Runner;

const CLI_HELP: &str = "rolidium-cli - headless Rolidium

USAGE:
    rolidium-cli run <scenario.txt>     run a scenario file
    rolidium-cli exec \"<command>\" ...   run commands given on the command line
    ... --file model.rolidium           (run/exec) open the file first if it exists and save it afterwards
    rolidium-cli bench                  measure the performance targets
    rolidium-cli times [machine|open]   table of the times of the main operations, with their stages (machine: only the small operations in the big document)
    rolidium-cli --version
    rolidium-cli help

Scenario commands: box, pull, round, chamfer, undo, redo, info, save, open, import-step, export-stl, export-step, render.
See the documentation of the rolidium-cli crate for details.";

/// The command-line tool (`rolidium-cli`, or `rolidium cli`). `args` are the arguments after the program name; returns the exit code.
pub fn cli(mut args: Vec<String>) -> i32 {
    // the number of threads: one per core less one, or ROLIDIUM_THREADS
    rolidium_kernel::par::configure(0);
    // `--file <path>`: work on a document file
    let file = args.iter().position(|a| a == "--file").and_then(|i| {
        if i + 1 < args.len() {
            let p = args.remove(i + 1);
            args.remove(i);
            Some(std::path::PathBuf::from(p))
        } else {
            None
        }
    });
    let code = match args.first().map(String::as_str) {
        Some("run") if args.len() == 2 => match std::fs::read_to_string(&args[1]) {
            Ok(text) => report(work(file.as_deref(), &text)),
            Err(e) => {
                eprintln!("cannot read {}: {e}", args[1]);
                2
            }
        },
        Some("exec") if args.len() >= 2 => report(work(file.as_deref(), &args[1..].join("\n"))),
        Some("--version") | Some("-V") => {
            println!(
                "rolidium-cli {} {}",
                env!("ROLIDIUM_VERSION_LABEL"),
                env!("ROLIDIUM_BUILD_HASH")
            );
            0
        }
        Some("bench") => crate::bench::run(),
        Some("times") => crate::bench::times(args.get(1).map(String::as_str)),
        Some("help") | Some("--help") | Some("-h") | None => {
            println!("{CLI_HELP}");
            0
        }
        Some(other) => {
            eprintln!("unknown command '{other}'\n\n{CLI_HELP}");
            2
        }
    };
    code
}

fn report(r: Result<(), crate::CliError>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// Runs a script, optionally on top of a document file that is saved afterwards.
fn work(file: Option<&std::path::Path>, script: &str) -> Result<(), crate::CliError> {
    let mut r = Runner::new();
    if let Some(f) = file {
        if f.exists() {
            r.open_file(f).map_err(crate::CliError::Other)?;
        } else {
            r.file = Some(f.to_path_buf());
        }
    }
    r.run_script(script)?;
    if let Some(f) = file {
        r.run_line(&format!("save {}", f.display()))
            .map_err(crate::CliError::Other)?;
    }
    Ok(())
}

use crate::mcp::Server;
use std::time::Duration;

const MCP_HELP: &str = "rolidium-mcp - Rolidium as an MCP server (Model Context Protocol over stdin/stdout)

USAGE:
    rolidium-mcp --root <directory> [--timeout <seconds>]
    rolidium-mcp --version
    rolidium-mcp --list-tools     the tools as rows of a Markdown table

OPTIONS:
    --root <directory>     the only directory files are read from and written to (without it file tools are refused)
    --timeout <seconds>    time limit of one operation (default 120); a slower one is cancelled and the model stays unchanged

The server keeps one document in memory for as long as it runs. It uses no network and runs no commands.
See docs/MCP.md.";

/// The MCP server (`rolidium-mcp`, or `rolidium mcp`). Returns the exit code.
pub fn mcp(mut args: Vec<String>) -> i32 {
    rolidium_kernel::par::configure(0);
    let mut take = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        if i + 1 < args.len() {
            let v = args.remove(i + 1);
            args.remove(i);
            Some(v)
        } else {
            None
        }
    };
    let root = take("--root");
    let timeout = take("--timeout");
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "rolidium-mcp {} {}",
            env!("ROLIDIUM_VERSION_LABEL"),
            env!("ROLIDIUM_BUILD_HASH")
        );
        return 0;
    }
    if args.iter().any(|a| a == "--list-tools") {
        for c in crate::spec::COMMANDS {
            println!(
                "| `{}` | {} |",
                c.tool,
                c.summary
                    .split(". ")
                    .next()
                    .unwrap_or(c.summary)
                    .trim_end_matches('.')
            );
        }
        return 0;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") || !args.is_empty() {
        println!("{MCP_HELP}");
        return if args.is_empty() { 0 } else { 2 };
    }
    let secs: f64 = match timeout.as_deref().map(str::parse::<f64>) {
        None => 120.0,
        Some(Ok(s)) if s > 0.0 && s <= 86400.0 => s,
        Some(_) => {
            eprintln!("--timeout needs a number of seconds between 0 and 86400");
            return 2;
        }
    };
    let mut server = match Server::new(
        root.as_deref().map(std::path::Path::new),
        Duration::from_secs_f64(secs),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rolidium-mcp: {e}");
            return 2;
        }
    };
    if root.is_none() {
        eprintln!("rolidium-mcp: no --root given, file tools are disabled");
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if let Err(e) = server.serve(stdin.lock(), stdout.lock()) {
        eprintln!("rolidium-mcp: {e}");
        return 1;
    }
    0
}

const MCP_CONFIG_HELP: &str = "rolidium mcp-config - prints ready-made commands that connect the Rolidium MCP server to an AI client

USAGE:
    rolidium mcp-config [--root <directory>]

The commands contain the real absolute path of this program, so they can be pasted as they are.
--root is the only directory the server reads files from and writes files to (default: a folder 'rolidium-models' in your home folder).";

fn quote(s: &str) -> String {
    if s.contains(' ') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// `rolidium mcp-config`: the commands to connect the MCP server to a client, with the real path of this program.
pub fn mcp_config(args: Vec<String>) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{MCP_CONFIG_HELP}");
        return 0;
    }
    let root = match args.iter().position(|a| a == "--root") {
        Some(i) => match args.get(i + 1) {
            Some(r) => std::path::PathBuf::from(r),
            None => {
                eprintln!("--root needs a directory\n\n{MCP_CONFIG_HELP}");
                return 2;
            }
        },
        None => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|h| std::path::PathBuf::from(h).join("rolidium-models"))
            .unwrap_or_else(|| std::path::PathBuf::from("rolidium-models")),
    };
    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| std::path::PathBuf::from("rolidium"));
    // canonicalize on Windows gives \\?\C:\...; the plain form is what people expect
    let clean = |p: &std::path::Path| p.display().to_string().trim_start_matches("\\\\?\\").to_string();
    let (exe, root) = (clean(&exe), clean(&root));
    println!("Rolidium MCP server: ready-made connection commands");
    println!("(files are read and written only under {root}; create the folder if it does not exist)\n");
    println!("1. Claude Code (run this once in a terminal):\n");
    println!(
        "   claude mcp add rolidium -- {} mcp --root {}\n",
        quote(&exe),
        quote(&root)
    );
    println!(
        "2. Any client with a JSON configuration (for example Claude Desktop: claude_desktop_config.json):\n"
    );
    let json = serde_json::json!({
        "mcpServers": { "rolidium": { "command": exe, "args": ["mcp", "--root", root] } }
    });
    println!("{}\n", serde_json::to_string_pretty(&json).unwrap_or_default());
    println!("The server needs no network and no graphics card. Add \"--timeout\", \"60\" to the arguments to change the time limit of one operation.");
    println!("Documentation: docs/MCP.md");
    0
}
