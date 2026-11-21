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
        let ver = crate::winsys::version().map_or_else(
            || "version not reported".to_string(),
            |(major, minor, build)| {
                let name = if major == 10 && build >= 22000 { 11 } else { major };
                format!("Windows {name}, version {major}.{minor}.{build}")
            },
        );
        format!("{ver}, {}", std::env::consts::ARCH)
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
    summary: &str,
) -> Result<(), String> {
    write_zip_with(
        path,
        selected,
        backend_setting,
        settings_text,
        summary,
        &run_isolated(),
    )
}

pub fn write_zip_with(
    path: &Path,
    selected: Option<&str>,
    backend_setting: &str,
    settings_text: &str,
    summary: &str,
    outcomes: &[BackendOutcome],
) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    // the readable summary is always the first file
    add("summary.txt", sanitize(summary).as_bytes())?;
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

// ---- the summary: the first file of the zip, written to be read by a person ----

/// What the graphics adapter of the window is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuFacts {
    pub name: String,
    pub vendor: String,
    pub driver: String,
    pub backend: String,
    pub software: bool,
}

/// The manufacturer for a PCI vendor number (what wgpu reports); the number itself when it is not known.
pub fn vendor_name(id: u32) -> String {
    match id {
        0x10de => "NVIDIA".into(),
        0x1002 | 0x1022 => "AMD".into(),
        0x8086 | 0x8087 => "Intel".into(),
        0x13b5 => "ARM".into(),
        0x5143 => "Qualcomm".into(),
        0x106b => "Apple".into(),
        0x1414 => "Microsoft".into(),
        0x1010 => "Imagination".into(),
        0x10005 => "Mesa".into(),
        0 => "unknown".into(),
        n => format!("unknown (vendor id {n:#06x})"),
    }
}

impl GpuFacts {
    pub fn from_info(info: &wgpu::AdapterInfo) -> Self {
        GpuFacts {
            name: info.name.clone(),
            vendor: vendor_name(info.vendor),
            driver: format!("{} {}", info.driver, info.driver_info).trim().to_string(),
            backend: backend_name(info.backend).to_string(),
            software: rolidium_render::is_software(info),
        }
    }
}

/// The name of an interface for people.
pub fn backend_title(name: &str) -> &'static str {
    match name {
        "dx12" => "DirectX 12",
        "vulkan" => "Vulkan",
        "gl" => "OpenGL",
        "metal" => "Metal",
        _ => "other",
    }
}

/// What the app knows about itself.
#[derive(Debug, Clone, Default)]
pub struct SummaryInput {
    /// `None` when there is no window (`--diagnose`).
    pub gpu: Option<GpuFacts>,
    /// The setting `backend` (`auto` or a name).
    pub backend_setting: String,
    /// Who forced the interface: "command line", "environment variable ROLIDIUM_BACKEND", "setting" or nothing (automatic).
    pub forced_by: Option<String>,
    pub blocked: Vec<String>,
    pub threads_setting: u32,
    pub threads_used: usize,
    /// Size of the window in pixels and the scale of the interface.
    pub window: Option<([u32; 2], f32)>,
    pub language: String,
}

/// The processor and the memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Machine {
    pub cpu_model: String,
    pub physical: Option<usize>,
    pub logical: usize,
    pub ram_mb: Option<u64>,
}

/// Model, number of physical and logical cores and memory of this computer. Only these facts are read: no serial numbers, no names.
pub fn machine() -> Machine {
    let logical = std::thread::available_parallelism().map_or(1, |n| n.get());
    #[cfg(windows)]
    {
        Machine {
            cpu_model: crate::winsys::cpu_model().unwrap_or_default(),
            physical: crate::winsys::physical_cores(),
            logical,
            ram_mb: crate::winsys::memory_mb(),
        }
    }
    #[cfg(not(windows))]
    {
        machine_from_linux(
            &std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default(),
            &std::fs::read_to_string("/proc/meminfo").unwrap_or_default(),
            logical,
        )
    }
}

