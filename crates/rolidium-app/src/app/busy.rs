// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A long calculation: after 0.3 s the view shows that it is computing, with a Cancel button (and Esc). The calculation
//! itself runs on the worker thread, so the view can still be turned meanwhile; the kernel stops at the next check of its
//! cancel flag and the model stays exactly as it was.

use super::*;
use crate::i18n::{tr, trf};

/// The indication appears only for calculations longer than this, so it does not flicker.
pub const SHOW_AFTER: std::time::Duration = std::time::Duration::from_millis(300);

/// What the worker reports for a cancelled calculation.
pub const CANCELLED: &str = "Cancelled";

impl App {
    /// Bookkeeping once a frame: since when the program has been busy.
    pub(super) fn busy_tick(&mut self) {
        self.take_chain_answers();
        self.warm_big_bodies();
        // a tool that was left (another tool, a new document) takes its panel state and its preview with it
        if self.tool != Tool::Shell
            && (self.shell.body.is_some() || self.shell.preview_on || self.shell.editing.is_some())
        {
            self.shell_close();
        }
        if self.tool != Tool::Text && !self.text.wait_sketch && self.text.target.is_some() {
            self.text_close();
        }
        if self.text.wait_sketch && self.sk.is_none() {
            self.text_close();
        }
        crate::memwatch::start();
        crate::memwatch::set_computing(!self.idle());
        if self.idle() {
            self.busy_since = None;
            rolidium_doc::progress::clear();
            if self.cancelling {
                // the flag is global: clear it so calculations on this thread are not stopped by mistake
                rolidium_kernel::reset_cancel();
            }
            self.cancelling = false;
        } else if self.busy_since.is_none() {
            self.busy_since = Some(Instant::now());
        }
    }

    /// True while a calculation runs that can be cancelled.
    pub(super) fn can_cancel(&self) -> bool {
        matches!(self.busy, Busy::Command | Busy::Export | Busy::Meshes)
    }

    pub(super) fn cancel_computation(&mut self) {
        if !self.can_cancel() || self.cancelling {
            return;
        }
        self.cancelling = true;
        rolidium_kernel::request_cancel();
        self.set_status("Cancelling…", false);
    }

    /// The result of a cancelled calculation arrived (or the text is something else: `false`).
    pub(super) fn was_cancelled(&mut self, msg: &str) -> bool {
        if msg == CANCELLED {
            if crate::memwatch::take_stopped() {
                self.set_status(
                    trf(
                        "The calculation needed too much memory (more than {} MB) and was stopped. The model is unchanged. A smaller or simpler shape needs less",
                        &[crate::memwatch::limit_mb().to_string()],
                    ),
                    true,
                );
            } else {
                self.set_status("Cancelled: the model is unchanged", false);
            }
            true
        } else {
            false
        }
    }

    #[allow(deprecated)]
    pub(super) fn busy_overlay(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(since) = self.busy_since else { return };
        if !self.can_cancel() || since.elapsed() < SHOW_AFTER {
            // keep repainting so the indication appears on time
            if self.can_cancel() {
                ctx.request_repaint_after(SHOW_AFTER);
            }
            return;
        }
        ctx.request_repaint();
        let secs = since.elapsed().as_secs_f32();
        let mut cancel = false;
        egui::Area::new(egui::Id::new("busy_overlay"))
            .fixed_pos(Pos2::new(rect.center().x - 110.0, rect.min.y + 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if self.cancelling {
                            tr("Cancelling…")
                        } else {
                            trf("Computing… {} s", &[format!("{secs:.1}")])
                        });
                        if ui
                            .add_enabled(!self.cancelling, egui::Button::new(tr("Cancel")))
                            .clicked()
                        {
                            cancel = true;
                        }
                        ui.label(egui::RichText::new("(Esc)").weak());
                    });
                    // what a long conversion is doing now, and how far it is
                    if let Some((stage, fraction)) = rolidium_doc::progress::get() {
                        ui.label(tr(&stage));
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(300.0)
                                .animate(true),
                        );
                    }
                });
            });
        if cancel {
            self.cancel_computation();
        }
    }
}

impl App {
    /// A body of many faces needs seconds of the kernel for the things the window asks about it (the rounds it recognises, the box, the volume, the chains of
    /// edges): they are worked out once on a thread of its own, as soon as the body is there, so that the window finds them ready when the pointer comes.
    fn warm_big_bodies(&mut self) {
        for b in self.doc.bodies() {
            let n = b.solid.face_count();
            if n <= 200 {
                continue;
            }
            let id = b.solid.shape_id();
            if !self.warmed.insert(id) {
                continue;
            }
            let solid = b.solid.clone();
            let _ = std::thread::Builder::new().stack_size(64 << 20).spawn(move || {
                let _ = solid.bounding_box();
                let _ = solid.volume();
                let _ = solid.blend_map();
                let _ = solid.smooth_chain(&[rolidium_kernel::EdgeId(0)], rolidium_kernel::CHAIN_ANGLE_DEG);
            });
        }
        if self.warmed.len() > 64 {
            let alive: std::collections::HashSet<usize> = self.doc.bodies().iter().map(|b| b.solid.shape_id()).collect();
            self.warmed.retain(|k| alive.contains(k));
        }
    }
}
