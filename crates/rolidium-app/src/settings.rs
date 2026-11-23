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
    /// Threads for the calculations: 0 = automatic (one per core less one).
    pub threads: u32,
    /// The person's answer to the question whether Rolidium may ask once a day for a new version: `None` = not asked yet (no network until answered).
    pub check_updates: Option<bool>,
    /// The version the person chose to skip, and the day (days since 1970) of the last question.
    pub skip_version: String,
    pub last_check_day: u64,
    /// Last folder used in the file dialogs.
    pub file_dir: String,
    /// Recently used documents, newest first.
    pub recent: Vec<String>,
    /// Recently used folders of the file dialogs, newest first.
    pub recent_dirs: Vec<String>,
    /// `dark` (default), `light` or `system`.
    pub theme: String,
    /// The slider "Brightness" of the appearance, -100 to 100 (0 is the default).
    pub brightness: i32,
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
    /// Files of the diagnostics the person switched off in "What is sent" (`summary.txt` cannot be switched off).
    pub report_skip: Vec<String>,
    /// The diagnostic panel (F12): its distance from the left and from the bottom edge of the model area (None = default corner).
    pub diag_pos: Option<[f32; 2]>,
    /// Ceiling of the frame rate: `monitor` (default), `30`, `60`, `120` or `none`. Between the monitor-synchronised group and the
    /// unsynchronised one (120, none) it takes effect after a restart.
    pub fps_cap: String,
}

/// The values of the frame rate ceiling.
pub const FPS_CAPS: [&str; 5] = ["monitor", "30", "60", "120", "none"];

/// The ceiling as a minimum time between two frames (None = no ceiling by the program).
pub fn fps_interval(cap: &str) -> Option<std::time::Duration> {
    cap.parse::<f64>()
        .ok()
        .filter(|f| *f > 0.0)
        .map(|f| std::time::Duration::from_secs_f64(1.0 / f))
}

/// Whether the frames are handed to the monitor (synchronised with its refresh) for this ceiling: "monitor", 30 and 60 are, 120 and
/// "none" are not (a ceiling above the usual 60 Hz needs frames that do not wait for it).
pub fn synchronised(cap: &str) -> bool {
    !matches!(cap, "120" | "none")
}

/// How much of the interval of a software ceiling is used when the frames also wait for the monitor: a little less, so that the monitor and
/// not the clock sets the pace (two clocks of about the same period beat against each other and drop frames, 60 fps became 52).
pub const SYNC_MARGIN: f64 = 0.94;

/// Waits until the deadline of the next frame and sets the one after it (`iv` later). The deadline moves by whole intervals, so the rate does
/// not drift whatever the frames cost; the last millisecond is waited for in a loop (the sleep of the system is not exact). A deadline that is
/// more than a frame behind (the window was idle) starts again from now.
pub fn pace(deadline: &mut Option<std::time::Instant>, iv: std::time::Duration) {
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let due = deadline.unwrap_or(now);
    if due > now {
        let wait = due - now;
        if wait > Duration::from_millis(2) {
            std::thread::sleep(wait - Duration::from_millis(1));
        }
        while Instant::now() < due {
            std::hint::spin_loop();
        }
    }
    let after = Instant::now();
    *deadline = Some(if after > due + iv { after + iv } else { due + iv });
}

