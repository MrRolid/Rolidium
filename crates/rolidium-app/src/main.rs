// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![recursion_limit = "256"]
#![deny(unsafe_code)]
// On Windows the program is a window program (no console box on a double click); the command-line modes attach to the console they were started from.
#![cfg_attr(windows, windows_subsystem = "windows")]
#[macro_use]
mod prof;
mod app;
mod assets_index;
mod bench;
mod browser;
mod diagnose;
mod dirs;
mod exportdialog;
mod filedialog;
mod graphics;
mod guide;
mod i18n;
mod icons;
mod logging;
mod memwatch;
mod session;
mod settings;
mod sketchguides;
mod snapcurves;
mod sketchmode;
mod sound;
mod startup;
mod support;
mod theme;
mod viewport;
mod winconsole;
#[cfg(windows)]
mod winsys;
mod worker;

/// Where bug reports and ideas go.
pub const ISSUES_URL: &str = "https://github.com/MrRolid/Rolidium/issues";
/// Where the source code (and so the licences of the parts) can be found.
pub const SOURCE_URL: &str = "https://github.com/MrRolid/Rolidium";

/// The file with the licences of the libraries, the geometry kernel and the fonts: next to the program (the tar.gz and the Windows zip) or
/// in `../share/doc/rolidium` (the .deb installs it to `/usr/share/doc/rolidium`, the AppImage has the same folders under `usr/`).
pub fn third_party_licenses_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.to_path_buf(),
        dir.join(".."),
        dir.join("../share/doc/rolidium"),
    ]
    .iter()
    .map(|d| d.join("THIRD-PARTY-LICENSES.txt"))
    .find(|p| p.is_file())
}

/// "0.1.1 a1b2c3d": version and build hash.
pub fn version_string() -> String {
    format!(
        "{} {}",
        env!("ROLIDIUM_VERSION_LABEL"),
        env!("ROLIDIUM_BUILD_HASH")
    )
}

/// `--window-size 1280x720` (for tests of the layout); the default is 1280 x 800.
fn window_size(args: &[String]) -> [f32; 2] {
    args.iter()
        .position(|a| a == "--window-size")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.split_once('x'))
        .and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))
        .unwrap_or([1280.0, 800.0])
}

