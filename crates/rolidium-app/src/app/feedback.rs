// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Report a bug or an idea": a folded row at the bottom of the right column that opens a panel. The person chooses Bug or Idea, writes up to
//! 1000 characters, may leave an e-mail, may attach the diagnostics and a picture of the window. Nothing is sent before the person presses
//! Send ("Show what is sent" lists every file of the zip, each with a check box). When the server does not answer (it may not exist yet) the
//! text stays and the report can be saved to a file (a zip with the text as a file of its own) and sent by e-mail.
//!
//! The picture of the window (`window.png`) is taken when the person clicks the folded row, before the panel opens: the program asks eframe for
//! a screenshot of its own window (`ViewportCommand::Screenshot`, the rendered frame is read back from the graphics card by eframe), never the
//! screen through the system. It stays in memory and is written to a disk only inside the zip (or the saved report).

use super::*;
use rolidium_net::Kind;
use std::sync::{Arc, Mutex};

type Slot<T> = Arc<Mutex<Option<T>>>;

/// The longest side of the picture of the window that is sent, in pixels.
pub(super) const WINDOW_SHOT_MAX_SIDE: u32 = 1600;
/// The smaller sizes tried when the whole zip does not fit into [`rolidium_net::MAX_ZIP`].
const SHRINK_STEPS: [u32; 5] = [1200, 900, 700, 500, 350];

/// The diagnostics without the graphics test: the zip (in memory) and the files in it with their sizes.
pub(super) struct Prepared {
    bytes: Vec<u8>,
    files: Vec<(String, u64)>,
}

/// What a background job made: the diagnostics, the graphics test, or both.
pub(super) struct Made {
    light: Option<Vec<u8>>,
    graphics: Option<Vec<(String, Vec<u8>)>>,
}

/// What to do when the zip is ready.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Then {
    Preview,
    Send,
    Save,
}

#[derive(Default)]
pub(super) enum Stage {
    #[default]
    Idle,
    Preparing(Slot<Result<Made, String>>, Then),
    Sending(Slot<Result<String, String>>),
    /// The number of the report (the panel is folded and the text is gone).
    Sent(String),
    /// The reason why the sending failed (the text stays).
    Failed(String),
}

/// The picture of the window: the pixels (at most 1600 on the longer side) and the PNG made of them. Only in memory.
pub(super) struct WindowShot {
    pub(super) rgba: Vec<u8>,
    pub(super) w: u32,
    pub(super) h: u32,
    pub(super) png: Vec<u8>,
    tex: Option<egui::TextureHandle>,
}

impl WindowShot {
    pub(super) fn from_rgba(rgba: Vec<u8>, w: u32, h: u32) -> Result<Self, String> {
        let (rgba, w, h) = crate::diagnose::shrink_rgba(&rgba, w, h, WINDOW_SHOT_MAX_SIDE);
        let png = rolidium_render::encode_png(&rgba, w, h).map_err(|e| e.to_string())?;
        Ok(WindowShot {
            rgba,
            w,
            h,
            png,
            tex: None,
        })
    }

    /// The PNG with the longer side at most `max_side`.
    fn png_at(&self, max_side: u32) -> Result<Vec<u8>, String> {
        if self.w.max(self.h) <= max_side {
            return Ok(self.png.clone());
        }
        let (px, w, h) = crate::diagnose::shrink_rgba(&self.rgba, self.w, self.h, max_side);
        rolidium_render::encode_png(&px, w, h).map_err(|e| e.to_string())
    }
}

/// Where the taking of the picture is.
#[derive(Default)]
pub(super) enum ShotState {
    #[default]
    Idle,
    /// Asked for in the last frame; the screenshot is requested in the next one. `expand`: open the panel when the picture is there.
    Request { expand: bool },
    Waiting {
        since: std::time::Instant,
        expand: bool,
        slot: Slot<Arc<egui::ColorImage>>,
    },
}

pub(super) struct Feedback {
    pub(super) expanded: bool,
    pub(super) kind: Kind,
    pub(super) text: String,
    pub(super) contact: String,
    /// Attach the diagnostics.
    pub(super) attach: bool,
    /// Attach the picture of the window (it is only sent when there is one).
    pub(super) attach_shot: bool,
    /// The graphics test (`scenes.txt` and `scene-*.png`): off, unless there was a crash, a backend is blocked or the drawing is by software.
    pub(super) graphics_test: bool,
    /// The program ended with a crash the last time (the graphics test is on then).
    crashed: bool,
    pub(super) stage: Stage,
    prepared: Option<Prepared>,
    graphics: Option<Vec<(String, Vec<u8>)>>,
    pub(super) shot: Option<WindowShot>,
    pub(super) shot_state: ShotState,
    /// The height of the panel when the last picture arrived (for the self test: the picture must show the folded panel).
    pub(super) shot_panel_h: f32,
    pub(super) big_open: bool,
    pub(super) preview_open: bool,
    /// Where the panel was drawn in the last frame, and the height of the column it sits in (for the self test).
    pub(super) rect: Rect,
    pub(super) column_h: f32,
    /// The height of the fields of the last frame.
    fields_h: f32,
    /// A saved file (shown as a line under the buttons).
    pub(super) saved: Option<PathBuf>,
    /// Where "Save to a file" writes (the Documents folder when not set; the self test sets it).
    pub(super) save_dir: Option<PathBuf>,
}

impl Default for Feedback {
    fn default() -> Self {
        Feedback {
            expanded: false,
            kind: Kind::Bug,
            text: String::new(),
            contact: String::new(),
            attach: true,
            attach_shot: true,
            graphics_test: false,
            crashed: false,
            stage: Stage::Idle,
            prepared: None,
            graphics: None,
            shot: None,
            shot_state: ShotState::Idle,
            shot_panel_h: 0.0,
            big_open: false,
            preview_open: false,
            rect: Rect::NOTHING,
            column_h: 0.0,
            fields_h: 200.0,
            saved: None,
            save_dir: None,
        }
    }
}

