// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Save diagnostics": one zip with the graphics adapters and backends, the chosen adapter, the system version, the log
//! and a test scene (grid, axes, a sketch region, a body) rendered off screen by every available backend.
//! Personal data (the home folder, the user and the computer name) is replaced before anything is written.

use rolidium_render::wgpu;
use rolidium_render::{backend_name, describe_adapter, enumerate, run_adapter};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Replaces the home folder, the user name and the computer name with placeholders.
pub fn sanitize(text: &str) -> String {
    mask_model_files(&sanitize_with(text, &personal_strings()))
}

/// Replaces every word that is the name of a model file (`.rolidium`, `.step`, `.stp`, `.stl`) with `<model file>`: names of the files of the
/// person do not leave the computer (the folders are already gone: the home folder is `~`).
pub fn mask_model_files(text: &str) -> String {
    let is_model = |w: &str| {
        let lower = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        [".rolidium", ".step", ".stp", ".stl"]
            .iter()
            .any(|e| lower.ends_with(e))
    };
    text.split_inclusive(char::is_whitespace)
        .map(|chunk| {
            let word = chunk.trim_end();
            if is_model(word) {
                format!("<model file>{}", &chunk[word.len()..])
            } else {
                chunk.to_string()
            }
        })
        .collect()
}

fn personal_strings() -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = Vec::new();
    for var in ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
        if let Some(p) = std::env::var_os(var) {
            let p = p.to_string_lossy().into_owned();
            if p.len() > 3 {
                v.push((p.replace('\\', "/"), "~"));
                v.push((p, "~"));
            }
        }
    }
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(u) = std::env::var(var) {
            if u.len() > 2 {
                v.push((u, "<user>"));
            }
        }
    }
    for var in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(h) = std::env::var(var) {
            if h.len() > 2 {
                v.push((h, "<computer>"));
            }
        }
    }
    if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
        let h = h.trim().to_string();
        if h.len() > 2 {
            v.push((h, "<computer>"));
        }
    }
    // the longest first, so that a folder is replaced before the user name inside it
    v.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
    v
}

pub fn sanitize_with(text: &str, personal: &[(String, &'static str)]) -> String {
    let mut out = text.to_string();
    for (what, with) in personal {
        out = out.replace(what.as_str(), with);
    }
    out
}

/// Name and version of the operating system.
pub fn system_version() -> String {
    #[cfg(windows)]
    {
        let ver = std::process::Command::new("cmd")
            .args(["/c", "ver"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        format!("Windows ({ver}), {}", std::env::consts::ARCH)
    }
    #[cfg(not(windows))]
    {
        let pretty = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("PRETTY_NAME=")
                        .map(|v| v.trim_matches('"').to_string())
                })
            })
            .unwrap_or_else(|| std::env::consts::OS.to_string());
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        format!("{pretty}, kernel {}, {}", kernel.trim(), std::env::consts::ARCH)
    }
}

// ---- every backend in a child process ----
//
// A graphics driver may crash the process that merely uses it. So the diagnostics (and the render part of the self test) never touch
// a backend themselves: each backend is tried by a child process (`rolidium --diag-child <backend> <folder>`) with a time limit. The
// child writes its findings as small files into the folder, adapter by adapter, so that what was done before a crash is kept. A crash
// or a timeout is a result like any other and the next backend is tried.

/// How one adapter did in the child.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterOutcome {
    pub label: String,
    pub description: String,
    /// The text of the check, or what went wrong.
    pub result: Result<String, String>,
    pub errors: Vec<String>,
    pub png: Option<Vec<u8>>,
}

/// What happened to the child of one backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    Finished,
    /// The process died; the code is the exit status when there is one.
    Crashed(Option<i32>),
    TimedOut,
    CannotStart(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendOutcome {
    pub backend: String,
    pub ending: Ending,
    pub adapters: Vec<AdapterOutcome>,
}

impl BackendOutcome {
    pub fn describe_ending(&self) -> String {
        match &self.ending {
            Ending::Finished => "finished".into(),
            Ending::Crashed(Some(c)) => format!("CRASHED (the test process ended with exit code {c:#x})"),
            Ending::Crashed(None) => "CRASHED (the test process was killed by a signal)".into(),
            Ending::TimedOut => {
                "TIMED OUT (no answer within the time limit; the test process was stopped)".into()
            }
            Ending::CannotStart(e) => format!("could not be started: {e}"),
        }
    }
}

/// The backends worth trying on this system, in the order they are reported.
pub fn child_backends() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["dx12", "vulkan", "gl"]
    } else if cfg!(target_os = "macos") {
        vec!["metal"]
    } else {
        vec!["vulkan", "gl"]
    }
}

