// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A guard of the memory of a calculation. While the program computes (an operation, a mesh, an export) a thread looks at the memory the process
//! uses; when it passes the limit (a share of the memory of the computer, or `ROLIDIUM_MEMORY_LIMIT_MB`) or the computer has almost none left, the
//! calculation is cancelled like by the Cancel button, so the operating system does not have to kill the program. The model stays as it was.
//! On Linux; elsewhere there is no guard (the other limits of the sizes still apply).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

static WATCH: AtomicBool = AtomicBool::new(false);
static STOPPED: AtomicBool = AtomicBool::new(false);
static LIMIT_MB: AtomicU64 = AtomicU64::new(0);

/// The program is computing (true) or idle (false).
pub fn set_computing(on: bool) {
    WATCH.store(on, Relaxed);
}

/// True once if the guard stopped a calculation (and clears the mark).
pub fn take_stopped() -> bool {
    STOPPED.swap(false, Relaxed)
}

/// The limit in use (MB), 0 when there is none.
pub fn limit_mb() -> u64 {
    LIMIT_MB.load(Relaxed)
}

/// The resident memory of this process (MB) and the memory that is available (MB), when they are known.
#[cfg(target_os = "linux")]
pub fn usage_mb() -> Option<(u64, u64)> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let rss = pages * 4096 / (1024 * 1024);
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb = |key: &str| -> Option<u64> {
        info.lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    Some((rss, kb("MemAvailable:")? / 1024))
}

#[cfg(not(target_os = "linux"))]
pub fn usage_mb() -> Option<(u64, u64)> {
    None
}

#[cfg(target_os = "linux")]
fn default_limit_mb() -> u64 {
    let total = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        })
        .unwrap_or(0)
        / 1024;
    (total * 6 / 10).max(1024)
}

#[cfg(not(target_os = "linux"))]
fn default_limit_mb() -> u64 {
    0
}

/// Whether a process that uses `rss` MB with `available` MB free on the computer has to be stopped.
pub fn over_limit(rss: u64, available: u64, limit: u64) -> bool {
    (limit > 0 && rss > limit) || available < 300
}

/// Starts the thread of the guard (once).
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let limit = std::env::var("ROLIDIUM_MEMORY_LIMIT_MB")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or_else(default_limit_mb);
        LIMIT_MB.store(limit, Relaxed);
        if usage_mb().is_none() {
            return;
        }
        let _ = std::thread::Builder::new().name("memory guard".into()).spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(150));
            if !WATCH.load(Relaxed) {
                continue;
            }
            if let Some((rss, available)) = usage_mb() {
                if over_limit(rss, available, limit) && !STOPPED.load(Relaxed) {
                    crate::logging::log(&format!(
                        "memory guard: {rss} MB used (limit {limit} MB), {available} MB available: the calculation is cancelled"
                    ));
                    STOPPED.store(true, Relaxed);
                    rolidium_kernel::request_cancel();
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_and_the_free_memory_decide() {
        assert!(!over_limit(500, 8000, 4000));
        assert!(over_limit(4500, 8000, 4000));
        assert!(over_limit(500, 200, 4000), "the computer has almost no memory left");
        assert!(!over_limit(5000, 8000, 0), "no limit");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_memory_of_this_process_is_known() {
        let (rss, available) = usage_mb().expect("/proc is there");
        assert!(rss > 0 && available > 0);
    }
}
