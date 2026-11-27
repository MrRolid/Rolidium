// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! What a long operation is doing now, for the window of the progress: a sentence (an English key that the window translates) and how far it is (0 to 1).
//! One operation runs at a time, so one slot is enough.

use std::sync::Mutex;

static NOW: Mutex<Option<(String, f32)>> = Mutex::new(None);

/// Says what the running operation is doing.
pub fn set(stage: &str, fraction: f32) {
    if let Ok(mut g) = NOW.lock() {
        *g = Some((stage.to_string(), fraction.clamp(0.0, 1.0)));
    }
}

/// The sentence and the fraction of the running operation; `None` when it said nothing.
pub fn get() -> Option<(String, f32)> {
    NOW.lock().ok().and_then(|g| g.clone())
}

/// The operation is over.
pub fn clear() {
    if let Ok(mut g) = NOW.lock() {
        *g = None;
    }
}
