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
        if self.idle() {
            self.busy_since = None;
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
            self.set_status("Cancelled: the model is unchanged", false);
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
                });
            });
        if cancel {
            self.cancel_computation();
        }
    }
}
