// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Where the time of an operation goes: the stages are recorded on the calling thread (Open CASCADE stages inside the native
//! code, the Rust ones through [`stage`]). `rolidium-cli times` and the log of the application print them.

use crate::native;
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static RUST_STAGES: RefCell<Vec<(String, f64)>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` and records how long it took under `name`.
pub fn stage<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let ms = t.elapsed().as_secs_f64() * 1e3;
    RUST_STAGES.with(|s| s.borrow_mut().push((name.to_string(), ms)));
    r
}

/// Adds a stage measured by the caller.
pub fn record(name: &str, ms: f64) {
    RUST_STAGES.with(|s| s.borrow_mut().push((name.to_string(), ms)));
}

/// The stages of this thread since the last call, the same name added up, in the order of first use. Clears them.
pub fn take() -> Vec<(String, f64)> {
    let mut all: Vec<(String, f64)> = Vec::new();
    for part in native::take_stages().split(';').filter(|p| !p.is_empty()) {
        if let Some((n, v)) = part.rsplit_once(':') {
            all.push((n.to_string(), v.parse().unwrap_or(0.0)));
        }
    }
    all.extend(RUST_STAGES.with(|s| std::mem::take(&mut *s.borrow_mut())));
    let mut out: Vec<(String, f64)> = Vec::new();
    for (n, v) in all {
        match out.iter_mut().find(|(m, _)| *m == n) {
            Some(e) => e.1 += v,
            None => out.push((n, v)),
        }
    }
    out
}

/// "boolean 12.3 ms, validity check 4.5 ms".
pub fn describe(stages: &[(String, f64)]) -> String {
    stages
        .iter()
        .map(|(n, v)| format!("{n} {v:.1} ms"))
        .collect::<Vec<_>>()
        .join(", ")
}