/// One program, three modes: the window (default), `rolidium cli ...` (scripts, benchmarks) and `rolidium mcp ...` (the MCP server).
/// A copy or a link named `rolidium-cli` or `rolidium-mcp` behaves like the mode of that name. These modes must not start the window,
/// the logger or anything else that could write to the standard output, which the MCP protocol owns.
fn command_line_mode(args: &[String]) -> Option<i32> {
    let program = std::env::args()
        .next()
        .map(|p| {
            std::path::Path::new(&p)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    // internal: the diagnostics run each graphics backend in a child process of this kind (see diagnose.rs)
    if args.first().map(String::as_str) == Some("--diag-child") && args.len() == 3 {
        return Some(diagnose::child_main(&args[1], std::path::Path::new(&args[2])));
    }
    let (mode, rest): (&str, Vec<String>) = match program.as_str() {
        "rolidium-cli" => ("cli", args.to_vec()),
        "rolidium-mcp" => ("mcp", args.to_vec()),
        _ => match args.first().map(String::as_str) {
            Some(m @ ("cli" | "mcp" | "mcp-config")) => (m, args[1..].to_vec()),
            _ => return None,
        },
    };
    winconsole::attach();
    Some(match mode {
        "cli" => rolidium_cli::entry::cli(rest),
        "mcp" => rolidium_cli::entry::mcp(rest),
        _ => rolidium_cli::entry::mcp_config(rest),
    })
}

fn main() -> eframe::Result {
    let started = std::time::Instant::now();
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if let Some(code) = command_line_mode(&args) {
            std::process::exit(code);
        }
    }
    logging::init();
    if let Some(path) = logging::log_path() {
        winconsole::install_crash_log(path);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let selftest = args
        .iter()
        .position(|a| a == "--selftest")
        .and_then(|i| args.get(i + 1))
        .cloned();
    // options that take a value: the value is not a document name
    let valued = ["--selftest", "--backend", "--diagnose", "--window-size"];
    let bench_viewport = args.iter().any(|a| a == "--bench-viewport");
    let demo = args.iter().any(|a| a == "--demo");
    // a document given on the command line: the first argument that is not an option (or the value of --selftest)
    let open_path: Option<std::path::PathBuf> = args
        .iter()
        .enumerate()
        .find(|(i, a)| !a.starts_with('-') && !(*i > 0 && valued.contains(&args[*i - 1].as_str())))
        .map(|(_, a)| a.into());
    let bench_run = args.iter().any(|a| a == "--bench");
    app::BENCH_RUN.store(bench_run, std::sync::atomic::Ordering::Relaxed);
    if bench_run && open_path.is_none() {
        eprintln!("--bench needs a file: rolidium --bench model.rolidium");
        std::process::exit(2);
    }
    let exit_after_first_frame = args.iter().any(|a| a == "--measure-start") || bench_run;
    if args
        .iter()
        .any(|a| a == "--version" || a == "-V" || a == "--help" || a == "-h" || a == "--diagnose")
    {
        winconsole::attach();
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("rolidium {}", version_string());
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "rolidium - direct modeling CAD (Alpha - developer version)\n\n\
             USAGE: rolidium [file.rolidium] [--demo] [--version] [--selftest <dir>] [--measure-start] [--bench-viewport]\n\
             rolidium cli ...       the command-line tool (rolidium cli help)\n\
             rolidium mcp --root <dir>   the MCP server for AI assistants (docs/MCP.md)\n\
             rolidium mcp-config    prints ready-made commands that connect the MCP server to a client\n\n\
             file.rolidium      open this document at start (the program starts with an empty document otherwise)\n\
             --demo             start with a 40 x 30 x 20 mm box\n\
             \
             --backend <b>      graphics backend: auto, dx12, vulkan or gl (default: the setting, else auto)\n\
             --diagnose [file]  save the graphics diagnostics (adapters, log, test pictures) as a zip and exit\n\
             --window-size WxH  size of the window at start, e.g. 1280x720\n\
             --selftest <dir>   drive the main tools automatically, save screenshots into <dir>, exit 0 on success\n\
             --measure-start    print the time to the first rendered frame and exit\n\
             --bench <file>     open the file, time the first frame with content, turn the view for 5 s and time the frames; print the\n\
                                result with the graphics adapter and the backend, write it to rolidium.log and exit\n\
             --bench-viewport   render 1 000 000 triangles for 10 s with vsync off, print and save rolidium-bench.txt"
        );
        return Ok(());
    }
    logging::log_sync(&format!(
        "starting: Rolidium {} on {}",
        version_string(),
        diagnose::system_version()
    ));
    let mut settings = settings::Settings::load();
    i18n::set_language(&settings.language);
    let backend = graphics::chosen_backend(&args, &settings.backend);
    if let Some(i) = args.iter().position(|a| a == "--diagnose") {
        let path = args
            .get(i + 1)
            .filter(|a| !a.starts_with('-'))
            .map_or_else(diagnose::default_path, std::path::PathBuf::from);
        return match diagnose::write_zip(
            &path,
            None,
            backend.name(),
            &settings::Settings::load().diagnostic_text(),
            &diagnose::summary_text(
                &diagnose::SummaryInput {
                    backend_setting: settings.backend.clone(),
                    forced_by: graphics::forced_by(&args, &settings.backend),
                    blocked: settings
                        .backend_blocked
                        .iter()
                        .map(|b| b.backend.clone())
                        .collect(),
                    threads_setting: settings.threads,
                    threads_used: rolidium_kernel::par::resolve(settings.threads as usize),
                    language: settings.language.clone(),
                    ..Default::default()
                },
                &diagnose::machine(),
            ),
        ) {
            Ok(()) => {
                println!("diagnostics written to {}", path.display());
                Ok(())
            }
            Err(e) => {
                eprintln!("cannot write the diagnostics: {e}");
                std::process::exit(1);
            }
        };
    }
    logging::log(&format!("graphics backend setting: {}", backend.name()));
    // --- the safe start of the graphics (see startup.rs) ---
    // ROLIDIUM_TEST_ORDER=windows: the Windows order of backends on any system (used to test the safeguard)
    let windows = cfg!(windows) || std::env::var("ROLIDIUM_TEST_ORDER").is_ok_and(|v| v == "windows");
    if backend == rolidium_render::BackendChoice::Auto
        && graphics::backend_given(&args)
        && !settings.backend_blocked.is_empty()
    {
        logging::log("--backend auto: the remembered skipped backends are forgotten");
        settings.backend_blocked.clear();
        settings.save();
    }
    let state_dir = logging::state_dir();
    let stale = state_dir
        .as_deref()
        .map(|d| startup::scan_stale(d, env!("CARGO_PKG_VERSION")))
        .unwrap_or_default();
    for m in &stale {
        logging::log_sync(&format!(
            "an earlier start with backend {} did not get through (adapter '{}', driver '{}')",
            m.backend, m.adapter, m.driver
        ));
    }
    let mut failed: Vec<String> = std::env::var(startup::FAILED_ENV)
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if backend == rolidium_render::BackendChoice::Auto {
        let new = startup::register_failures(windows, &mut settings.backend_blocked, &stale);
        if !new.is_empty() {
            settings.save();
            failed.extend(new);
        }
    }
    let steps = startup::plan(windows, backend, &settings.backend_blocked);
    logging::log_sync(&format!(
        "graphics: order of backends: {}",
        steps
            .iter()
            .map(|s| format!(
                "{}{}",
                s.backend.name(),
                if s.hardware_only { " (card)" } else { "" }
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let Some((step, found)) = startup::select(&steps, |s| startup::probe(s, state_dir.as_deref())) else {
        startup::finish();
        let text = startup::nothing_works_text(logging::log_path().as_deref(), &settings.backend_blocked);
        logging::log_sync("graphics: no backend worked, showing the message");
        winconsole::message_box(&startup::window_title(), &text);
        std::process::exit(1);
    };
    if windows {
        startup::set_notice(failed.clone(), step.backend.name().to_string());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rolidium - direct modeling CAD (Alpha - developer version)")
            .with_app_id("rolidium")
            .with_icon(std::sync::Arc::new(eframe::egui::IconData {
                rgba: include_bytes!("../../../assets/appicon/rolidium-128.rgba").to_vec(),
                width: 128,
                height: 128,
            }))
            .with_inner_size(window_size(&args))
            .with_min_inner_size([640.0, 400.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let mut options = options;
    options.wgpu_options.wgpu_setup = graphics::wgpu_setup(step);
    let cap = settings::Settings::load().fps_cap;
    let sync = settings::synchronised(&cap) && !bench_viewport;
    options.wgpu_options.surface.present_mode = if sync {
        eframe::wgpu::PresentMode::Fifo
    } else {
        eframe::wgpu::PresentMode::AutoNoVsync
    };
    let _ = settings::PRESENT.set(
        if sync {
            "Fifo (every frame waits for the monitor)"
        } else {
            "AutoNoVsync (Immediate or Mailbox: no waiting for the monitor)"
        }
        .to_string(),
    );
    let result = eframe::run_native(
        "rolidium",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                cc,
                started,
                selftest.map(Into::into),
                exit_after_first_frame,
                bench_viewport,
                open_path,
                demo,
            )))
        }),
    );
    let drawn = startup::frames_drawn();
    startup::finish();
    let Err(e) = result else { return Ok(()) };
    logging::log_sync(&format!("window start-up error: {e}"));
    if drawn {
        return Err(e); // the window ran; this is not a start-up problem
    }
    // the window could not be created with this backend: remember it and start again with the next one
    // only a graphics error counts against the backend (a missing display or the like says nothing about it)
    let graphics_error = matches!(e, eframe::Error::Wgpu(_));
    if graphics_error
        && backend == rolidium_render::BackendChoice::Auto
        && steps.len() > 1
        && startup::restarts() < 8
    {
        startup::block_now(&mut settings, step.backend.name(), &found.adapter, &found.driver);
        failed.push(step.backend.name().to_string());
        if startup::restart(&args, &failed) {
            std::process::exit(0);
        }
    }
    let text = if graphics_error {
        format!(
            "{e}\n\n{}",
            startup::nothing_works_text(logging::log_path().as_deref(), &settings.backend_blocked)
        )
    } else {
        format!(
            "{e}\n\n{}",
            logging::log_path().map_or_else(String::new, |p| p.display().to_string())
        )
    };
    winconsole::message_box(&startup::window_title(), &text);
    std::process::exit(1);
}
