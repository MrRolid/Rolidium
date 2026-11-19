// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Safe start of the graphics.
//!
//! A graphics driver can crash the whole program while it is merely loaded or initialised, and nothing in Rust can catch that. So the
//! start is defended in three ways:
//!
//! * `auto` tries the backends one after another, each with an instance of its own (on Windows DirectX 12 first, so the Vulkan driver is
//!   not even loaded while DirectX 12 works; see `rolidium_render::auto_order`).
//! * Before a backend is tried, a marker file is written to the state folder and locked. The operating system releases the lock when the
//!   process ends, however it ends. A marker that is not locked any more and still exists means that an earlier start with that backend
//!   did not get through. The backend is then skipped and remembered in the settings. A marker of a program that is still running
//!   (another window that is just starting) is locked and is left alone, and a marker written by another version of the program is
//!   ignored. A program that is closed during the start removes its marker itself.
//! * Every step is written to the log and forced to the disk, so that after a crash the last step is visible.

use crate::logging::{log, log_sync};
use crate::settings::{BlockedBackend, Settings};
use rolidium_render::{auto_order, best_adapter_index, wgpu, BackendChoice, Step};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// What a start-up marker says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marker {
    pub version: String,
    pub backend: String,
    pub adapter: String,
    pub driver: String,
}

impl Marker {
    pub fn to_text(&self) -> String {
        let one = |s: &str| s.replace(['\n', '\r'], " ");
        format!(
            "version={}\nbackend={}\nadapter={}\ndriver={}\n",
            one(&self.version),
            one(&self.backend),
            one(&self.adapter),
            one(&self.driver)
        )
    }

    pub fn parse(text: &str) -> Option<Marker> {
        let mut m = Marker::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k {
                "version" => m.version = v.to_string(),
                "backend" => m.backend = v.to_string(),
                "adapter" => m.adapter = v.to_string(),
                "driver" => m.driver = v.to_string(),
                _ => {}
            }
        }
        (!m.backend.is_empty()).then_some(m)
    }
}

// ---- the marker of this process ----

struct Own {
    file: std::fs::File,
    path: PathBuf,
    marker: Marker,
}

static OWN: Mutex<Option<Own>> = Mutex::new(None);
static FRAMES: AtomicU32 = AtomicU32::new(0);
/// How many frames must be drawn before the start counts as successful.
const GOOD_FRAMES: u32 = 5;

fn marker_name(pid: u32) -> String {
    format!("startup-{pid}.mark")
}

fn write_own(own: &mut Own) {
    let text = own.marker.to_text();
    let _ = own.file.set_len(0);
    let _ = own.file.rewind();
    let _ = own.file.write_all(text.as_bytes());
    let _ = own.file.sync_all();
}

/// Writes (or rewrites) the marker of this process: this backend is being tried now.
pub fn begin(dir: &Path, backend: &str) {
    let mut guard = OWN.lock().unwrap_or_else(|e| e.into_inner());
    let marker = Marker {
        version: env!("CARGO_PKG_VERSION").to_string(),
        backend: backend.to_string(),
        adapter: String::new(),
        driver: String::new(),
    };
    if let Some(own) = guard.as_mut() {
        own.marker = marker;
        write_own(own);
        return;
    }
    let path = dir.join(marker_name(std::process::id()));
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
    else {
        log("start-up marker cannot be written");
        return;
    };
    if file.try_lock().is_err() {
        log("start-up marker cannot be locked");
        return;
    }
    let mut own = Own { file, path, marker };
    write_own(&mut own);
    *guard = Some(own);
}

/// Adds the adapter to the marker once it is known (a crash while the device is created then names it).
pub fn note_adapter(adapter: &str, driver: &str) {
    let mut guard = OWN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(own) = guard.as_mut() {
        own.marker.adapter = adapter.to_string();
        own.marker.driver = driver.to_string();
        write_own(own);
    }
}

/// Removes the marker of this process: the start got through, or the program is being closed on purpose.
pub fn finish() {
    let own = OWN.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(own) = own {
        let path = own.path.clone();
        drop(own);
        let _ = std::fs::remove_file(path);
    }
}

pub fn frames_drawn() -> bool {
    FRAMES.load(Ordering::Relaxed) > 0
}

/// Called for every frame: after the first few frames were drawn the start counts as successful and the marker goes.
pub fn frame_drawn() {
    let n = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if n == GOOD_FRAMES {
        log_sync("graphics start-up completed: the first frames were drawn");
        finish();
    }
}

