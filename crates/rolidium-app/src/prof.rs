//! A small profiler of the frame: the parts of the frame function are timed by name (`prof!("name", expr)` or `let _g = prof::scope("name")`),
//! summed per frame and smoothed over frames. The three dearest parts are shown in the F12 panel, all of them in the log
//! (`ROLIDIUM_FRAME_LOG`). It costs two clock reads per part, so it is always on.
use std::cell::RefCell;
use std::time::Instant;

#[derive(Default)]
struct Book {
    /// this frame: name, milliseconds (the same name is summed)
    now: Vec<(&'static str, f32)>,
    /// smoothed over frames
    smooth: Vec<(&'static str, f32)>,
    worst: Option<(String, f32)>,
}

thread_local! {
    static BOOK: RefCell<Book> = RefCell::new(Book::default());
}

pub struct Guard(&'static str, Instant);

pub fn scope(name: &'static str) -> Guard {
    Guard(name, Instant::now())
}

impl Drop for Guard {
    fn drop(&mut self) {
        add(self.0, self.1.elapsed().as_secs_f32() * 1000.0);
    }
}

pub fn add(name: &'static str, ms: f32) {
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        match b.now.iter_mut().find(|(n, _)| *n == name) {
            Some(e) => e.1 += ms,
            None => b.now.push((name, ms)),
        }
    });
}

/// Times an expression under a name.
macro_rules! prof {
    ($name:expr, $e:expr) => {{
        let _g = $crate::prof::scope($name);
        $e
    }};
}

/// At the start of a frame: the parts of the frame before are smoothed in and the book is emptied.
pub fn next_frame() {
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        let now = std::mem::take(&mut b.now);
        for (n, ms) in &now {
            match b.smooth.iter_mut().find(|(s, _)| s == n) {
                Some(e) => e.1 = e.1 * 0.9 + ms * 0.1,
                None => b.smooth.push((n, *ms * 0.1)),
            }
        }
        // a part that did not run this frame fades
        for e in b.smooth.iter_mut() {
            if !now.iter().any(|(n, _)| *n == e.0) {
                e.1 *= 0.9;
            }
        }
    });
}

/// The parts by cost, dearest first.
pub fn ranked() -> Vec<(&'static str, f32)> {
    BOOK.with(|b| {
        let mut v = b.borrow().smooth.clone();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    })
}

pub fn top3_text() -> String {
    ranked()
        .iter()
        .take(3)
        .map(|(n, ms)| format!("{n} {ms:.1}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn all_text() -> String {
    ranked()
        .iter()
        .filter(|(_, ms)| *ms >= 0.05)
        .map(|(n, ms)| format!("{n} {ms:.2}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The native calls the window thread made since the last frame are entered as parts of the frame (the time waiting for the lock, which a
/// calculation on another thread holds, and the time running), with the dearest call named.
pub fn take_kernel() {
    let (calls, wait, run, worst_ms, worst) = rolidium_kernel::take_thread_stats();
    if calls == 0 {
        return;
    }
    add("kernel calls: waiting for the lock", wait as f32);
    add("kernel calls: running", (run - 0.0) as f32);
    if worst_ms > 1.0 {
        BOOK.with(|b| b.borrow_mut().worst = Some((worst, worst_ms as f32)));
    }
}

pub fn worst_kernel_call() -> Option<(String, f32)> {
    BOOK.with(|b| b.borrow().worst.clone())
}
