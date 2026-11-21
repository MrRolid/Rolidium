// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Report a bug or an idea": a folded row at the bottom of the right column that opens a panel. The person chooses Bug or Idea, writes up to
//! 1000 characters, may leave an e-mail and may attach the diagnostics (on by default; "Show what is sent" lists every file of the zip).
//! Nothing is sent before the person presses Send. When the server does not answer (it may not exist yet) the text stays and the report can be
//! saved to a file (a zip with the text as a file of its own) and sent by e-mail.

use super::*;
use rolidium_net::Kind;
use std::sync::{Arc, Mutex};

type Slot<T> = Arc<Mutex<Option<T>>>;

/// The zip that is ready to be sent: the bytes, where a copy is on the disk, and the files in it with their sizes.
pub(super) struct Prepared {
    path: PathBuf,
    bytes: Vec<u8>,
    files: Vec<(String, u64)>,
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
    Preparing(Slot<Result<Prepared, String>>, Then),
    Sending(Slot<Result<String, String>>),
    /// The number of the report (the panel is folded and the text is gone).
    Sent(String),
    /// The reason why the sending failed (the text stays).
    Failed(String),
}

pub(super) struct Feedback {
    pub(super) expanded: bool,
    pub(super) kind: Kind,
    pub(super) text: String,
    pub(super) contact: String,
    pub(super) attach: bool,
    pub(super) stage: Stage,
    prepared: Option<Prepared>,
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
            stage: Stage::Idle,
            prepared: None,
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
    /// After a crash: the panel is open with a note that the program ended badly the last time. Sending still waits for the person.
    pub(super) fn open_after_crash(&mut self) {
        self.expanded = true;
        self.kind = Kind::Bug;
        self.attach = true;
        if self.text.is_empty() {
            self.text = tr("The program ended with a crash the last time.");
        }
    }

    /// Whether Send can be pressed: some text, or the diagnostics.
    pub(super) fn can_send(&self) -> bool {
        !self.text.trim().is_empty() || self.attach
    }

    /// Cuts the text to the limit (a pasted text may be longer than the field takes).
    pub(super) fn clamp_text(&mut self) {
        if self.text.chars().count() > rolidium_net::MAX_NOTE_CHARS {
            self.text = self.text.chars().take(rolidium_net::MAX_NOTE_CHARS).collect();
        }
    }

    fn forget_zip(&mut self) {
        if let Some(p) = self.prepared.take() {
            let _ = std::fs::remove_file(&p.path);
        }
        self.preview_open = false;
    }

