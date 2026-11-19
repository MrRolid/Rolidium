// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Smoke test of the real application window: runs `rolidium --selftest` in a virtual X server.
//! The self test drives the main tools (select, Pull with preview and numeric input, undo/redo,
//! failing operation, STL export) and exits non-zero on any failed check.

use std::process::Command;

#[test]
fn selftest_in_virtual_display() {
    let have_xvfb = Command::new("xvfb-run").arg("--help").output().is_ok();
    assert!(
        have_xvfb,
        "xvfb-run is not installed, so the GUI smoke test cannot run (install the xvfb package); \
         this test is deliberately not skipped"
    );
    let dir = std::env::temp_dir().join(format!("rolidium-gui-smoke-{}", std::process::id()));
    let out = Command::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1280x800x24"])
        .arg(env!("CARGO_BIN_EXE_rolidium"))
        .arg("--selftest")
        .arg(&dir)
        .current_dir(std::env::temp_dir())
        .output()
        .expect("cannot start xvfb-run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "selftest failed:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("0 failure(s)"), "{text}");
    assert!(dir.join("2-pull-preview.png").exists());
}
