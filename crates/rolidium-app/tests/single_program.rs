// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The package has one program with three modes. The MCP mode must keep the standard output clean (only protocol messages), also
//! when the program is started under the name `rolidium-mcp`, and `mcp-config` must print commands with the real absolute path.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_rolidium");

fn talk(mut cmd: Command) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    for (id, m) in [
        (
            1,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        ),
        (2, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#),
    ] {
        writeln!(stdin, "{m}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        // the very first line of the output is already an answer: nothing was printed before it
        let v: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
        assert_eq!(v["id"], id);
        assert!(v["result"].is_object());
    }
    drop(stdin);
    let _ = child.wait();
}

#[test]
fn the_mcp_mode_of_the_single_program_speaks_only_the_protocol() {
    let mut c = Command::new(EXE);
    c.args(["mcp", "--root"]).arg(std::env::temp_dir());
    talk(c);
}

#[cfg(unix)]
#[test]
fn a_link_named_rolidium_mcp_starts_the_server() {
    let dir = std::env::temp_dir().join(format!("rolidium-link-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join("rolidium-mcp");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(EXE, &link).unwrap();
    let mut c = Command::new(&link);
    c.arg("--root").arg(&dir);
    talk(c);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mcp_config_prints_ready_made_commands_with_the_real_path() {
    let out = Command::new(EXE)
        .args(["mcp-config", "--root", "/some/models"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let exe = std::fs::canonicalize(EXE).unwrap();
    let exe = exe.display().to_string();
    assert!(
        text.contains(&format!(
            "claude mcp add rolidium -- {exe} mcp --root /some/models"
        )),
        "{text}"
    );
    // the JSON block is valid and names the same program
    let start = text.find('{').unwrap();
    let end = text.rfind('}').unwrap();
    let v: serde_json::Value = serde_json::from_str(&text[start..=end]).unwrap();
    assert_eq!(v["mcpServers"]["rolidium"]["command"], exe);
    assert_eq!(
        v["mcpServers"]["rolidium"]["args"],
        serde_json::json!(["mcp", "--root", "/some/models"])
    );
}

#[test]
fn the_cli_mode_runs_a_command() {
    let out = Command::new(EXE)
        .args(["cli", "exec", "box 10 10 10", "info"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1000"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The diagnostics examine every graphics backend in a child process. When the child of one backend dies (a driver crash is
/// simulated here), that is written into the report as a result and the other backends are still examined.
#[test]
fn diagnostics_survive_a_crashing_backend() {
    let dir = std::env::temp_dir().join(format!("rolidium-diag-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("d.zip");
    let status = Command::new(EXE)
        .args(["--diagnose", zip_path.to_str().unwrap()])
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("XDG_CONFIG_HOME", dir.join("cfg"))
        .env("ROLIDIUM_TEST_CRASH_DIAG", "vulkan")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("starts");
    assert!(status.success(), "the diagnostics must not die with the child");
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut report = String::new();
    {
        use std::io::Read;
        z.by_name("report.txt")
            .unwrap()
            .read_to_string(&mut report)
            .unwrap();
    }
    assert!(report.contains("backend vulkan: CRASHED"), "{report}");
    assert!(
        report.contains("backend gl: finished"),
        "the next backend was still examined: {report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