    /// The report was sent: the number is shown, the text is gone, the panel is folded.
    pub(super) fn finish_sent(&mut self, id: String) {
        self.text.clear();
        self.expanded = false;
        self.forget_zip();
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

/// Makes the zip for sending: at most [`rolidium_net::MAX_ZIP`] bytes (the test pictures are left out when it is larger).
fn prepare(
    desc: String,
    backend: String,
    settings_text: String,
    summary: String,
) -> Result<Prepared, String> {
    let path = std::env::temp_dir().join(format!("rolidium-diagnostics-send-{}.zip", std::process::id()));
    crate::diagnose::write_zip(&path, Some(&desc), &backend, &settings_text, &summary)?;
    let mut bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > rolidium_net::MAX_ZIP {
        bytes = crate::diagnose::without_pictures(&bytes)?;
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    }
    if bytes.len() > rolidium_net::MAX_ZIP {
        return Err("the diagnostics are larger than 2 MB".into());
    }
    let files = list_zip(&bytes)?;
    Ok(Prepared { path, bytes, files })
}

impl App {
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

    /// Starts making the diagnostics zip (the next step runs when it is ready).
    pub(super) fn prepare_zip(&mut self, ctx: &egui::Context, then: Then) {
        let slot: Slot<Result<Prepared, String>> = Arc::new(Mutex::new(None));
        let (s2, c) = (slot.clone(), ctx.clone());
        let (desc, backend, stext, summary) = (
            self.adapter_description.clone(),
            self.settings.backend.clone(),
            self.settings.diagnostic_text(),
            self.summary_for(ctx),
        );
        std::thread::spawn(move || {
            let r = prepare(desc, backend, stext, summary);
            if let Ok(mut s) = s2.lock() {
                *s = Some(r);
            }
            c.request_repaint();
        });
        self.feedback.stage = Stage::Preparing(slot, then);
    }

    /// Sends the report in the background. The zip is the one the person could look at.
    pub(super) fn start_send(&mut self, ctx: &egui::Context) {
        let slot: Slot<Result<String, String>> = Arc::new(Mutex::new(None));
        let (s2, c) = (slot.clone(), ctx.clone());
        let fb = &self.feedback;
        let zip = fb
            .attach
            .then(|| fb.prepared.as_ref().map(|p| p.bytes.clone()))
            .flatten();
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

    /// The Send button: prepares the diagnostics when they are attached and not made yet, then sends.
    pub(super) fn press_send(&mut self, ctx: &egui::Context) {
        if !self.feedback.can_send()
            || matches!(self.feedback.stage, Stage::Preparing(..) | Stage::Sending(_))
        {
            return;
        }
        if self.feedback.attach && self.feedback.prepared.is_none() {
            self.prepare_zip(ctx, Then::Send);
        } else {
            self.start_send(ctx);
        }
    }

    /// The report as a zip in the Documents folder: the diagnostics (when attached) and the text as a file of its own.
    pub(super) fn press_save(&mut self, ctx: &egui::Context) {
        if matches!(self.feedback.stage, Stage::Preparing(..) | Stage::Sending(_)) {
            return;
        }
        if self.feedback.attach && self.feedback.prepared.is_none() {
            self.prepare_zip(ctx, Then::Save);
        } else {
            self.save_report_zip();
        }
    }

    fn save_report_zip(&mut self) {
        let msg = self.feedback.message_text();
        let made = match (&self.feedback.prepared, self.feedback.attach) {
            (Some(p), true) => crate::diagnose::with_file(&p.bytes, "message.txt", msg.as_bytes()),
            _ => crate::diagnose::zip_of(&[("message.txt", msg.as_bytes())]),
        };
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

    /// Takes in what the background work has finished.
    pub(super) fn feedback_poll(&mut self, ctx: &egui::Context) {
        let stage = std::mem::take(&mut self.feedback.stage);
        self.feedback.stage = match stage {
            Stage::Preparing(slot, then) => {
                let r = slot.lock().ok().and_then(|mut s| s.take());
                match r {
                    None => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                        Stage::Preparing(slot, then)
                    }
                    Some(Ok(p)) => {
                        self.feedback.prepared = Some(p);
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
        let expanded = self.feedback.expanded;
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
                    .add_enabled(!busy, egui::Button::new(tr("Save to a file")))
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
            let out = egui::ScrollArea::vertical()
                .id_salt("feedback-scroll")
                .max_height(h)
                .min_scrolled_height(h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // the panel is laid out from the bottom up; the fields go from the top down
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        let fb = &mut self.feedback;
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut fb.kind, Kind::Bug, tr("Bug"));
                            ui.selectable_value(&mut fb.kind, Kind::Idea, tr("Idea"));
                        });
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
                        if fb.attach && ui.link(tr("Show what is sent")).clicked() {
                            act = PanelAct::Preview;
                        }
                    });
                });
            self.feedback.fields_h = out.content_size.y;
        }
        // 4. the report number after a sending
        if let Stage::Sent(id) = &self.feedback.stage {
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
                self.feedback.expanded = !expanded;
                if !expanded {
                    if let Stage::Sent(_) = self.feedback.stage {
                        self.feedback.stage = Stage::Idle;
                    }
                }
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
                if self.feedback.prepared.is_some() {
                    self.feedback.preview_open = true;
                } else if !busy {
                    self.prepare_zip(&ctx, Then::Preview);
                }
            }
        }
    }

    /// The list of what is sent (every file can be opened). Nothing can be changed here and nothing is sent from here.
    pub(super) fn feedback_preview(&mut self, ctx: &egui::Context) {
        if !self.feedback.preview_open {
            return;
        }
        let Some(p) = &self.feedback.prepared else {
            self.feedback.preview_open = false;
            return;
        };
        let mut close = false;
        let mut open_name = None;
        egui::Window::new(tr("What is sent"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(tr("This is exactly what will be sent with the report. Nothing is sent before you press Send. The zip has a summary, the log, the settings (without file names), data about the graphics, the system and the version of the program. It has no models and no names of models, and the home folder is replaced by ~."));
                ui.add_space(4.0);
                for (name, size) in &p.files {
                    ui.horizontal(|ui| {
                        ui.label(format!("{name}  ({} kB)", size.div_ceil(1024)));
                        if ui.small_button(tr("Open")).clicked() {
                            open_name = Some(name.clone());
                        }
                    });
                }
                ui.add_space(4.0);
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        if let (Some(name), Some(p)) = (open_name, &self.feedback.prepared) {
            open_zip_entry(&p.bytes, &name);
        }
        if close {
            self.feedback.preview_open = false;
        }
    }
}

enum PanelAct {
    None,
    CloseSent,
    Send,
    Save,
    Preview,
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
    fn send_needs_text_or_diagnostics() {
        let mut f = Feedback::default();
        assert!(
            f.attach && f.can_send(),
            "the diagnostics are attached by default"
        );
        f.attach = false;
        assert!(!f.can_send());
        f.text = "  ".into();
        assert!(!f.can_send());
        f.text = "an idea".into();
        assert!(f.can_send());
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
