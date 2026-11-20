// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Several cores for the work that is the same for many independent bodies (meshes, checks, file reading).
//!
//! The number of threads is one per core less one (at least one) unless the person chose another number in the
//! settings or the environment variable `ROLIDIUM_THREADS` (a number, or `auto`) says so; the variable wins.
//! Bodies are worked on at the same time only when they share no data in the kernel ([`Solid::shared_keys`]); bodies that
//! do share data (for example the faces that an operation left unchanged) are worked on one after the other in one group.

use crate::Solid;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 0: not decided yet / automatic.
static CHOICE: AtomicUsize = AtomicUsize::new(0);
static LAST_NATIVE: AtomicUsize = AtomicUsize::new(0);
static CONFIGURED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The number of threads for a choice (0 = automatic).
pub fn resolve(choice: usize) -> usize {
    if choice > 0 {
        return choice.min(256);
    }
    std::thread::available_parallelism()
        .map_or(1, |n| n.get().saturating_sub(1))
        .max(1)
}

/// The value of `ROLIDIUM_THREADS`: `Some(0)` for `auto`, `Some(n)` for a number, `None` when it is not set or not valid.
pub fn env_choice() -> Option<usize> {
    let v = std::env::var("ROLIDIUM_THREADS").ok()?;
    let v = v.trim();
    if v.eq_ignore_ascii_case("auto") {
        return Some(0);
    }
    v.parse::<usize>().ok().filter(|n| (1..=256).contains(n))
}

/// Sets the number of threads: `choice` is the setting of the person (0 = automatic); the environment variable overrides it.
pub fn configure(choice: usize) {
    let c = env_choice().unwrap_or(choice);
    // the number can be set again and again (every `cli` or `mcp` call does): the kernel hears of it only when it changed
    let n = resolve(c);
    if LAST_NATIVE.swap(n, Ordering::AcqRel) != n {
        crate::set_native_threads(n);
    }
    CHOICE.store(c, Ordering::Relaxed);
    CONFIGURED.store(true, Ordering::Release);
}

/// The number of threads in use. Until [`configure`] was called, the automatic number (or `ROLIDIUM_THREADS`) is in use.
pub fn threads() -> usize {
    if !CONFIGURED.load(Ordering::Acquire) {
        configure(0);
    }
    resolve(CHOICE.load(Ordering::Relaxed))
}

/// `f` over `items` on up to [`threads`] threads, the results in the order of the items. `f` must not touch shared data.
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let groups: Vec<Vec<usize>> = (0..items.len()).map(|i| vec![i]).collect();
    run(items, &groups, &f)
}

/// Like [`map`] for bodies: those that share kernel data with each other are never worked on at the same moment.
pub fn map_groups<T: Sync, R: Send>(
    items: &[T],
    solid: impl Fn(&T) -> &Solid,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    if threads() <= 1 || items.len() < 2 {
        return items.iter().map(&f).collect();
    }
    run(items, &share_groups(items, solid), &f)
}

/// The indices of `items` grouped so that solids sharing a part of the kernel shape are in one group.
pub fn share_groups<T>(items: &[T], solid: impl Fn(&T) -> &Solid) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..items.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut owner: HashMap<u64, usize> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        match solid(it).shared_keys() {
            Ok(keys) => {
                for k in keys {
                    if let Some(&o) = owner.get(&k) {
                        let (a, b) = (find(&mut parent, o), find(&mut parent, i));
                        if a != b {
                            parent[a.max(b)] = a.min(b);
                        }
                    } else {
                        owner.insert(k, i);
                    }
                }
            }
            // unknown: keep it away from everything (it goes into the group of the first item)
            Err(_) => {
                let (a, b) = (find(&mut parent, 0), find(&mut parent, i));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..items.len() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut v: Vec<Vec<usize>> = groups.into_values().collect();
    v.sort_by_key(|g| g[0]);
    v
}

fn run<T: Sync, R: Send>(items: &[T], groups: &[Vec<usize>], f: &(impl Fn(&T) -> R + Sync)) -> Vec<R> {
    let n = threads().min(groups.len());
    if n <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    let parts: Vec<Vec<(usize, R)>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let g = next.fetch_add(1, Ordering::Relaxed);
                        let Some(group) = groups.get(g) else { break };
                        for &i in group {
                            mine.push((i, f(&items[i])));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });
    for part in parts {
        for (i, r) in part {
            results[i] = Some(r);
        }
    }
    results
        .into_iter()
        .map(|r| r.expect("every item was worked on"))
        .collect()
}
