// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Where settings, autosave and the log live: the XDG folders on Linux, `%APPDATA%` and `%LOCALAPPDATA%` on Windows.
//! Each function returns the base folder; the program adds `rolidium` below it.

use std::path::PathBuf;

fn base(xdg_var: &str, home_relative: &str, windows_var: &str) -> Option<PathBuf> {
    if let Some(x) = std::env::var_os(xdg_var) {
        return Some(PathBuf::from(x));
    }
    if cfg!(windows) {
        if let Some(w) = std::env::var_os(windows_var).or_else(|| std::env::var_os("USERPROFILE")) {
            return Some(PathBuf::from(w));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(home_relative))
}

/// Settings and user language files.
pub fn config_base() -> Option<PathBuf> {
    base("XDG_CONFIG_HOME", ".config", "APPDATA")
}

/// Autosave and other data of the program.
pub fn data_base() -> Option<PathBuf> {
    base("XDG_DATA_HOME", ".local/share", "LOCALAPPDATA")
}

/// The log file.
pub fn state_base() -> Option<PathBuf> {
    base("XDG_STATE_HOME", ".local/state", "LOCALAPPDATA")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_override_wins_and_the_program_folder_is_added_by_the_caller() {
        let _g = crate::settings::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::env::set_var("XDG_DATA_HOME", "/somewhere/data");
        assert_eq!(data_base(), Some(PathBuf::from("/somewhere/data")));
        std::env::remove_var("XDG_DATA_HOME");
    }
}