fn backends_of(name: &str) -> Option<wgpu::Backends> {
    Some(match name {
        "dx12" => wgpu::Backends::DX12,
        "vulkan" => wgpu::Backends::VULKAN,
        "gl" => wgpu::Backends::GL,
        "metal" => wgpu::Backends::METAL,
        _ => return None,
    })
}

/// The body of the child process: enumerates the adapters of one backend and renders the test scene on each, writing the findings
/// into `dir`. Exit code 0 when it got to the end.
pub fn child_main(backend: &str, dir: &Path) -> i32 {
    let Some(b) = backends_of(backend) else {
        return 2;
    };
    let put = |name: &str, bytes: &[u8]| {
        if let Ok(mut f) = std::fs::File::create(dir.join(name)) {
            let _ = f.write_all(bytes);
            let _ = f.sync_all();
        }
    };
    put("stage.txt", b"enumerating adapters");
    if std::env::var("ROLIDIUM_TEST_CRASH_DIAG").is_ok_and(|v| v == backend) {
        std::process::abort(); // internal test hook
    }
    let adapters = enumerate(b);
    put("count.txt", adapters.len().to_string().as_bytes());
    for (n, a) in adapters.iter().enumerate() {
        let label = format!("{}-{}", backend_name(a.get_info().backend), n);
        put(&format!("{n}.label.txt"), label.as_bytes());
        put(&format!("{n}.desc.txt"), describe_adapter(a).as_bytes());
        put("stage.txt", format!("rendering on adapter {n}").as_bytes());
        let run = run_adapter(a, label, [640, 480]);
        match &run.result {
            Ok((rgba, check)) => {
                put(
                    &format!("{n}.result.txt"),
                    format!("ok\n{}", check.describe()).as_bytes(),
                );
                if let Ok(png) = rolidium_render::encode_png(rgba, 640, 480) {
                    put(&format!("{n}.png"), &png);
                }
            }
            Err(e) => put(&format!("{n}.result.txt"), format!("failed\n{e}").as_bytes()),
        }
        put(&format!("{n}.errors.txt"), run.errors.join("\n").as_bytes());
    }
    put("done.txt", b"done");
    0
}

/// Reads what a child left in `dir`.
pub fn read_child_dir(dir: &Path) -> Vec<AdapterOutcome> {
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).ok();
    let count: usize = read("count.txt").and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let mut out = Vec::new();
    for n in 0..count {
        let Some(description) = read(&format!("{n}.desc.txt")) else {
            continue;
        };
        let label = read(&format!("{n}.label.txt")).unwrap_or_else(|| format!("adapter-{n}"));
        let result = match read(&format!("{n}.result.txt")) {
            Some(t) => match t.split_once('\n') {
                Some(("ok", rest)) => Ok(rest.to_string()),
                Some((_, rest)) => Err(rest.to_string()),
                None => Err(t),
            },
            None => Err("the test process ended while this adapter was being used".to_string()),
        };
        let errors = read(&format!("{n}.errors.txt"))
            .map(|t| t.lines().map(String::from).collect())
            .unwrap_or_default();
        let png = std::fs::read(dir.join(format!("{n}.png"))).ok();
        out.push(AdapterOutcome {
            label,
            description,
            result,
            errors,
            png,
        });
    }
    out
}

