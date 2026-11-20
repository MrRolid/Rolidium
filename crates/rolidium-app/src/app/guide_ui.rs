// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The first start of a new document and the window of the guide of the first steps (the state machine is in `guide.rs`).

use super::*;
use crate::guide::{Facts, Hint};

/// Seconds the isometric view is shown before it turns square to the plane at the very first start.
pub(super) const FIRST_PAUSE: f32 = 0.5;
/// Seconds of the turn into the plane view at the start of a new document.
const TURN_SECONDS: f32 = 0.5;

impl App {
    /// A new empty document starts in a sketch on the XY plane with the Circle tool (when the setting is on). At the program
    /// start the isometric view is shown for `pause` seconds first, then it turns square to the plane.
    pub(super) fn start_first_steps(&mut self, pause: f32) {
        self.guide.document_started(self.settings.new_doc_sketch);
        if !self.settings.new_doc_sketch {
            return;
        }
        self.enter_sketch_on(PlaneDef::base());
        if let Some(sm) = &mut self.sk {
            sm.set_tool(SkTool::Circle);
        }
        if let Some(a) = &mut self.view_anim {
            a.start = Instant::now() + std::time::Duration::from_secs_f32(pause);
            a.seconds = TURN_SECONDS;
        }
    }

    /// Once a frame: tells the guide what the person has done.
    pub(super) fn guide_tick(&mut self) {
        if !self.idle() {
            return;
        }
        let facts = Facts {
            closed_shape: self.regions.iter().any(|r| !r.regions.is_empty()),
            pull_tool: self.tool == Tool::Pull || self.pull.is_some(),
            body: !self.doc.bodies().is_empty(),
        };
        if self.guide.observe(facts) {
            self.settings.guide_mask = self.guide.mask;
            self.settings.save();
        }
    }

    /// The hint that is due, drawn next to the place it is about.
    #[allow(deprecated)]
    pub(super) fn guide_window(&mut self, ctx: &egui::Context) {
        let Some(hint) = self.guide.due() else { return };
        let vp = self.viewport_rect;
        // draw: a hint needs the sketch; without it (the person left the sketch) it waits
        if hint == Hint::Draw && self.sk.is_none() {
            return;
        }
        let text = match hint {
            Hint::Draw => tr("Click once to place the centre of a circle, then click again to set its size. You can also type the size. Other shapes are in the top bar."),
            Hint::PressPull => tr("Your shape is ready. Now pull it out into space: press P (Pull)."),
            Hint::UsePull => tr("Grab the shape and drag with the mouse, or type the height and press Enter."),
            Hint::Navigate => tr("This is your first body. Turn the view: drag with the right or middle mouse button. Move it: the same with Shift held. Zoom: mouse wheel. To draw on any flat face, select it and press K."),
        };
        let pos = match hint {
            Hint::PressPull if self.pull_btn_rect.width() > 1.0 => {
                Pos2::new(self.pull_btn_rect.right() + 12.0, self.pull_btn_rect.top() - 4.0)
            }
            Hint::Navigate => Pos2::new(vp.center().x - 160.0, vp.bottom() - 150.0),
            _ => Pos2::new(vp.center().x - 160.0, vp.top() + 16.0),
        };
        if hint == Hint::PressPull && self.pull_btn_rect.width() > 1.0 {
            // the Pull button pulses, quietly
            let t = ctx.input(|i| i.time) as f32;
            let k = 0.5 + 0.5 * (t * 4.0).sin();
            let col = Color32::from_rgba_unmultiplied(255, 190, 90, (40.0 + 150.0 * k) as u8);
            ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("guide_pulse")))
                .rect_stroke(
                    self.pull_btn_rect.expand(2.0 + 2.0 * k),
                    5.0,
                    Stroke::new(2.0, col),
                    egui::StrokeKind::Outside,
                );
            ctx.request_repaint();
        }
        let mut close = false;
        let mut off = false;
        egui::Area::new(egui::Id::new("guide_card"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .stroke(Stroke::new(1.5, crate::icons::file_frame_color(ui.visuals().dark_mode)))
                    .show(ui, |ui| {
                        ui.set_max_width(320.0);
                        ui.label(text);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button(tr("Close")).clicked() {
                                close = true;
                            }
                            if ui.small_button(tr("Do not show the guide again")).clicked() {
                                off = true;
                            }
                        });
                    });
            });
        if close && self.guide.close() {
            self.settings.guide_mask = self.guide.mask;
            self.settings.save();
        }
        if off {
            self.guide.switch_off();
            self.settings.guide_enabled = false;
            self.settings.save();
        }
    }

    /// About window: start the guide from the beginning.
    pub(super) fn restart_guide(&mut self) {
        self.guide.restart();
        self.settings.guide_enabled = true;
        self.settings.guide_mask = 0;
        self.settings.save();
        if self.doc.bodies().is_empty() && self.sk.is_none() && self.settings.new_doc_sketch {
            self.start_first_steps(0.0);
        }
    }
}
