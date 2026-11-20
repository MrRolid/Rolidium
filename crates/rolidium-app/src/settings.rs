// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Small persistent user settings (`key=value` lines in the user's config directory).

use std::path::PathBuf;

/// Tests that change `XDG_CONFIG_HOME` must not run at the same time.
#[cfg(test)]
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A graphics backend that failed to start, with what was known about it at that moment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockedBackend {
    pub backend: String,
    pub adapter: String,
    pub driver: String,
    pub version: String,
}

impl BlockedBackend {
    fn clean(s: &str) -> String {
        s.replace(['|', '\n', '\r'], " ").trim().to_string()
    }

    pub fn to_line(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            Self::clean(&self.backend),
            Self::clean(&self.adapter),
            Self::clean(&self.driver),
            Self::clean(&self.version)
        )
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut it = line.split('|');
        let backend = it.next()?.trim().to_string();
        if !matches!(backend.as_str(), "dx12" | "vulkan" | "gl" | "auto") {
            return None;
        }
        Some(BlockedBackend {
            backend,
            adapter: it.next().unwrap_or("").to_string(),
            driver: it.next().unwrap_or("").to_string(),
            version: it.next().unwrap_or("").to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub export_dir: String,
    pub preset: String,
    pub deviation: f64,
    pub angle_deg: f64,
    pub ascii: bool,
    /// Grid step in mm (sketching and drag steps) and whether snapping to it is on.
    pub grid_step: f64,
    pub snap: bool,
    /// Language code (`en`, `cs`, ...); empty means "follow the system".
    pub language: String,
    pub sound: bool,
    /// 0.0..=1.0
    pub volume: f64,
    /// Minutes between autosaves; 0 switches autosave off.
    pub autosave_minutes: u32,
    /// Last folder used in the file dialogs.
    pub file_dir: String,
    /// Recently used documents, newest first.
    pub recent: Vec<String>,
    /// Recently used folders of the file dialogs, newest first.
    pub recent_dirs: Vec<String>,
    /// `dark` (default), `light` or `system`.
    pub theme: String,
    /// Graphics backend: `auto`, `dx12`, `vulkan` or `gl` (takes effect after a restart).
    pub backend: String,
    /// Backends that did not get through the start (a crash or an error) and are skipped by `auto`. Choosing `auto` in the
    /// settings forgets them.
    pub backend_blocked: Vec<BlockedBackend>,
    /// Size of the file dialog window (0 = default).
    pub dialog_w: f32,
    pub dialog_h: f32,
    /// Which of the support addresses this installation shows (only the number is stored).
    pub donate_index: Option<usize>,
    /// Turn the view square to the sketch plane when a sketch starts (and back when it ends).
    pub auto_turn_view: bool,
    /// A new empty document starts in a sketch on the XY plane with the Circle tool.
    pub new_doc_sketch: bool,
    /// The guide of the first steps is on, and the milestones it has seen (see `guide.rs`).
    pub guide_enabled: bool,
    pub guide_mask: u8,
    /// The largest gap of the Join ends tool, mm.
    pub join_gap: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            export_dir: String::new(),
            preset: "Standard".into(),
            deviation: 0.02,
            angle_deg: 10.0,
            ascii: false,
            grid_step: 1.0,
            snap: true,
            language: String::new(),
            sound: true,
            volume: 0.5,
            autosave_minutes: 2,
            file_dir: String::new(),
            recent: Vec::new(),
            recent_dirs: Vec::new(),
            theme: "dark".into(),
            backend: "auto".into(),
            backend_blocked: Vec::new(),
            dialog_w: 0.0,
            dialog_h: 0.0,
            donate_index: None,
            auto_turn_view: true,
            new_doc_sketch: true,
            guide_enabled: true,
            guide_mask: 0,
            join_gap: 1.0,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = crate::dirs::config_base()?;
    Some(base.join("rolidium").join("settings.conf"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "export_dir" => s.export_dir = v.to_string(),
                "preset" => s.preset = v.to_string(),
                "deviation" => s.deviation = v.parse().unwrap_or(s.deviation),
                "angle_deg" => s.angle_deg = v.parse().unwrap_or(s.angle_deg),
                "ascii" => s.ascii = v == "true",
                "grid_step" => s.grid_step = v.parse().unwrap_or(s.grid_step),
                "snap" => s.snap = v != "false",
                "language" => s.language = v.to_string(),
                "sound" => s.sound = v != "false",
                "volume" => s.volume = v.parse().unwrap_or(s.volume),
                "autosave_minutes" => s.autosave_minutes = v.parse().unwrap_or(s.autosave_minutes).min(240),
                "file_dir" => s.file_dir = v.to_string(),
                "recent" if !v.is_empty() => s.recent.push(v.to_string()),
                "recent_dir" if !v.is_empty() => s.recent_dirs.push(v.to_string()),
                "theme" if matches!(v, "dark" | "light" | "system") => s.theme = v.to_string(),
                "backend" if matches!(v, "auto" | "dx12" | "vulkan" | "gl") => s.backend = v.to_string(),
                "backend_blocked" => s.backend_blocked.extend(BlockedBackend::parse(v)),
                "dialog_w" => s.dialog_w = v.parse().unwrap_or(0.0),
                "dialog_h" => s.dialog_h = v.parse().unwrap_or(0.0),
                "donate_index" => s.donate_index = v.parse().ok(),
                "auto_turn_view" => s.auto_turn_view = v != "false",
                "new_doc_sketch" => s.new_doc_sketch = v != "false",
                "guide_enabled" => s.guide_enabled = v != "false",
                "guide_mask" => s.guide_mask = v.parse().unwrap_or(0),
                "join_gap" => s.join_gap = v.parse().ok().filter(|g: &f64| g.is_finite() && *g > 0.0).unwrap_or(1.0),
                _ => {}
            }
        }
        if !(s.deviation > 0.0 && s.angle_deg > 0.0) {
            return Settings::default();
        }
        if !(s.grid_step.is_finite() && s.grid_step > 0.0) {
            s.grid_step = 1.0;
        }
        s.volume = s.volume.clamp(0.0, 1.0);
        s
    }

    /// Puts `path` at the top of the recent list (at most eight entries).
    pub fn remember(&mut self, path: &str) {
        self.recent.retain(|r| r != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(8);
    }

    /// Puts a folder at the top of the recently used folders (at most eight).
    pub fn remember_dir(&mut self, dir: &str) {
        self.recent_dirs.retain(|r| r != dir);
        self.recent_dirs.insert(0, dir.to_string());
        self.recent_dirs.truncate(8);
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let text = format!(
            "theme={}\nbackend={}\nautosave_minutes={}\nfile_dir={}\nexport_dir={}\npreset={}\ndeviation={}\nangle_deg={}\nascii={}\ngrid_step={}\nsnap={}\nlanguage={}\nsound={}\nvolume={}\n",
            self.theme,
            self.backend,
            self.autosave_minutes,
            self.file_dir,
            self.export_dir,
            self.preset,
            self.deviation,
            self.angle_deg,
            self.ascii,
            self.grid_step,
            self.snap,
            self.language,
            self.sound,
            self.volume
        );
        let mut text = text;
        text.push_str(&format!(
            "auto_turn_view={}\nnew_doc_sketch={}\nguide_enabled={}\nguide_mask={}\njoin_gap={}\n",
            self.auto_turn_view, self.new_doc_sketch, self.guide_enabled, self.guide_mask, self.join_gap
        ));
        if let Some(i) = self.donate_index {
            text.push_str(&format!("donate_index={i}\n"));
        }
        for b in &self.backend_blocked {
            text.push_str(&format!("backend_blocked={}\n", b.to_line()));
        }
        for r in &self.recent {
            text.push_str(&format!("recent={r}\n"));
        }
        for r in &self.recent_dirs {
            text.push_str(&format!("recent_dir={r}\n"));
        }
        if self.dialog_w > 0.0 && self.dialog_h > 0.0 {
            text.push_str(&format!(
                "dialog_w={}\ndialog_h={}\n",
                self.dialog_w, self.dialog_h
            ));
        }
        let _ = std::fs::write(p, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("rolidium-settings-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let s = Settings {
            export_dir: "/tmp/x".into(),
            preset: "Custom".into(),
            deviation: 0.02,
            angle_deg: 7.5,
            ascii: true,
            grid_step: 0.25,
            snap: false,
            language: "cs".into(),
            sound: false,
            volume: 0.25,
            autosave_minutes: 5,
            file_dir: "/x".into(),
            recent: vec!["/a/b.rolidium".into(), "/c.rolidium".into()],
            recent_dirs: vec!["/d".into(), "/e".into()],
            theme: "light".into(),
            backend: "vulkan".into(),
            backend_blocked: vec![BlockedBackend {
                backend: "vulkan".into(),
                adapter: "Test GPU".into(),
                driver: "1.2.3".into(),
                version: "0.2.0".into(),
            }],
            dialog_w: 900.0,
            dialog_h: 600.0,
            donate_index: Some(1),
            auto_turn_view: false,
            new_doc_sketch: false,
            guide_enabled: false,
            guide_mask: 5,
            join_gap: 0.25,
        };
        s.save();
        assert_eq!(Settings::load(), s);
    }
}