impl Feedback {
    /// After a crash: the panel is open with a note that the program ended badly the last time. There is no picture of a window (the program
    /// was not running), and the graphics test is on. Sending still waits for the person.
    pub(super) fn open_after_crash(&mut self) {
        self.expanded = true;
        self.kind = Kind::Bug;
        self.attach = true;
        self.shot = None;
        self.crashed = true;
        self.graphics_test = true;
        if self.text.is_empty() {
            self.text = tr("The program ended with a crash the last time.");
        }
    }

    /// Whether the picture of the window goes with the report.
    pub(super) fn shot_on(&self) -> bool {
        self.attach_shot && self.shot.is_some()
    }

    /// Whether Send can be pressed: an idea needs text, a bug needs text, the diagnostics or the picture.
    pub(super) fn can_send(&self) -> bool {
        !self.text.trim().is_empty() || (self.kind == Kind::Bug && (self.attach || self.shot_on()))
    }

    /// The diagnostics and the picture are attached by default to a bug and not to an idea (the person can change it).
    pub(super) fn set_kind(&mut self, kind: Kind) {
        if self.kind != kind {
            self.kind = kind;
            self.attach = kind == Kind::Bug;
            self.attach_shot = kind == Kind::Bug;
            self.forget_zip();
        }
    }

    /// Cuts the text to the limit (a pasted text may be longer than the field takes).
    pub(super) fn clamp_text(&mut self) {
        if self.text.chars().count() > rolidium_net::MAX_NOTE_CHARS {
            self.text = self.text.chars().take(rolidium_net::MAX_NOTE_CHARS).collect();
        }
    }

    /// The diagnostics without the graphics test are made (the self test also uses it to have them "ready" without starting anything).
    pub(super) fn set_light(&mut self, bytes: Vec<u8>) {
        let files = list_zip(&bytes).unwrap_or_default();
        self.prepared = Some(Prepared { bytes, files });
    }

    /// For the self test: the result of the graphics test without running it.
    pub(super) fn set_graphics_for_test(&mut self, files: Vec<(String, Vec<u8>)>) {
        self.graphics = Some(files);
    }

    fn forget_zip(&mut self) {
        self.prepared = None;
        self.preview_open = false;
    }

    /// The report was sent: the number is shown, the text is gone, the panel is folded, the picture and the test are forgotten.
    pub(super) fn finish_sent(&mut self, id: String) {
        self.text.clear();
        self.expanded = false;
        self.forget_zip();
        self.graphics = None;
        self.shot = None;
        self.big_open = false;
        self.crashed = false;
        self.graphics_test = false;
        self.stage = Stage::Sent(id);
    }

    /// The answer of the sending: the report number (the panel is folded, the text is gone) or the reason of the failure (the text stays).
    pub(super) fn apply_send_result(&mut self, r: Result<String, String>) -> Option<String> {
        match r {
            Ok(id) => {
                self.finish_sent(id.clone());
                Some(id)
            }
            Err(e) => {
                self.stage = Stage::Failed(e);
                None
            }
        }
    }

    /// The text of the file `message.txt` of a saved report.
    pub(super) fn message_text(&self) -> String {
        let mut t = format!(
            "Rolidium report ({})\nVersion: {}\n\n{}\n",
            self.kind.as_str(),
            crate::version_string(),
            self.text.trim()
        );
        if !self.contact.trim().is_empty() {
            t.push_str(&format!("\nContact: {}\n", self.contact.trim()));
        }
        crate::diagnose::sanitize(&t)
    }
}

/// Whether the graphics test is on by itself: after a crash, when a graphics interface is blocked after a crash, and with software drawing.
pub(super) fn graphics_test_on(crashed: bool, blocked: bool, software: bool) -> bool {
    crashed || blocked || software
}

/// The files of a zip with their sizes.
pub(super) fn list_zip(bytes: &[u8]) -> Result<Vec<(String, u64)>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut v = Vec::new();
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(|e| e.to_string())?;
        v.push((f.name().map(|n| n.to_string()).unwrap_or_default(), f.size()));
    }
    Ok(v)
}

/// All files of a zip with their contents.
fn read_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut v = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map(|n| n.to_string()).unwrap_or_default();
        let mut d = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut d).map_err(|e| e.to_string())?;
        v.push((name, d));
    }
    Ok(v)
}

/// Puts the zip of a report together from what is switched on: the diagnostics (`light`, without the files in `skip`; `summary.txt` always stays),
/// the graphics test, and the picture of the window (`window.png`, right after the summary). `None` when there is nothing to send. The zip must be
/// at most [`rolidium_net::MAX_ZIP`] bytes: when it is larger the pictures of the graphics test go first, then the picture of the window gets
/// smaller step by step. `summary.txt` is never left out.
pub(super) fn assemble_zip(
    light: Option<&[u8]>,
    skip: &[String],
    graphics: Option<&[(String, Vec<u8>)]>,
    window: Option<&WindowShot>,
) -> Result<Option<Vec<u8>>, String> {
    let mut text_files: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(l) = light {
        for (n, d) in read_zip(l)? {
            if n == "summary.txt" || !skip.contains(&n) {
                text_files.push((n, d));
            }
        }
    }
    let pictures: Vec<(String, Vec<u8>)> = graphics.map(|g| g.to_vec()).unwrap_or_default();
    if text_files.is_empty() && pictures.is_empty() && window.is_none() {
        return Ok(None);
    }
    let write = |drop_scene_pictures: bool, win: Option<Vec<u8>>| -> Result<Vec<u8>, String> {
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let deflate =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut put = |name: &str, d: &[u8], o| -> Result<(), String> {
            out.start_file(name, o).map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut out, d).map_err(|e| e.to_string())
        };
        let mut window_done = win.is_none();
        for (i, (n, d)) in text_files.iter().enumerate() {
            put(n, d, deflate)?;
            if i == 0 && !window_done {
                put("window.png", win.as_deref().unwrap_or(&[]), stored)?;
                window_done = true;
            }
        }
        if !window_done {
            put("window.png", win.as_deref().unwrap_or(&[]), stored)?;
        }
        for (n, d) in &pictures {
            if drop_scene_pictures && n.ends_with(".png") {
                continue;
            }
            put(n, d, if n.ends_with(".png") { stored } else { deflate })?;
        }
        Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
    };
    let mut bytes = write(false, window.map(|w| w.png.clone()))?;
    if bytes.len() > rolidium_net::MAX_ZIP && pictures.iter().any(|(n, _)| n.ends_with(".png")) {
        bytes = write(true, window.map(|w| w.png.clone()))?;
    }
    if let Some(w) = window {
        for side in SHRINK_STEPS {
            if bytes.len() <= rolidium_net::MAX_ZIP {
                break;
            }
            bytes = write(true, Some(w.png_at(side)?))?;
        }
    }
    if bytes.len() > rolidium_net::MAX_ZIP {
        return Err("the report is larger than 2 MB".into());
    }
    Ok(Some(bytes))
}

