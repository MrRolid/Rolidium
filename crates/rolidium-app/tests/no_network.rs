// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The network code is reachable from two places only, both in the window: the question about a new version (after the person said yes) and
//! the sending of diagnostics (after the person pressed Send). `rolidium cli`, `rolidium mcp`, `--selftest`, `--bench` and `--diagnose` have no way to it.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rs_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn the_cli_and_the_mcp_server_do_not_depend_on_the_network_code() {
    for krate in [
        "rolidium-cli",
        "rolidium-kernel",
        "rolidium-doc",
        "rolidium-render",
    ] {
        let toml = std::fs::read_to_string(root().join("crates").join(krate).join("Cargo.toml")).unwrap();
        assert!(
            !toml.contains("rolidium-net") && !toml.contains("ureq"),
            "{krate} reaches the network code"
        );
    }
}

#[test]
fn only_two_places_of_the_window_call_the_network() {
    let mut files = Vec::new();
    rs_files(&root().join("crates/rolidium-app/src"), &mut files);
    let mut callers = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let text = text.split("#[cfg(test)]").next().unwrap_or(&text).to_string();
        if text.contains("fetch_latest(") || text.contains("send_report(") {
            callers.push(f.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    callers.sort();
    assert_eq!(callers, ["feedback.rs", "updates.rs"]);
    // the question is asked from one function that checks the consent, ROLIDIUM_OFFLINE and the mode of the program
    let updates = std::fs::read_to_string(root().join("crates/rolidium-app/src/app/updates.rs")).unwrap();
    let call = updates.find("rolidium_net::fetch_latest(").unwrap();
    let guard = updates.find("if !question_due(").unwrap();
    assert!(guard < call, "the question must come after the check");
    // and the main program calls it only from the window, never from --diagnose, --selftest or --bench
    let main = std::fs::read_to_string(root().join("crates/rolidium-app/src/main.rs")).unwrap();
    assert!(!main.contains("fetch_latest") && !main.contains("send_report"));
}
