// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Did the program end properly last time? A marker file (`session.running` in the state folder) is created and locked at the start and
//! removed at a proper end. A marker that exists and is not locked belongs to a program that was killed or crashed: the next start offers to send
//! diagnostics (the person decides; nothing is sent by itself).

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Session {
    file: File,
    path: PathBuf,
}

static CURRENT: Mutex<Option<Session>> = Mutex::new(None);

impl Session {
    /// Starts a session in `dir`; the second value is true when the session before this one did not end properly. `None` when another
    /// running program holds the marker (it is left alone) or the marker cannot be written.
    pub fn begin_in(dir: &Path) -> Option<(Session, bool)> {
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join("session.running");
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .ok()?;
        file.try_lock().ok()?;
        let mut old = String::new();
        let _ = file.read_to_string(&mut old);
        let crashed = !old.trim().is_empty();
        file.set_len(0).ok()?;
        file.rewind().ok()?;
        let _ = write!(file, "{} {}", std::process::id(), env!("CARGO_PKG_VERSION"));
        let _ = file.flush();
        Some((Session { file, path }, crashed))
    }

    /// The proper end: the marker is removed.
    pub fn finish(self) {
        let path = self.path.clone();
        drop(self.file);
        let _ = std::fs::remove_file(path);
    }
}

/// Starts the session of this program; true when the earlier one ended badly.
pub fn begin() -> bool {
    let Some(dir) = crate::logging::state_dir() else {
        return false;
    };
    match Session::begin_in(&dir) {
        Some((s, crashed)) => {
            *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
            crashed
        }
        None => false,
    }
}

/// The proper end of this program.
pub fn end() {
    if let Some(s) = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_left_behind_means_the_last_session_ended_badly() {
        let dir = std::env::temp_dir().join(format!("rolidium-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (s1, crashed) = Session::begin_in(&dir).unwrap();
        assert!(!crashed, "the first start");
        // a second program while the first runs leaves the marker alone
        assert!(Session::begin_in(&dir).is_none());
        // the first one is killed: the file stays, the lock goes
        drop(s1);
        let (s2, crashed) = Session::begin_in(&dir).unwrap();
        assert!(crashed, "the marker of a program that did not finish");
        s2.finish();
        let (s3, crashed) = Session::begin_in(&dir).unwrap();
        assert!(!crashed, "after a proper end");
        s3.finish();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