/// Makes what is not made yet, in a background thread: the diagnostics without the graphics test, and/or the graphics test (every backend in a
/// process of its own).
fn prepare(
    desc: String,
    backend: String,
    settings_text: String,
    summary: String,
    light: bool,
    graphics: bool,
) -> Result<Made, String> {
    let light = if light {
        Some(crate::diagnose::light_zip(
            Some(&desc),
            &backend,
            &settings_text,
            &summary,
        )?)
    } else {
        None
    };
    let graphics = graphics.then(|| crate::diagnose::graphics_files(&crate::diagnose::run_isolated()));
    Ok(Made { light, graphics })
}

/// The RGBA bytes of a screenshot of eframe (opaque).
fn rgba_of(img: &egui::ColorImage) -> Vec<u8> {
    let mut v = Vec::with_capacity(img.pixels.len() * 4);
    for p in &img.pixels {
        v.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    v
}

impl App {
    /// The zip as it goes out (see [`assemble_zip`]).
    pub(super) fn outgoing_zip(&self) -> Result<Option<Vec<u8>>, String> {
        let fb = &self.feedback;
        assemble_zip(
            fb.attach
                .then(|| fb.prepared.as_ref().map(|p| p.bytes.as_slice()))
                .flatten(),
            &self.settings.report_skip,
            (fb.attach && fb.graphics_test)
                .then_some(fb.graphics.as_deref())
                .flatten(),
            fb.shot.as_ref().filter(|_| fb.attach_shot),
        )
    }

    /// What the app knows about itself, for the first file of the diagnostics.
    pub(super) fn summary_input(&self, ctx: &egui::Context) -> crate::diagnose::SummaryInput {
        let args: Vec<String> = std::env::args().collect();
        let screen = ctx.content_rect();
        let ppp = ctx.pixels_per_point();
        crate::diagnose::SummaryInput {
            gpu: Some(self.gpu.clone()),
            backend_setting: self.settings.backend.clone(),
            forced_by: crate::graphics::forced_by(&args, &self.settings.backend),
            blocked: self
                .settings
                .backend_blocked
                .iter()
                .map(|b| b.backend.clone())
                .collect(),
            threads_setting: self.settings.threads,
            threads_used: rolidium_kernel::par::threads(),
            window: Some((
                [(screen.width() * ppp) as u32, (screen.height() * ppp) as u32],
                ppp,
            )),
            language: if self.settings.language.is_empty() {
                format!("{} (from the system)", crate::i18n::system_language())
            } else {
                self.settings.language.clone()
            },
        }
    }

    /// The summary text (the first file of the zip).
    pub(super) fn summary_for(&self, ctx: &egui::Context) -> String {
        crate::diagnose::summary_text(&self.summary_input(ctx), &crate::diagnose::machine())
    }

    /// The graphics test is on by itself after a crash, when a graphics interface is blocked after a crash, and when the drawing is by software.
    pub(super) fn graphics_test_default(&self) -> bool {
        graphics_test_on(
            self.feedback.crashed,
            !self.settings.backend_blocked.is_empty(),
            self.gpu.software,
        )
    }

    /// Whether something that is switched on is not made yet.
    fn needs_light(&self) -> bool {
        self.feedback.attach && self.feedback.prepared.is_none()
    }

    fn needs_graphics(&self) -> bool {
        self.feedback.attach && self.feedback.graphics_test && self.feedback.graphics.is_none()
    }

    /// Starts making what is missing (the next step runs when it is ready).
    pub(super) fn prepare_zip(&mut self, ctx: &egui::Context, then: Then) {
        let slot: Slot<Result<Made, String>> = Arc::new(Mutex::new(None));
        let (s2, c) = (slot.clone(), ctx.clone());
        let (desc, backend, stext, summary) = (
            self.adapter_description.clone(),
            self.settings.backend.clone(),
            self.settings.diagnostic_text(),
            self.summary_for(ctx),
        );
        let (light, graphics) = (self.needs_light(), self.needs_graphics());
        std::thread::spawn(move || {
            let r = prepare(desc, backend, stext, summary, light, graphics);
            if let Ok(mut s) = s2.lock() {
                *s = Some(r);
            }
            c.request_repaint();
        });
        self.feedback.stage = Stage::Preparing(slot, then);
    }

    /// Does `then` now, or makes first what is missing.
    pub(super) fn ready_then(&mut self, ctx: &egui::Context, then: Then) {
        if self.needs_light() || self.needs_graphics() {
            self.prepare_zip(ctx, then);
            return;
        }
        match then {
            Then::Preview => self.feedback.preview_open = true,
            Then::Send => self.start_send(ctx),
            Then::Save => self.save_report_zip(),
        }
    }

    /// Sends the report in the background. The zip is the one the person could look at.
    pub(super) fn start_send(&mut self, ctx: &egui::Context) {
        let zip = match self.outgoing_zip() {
            Ok(z) => z,
            Err(e) => {
                log(&format!("report not made: {e}"));
                self.feedback.stage = Stage::Failed(e);
                return;
            }
        };
        let slot: Slot<Result<String, String>> = Arc::new(Mutex::new(None));
        let (s2, c) = (slot.clone(), ctx.clone());
        let fb = &self.feedback;
        let (kind, note, contact) = (fb.kind, fb.text.clone(), fb.contact.clone());
        std::thread::spawn(move || {
            let res =
                rolidium_net::send_report(kind, zip.as_deref(), &note, &contact, env!("CARGO_PKG_VERSION"));
            if let Ok(mut s) = s2.lock() {
                *s = Some(res);
            }
            c.request_repaint();
        });
        self.feedback.stage = Stage::Sending(slot);
    }

    /// The Send button: makes what is switched on and not made yet, then sends.
    pub(super) fn press_send(&mut self, ctx: &egui::Context) {
        if !self.feedback.can_send()
            || matches!(self.feedback.stage, Stage::Preparing(..) | Stage::Sending(_))
        {
            return;
        }
        self.ready_then(ctx, Then::Send);
    }

    /// The report as a zip in the Documents folder: what is attached and the text as a file of its own.
    pub(super) fn press_save(&mut self, ctx: &egui::Context) {
        if !self.feedback.can_send()
            || matches!(self.feedback.stage, Stage::Preparing(..) | Stage::Sending(_))
        {
            return;
        }
        self.ready_then(ctx, Then::Save);
    }

    fn save_report_zip(&mut self) {
        let msg = self.feedback.message_text();
        let made = self.outgoing_zip().and_then(|z| match z {
            Some(b) => crate::diagnose::with_file(&b, "message.txt", msg.as_bytes()),
            None => crate::diagnose::zip_of(&[("message.txt", msg.as_bytes())]),
        });
        let mut path = crate::diagnose::default_path();
        if let (Some(dir), Some(name)) = (&self.feedback.save_dir, path.file_name()) {
            path = dir.join(name);
        }
        match made.and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.set_status(trf("Saved to {}", &[path.display().to_string()]), false);
                self.feedback.saved = Some(path);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    /// The click on the folded row: the picture of the window is taken first, then the panel opens (a click on the open panel folds it).
    pub(super) fn click_report_row(&mut self) {
        if self.feedback.expanded {
            self.feedback.expanded = false;
        } else if matches!(self.feedback.shot_state, ShotState::Idle) {
            self.feedback.shot_state = ShotState::Request { expand: true };
        }
        if let Stage::Sent(_) = self.feedback.stage {
            if !self.feedback.expanded {
                self.feedback.stage = Stage::Idle;
            }
        }
    }

    /// "Take again": the panel and the list of what is sent are hidden until the picture is there.
    pub(super) fn retake_shot(&mut self) {
        if matches!(self.feedback.shot_state, ShotState::Idle) {
            self.feedback.shot_state = ShotState::Request { expand: false };
        }
    }

    /// Whether the panel is hidden for the picture.
    pub(super) fn shot_hides_panel(&self) -> bool {
        self.feedback.expanded && !matches!(self.feedback.shot_state, ShotState::Idle)
    }

    /// Asks eframe for the screenshot (of the next rendered frame of this window), and takes the picture in when it arrives.
    fn shot_poll(&mut self, ctx: &egui::Context) {
        match std::mem::take(&mut self.feedback.shot_state) {
            ShotState::Idle => {}
            ShotState::Request { expand } => {
                let slot: Slot<Arc<egui::ColorImage>> = Arc::new(Mutex::new(None));
                let (s2, c) = (slot.clone(), ctx.clone());
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |img| {
                        if let Ok(mut s) = s2.lock() {
                            *s = Some(img);
                        }
                        c.request_repaint();
                    },
                )));
                ctx.request_repaint();
                self.feedback.shot_state = ShotState::Waiting {
                    since: std::time::Instant::now(),
                    expand,
                    slot,
                };
            }
            ShotState::Waiting { since, expand, slot } => {
                let img = slot.lock().ok().and_then(|mut s| s.take());
                match img {
                    Some(img) => {
                        self.feedback.shot_panel_h = self.feedback.rect.height();
                        let [w, h] = img.size;
                        match WindowShot::from_rgba(rgba_of(&img), w as u32, h as u32) {
                            Ok(mut s) => {
                                s.tex = Some(ctx.load_texture(
                                    "window-shot",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [s.w as usize, s.h as usize],
                                        &s.rgba,
                                    ),
                                    egui::TextureOptions::LINEAR,
                                ));
                                self.feedback.shot = Some(s);
                            }
                            Err(e) => log(&format!("picture of the window not made: {e}")),
                        }
                        self.finish_open(expand);
                    }
                    None if since.elapsed() > std::time::Duration::from_secs(3) => {
                        log("picture of the window: no answer of the screenshot request");
                        self.finish_open(expand);
                    }
                    None => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(40));
                        self.feedback.shot_state = ShotState::Waiting { since, expand, slot };
                    }
                }
            }
        }
    }

    /// The picture is taken (or could not be): the panel opens when this was the click on the folded row.
    fn finish_open(&mut self, expand: bool) {
        if expand {
            self.feedback.graphics_test = self.graphics_test_default();
            self.feedback.expanded = true;
        }
    }

    /// Takes in what the background work has finished.
    pub(super) fn feedback_poll(&mut self, ctx: &egui::Context) {
        self.shot_poll(ctx);
        let stage = std::mem::take(&mut self.feedback.stage);
        self.feedback.stage = match stage {
            Stage::Preparing(slot, then) => {
                let r = slot.lock().ok().and_then(|mut s| s.take());
                match r {
                    None => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                        Stage::Preparing(slot, then)
                    }
                    Some(Ok(m)) => {
                        if let Some(l) = m.light {
                            self.feedback.set_light(l);
                        }
                        if let Some(g) = m.graphics {
                            self.feedback.graphics = Some(g);
                        }
                        match then {
                            Then::Preview => {
                                self.feedback.preview_open = true;
                                Stage::Idle
                            }
                            Then::Save => {
                                self.save_report_zip();
                                Stage::Idle
                            }
                            Then::Send => {
                                self.start_send(ctx);
                                std::mem::take(&mut self.feedback.stage)
                            }
                        }
                    }
                    Some(Err(e)) => {
                        log(&format!("diagnostics not made: {e}"));
                        Stage::Failed(e)
                    }
                }
            }
            Stage::Sending(slot) => {
                let r = slot.lock().ok().and_then(|mut s| s.take());
                match r {
                    None => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                        Stage::Sending(slot)
                    }
                    Some(r) => {
                        match &r {
                            Ok(id) => log(&format!("report sent, number {id}")),
                            Err(e) => log(&format!("report not sent: {e}")),
                        }
                        if let Some(id) = self.feedback.apply_send_result(r) {
                            self.set_status(trf("Report number {}.", &[id]), false);
                        }
                        return;
                    }
                }
            }
            other => other,
        };
    }

    /// The panel at the bottom of the right column: one row when folded, at most about 40 % of the column when open. `ui` has a bottom-up layout
    /// and about 40 % of the column as its area: the buttons are drawn first (they are always visible), the fields above them scroll.
    pub(super) fn feedback_panel(&mut self, ui: &mut egui::Ui, column_h: f32) {
        self.feedback_poll(ui.ctx());
        self.feedback.column_h = column_h;
        let ctx = ui.ctx().clone();
        let offline = rolidium_net::offline();
        let mut act = PanelAct::None;
        let busy = matches!(self.feedback.stage, Stage::Preparing(..) | Stage::Sending(_));
        let expanded = self.feedback.expanded && !self.shot_hides_panel();
        // 1. the buttons (the lowest row)
        if expanded {
            ui.horizontal_wrapped(|ui| {
                if !offline
                    && ui
                        .add_enabled(!busy && self.feedback.can_send(), egui::Button::new(tr("Send")))
                        .clicked()
                {
                    act = PanelAct::Send;
                }
                if ui
                    .add_enabled(
                        !busy && self.feedback.can_send(),
                        egui::Button::new(tr("Save to a file")),
                    )
                    .clicked()
                {
                    act = PanelAct::Save;
                }
            });
            // 2. what happened
            let fb = &self.feedback;
            if let Some(p) = &fb.saved {
                ui.weak(trf("Saved to {}", &[p.display().to_string()]));
            }
            match &fb.stage {
                Stage::Preparing(..) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Preparing the diagnostics…"));
                    });
                }
                Stage::Sending(_) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Sending…"));
                    });
                }
                Stage::Failed(e) => {
                    if !offline {
                        ui.weak(e.as_str());
                    }
                    let mut reason = String::new();
                    if offline {
                        reason.push_str(&tr("The network is switched off (ROLIDIUM_OFFLINE)."));
                        reason.push(' ');
                    }
                    reason.push_str(&trf(
                        "The report could not be sent: the server does not answer or refused it. Your text is kept. Save the report to a file and send it by e-mail to {}.",
                        &[rolidium_net::CONTACT_EMAIL.to_string()],
                    ));
                    ui.label(egui::RichText::new(reason).color(ui.visuals().warn_fg_color));
                }
                _ => {
                    if offline {
                        ui.weak(tr("The network is switched off (ROLIDIUM_OFFLINE). The report can only be saved to a file."));
                    }
                }
            }
            // 3. the fields (they scroll when the window is low)
            // what is below the fields is drawn already; the folded row above them needs about 30 points. The height is fixed (the content of the
            // last frame, at most what is left), so that the panel never grows beyond its area.
            let budget = (ui.available_height() - 34.0).max(60.0);
            let h = budget.min(self.feedback.fields_h + 4.0).max(60.0);
            let mut used_h = self.feedback.fields_h;
            // the scroll area takes all the height it is offered (even with max_height), so it is offered exactly h: no gap under the fields
            let w = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::Vec2::new(w, h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("feedback-scroll")
                        .max_height(h)
                        .min_scrolled_height(h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // the panel is laid out from the bottom up; the fields go from the top down
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                let start_y = ui.cursor().min.y;
                                let fb = &mut self.feedback;
                                let mut kind = fb.kind;
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut kind, Kind::Bug, tr("Bug"));
                                    ui.selectable_value(&mut kind, Kind::Idea, tr("Idea"));
                                });
                                fb.set_kind(kind);
                                ui.add(
                                    egui::TextEdit::multiline(&mut fb.text)
                                        .id_salt("feedback-text")
                                        .desired_rows(4)
                                        .desired_width(f32::INFINITY)
                                        .char_limit(rolidium_net::MAX_NOTE_CHARS)
                                        .hint_text(tr("What happened, or what would you like?")),
                                );
                                fb.clamp_text();
                                ui.weak(format!(
                                    "{} / {}",
                                    fb.text.chars().count(),
                                    rolidium_net::MAX_NOTE_CHARS
                                ));
                                ui.label(tr("Your e-mail for a reply (optional)"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut fb.contact)
                                        .id_salt("feedback-contact")
                                        .desired_width(f32::INFINITY)
                                        .char_limit(200),
                                );
                                ui.checkbox(&mut fb.attach, tr("Attach diagnostics"));
                                if fb.shot.is_some() {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut fb.attach_shot, tr("Attach window picture"));
                                        if ui.small_button(tr("Take again")).clicked() {
                                            act = PanelAct::Retake;
                                        }
                                    });
                                    if let Some(s) = &fb.shot {
                                        if let Some(tex) = &s.tex {
                                            let k = (120.0 / s.w as f32).min(68.0 / s.h as f32);
                                            let size = egui::vec2(s.w as f32 * k, s.h as f32 * k);
                                            let img = egui::Image::new(egui::load::SizedTexture::new(
                                                tex.id(),
                                                size,
                                            ))
                                            .sense(egui::Sense::click());
                                            let r =
                                                ui.add(img).on_hover_cursor(egui::CursorIcon::PointingHand);
                                            if r.clicked() {
                                                act = PanelAct::Big;
                                            }
                                        }
                                    }
                                }
                                if (fb.attach || fb.shot_on()) && ui.link(tr("Show what is sent")).clicked() {
                                    act = PanelAct::Preview;
                                }
                                // what the fields really use (the area given to the layout is larger)
                                used_h = ui.cursor().min.y - start_y;
                            });
                        });
                },
            );
            self.feedback.fields_h = used_h;
        }
        // 4. the report number after a sending
        if let (Stage::Sent(id), false) = (&self.feedback.stage, self.shot_hides_panel()) {
            ui.horizontal(|ui| {
                ui.label(trf("Report number {}.", std::slice::from_ref(id)));
                if ui.small_button(tr("Close")).clicked() {
                    act = PanelAct::CloseSent;
                }
            });
        }
        // 5. the folded row (the top of the panel)
        ui.horizontal(|ui| {
            let arrow = if expanded { "v" } else { ">" };
            let mut label = format!("{arrow} {}", tr("Report a bug or an idea"));
            if !expanded && !self.feedback.text.trim().is_empty() {
                label.push_str(&format!(" ({})", tr("draft")));
            }
            let row = ui
                .selectable_label(expanded, label)
                .on_hover_ui(|ui| self.help.card(ui, "feedback"));
            if row.clicked() {
                act = PanelAct::Row;
            }
        });
        // what the panel used of its area (the area grows upwards from the bottom)
        let area = ui.max_rect();
        let used = (area.height() - ui.available_height()).clamp(0.0, area.height());
        self.feedback.rect = Rect::from_min_max(Pos2::new(area.min.x, area.max.y - used), area.max);
        match act {
            PanelAct::None => {}
            PanelAct::CloseSent => self.feedback.stage = Stage::Idle,
            PanelAct::Send => self.press_send(&ctx),
            PanelAct::Save => self.press_save(&ctx),
            PanelAct::Preview => {
                if !busy {
                    self.ready_then(&ctx, Then::Preview);
                }
            }
            PanelAct::Row => self.click_report_row(),
            PanelAct::Retake => self.retake_shot(),
            PanelAct::Big => self.feedback.big_open = true,
        }
    }

    /// The list of what is sent (every file can be opened and switched off). Nothing is sent from here.
    pub(super) fn feedback_preview(&mut self, ctx: &egui::Context) {
        if !self.feedback.preview_open || !matches!(self.feedback.shot_state, ShotState::Idle) {
            return;
        }
        let fb = &self.feedback;
        let have_light = fb.attach && fb.prepared.is_some();
        if (fb.attach && !have_light) || (!fb.attach && !fb.shot_on()) {
            self.feedback.preview_open = false;
            return;
        }
        let files = if fb.attach {
            fb.prepared.as_ref().map(|p| p.files.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let shot_size = fb.shot.as_ref().map(|s| s.png.len() as u64);
        let graphics_size = fb
            .graphics
            .as_ref()
            .map(|g| g.iter().map(|f| f.1.len() as u64).sum::<u64>());
        let (mut attach_shot, mut graphics_test) = (fb.attach_shot, fb.graphics_test);
        let attach = fb.attach;
        let computing = matches!(fb.stage, Stage::Preparing(..));
        let mut skip = self.settings.report_skip.clone();
        let mut close = false;
        let mut open_name: Option<String> = None;
        egui::Window::new(tr("What is sent"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(tr("This is exactly what will be sent with the report. Nothing is sent before you press Send. The zip has a summary, the log, the settings (without file names), data about the graphics, the system and the version of the program. These files hold no models and no file names, and the home folder is replaced by ~."));
                if shot_size.is_some() {
                    ui.label(tr("The window picture shows what is visible in the window, including the model and the names of the bodies. Switch it off if you do not want to send it."));
                }
                if attach {
                    ui.label(tr("The Graphics test has the pictures scene-*.png of a test scene built into the program, not of your screen or your model. It is off unless there was a crash, a graphics interface is blocked or the drawing is done by software."));
                }
                ui.weak(tr("Switch off a file and it is not sent and not saved to the zip. summary.txt is always sent with the diagnostics."));
                ui.add_space(4.0);
                if let Some(size) = shot_size {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut attach_shot, format!("window.png  ({} kB)", size.div_ceil(1024)));
                        if ui.small_button(tr("Open")).clicked() {
                            open_name = Some("window.png".into());
                        }
                    });
                }
                for (name, size) in &files {
                    ui.horizontal(|ui| {
                        let required = name == "summary.txt";
                        let mut on = required || !skip.contains(name);
                        let label = format!("{name}  ({} kB)", size.div_ceil(1024));
                        if ui.add_enabled(!required, egui::Checkbox::new(&mut on, label)).changed() {
                            skip.retain(|n| n != name);
                            if !on {
                                skip.push(name.clone());
                            }
                        }
                        if ui.small_button(tr("Open")).clicked() {
                            open_name = Some(name.clone());
                        }
                    });
                }
                if attach {
                    ui.horizontal(|ui| {
                        let mut label = tr("Graphics test");
                        if let Some(sz) = graphics_size {
                            label.push_str(&format!("  ({} kB)", sz.div_ceil(1024)));
                        }
                        ui.checkbox(&mut graphics_test, label);
                        if computing {
                            ui.spinner();
                        }
                    });
                }
                ui.add_space(4.0);
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        if skip != self.settings.report_skip {
            self.settings.report_skip = skip;
            self.settings.save();
        }
        self.feedback.attach_shot = attach_shot;
        if graphics_test != self.feedback.graphics_test {
            self.feedback.graphics_test = graphics_test;
            // the scenes are computed only when the item is switched on
            if graphics_test && !computing {
                self.ready_then(ctx, Then::Preview);
            }
        }
        if let Some(name) = open_name {
            if name == "window.png" {
                if let Some(s) = &self.feedback.shot {
                    open_bytes("window.png", &s.png);
                }
            } else if let Some(p) = &self.feedback.prepared {
                open_zip_entry(&p.bytes, &name);
            }
        }
        if close {
            self.feedback.preview_open = false;
        }
    }

    /// The picture of the window, large.
    pub(super) fn feedback_big(&mut self, ctx: &egui::Context) {
        if !self.feedback.big_open || !matches!(self.feedback.shot_state, ShotState::Idle) {
            return;
        }
        let Some((tex, w, h)) = self
            .feedback
            .shot
            .as_ref()
            .and_then(|s| s.tex.clone().map(|t| (t, s.w as f32, s.h as f32)))
        else {
            self.feedback.big_open = false;
            return;
        };
        let screen = ctx.content_rect();
        let k = ((screen.width() * 0.8) / w)
            .min((screen.height() * 0.7) / h)
            .min(1.0);
        let mut close = false;
        egui::Window::new(tr("Window picture"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    tex.id(),
                    egui::vec2(w * k, h * k),
                )));
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        if close {
            self.feedback.big_open = false;
        }
    }
}

