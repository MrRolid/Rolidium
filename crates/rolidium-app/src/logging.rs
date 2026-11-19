// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal file logger in the user's state directory, plus a panic hook that writes there.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn state_dir() -> Option<PathBuf> {
    crate::dirs::state_base().map(|b| b.join("rolidium"))
}

pub fn init() {
    let Some(dir) = state_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("rolidium.log");
    // Keep the log from growing without bound.
    if std::fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, dir.join("rolidium.log.old"));
    }
    let _ = LOG_PATH.set(path);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("PANIC: {info}"));
        default_hook(info);
    }));
}

/// The text of the log (the current file), for the diagnostics.
pub fn read_log() -> String {
    LOG_PATH
        .get()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

pub fn log(msg: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

/// Like `log`, but the line is forced to the disk before the function returns. Used for the steps of the graphics start-up,
/// so that after a crash inside a driver the last step is in the file.
pub fn log_sync(msg: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {msg}");
        let _ = f.sync_all();
    }
}

/// The path of the log file (for messages to the user).
pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}
