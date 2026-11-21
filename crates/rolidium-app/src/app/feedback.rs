// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! "Send diagnostics to the publisher": only on the person's request. The zip is made first, then the window shows exactly what is in it (every file
//! can be opened), a field for a note and one for an e-mail (optional); nothing is sent before the person confirms. When the server does not answer
//! (it may not exist yet) the zip can be saved to a file and sent by e-mail.

use super::*;
use std::sync::{Arc, Mutex};

type Slot<T> = Arc<Mutex<Option<T>>>;

/// The zip that is ready to be sent: the bytes, where a copy is on the disk, and the files in it with their sizes.
pub(super) struct Prepared {
    path: PathBuf,
    bytes: Vec<u8>,
    files: Vec<(String, u64)>,
}

pub(super) struct Review {
    prepared: Prepared,
    note: String,
    contact: String,
}

#[derive(Default)]
pub(super) enum Feedback {
    #[default]
    Closed,
    Preparing(Slot<Result<Prepared, String>>),
    Review(Review),
    Sending(Slot<Result<String, String>>, Review),
    Sent(String),
    Failed(Review, String),
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
fn prepare(desc: String, backend: String, settings_text: String) -> Result<Prepared, String> {
    let path = std::env::temp_dir().join(format!("rolidium-diagnostics-send-{}.zip", std::process::id()));
    crate::diagnose::write_zip(&path, Some(&desc), &backend, &settings_text)?;
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
    /// Starts making the diagnostics for sending (the window opens when they are ready).
    pub(super) fn start_feedback(&mut self, ctx: &egui::Context) {
        if !matches!(self.feedback, Feedback::Closed) {
            return;
        }
        let slot: Slot<Result<Prepared, String>> = Arc::new(Mutex::new(None));
        let (s2, c) = (slot.clone(), ctx.clone());
        let (desc, backend, stext) = (
            self.adapter_description.clone(),
            self.settings.backend.clone(),
            self.settings.diagnostic_text(),
        );
        std::thread::spawn(move || {
            let r = prepare(desc, backend, stext);
            if let Ok(mut s) = s2.lock() {
                *s = Some(r);
            }
            c.request_repaint();
        });
        self.feedback = Feedback::Preparing(slot);
    }

    /// The card offered after a program that did not end properly.
    #[allow(deprecated)]
    pub(super) fn crash_offer_ui(&mut self, ctx: &egui::Context) {
        if !self.crash_offer || !matches!(self.feedback, Feedback::Closed) {
            return;
        }
        let vp = self.viewport_rect;
        if vp.width() < 50.0 {
            return;
        }
        let mut action = None;
        egui::Area::new(egui::Id::new("crash_offer"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new((vp.right() - 352.0).max(vp.left() + 8.0), vp.top() + 12.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(330.0);
                    ui.label(tr("Rolidium did not close properly the last time. Do you want to send diagnostics to the publisher? You will see what is sent first, and nothing is sent without your confirmation."));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Review and send")).clicked() {
                            action = Some(true);
                        }
                        if ui.button(tr("Not now")).clicked() {
                            action = Some(false);
                        }
                    });
                });
            });
        match action {
            Some(true) => {
                self.crash_offer = false;
                self.start_feedback(ctx);
            }
            Some(false) => self.crash_offer = false,
            None => {}
        }
    }

    fn save_feedback_zip(&mut self, bytes: &[u8]) {
        let path = crate::diagnose::default_path();
        match std::fs::write(&path, bytes) {
            Ok(()) => self.set_status(trf("Saved to {}", &[path.display().to_string()]), false),
            Err(e) => self.set_status(e.to_string(), true),
        }
    }

    /// The window of the review and of the sending.
    pub(super) fn feedback_window(&mut self, ctx: &egui::Context) {
        // take in the results of the background work
        match &self.feedback {
            Feedback::Preparing(slot) => {
                let r = slot.lock().ok().and_then(|mut s| s.take());
                if let Some(r) = r {
                    self.feedback = match r {
                        Ok(prepared) => Feedback::Review(Review {
                            prepared,
                            note: String::new(),
                            contact: String::new(),
                        }),
                        Err(e) => {
                            self.set_status(e, true);
                            Feedback::Closed
                        }
                    };
                }
            }
            Feedback::Sending(slot, _) => {
                let r = slot.lock().ok().and_then(|mut s| s.take());
                if let Some(r) = r {
                    let Feedback::Sending(_, review) = std::mem::take(&mut self.feedback) else {
                        return;
                    };
                    self.feedback = match r {
                        Ok(id) => {
                            log(&format!("diagnostics sent, report {id}"));
                            Feedback::Sent(id)
                        }
                        Err(e) => {
                            log(&format!("diagnostics not sent: {e}"));
                            Feedback::Failed(review, e)
                        }
                    };
                }
            }
            _ => {}
        }
        if matches!(self.feedback, Feedback::Closed) {
            return;
        }
        let mut close = false;
        let mut act = ReviewAct::None;
        let mut state = std::mem::take(&mut self.feedback);
        egui::Window::new(tr("Send diagnostics"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                match &mut state {
                    Feedback::Preparing(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Preparing the diagnostics…"));
                        });
                        if ui.button(tr("Cancel")).clicked() {
                            close = true;
                        }
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    }
                    Feedback::Review(r) => act = review_ui(ui, r, false, None),
                    Feedback::Sending(_, r) => {
                        act = review_ui(ui, r, true, None);
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    }
                    Feedback::Failed(r, e) => act = review_ui(ui, r, false, Some(e.as_str())),
                    Feedback::Sent(id) => {
                        ui.label(trf(
                            "Report number {}. Mention it in the forum if you write there.",
                            std::slice::from_ref(id),
                        ));
                        ui.horizontal(|ui| {
                            if ui.button(tr("Open the forum")).clicked() {
                                crate::diagnose::open_path(std::path::Path::new(rolidium_net::urls::FORUM));
                            }
                            if ui.button(tr("Close")).clicked() {
                                close = true;
                            }
                        });
                    }
                    Feedback::Closed => {}
                }
            });
        match act {
            ReviewAct::None => {}
            ReviewAct::Cancel => close = true,
            ReviewAct::Open(name) => {
                if let Feedback::Review(r) | Feedback::Sending(_, r) | Feedback::Failed(r, _) = &state {
                    open_zip_entry(&r.prepared.bytes, &name);
                }
            }
            ReviewAct::Save => {
                if let Feedback::Review(r) | Feedback::Sending(_, r) | Feedback::Failed(r, _) = &state {
                    let bytes = r.prepared.bytes.clone();
                    self.save_feedback_zip(&bytes);
                }
            }
            ReviewAct::Send => {
                if let Feedback::Review(r) | Feedback::Failed(r, _) = state {
                    let slot: Slot<Result<String, String>> = Arc::new(Mutex::new(None));
                    let (s2, c) = (slot.clone(), ctx.clone());
                    let (bytes, note, contact) =
                        (r.prepared.bytes.clone(), r.note.clone(), r.contact.clone());
                    std::thread::spawn(move || {
                        let res = rolidium_net::send_diagnostics(
                            &bytes,
                            &note,
                            &contact,
                            env!("CARGO_PKG_VERSION"),
                        );
                        if let Ok(mut s) = s2.lock() {
                            *s = Some(res);
                        }
                        c.request_repaint();
                    });
                    state = Feedback::Sending(slot, r);
                }
            }
        }
        if close {
            if let Feedback::Review(r) | Feedback::Sending(_, r) | Feedback::Failed(r, _) = &state {
                let _ = std::fs::remove_file(&r.prepared.path);
            }
            self.feedback = Feedback::Closed;
        } else {
            self.feedback = state;
        }
    }
}