/// Starts the child of one backend and waits for it at most `limit`.
pub fn run_child(exe: &Path, backend: &str, limit: std::time::Duration) -> BackendOutcome {
    let dir = std::env::temp_dir().join(format!("rolidium-diag-{}-{backend}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let spawned = std::process::Command::new(exe)
        .arg("--diag-child")
        .arg(backend)
        .arg(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let ending = match spawned {
        Err(e) => Ending::CannotStart(e.to_string()),
        Ok(mut child) => {
            let start = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(st)) => {
                        break if st.success() && dir.join("done.txt").exists() {
                            Ending::Finished
                        } else {
                            Ending::Crashed(st.code())
                        };
                    }
                    Ok(None) if start.elapsed() > limit => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Ending::TimedOut;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(e) => break Ending::CannotStart(e.to_string()),
                }
            }
        }
    };
    let adapters = read_child_dir(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    BackendOutcome {
        backend: backend.to_string(),
        ending,
        adapters,
    }
}

/// Every backend of this system in a child process of its own.
pub fn run_isolated() -> Vec<BackendOutcome> {
    let limit = std::time::Duration::from_secs(60);
    match std::env::current_exe() {
        Ok(exe) => child_backends()
            .into_iter()
            .map(|b| run_child(&exe, b, limit))
            .collect(),
        Err(e) => child_backends()
            .into_iter()
            .map(|b| BackendOutcome {
                backend: b.to_string(),
                ending: Ending::CannotStart(e.to_string()),
                adapters: Vec::new(),
            })
            .collect(),
    }
}

/// The text of `report.txt`. `selected` describes the adapter the window uses, when there is a window.
pub fn report(selected: Option<&str>, backend_setting: &str, outcomes: &[BackendOutcome]) -> String {
    let mut t = String::new();
    t.push_str(&format!("Rolidium {}\n", crate::version_string()));
    t.push_str(&format!("System: {}\n", system_version()));
    t.push_str(&format!("Backend setting: {backend_setting}\n\n"));
    match selected {
        Some(s) => t.push_str(&format!("== Adapter used by the window ==\n{s}\n")),
        None => t.push_str("== Adapter used by the window ==\n(no window: run from the command line)\n\n"),
    }
    t.push_str("== All adapters (each backend was examined by a separate process) ==\n");
    let mut any = false;
    for o in outcomes {
        t.push_str(&format!("-- backend {}: {} --\n", o.backend, o.describe_ending()));
        for a in &o.adapters {
            any = true;
            t.push_str(&a.description);
            t.push('\n');
        }
    }
    if !any {
        t.push_str("(none found)\n");
    }
    sanitize(&t)
}

/// Writes the diagnostics zip to `path`. Every backend is examined by a child process (see above).
pub fn write_zip(
    path: &Path,
    selected: Option<&str>,
    backend_setting: &str,
    settings_text: &str,
) -> Result<(), String> {
    write_zip_with(path, selected, backend_setting, settings_text, &run_isolated())
}

pub fn write_zip_with(
    path: &Path,
    selected: Option<&str>,
    backend_setting: &str,
    settings_text: &str,
    outcomes: &[BackendOutcome],
) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    add(
        "report.txt",
        report(selected, backend_setting, outcomes).as_bytes(),
    )?;
    add("log.txt", sanitize(&crate::logging::read_log()).as_bytes())?;
    add("settings.txt", sanitize(settings_text).as_bytes())?;
    let mut scenes =
        String::from("Test scene (grid, axes, sketch region, body) drawn off screen by each adapter. Each backend ran in a process of its own with a time limit:\n\n");
    for o in outcomes {
        scenes.push_str(&format!(
            "##### backend {}: {}\n\n",
            o.backend,
            o.describe_ending()
        ));
        for run in &o.adapters {
            scenes.push_str(&format!("== {} ==\n{}\n", run.label, sanitize(&run.description)));
            match &run.result {
                Ok(check) => {
                    scenes.push_str(&format!("result: {check}\n"));
                    if let Some(png) = &run.png {
                        add(&format!("scene-{}.png", run.label), png)?;
                    }
                }
                Err(e) => scenes.push_str(&format!("result: FAILED: {}\n", sanitize(e))),
            }
            for e in &run.errors {
                scenes.push_str(&format!("graphics error: {}\n", sanitize(e)));
            }
            scenes.push('\n');
        }
    }
    add("scenes.txt", scenes.as_bytes())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// The same zip without the test pictures (`scene-*.png`): when it is too large to be sent.
pub fn without_pictures(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..src.len() {
        let mut f = src.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map(|n| n.to_string()).map_err(|e| e.to_string())?;
        if name.ends_with(".png") {
            continue;
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).map_err(|e| e.to_string())?;
        out.start_file(name, opts).map_err(|e| e.to_string())?;
        out.write_all(&data).map_err(|e| e.to_string())?;
    }
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Where the diagnostics go: the user's Documents folder when it exists, else the home folder, else the current folder.
pub fn default_path() -> PathBuf {
    let dir = rolidium_places_documents().unwrap_or_else(|| PathBuf::from("."));
    dir.join(format!("rolidium-diagnostics-{}.zip", std::env::consts::OS))
}

fn rolidium_places_documents() -> Option<PathBuf> {
    let home = crate::browser::places::home_dir()?;
    let docs = home.join("Documents");
    Some(if docs.is_dir() { docs } else { home })
}

/// Opens a folder in the file manager.
pub fn open_folder(dir: &Path) {
    open_path(dir);
}

/// Opens a folder, a file or a web address with the system's default program.
pub fn open_path(target: &Path) {
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(target).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_data_is_replaced() {
        let personal = vec![
            ("C:/Users/alice".to_string(), "~"),
            ("alice".to_string(), "<user>"),
            ("DESKTOP-ABC".to_string(), "<computer>"),
        ];
        let text = "opened C:/Users/alice/Documents/a.rolidium by alice on DESKTOP-ABC";
        let clean = sanitize_with(text, &personal);
        assert_eq!(clean, "opened ~/Documents/a.rolidium by <user> on <computer>");
        assert!(!clean.contains("alice") && !clean.contains("DESKTOP"));
    }

    #[test]
    fn names_of_model_files_are_masked() {
        let t = "opened ~/models/Secret Machine.rolidium ok\nexport \"x/y/part.STEP\" done\nsaved a.stl.\nnothing here";
        let m = mask_model_files(t);
        assert!(!m.contains("part") && !m.contains("a.stl") && !m.contains("Machine.rolidium"));
        assert!(m.contains("nothing here") && m.contains("ok\n") && m.contains("<model file>"));
    }

    fn fake_outcomes() -> Vec<BackendOutcome> {
        vec![
            BackendOutcome {
                backend: "vulkan".into(),
                ending: Ending::Crashed(Some(0xc0000005u32 as i32)),
                adapters: vec![AdapterOutcome {
                    label: "vulkan-0".into(),
                    description: "name: Test GPU\n".into(),
                    result: Err("the test process ended while this adapter was being used".into()),
                    errors: vec![],
                    png: None,
                }],
            },
            BackendOutcome {
                backend: "gl".into(),
                ending: Ending::Finished,
                adapters: vec![AdapterOutcome {
                    label: "gl-0".into(),
                    description: "name: Other GPU\n".into(),
                    result: Ok("all present".into()),
                    errors: vec!["a warning".into()],
                    png: Some(vec![1, 2, 3]),
                }],
            },
            BackendOutcome {
                backend: "dx12".into(),
                ending: Ending::TimedOut,
                adapters: vec![],
            },
        ]
    }

    #[test]
    fn a_crashed_backend_is_a_result_and_the_next_one_is_still_reported() {
        let dir = std::env::temp_dir().join(format!("rolidium-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.zip");
        write_zip_with(
            &path,
            Some("test adapter"),
            "auto",
            "theme=dark\n",
            &fake_outcomes(),
        )
        .unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len())
            .map(|i| {
                z.by_index(i)
                    .unwrap()
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            })
            .collect();
        for n in ["report.txt", "log.txt", "scenes.txt", "scene-gl-0.png"] {
            assert!(names.contains(&n.to_string()), "{n} missing in {names:?}");
        }
        let mut all = String::new();
        for n in ["report.txt", "log.txt", "scenes.txt"] {
            use std::io::Read;
            z.by_name(n).unwrap().read_to_string(&mut all).unwrap();
        }
        assert!(
            all.contains("backend vulkan: CRASHED") && all.contains("0xc0000005"),
            "{all}"
        );
        assert!(all.contains("backend gl: finished") && all.contains("Other GPU"));
        assert!(all.contains("backend dx12: TIMED OUT"));
        assert!(all.contains("ended while this adapter was being used"));
        if let Ok(home) = std::env::var("HOME") {
            assert!(
                !all.contains(&home),
                "the home folder leaked into the diagnostics"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_a_child_left_behind_is_read_back_even_when_it_died_midway() {
        let d = std::env::temp_dir().join(format!("rolidium-child-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("count.txt"), "2").unwrap();
        std::fs::write(d.join("0.label.txt"), "gl-0").unwrap();
        std::fs::write(d.join("0.desc.txt"), "name: A\n").unwrap();
        std::fs::write(d.join("0.result.txt"), "ok\nall present").unwrap();
        std::fs::write(d.join("0.errors.txt"), "").unwrap();
        std::fs::write(d.join("1.label.txt"), "gl-1").unwrap();
        std::fs::write(d.join("1.desc.txt"), "name: B\n").unwrap(); // died before the result
        let got = read_child_dir(&d);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].result, Ok("all present".to_string()));
        assert!(got[1].result.is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_child_that_cannot_start_is_reported_and_does_not_stop_anything() {
        let o = run_child(
            Path::new("/nonexistent/rolidium"),
            "gl",
            std::time::Duration::from_secs(5),
        );
        assert!(matches!(o.ending, Ending::CannotStart(_)));
    }
}