enum PanelAct {
    None,
    CloseSent,
    Send,
    Save,
    Preview,
    Row,
    Retake,
    Big,
}

/// Writes bytes to the temporary folder under the name and opens the file with the program of the system.
fn open_bytes(name: &str, data: &[u8]) {
    let dir = std::env::temp_dir().join(format!("rolidium-diagnostics-view-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    if std::fs::write(&path, data).is_ok() {
        crate::diagnose::open_path(&path);
    }
}

/// Extracts one file of the zip to the temporary folder and opens it with the program of the system.
fn open_zip_entry(bytes: &[u8], name: &str) {
    let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return;
    };
    let Ok(mut f) = z.by_name(name) else { return };
    let mut data = Vec::new();
    if std::io::Read::read_to_end(&mut f, &mut data).is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("rolidium-diagnostics-view-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    // only the last part of the name: a name in the zip must not point out of the folder
    let leaf = std::path::Path::new(name)
        .file_name()
        .map_or("file".into(), |n| n.to_os_string());
    let path = dir.join(leaf);
    if std::fs::write(&path, data).is_ok() {
        crate::diagnose::open_path(&path);
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_keeps_the_text_when_folded_and_unfolded() {
        let mut f = Feedback::default();
        assert!(!f.expanded, "the panel starts folded");
        f.expanded = true;
        f.text = "the hole is square".into();
        f.expanded = false;
        f.expanded = true;
        assert_eq!(f.text, "the hole is square");
    }

    #[test]
    fn the_text_is_limited_to_a_thousand_characters() {
        let mut f = Feedback::default();
        f.text = "x".repeat(1500);
        f.clamp_text();
        assert_eq!(f.text.chars().count(), 1000);
        assert_eq!(rolidium_net::MAX_NOTE_CHARS, 1000);
    }

    #[test]
    fn a_bug_needs_text_or_diagnostics_and_an_idea_needs_text() {
        let mut f = Feedback::default();
        assert!(
            f.kind == Kind::Bug && f.attach && f.can_send(),
            "a bug without text goes with the diagnostics"
        );
        f.attach = false;
        assert!(!f.can_send());
        f.text = "  ".into();
        assert!(!f.can_send());
        f.text = "a crash".into();
        assert!(f.can_send());
        // an idea: no text, no sending, even when the diagnostics are on
        let mut g = Feedback::default();
        g.set_kind(Kind::Idea);
        g.attach = true;
        assert!(!g.can_send());
        g.text = " \n ".into();
        assert!(!g.can_send());
        g.text = "a ruler".into();
        assert!(g.can_send());
    }

    #[test]
    fn the_default_of_the_diagnostics_follows_the_kind_and_can_be_changed() {
        let mut f = Feedback::default();
        assert!(f.attach);
        f.set_kind(Kind::Idea);
        assert!(!f.attach, "an idea starts without the diagnostics");
        f.attach = true; // the person's choice stays until the kind changes
        f.set_kind(Kind::Idea);
        assert!(f.attach);
        f.set_kind(Kind::Bug);
        assert!(f.attach);
        f.attach = false;
        f.set_kind(Kind::Idea);
        f.set_kind(Kind::Bug);
        assert!(f.attach, "back to a bug: on again");
    }

    fn light() -> Vec<u8> {
        crate::diagnose::zip_of(&[
            ("summary.txt", b"s"),
            ("report.txt", b"r"),
            ("log.txt", b"l"),
            ("settings.txt", b"t"),
        ])
        .unwrap()
    }

    fn names(z: Option<Vec<u8>>) -> Vec<String> {
        list_zip(&z.unwrap()).unwrap().into_iter().map(|f| f.0).collect()
    }

    fn picture(w: u32, h: u32, noise: bool) -> WindowShot {
        let mut x = 12345u32;
        let px: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                if noise {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                } else {
                    (i % 7) as u8 * 30
                }
            })
            .collect();
        WindowShot::from_rgba(px, w, h).unwrap()
    }

    #[test]
    fn files_switched_off_are_not_in_the_zip_but_the_summary_always_is() {
        let skip = vec!["log.txt".to_string(), "summary.txt".to_string()];
        let z = assemble_zip(Some(&light()), &skip, None, None).unwrap();
        assert_eq!(names(z), ["summary.txt", "report.txt", "settings.txt"]);
    }

    #[test]
    fn the_window_picture_is_independent_of_the_diagnostics() {
        let w = picture(64, 48, false);
        // diagnostics off, picture on: only window.png
        assert_eq!(
            names(assemble_zip(None, &[], None, Some(&w)).unwrap()),
            ["window.png"]
        );
        // both: summary first, then the picture
        let both = names(assemble_zip(Some(&light()), &[], None, Some(&w)).unwrap());
        assert_eq!(
            both,
            [
                "summary.txt",
                "window.png",
                "report.txt",
                "log.txt",
                "settings.txt"
            ]
        );
        // the picture off: nothing of it
        assert_eq!(
            names(assemble_zip(Some(&light()), &[], None, None).unwrap()).len(),
            4
        );
        // nothing at all: no zip
        assert!(assemble_zip(None, &[], None, None).unwrap().is_none());
    }

    #[test]
    fn the_graphics_test_is_only_in_the_zip_when_it_is_given() {
        let g = vec![
            ("scenes.txt".to_string(), b"x".to_vec()),
            ("scene-gl-0.png".to_string(), b"p".to_vec()),
        ];
        let with = names(assemble_zip(Some(&light()), &[], Some(&g), None).unwrap());
        assert!(with.contains(&"scenes.txt".to_string()) && with.contains(&"scene-gl-0.png".to_string()));
        let without = names(assemble_zip(Some(&light()), &[], None, None).unwrap());
        assert!(!without.iter().any(|n| n.starts_with("scene")));
    }

    #[test]
    fn a_zip_over_two_megabytes_is_made_smaller_and_keeps_the_summary() {
        let big = picture(1600, 1000, true);
        assert!(
            big.png.len() > rolidium_net::MAX_ZIP,
            "the test picture must not fit: {}",
            big.png.len()
        );
        let z = assemble_zip(Some(&light()), &[], None, Some(&big))
            .unwrap()
            .unwrap();
        assert!(z.len() <= rolidium_net::MAX_ZIP, "{} bytes", z.len());
        let n = names(Some(z));
        assert!(n.contains(&"summary.txt".to_string()) && n.contains(&"window.png".to_string()));
        // the scene pictures go first
        let pics = vec![("scene-gl-0.png".to_string(), vec![7u8; 2_200_000])];
        let small = picture(200, 100, false);
        let z = assemble_zip(Some(&light()), &[], Some(&pics), Some(&small))
            .unwrap()
            .unwrap();
        let n = names(Some(z));
        assert!(!n.contains(&"scene-gl-0.png".to_string()) && n.contains(&"window.png".to_string()));
    }

    #[test]
    fn the_picture_is_at_most_1600_on_the_longer_side_and_keeps_its_proportions() {
        let w = picture(3200, 1800, false);
        assert_eq!((w.w, w.h), (1600, 900));
        let w = picture(1000, 3000, false);
        assert_eq!((w.w, w.h), (533, 1600));
        let w = picture(800, 600, false);
        assert_eq!((w.w, w.h), (800, 600));
        assert_eq!(&w.png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn the_defaults_follow_the_kind_for_the_picture_too() {
        let mut f = Feedback::default();
        assert!(f.attach && f.attach_shot && !f.graphics_test);
        f.set_kind(Kind::Idea);
        assert!(!f.attach && !f.attach_shot);
        f.set_kind(Kind::Bug);
        assert!(f.attach && f.attach_shot);
    }

    #[test]
    fn there_is_no_picture_after_a_crash_and_the_graphics_test_is_on() {
        let mut f = Feedback::default();
        f.shot = Some(picture(10, 10, false));
        f.open_after_crash();
        assert!(f.shot.is_none() && !f.shot_on() && f.graphics_test);
    }

    #[test]
    fn the_graphics_test_switches_on_by_itself_in_three_cases() {
        assert!(!graphics_test_on(false, false, false));
        assert!(graphics_test_on(true, false, false));
        assert!(graphics_test_on(false, true, false));
        assert!(graphics_test_on(false, false, true));
    }

    #[test]
    fn a_picture_alone_is_enough_for_a_bug_but_not_for_an_idea() {
        let mut f = Feedback::default();
        f.attach = false;
        f.shot = Some(picture(10, 10, false));
        assert!(f.can_send());
        f.attach_shot = false;
        assert!(!f.can_send());
        f.attach_shot = true;
        f.set_kind(Kind::Idea);
        f.attach_shot = true;
        assert!(!f.can_send(), "an idea needs a text");
    }

    #[test]
    fn a_sent_report_shows_its_number_clears_the_text_and_folds_the_panel() {
        let mut f = Feedback::default();
        f.expanded = true;
        f.text = "crash".into();
        assert_eq!(f.apply_send_result(Ok("R-9".into())), Some("R-9".into()));
        assert!(f.text.is_empty() && !f.expanded);
        assert!(matches!(&f.stage, Stage::Sent(id) if id == "R-9"));
    }

    #[test]
    fn a_failed_sending_keeps_the_text_and_the_panel_open() {
        for reason in ["the server answered 500", "offline", "timed out"] {
            let mut f = Feedback::default();
            f.expanded = true;
            f.text = "crash".into();
            assert_eq!(f.apply_send_result(Err(reason.into())), None);
            assert_eq!(f.text, "crash");
            assert!(f.expanded);
            assert!(matches!(&f.stage, Stage::Failed(e) if e == reason));
        }
    }

    #[test]
    fn after_a_crash_the_panel_opens_with_a_note_and_sends_nothing() {
        let mut f = Feedback::default();
        f.open_after_crash();
        assert!(f.expanded && f.attach && f.kind == Kind::Bug);
        assert!(!f.text.is_empty());
        assert!(matches!(f.stage, Stage::Idle), "nothing is sent by itself");
        // a text the person already wrote is not replaced
        let mut g = Feedback::default();
        g.text = "mine".into();
        g.open_after_crash();
        assert_eq!(g.text, "mine");
    }

    #[test]
    fn the_saved_report_has_the_text_as_a_file_of_its_own() {
        let mut f = Feedback::default();
        f.kind = Kind::Idea;
        f.text = "a ruler tool".into();
        f.contact = "me@example.org".into();
        let m = f.message_text();
        assert!(m.contains("idea") && m.contains("a ruler tool") && m.contains("me@example.org"));
        // without diagnostics: a zip with this one file; with them: the files of the diagnostics and this one
        let alone = crate::diagnose::zip_of(&[("message.txt", m.as_bytes())]).unwrap();
        assert_eq!(list_zip(&alone).unwrap().len(), 1);
        let more = crate::diagnose::with_file(&alone, "summary.txt", b"x").unwrap();
        let names: Vec<String> = list_zip(&more).unwrap().into_iter().map(|f| f.0).collect();
        assert_eq!(names, ["message.txt", "summary.txt"]);
    }
}