/// The way the frames reach the screen, as started (set once in `main`).
pub static PRESENT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

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
            threads: 0,
            check_updates: None,
            skip_version: String::new(),
            last_check_day: 0,
            file_dir: String::new(),
            recent: Vec::new(),
            recent_dirs: Vec::new(),
            theme: "dark".into(),
            brightness: 0,
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
            report_skip: Vec::new(),
            diag_pos: None,
            fps_cap: "monitor".into(),
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
                "check_updates" => {
                    s.check_updates = match v {
                        "yes" => Some(true),
                        "no" => Some(false),
                        _ => None,
                    }
                }
                "skip_version" => {
                    s.skip_version = v
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.')
                        .take(24)
                        .collect()
                }
                "last_check_day" => s.last_check_day = v.parse().unwrap_or(0),
                "threads" => s.threads = v.parse().unwrap_or(0).min(256),
                "autosave_minutes" => s.autosave_minutes = v.parse().unwrap_or(s.autosave_minutes).min(240),
                "file_dir" => s.file_dir = v.to_string(),
                "recent" if !v.is_empty() => s.recent.push(v.to_string()),
                "recent_dir" if !v.is_empty() => s.recent_dirs.push(v.to_string()),
                "theme" if matches!(v, "dark" | "light" | "system") => s.theme = v.to_string(),
                "brightness" => s.brightness = v.parse::<i32>().unwrap_or(0).clamp(-100, 100),
                "backend" if matches!(v, "auto" | "dx12" | "vulkan" | "gl") => s.backend = v.to_string(),
                "backend_blocked" => s.backend_blocked.extend(BlockedBackend::parse(v)),
                "dialog_w" => s.dialog_w = v.parse().unwrap_or(0.0),
                "dialog_h" => s.dialog_h = v.parse().unwrap_or(0.0),
                "donate_index" => s.donate_index = v.parse().ok(),
                "auto_turn_view" => s.auto_turn_view = v != "false",
                "new_doc_sketch" => s.new_doc_sketch = v != "false",
                "guide_enabled" => s.guide_enabled = v != "false",
                "guide_mask" => s.guide_mask = v.parse().unwrap_or(0),
                "join_gap" => {
                    s.join_gap = v
                        .parse()
                        .ok()
                        .filter(|g: &f64| g.is_finite() && *g > 0.0)
                        .unwrap_or(1.0)
                }
                "fps_cap" => {
                    s.fps_cap = if FPS_CAPS.contains(&v) {
                        v.to_string()
                    } else {
                        "monitor".into()
                    }
                }
                "diag_pos" => {
                    let mut it = v.split(',').filter_map(|n| n.trim().parse::<f32>().ok());
                    if let (Some(x), Some(y)) = (it.next(), it.next()) {
                        if x.is_finite() && y.is_finite() {
                            s.diag_pos = Some([x, y]);
                        }
                    }
                }
                "report_skip" => {
                    s.report_skip = v
                        .split(',')
                        .map(str::trim)
                        .filter(|n| !n.is_empty() && *n != "summary.txt")
                        .map(String::from)
                        .collect()
                }
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
            "theme={}\nbackend={}\nautosave_minutes={}\nthreads={}\ncheck_updates={}\nskip_version={}\nlast_check_day={}\nfile_dir={}\nexport_dir={}\npreset={}\ndeviation={}\nangle_deg={}\nascii={}\ngrid_step={}\nsnap={}\nlanguage={}\nsound={}\nvolume={}\n",
            self.theme,
            self.backend,
            self.autosave_minutes,
            self.threads,
            match self.check_updates {
                Some(true) => "yes",
                Some(false) => "no",
                None => "ask",
            },
            self.skip_version,
            self.last_check_day,
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
            "auto_turn_view={}\nnew_doc_sketch={}\nguide_enabled={}\nguide_mask={}\njoin_gap={}\nreport_skip={}\nbrightness={}\n",
            self.auto_turn_view, self.new_doc_sketch, self.guide_enabled, self.guide_mask, self.join_gap, self.report_skip.join(","), self.brightness
        ));
        text.push_str(&format!("fps_cap={}\n", self.fps_cap));
        if let Some([x, y]) = self.diag_pos {
            text.push_str(&format!("diag_pos={x},{y}\n"));
        }
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
            threads: 4,
            check_updates: Some(true),
            skip_version: "0.3.0".into(),
            last_check_day: 20_000,
            file_dir: "/x".into(),
            recent: vec!["/a/b.rolidium".into(), "/c.rolidium".into()],
            recent_dirs: vec!["/d".into(), "/e".into()],
            theme: "light".into(),
            brightness: -35,
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
            report_skip: vec!["log.txt".into()],
            diag_pos: Some([12.0, 34.5]),
            fps_cap: "120".into(),
        };
        s.save();
        assert_eq!(Settings::load(), s);
    }
}

impl Settings {
    /// The settings for the diagnostics that are sent: what the program is set to, without any file or folder names.
    pub fn diagnostic_text(&self) -> String {
        format!(
            "theme={}\nbackend={}\nautosave_minutes={}\nthreads={}\ncheck_updates={:?}\nlanguage={}\ndeviation={}\nangle_deg={}\nascii={}\ngrid_step={}\nsnap={}\nsound={}\nvolume={}\n",
            self.theme,
            self.backend,
            self.autosave_minutes,
            self.threads,
            self.check_updates,
            self.language,
            self.deviation,
            self.angle_deg,
            self.ascii,
            self.grid_step,
            self.snap,
            self.sound,
            self.volume
        )
    }
}

/// Whether the number of threads chosen in Settings differs from the one this run uses (it is decided at the start and does not change while
/// the program runs, so the choice is for the next start). With `ROLIDIUM_THREADS` set the choice has no effect at all.
pub fn threads_pending(choice: u32, in_use: usize, env_set: bool) -> bool {
    !env_set && rolidium_kernel::par::resolve(choice as usize) != in_use
}

#[cfg(test)]
mod threads_tests {
    use super::*;

    #[test]
    fn a_changed_number_of_threads_waits_for_the_next_start() {
        assert!(!threads_pending(4, 4, false));
        assert!(threads_pending(2, 4, false));
        let auto = rolidium_kernel::par::resolve(0);
        assert!(!threads_pending(0, auto, false));
        // with the environment variable set the choice never applies, so there is nothing to wait for
        assert!(!threads_pending(8, 4, true));
    }

    #[test]
    fn the_running_number_does_not_change_when_it_is_configured_again() {
        let first = rolidium_kernel::par::configure(3);
        assert_eq!(rolidium_kernel::par::configure(7), first);
        assert_eq!(rolidium_kernel::par::threads(), first);
    }
}

#[cfg(test)]
mod pace_tests {
    use super::*;

    #[test]
    fn the_ceiling_is_met_closely_and_without_drift() {
        for (cap, frames) in [("30", 30), ("60", 60), ("120", 120)] {
            let iv = fps_interval(cap).unwrap();
            let mut deadline = None;
            let t = std::time::Instant::now();
            // frames that cost a little, as in the program
            for _ in 0..frames + 1 {
                pace(&mut deadline, iv);
                std::thread::sleep(std::time::Duration::from_micros(300));
            }
            let secs = t.elapsed().as_secs_f64();
            assert!((secs - 1.0).abs() < 0.04, "{cap}: {frames} frames took {secs} s");
        }
    }

    #[test]
    fn the_modes_of_the_ceilings() {
        // the monitor, 30 and 60 wait for the monitor (Fifo, which every system has); 120 and unlimited do not
        for (cap, sync) in [
            ("monitor", true),
            ("30", true),
            ("60", true),
            ("120", false),
            ("none", false),
        ] {
            assert_eq!(synchronised(cap), sync, "{cap}");
        }
        assert!(fps_interval("monitor").is_none() && fps_interval("none").is_none());
        const { assert!(SYNC_MARGIN < 1.0 && SYNC_MARGIN > 0.8) };
    }
}