/// Markers of earlier starts that did not get through (their process is gone and they were not removed). They are deleted.
/// Markers of running programs (locked) and of other versions of the program are left out.
pub fn scan_stale(dir: &Path, current_version: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !(name.starts_with("startup-") && name.ends_with(".mark")) {
            continue;
        }
        let path = e.path();
        let Ok(mut f) = std::fs::OpenOptions::new().read(true).write(true).open(&path) else {
            continue;
        };
        if f.try_lock().is_err() {
            continue; // the program that wrote it is still running
        }
        let mut text = String::new();
        {
            use std::io::Read;
            let _ = f.read_to_string(&mut text);
        }
        drop(f);
        let _ = std::fs::remove_file(&path);
        match Marker::parse(&text) {
            Some(m) if m.version == current_version => out.push(m),
            Some(m) => log(&format!("start-up marker of version {} ignored", m.version)),
            None => {}
        }
    }
    out
}

// ---- decisions (pure) ----

/// The steps to try. An explicit choice means exactly that backend and nothing else (the remembered skips do not apply); `auto`
/// is the ordered list without the backends that failed earlier.
pub fn plan(windows: bool, choice: BackendChoice, blocked: &[BlockedBackend]) -> Vec<Step> {
    if choice != BackendChoice::Auto {
        return vec![Step {
            backend: choice,
            hardware_only: false,
        }];
    }
    let order = auto_order(windows);
    if order.len() < 2 {
        return order; // one step for wgpu to decide: nothing to skip to
    }
    order
        .into_iter()
        .filter(|s| !blocked.iter().any(|b| b.backend == s.backend.name()))
        .collect()
}

/// Remembers the backends of stale markers as blocked (only where `auto` has an order to skip in). Returns the names that are new.
pub fn register_failures(windows: bool, blocked: &mut Vec<BlockedBackend>, stale: &[Marker]) -> Vec<String> {
    let mut new = Vec::new();
    if auto_order(windows).len() < 2 {
        return new;
    }
    for m in stale {
        if blocked.iter().any(|b| b.backend == m.backend) {
            continue;
        }
        blocked.push(BlockedBackend {
            backend: m.backend.clone(),
            adapter: m.adapter.clone(),
            driver: m.driver.clone(),
            version: m.version.clone(),
        });
        new.push(m.backend.clone());
    }
    new
}

/// Tries the steps in order with `probe`; the first that works wins. `None` when every step failed.
pub fn select<T>(steps: &[Step], mut probe: impl FnMut(&Step) -> Option<T>) -> Option<(Step, T)> {
    steps.iter().find_map(|s| probe(s).map(|t| (*s, t)))
}