pub(super) enum ReviewAct {
    None,
    Cancel,
    Send,
    Save,
    Open(String),
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

/// The content of the review: what is sent, the note, the e-mail, the buttons. `error` is the reason why the last sending failed.
fn review_ui(ui: &mut egui::Ui, r: &mut Review, sending: bool, error: Option<&str>) -> ReviewAct {
    let mut act = ReviewAct::None;
    ui.label(tr("This is exactly what will be sent. Nothing is sent before you press Send. The zip has the log, the settings (without file names), data about the graphics, the system and the version of the program. It has no models and no names of models, and the home folder is replaced by ~."));
    ui.add_space(4.0);
    for (name, size) in &r.prepared.files {
        ui.horizontal(|ui| {
            ui.label(format!("{name}  ({} kB)", size.div_ceil(1024)));
            if ui.small_button(tr("Open")).clicked() {
                act = ReviewAct::Open(name.clone());
            }
        });
    }
    ui.add_space(4.0);
    ui.label(trf(
        "Note (up to {} characters)",
        &[rolidium_net::MAX_NOTE_CHARS.to_string()],
    ));
    ui.add(
        egui::TextEdit::multiline(&mut r.note)
            .desired_rows(3)
            .desired_width(440.0)
            .char_limit(rolidium_net::MAX_NOTE_CHARS),
    );
    ui.label(tr("Your e-mail for a reply (optional)"));
    ui.add(
        egui::TextEdit::singleline(&mut r.contact)
            .desired_width(440.0)
            .char_limit(200),
    );
    if let Some(e) = error {
        ui.add_space(4.0);
        let mut reason = String::new();
        if rolidium_net::offline() {
            reason.push_str(&tr("The network is switched off (ROLIDIUM_OFFLINE)."));
            reason.push(' ');
        }
        reason.push_str(&trf(
            "The diagnostics could not be sent: the server does not answer or refused them. Save the zip to a file and send it by e-mail to {}.",
            &[rolidium_net::CONTACT_EMAIL.to_string()],
        ));
        ui.label(egui::RichText::new(reason).color(ui.visuals().warn_fg_color));
        if !rolidium_net::offline() {
            ui.weak(e);
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if sending {
            ui.spinner();
            ui.label(tr("Sending…"));
        } else if ui.button(tr("Send")).clicked() {
            act = ReviewAct::Send;
        }
        if ui.button(tr("Save zip to a file")).clicked() {
            act = ReviewAct::Save;
        }
        if ui.button(tr("Cancel")).clicked() {
            act = ReviewAct::Cancel;
        }
    });
    act
}