/// The same from the text of `/proc/cpuinfo` and `/proc/meminfo`.
#[cfg_attr(windows, allow(dead_code))]
pub fn machine_from_linux(cpuinfo: &str, meminfo: &str, logical: usize) -> Machine {
    let model = cpuinfo
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == "model name").then(|| v.trim().to_string())
        })
        .unwrap_or_default();
    // physical cores: the distinct pairs (physical id, core id)
    let mut cores = std::collections::HashSet::new();
    let (mut phys, mut core) = (None::<String>, None::<String>);
    for l in cpuinfo.lines().chain(std::iter::once("")) {
        if l.trim().is_empty() {
            if let (Some(p), Some(c)) = (phys.take(), core.take()) {
                cores.insert((p, c));
            }
            phys = None;
            core = None;
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            match k.trim() {
                "physical id" => phys = Some(v.trim().to_string()),
                "core id" => core = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    let ram_mb = meminfo.lines().find_map(|l| {
        let rest = l.strip_prefix("MemTotal:")?;
        let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kb / 1024)
    });
    Machine {
        cpu_model: model,
        physical: (!cores.is_empty()).then_some(cores.len()),
        logical,
        ram_mb,
    }
}

/// The text of `summary.txt`: what a person needs first. No computer name, no user name, no serial numbers, no network addresses.
pub fn summary_text(i: &SummaryInput, m: &Machine) -> String {
    let mut t = String::new();
    t.push_str("Rolidium diagnostics: summary\n\n");
    t.push_str("== Program ==\n");
    t.push_str(&format!("Version: {}\n", crate::version_string()));
    t.push_str(&format!(
        "Interface language: {}\n\n",
        if i.language.is_empty() {
            "automatic (from the system)"
        } else {
            &i.language
        }
    ));
    t.push_str("== Graphics ==\n");
    match &i.gpu {
        Some(g) => {
            t.push_str(&format!("Graphics card: {}\n", g.name));
            t.push_str(&format!("Manufacturer: {}\n", g.vendor));
            t.push_str(&format!(
                "Driver and its version: {}\n",
                if g.driver.is_empty() {
                    "not reported"
                } else {
                    &g.driver
                }
            ));
            t.push_str(&format!("Interface used: {}\n", backend_title(&g.backend)));
            t.push_str(&format!(
                "Software rendering (no graphics card): {}\n",
                if g.software {
                    "YES, the picture is drawn by the processor"
                } else {
                    "no"
                }
            ));
        }
        None => {
            t.push_str("No window was open (run from the command line): see report.txt for the adapters.\n")
        }
    }
    t.push_str(&format!(
        "Interface chosen: {}\n",
        match &i.forced_by {
            Some(by) => format!("forced ({by}: {})", i.backend_setting),
            None => "automatically".to_string(),
        }
    ));
    t.push_str(&format!(
        "Interfaces blocked after a crash: {}\n\n",
        if i.blocked.is_empty() {
            "none".to_string()
        } else {
            i.blocked.join(", ")
        }
    ));
    t.push_str("== Computer ==\n");
    t.push_str(&format!(
        "Processor: {}\n",
        if m.cpu_model.is_empty() {
            "not reported"
        } else {
            &m.cpu_model
        }
    ));
    t.push_str(&format!(
        "Cores: {} physical, {} logical\n",
        m.physical
            .map_or("unknown number of".to_string(), |p| p.to_string()),
        m.logical
    ));
    t.push_str(&format!(
        "Threads: set to {}, in use {}\n",
        if i.threads_setting == 0 {
            "automatic".to_string()
        } else {
            i.threads_setting.to_string()
        },
        i.threads_used
    ));
    t.push_str(&format!(
        "Memory: {}\n",
        m.ram_mb.map_or("not reported".to_string(), |mb| format!(
            "{:.1} GB",
            mb as f64 / 1024.0
        ))
    ));
    t.push_str(&format!("System: {}\n", system_version()));
    t.push_str(&format!("Architecture: {}\n", std::env::consts::ARCH));
    match i.window {
        Some(([w, h], scale)) => {
            t.push_str(&format!("Window: {w} x {h} pixels, interface scale {scale:.2}\n"))
        }
        None => t.push_str("Window: none\n"),
    }
    sanitize(&t)
}

/// A zip made of the given files only (the report without diagnostics, saved to a file).
pub fn zip_of(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        out.start_file(*name, opts).map_err(|e| e.to_string())?;
        out.write_all(data).map_err(|e| e.to_string())?;
    }
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The same zip with one more file.
pub fn with_file(bytes: &[u8], name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..src.len() {
        let mut f = src.by_index(i).map_err(|e| e.to_string())?;
        let n = f.name().map(|n| n.to_string()).map_err(|e| e.to_string())?;
        let mut d = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut d).map_err(|e| e.to_string())?;
        out.start_file(n, opts).map_err(|e| e.to_string())?;
        out.write_all(&d).map_err(|e| e.to_string())?;
    }
    out.start_file(name, opts).map_err(|e| e.to_string())?;
    out.write_all(data).map_err(|e| e.to_string())?;
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The same zip without the named files (the ones the person switched off). `summary.txt` is never left out.
pub fn without_files(bytes: &[u8], skip: &[String]) -> Result<Vec<u8>, String> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..src.len() {
        let mut f = src.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map(|n| n.to_string()).map_err(|e| e.to_string())?;
        if name != "summary.txt" && skip.contains(&name) {
            continue;
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).map_err(|e| e.to_string())?;
        out.start_file(name, opts).map_err(|e| e.to_string())?;
        out.write_all(&data).map_err(|e| e.to_string())?;
    }
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
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

    fn sample_input() -> SummaryInput {
        SummaryInput {
            gpu: Some(GpuFacts {
                name: "AMD Radeon RX 580".into(),
                vendor: "AMD".into(),
                driver: "AMD proprietary 24.1".into(),
                backend: "dx12".into(),
                software: false,
            }),
            backend_setting: "auto".into(),
            forced_by: None,
            blocked: vec!["vulkan".into()],
            threads_setting: 0,
            threads_used: 8,
            window: Some(([1280, 720], 1.0)),
            language: "cs".into(),
        }
    }

    #[test]
    fn the_summary_names_the_graphics_and_the_processor_and_nothing_personal() {
        let m = machine_from_linux(
            "processor : 0\nmodel name\t: Test CPU 3000\nphysical id\t: 0\ncore id\t: 0\n\nprocessor : 1\nmodel name\t: Test CPU 3000\nphysical id\t: 0\ncore id\t: 0\n\nprocessor : 2\nmodel name\t: Test CPU 3000\nphysical id\t: 0\ncore id\t: 1\n",
            "MemTotal:       16384000 kB\n",
            3,
        );
        assert_eq!(m.cpu_model, "Test CPU 3000");
        assert_eq!((m.physical, m.logical, m.ram_mb), (Some(2), 3, Some(16000)));
        let t = summary_text(&sample_input(), &m);
        for want in [
            "Graphics card: AMD Radeon RX 580",
            "Manufacturer: AMD",
            "Driver and its version: AMD proprietary 24.1",
            "Interface used: DirectX 12",
            "Software rendering (no graphics card): no",
            "Interface chosen: automatically",
            "blocked after a crash: vulkan",
            "Processor: Test CPU 3000",
            "2 physical, 3 logical",
            "set to automatic, in use 8",
            "Memory: 15.6 GB",
            "Architecture:",
            "Window: 1280 x 720 pixels, interface scale 1.00",
            "Interface language: cs",
            "Version: ",
        ] {
            assert!(t.contains(want), "{want} missing in\n{t}");
        }
        // software rendering and a forced interface are said plainly
        let mut i = sample_input();
        i.gpu.as_mut().unwrap().software = true;
        i.forced_by = Some("setting".into());
        i.backend_setting = "gl".into();
        let t = summary_text(&i, &m);
        assert!(t.contains("YES, the picture is drawn by the processor"));
        assert!(t.contains("forced (setting: gl)"));
        // nothing that identifies the person or the computer
        for (what, _) in personal_strings() {
            assert!(!t.contains(&what) || what == "~", "'{what}' is in the summary");
        }
        let lower = t.to_lowercase();
        for banned in ["serial", "mac address", "hostname", "username", "ip address"] {
            assert!(!lower.contains(banned), "{banned}");
        }
    }

    #[test]
    fn the_real_machine_is_described() {
        let m = machine();
        assert!(m.logical >= 1);
        let t = summary_text(&sample_input(), &m);
        assert!(t.contains("== Graphics ==") && t.contains("== Computer =="));
        if cfg!(target_os = "linux") {
            assert!(!m.cpu_model.is_empty() && m.ram_mb.is_some());
        }
    }

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
            "summary text\n",
            &fake_outcomes(),
        )
        .unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            z.by_index(0).unwrap().name().unwrap(),
            "summary.txt",
            "the summary is the first file"
        );
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