/// The line "skipped earlier" for the settings window.
pub fn blocked_names(blocked: &[BlockedBackend]) -> String {
    blocked
        .iter()
        .map(|b| b.backend.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- the real probe ----

/// What a successful probe found.
pub struct Found {
    pub adapter: String,
    pub driver: String,
}

/// Creates an instance of the one backend, lists its adapters and checks that an acceptable one exists. Every stage is logged to
/// the disk first; after a crash inside the driver the log ends at the stage that did it.
pub fn probe(step: &Step, dir: Option<&Path>) -> Option<Found> {
    let name = step.backend.name();
    if let Some(d) = dir {
        begin(d, name);
    }
    log_sync(&format!(
        "graphics: trying backend {name}{}",
        if step.hardware_only {
            " (real graphics card only)"
        } else {
            ""
        }
    ));
    test_crash_hook(name);
    log_sync(&format!(
        "graphics: creating the {name} instance and requesting its adapters"
    ));
    let adapters = rolidium_render::enumerate(step.backend.backends());
    let infos: Vec<wgpu::AdapterInfo> = adapters.iter().map(|a| a.get_info()).collect();
    for i in &infos {
        log_sync(&format!(
            "graphics: adapter {} ({:?}, {:?}) driver {} {}",
            i.name, i.backend, i.device_type, i.driver, i.driver_info
        ));
    }
    let Some(n) = best_adapter_index(&infos, step.hardware_only) else {
        log_sync(&format!("graphics: backend {name} has no acceptable adapter"));
        return None;
    };
    let i = &infos[n];
    let driver = format!("{} {}", i.driver, i.driver_info).trim().to_string();
    note_adapter(&i.name, &driver);
    log_sync(&format!("graphics: {name} accepted with {}", i.name));
    Some(Found {
        adapter: i.name.clone(),
        driver,
    })
}

/// Internal test hook: `ROLIDIUM_TEST_CRASH=<backend>` ends the program abruptly when that backend is tried, `<backend>:av`
/// causes an access violation instead (on Windows, to test the exception log line). Used by the tests of the safeguard.
fn test_crash_hook(name: &str) {
    let Ok(v) = std::env::var("ROLIDIUM_TEST_CRASH") else {
        return;
    };
    let (b, av) = v
        .split_once(':')
        .map_or((v.as_str(), false), |(b, k)| (b, k == "av"));
    if b == name {
        log_sync(&format!("test hook: crashing in backend {name}"));
        if av {
            crate::winconsole::test_access_violation();
        }
        std::process::abort();
    }
}

// ---- the notice after a fallback start ----

static NOTICE: Mutex<Option<(Vec<String>, String)>> = Mutex::new(None);

/// Remembers that the window must tell the user which backends failed and which one runs now.
pub fn set_notice(failed: Vec<String>, running: String) {
    if !failed.is_empty() {
        *NOTICE.lock().unwrap_or_else(|e| e.into_inner()) = Some((failed, running));
    }
}

pub fn take_notice() -> Option<(Vec<String>, String)> {
    NOTICE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// The environment variable that carries the failed backends over to the program that is started again after an error.
pub const FAILED_ENV: &str = "ROLIDIUM_START_FAILED";
const RESTARTS_ENV: &str = "ROLIDIUM_START_RESTARTS";

pub fn restarts() -> u32 {
    std::env::var(RESTARTS_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Starts this program again (after a start-up error of the chosen backend, which has been remembered as blocked).
pub fn restart(args: &[String], failed: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .args(args)
        .env(RESTARTS_ENV, (restarts() + 1).to_string())
        .env(FAILED_ENV, failed.join(","))
        .spawn()
        .is_ok()
}

/// Remembers a backend that returned an error while the window was created.
pub fn block_now(settings: &mut Settings, backend: &str, adapter: &str, driver: &str) {
    if !settings.backend_blocked.iter().any(|b| b.backend == backend) {
        settings.backend_blocked.push(BlockedBackend {
            backend: backend.to_string(),
            adapter: adapter.to_string(),
            driver: driver.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        });
        settings.save();
    }
}

/// What the user is told when nothing works: what happened, what helps, how to try by hand, where the log is.
pub fn nothing_works_text(log: Option<&Path>, blocked: &[BlockedBackend]) -> String {
    use crate::i18n::trf;
    let skipped = if blocked.is_empty() {
        String::new()
    } else {
        trf(
            "Skipped because they failed to start earlier: {}.",
            &[blocked_names(blocked)],
        )
    };
    trf(
        "Rolidium could not start its graphics: no graphics backend worked. {}\n\nUpdating the driver of your graphics card usually helps. You can also try a backend by hand: start the program with --backend dx12, --backend vulkan or --backend gl; --backend auto forgets the backends that were skipped. The log is here: {}",
        &[
            skipped,
            log.map_or_else(|| "-".to_string(), |p| p.display().to_string()),
        ],
    )
}

pub fn window_title() -> String {
    crate::i18n::tr("Rolidium cannot start")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(names: &[&str]) -> Vec<BlockedBackend> {
        names
            .iter()
            .map(|n| BlockedBackend {
                backend: n.to_string(),
                ..Default::default()
            })
            .collect()
    }

    fn names(steps: &[Step]) -> Vec<(&'static str, bool)> {
        steps
            .iter()
            .map(|s| (s.backend.name(), s.hardware_only))
            .collect()
    }

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rolidium-startup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn windows_order_is_dx12_then_vulkan_then_gl_then_software() {
        let p = plan(true, BackendChoice::Auto, &[]);
        assert_eq!(
            names(&p),
            vec![
                ("dx12", true),
                ("vulkan", true),
                ("gl", true),
                ("dx12", false),
                ("vulkan", false),
                ("gl", false)
            ]
        );
        // every step has one backend only: Vulkan is never in an instance together with DirectX 12
        for s in &p {
            assert_eq!(s.backend.backends().iter().count(), 1);
        }
    }

    #[test]
    fn other_systems_keep_the_single_step() {
        let p = plan(false, BackendChoice::Auto, &blocked(&["vulkan"]));
        assert_eq!(names(&p), vec![("auto", false)]);
    }

    #[test]
    fn an_explicit_choice_initialises_only_that_backend() {
        for (c, n) in [
            (BackendChoice::Dx12, "dx12"),
            (BackendChoice::Vulkan, "vulkan"),
            (BackendChoice::Gl, "gl"),
        ] {
            // also when it was skipped before
            assert_eq!(names(&plan(true, c, &blocked(&[n]))), vec![(n, false)]);
            assert_eq!(names(&plan(false, c, &[])), vec![(n, false)]);
        }
    }

    #[test]
    fn a_remembered_failure_skips_the_backend() {
        let p = plan(true, BackendChoice::Auto, &blocked(&["vulkan"]));
        assert!(p.iter().all(|s| s.backend != BackendChoice::Vulkan));
        assert_eq!(p[0].backend, BackendChoice::Dx12);
        let p = plan(true, BackendChoice::Auto, &blocked(&["dx12"]));
        assert_eq!(p[0].backend, BackendChoice::Vulkan);
    }

    #[test]
    fn when_everything_failed_there_is_nothing_to_try_and_a_message() {
        let p = plan(true, BackendChoice::Auto, &blocked(&["dx12", "vulkan", "gl"]));
        assert!(p.is_empty());
        assert!(select(&p, |_| Some(1)).is_none());
        let text = nothing_works_text(Some(Path::new("/x/rolidium.log")), &blocked(&["dx12", "vulkan"]));
        assert!(
            text.contains("dx12, vulkan") && text.contains("--backend") && text.contains("/x/rolidium.log")
        );
        // and when the steps run but none finds an adapter
        let p = plan(true, BackendChoice::Auto, &[]);
        assert!(select(&p, |_: &Step| None::<u8>).is_none());
    }

    #[test]
    fn select_stops_at_the_first_step_that_works() {
        let p = plan(true, BackendChoice::Auto, &[]);
        let mut tried = Vec::new();
        let got = select(&p, |s| {
            tried.push(s.backend.name());
            (s.backend == BackendChoice::Vulkan).then_some(7)
        });
        assert_eq!(got.map(|(s, v)| (s.backend, v)), Some((BackendChoice::Vulkan, 7)));
        assert_eq!(tried, vec!["dx12", "vulkan"]);
    }

    #[test]
    fn failures_are_remembered_once_and_only_where_auto_has_an_order() {
        let m = |b: &str| Marker {
            version: "0.2.0".into(),
            backend: b.into(),
            adapter: "GPU".into(),
            driver: "1.0".into(),
        };
        let mut list = Vec::new();
        assert_eq!(
            register_failures(true, &mut list, &[m("vulkan"), m("vulkan")]),
            vec!["vulkan"]
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].adapter, "GPU");
        assert!(register_failures(true, &mut list, &[m("vulkan")]).is_empty());
        // on Linux a stale marker never locks the program out
        let mut list = Vec::new();
        assert!(register_failures(false, &mut list, &[m("auto")]).is_empty());
        assert!(list.is_empty());
    }

    #[test]
    fn the_marker_round_trips() {
        let m = Marker {
            version: "0.2.0".into(),
            backend: "vulkan".into(),
            adapter: "Intel(R) UHD".into(),
            driver: "31.0.101.2137".into(),
        };
        assert_eq!(Marker::parse(&m.to_text()), Some(m));
        assert_eq!(Marker::parse(""), None);
    }

    #[test]
    fn a_marker_without_a_process_is_a_failed_start_and_is_deleted() {
        let d = temp("stale");
        let m = Marker {
            version: env!("CARGO_PKG_VERSION").into(),
            backend: "vulkan".into(),
            ..Default::default()
        };
        std::fs::write(d.join("startup-99999999.mark"), m.to_text()).unwrap();
        let got = scan_stale(&d, env!("CARGO_PKG_VERSION"));
        assert_eq!(got, vec![m]);
        assert!(!d.join("startup-99999999.mark").exists());
        assert!(scan_stale(&d, env!("CARGO_PKG_VERSION")).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_false_alarm_does_no_harm() {
        let cur = env!("CARGO_PKG_VERSION");
        let d = temp("false");
        // 1. a window that is still starting (its marker is locked): left alone, not counted
        let live = d.join("startup-1.mark");
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&live)
            .unwrap();
        f.try_lock().unwrap();
        (&f).write_all(format!("version={cur}\nbackend=dx12\n").as_bytes())
            .unwrap();
        assert!(scan_stale(&d, cur).is_empty());
        assert!(live.exists());
        drop(f); // the owner ends: its marker is now a stale one
                 // 2. a marker of another version of the program is ignored (and removed)
        std::fs::write(d.join("startup-2.mark"), "version=0.0.1\nbackend=vulkan\n").unwrap();
        // 3. rubbish
        std::fs::write(d.join("startup-3.mark"), "x").unwrap();
        let got = scan_stale(&d, cur);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].backend, "dx12");
        assert!(!d.join("startup-2.mark").exists() && !d.join("startup-3.mark").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn begin_and_finish_leave_no_marker_behind() {
        let d = temp("own");
        begin(&d, "dx12");
        let me = d.join(marker_name(std::process::id()));
        assert!(me.exists());
        note_adapter("GPU", "1.0");
        // a second process looking at it must see a running program, not a crash
        assert!(scan_stale(&d, env!("CARGO_PKG_VERSION")).is_empty());
        finish();
        assert!(!me.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn clearing_the_memory_brings_the_backend_back() {
        let mut s = Settings {
            backend_blocked: blocked(&["vulkan"]),
            ..Default::default()
        };
        assert!(plan(true, BackendChoice::Auto, &s.backend_blocked)
            .iter()
            .all(|x| x.backend != BackendChoice::Vulkan));
        s.backend_blocked.clear();
        assert!(plan(true, BackendChoice::Auto, &s.backend_blocked)
            .iter()
            .any(|x| x.backend == BackendChoice::Vulkan));
    }
}
