// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Panels next to an object (Edges, face, dimensions, Move, Combine, Loft, Revolve) that can be dragged away by their
//! header so they do not hide the model. A dashed line leads from a moved panel to the place it belongs to, a panel
//! never leaves the window, a double click on the header puts it back, and the place is remembered per kind of panel
//! until the program ends.

#![allow(deprecated)]

use eframe::egui::{self, Color32, CursorIcon, Id, Order, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::HashMap;

#[derive(Default)]
pub struct Docks {
    moved: HashMap<&'static str, Pos2>,
    size: HashMap<&'static str, Vec2>,
    last_click: HashMap<&'static str, f64>,
}

pub struct DockOut {
    pub dismiss: bool,
    #[allow(dead_code)]
    pub rect: Rect,
}

/// The position of a panel of `size` clamped so that the whole panel is inside `screen`.
pub fn clamp_inside(p: Pos2, size: Vec2, screen: Rect) -> Pos2 {
    let m = 2.0;
    let max_x = (screen.max.x - size.x - m).max(screen.min.x + m);
    let max_y = (screen.max.y - size.y - m).max(screen.min.y + m);
    Pos2::new(
        p.x.clamp(screen.min.x + m, max_x),
        p.y.clamp(screen.min.y + m, max_y),
    )
}

impl Docks {
    /// Where the panel of this kind is now (its default place while it was not moved).
    #[allow(dead_code)]
    pub fn position(&self, kind: &'static str, default_pos: Pos2) -> Pos2 {
        self.moved.get(kind).copied().unwrap_or(default_pos)
    }

    pub fn is_moved(&self, kind: &'static str) -> bool {
        self.moved.contains_key(kind)
    }

    /// Shows a panel. `default_pos` is where it appears; `anchor` the screen point it belongs to (a dashed line leads
    /// there once the panel was moved away).
    #[allow(clippy::too_many_arguments)]
    pub fn panel(
        &mut self,
        ctx: &egui::Context,
        kind: &'static str,
        default_pos: Pos2,
        anchor: Option<Pos2>,
        width: f32,
        title: &str,
        closable: bool,
        body: impl FnOnce(&mut egui::Ui),
    ) -> DockOut {
        let screen = ctx.content_rect();
        let size = self
            .size
            .get(kind)
            .copied()
            .unwrap_or(Vec2::new(width + 16.0, 140.0));
        let pos = clamp_inside(self.moved.get(kind).copied().unwrap_or(default_pos), size, screen);
        // the line to the place the panel belongs to, under the panel
        if let (Some(a), true) = (anchor, self.is_moved(kind)) {
            let rect = Rect::from_min_size(pos, size);
            if !rect.contains(a) {
                let from = Pos2::new(
                    a.x.clamp(rect.min.x, rect.max.x),
                    a.y.clamp(rect.min.y, rect.max.y),
                );
                let painter =
                    ctx.layer_painter(egui::LayerId::new(Order::Middle, Id::new(("dock_leader", kind))));
                let stroke = Stroke::new(1.5, Color32::from_rgb(255, 200, 100));
                for s in egui::Shape::dashed_line(&[from, a], stroke, 8.0, 4.0) {
                    painter.add(s);
                }
                painter.circle_filled(a, 3.5, Color32::from_rgb(255, 200, 100));
            }
        }
        let mut drag = Vec2::ZERO;
        let mut reset = false;
        let mut dismiss = false;
        let out = egui::Area::new(Id::new(("dock", kind)))
            .fixed_pos(pos)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        let t = ui
                            .add(
                                egui::Label::new(egui::RichText::new(title).strong())
                                    .sense(Sense::click_and_drag()),
                            )
                            .on_hover_cursor(CursorIcon::Grab);
                        let spare = (ui.available_width() - if closable { 26.0 } else { 0.0 }).max(0.0);
                        let s = ui
                            .allocate_response(Vec2::new(spare, 18.0), Sense::click_and_drag())
                            .on_hover_cursor(CursorIcon::Grab);
                        for r in [&t, &s] {
                            drag += r.drag_delta();
                            // two clicks within half a second count as a double click even when the frames between
                            // them were slow
                            if r.clicked() {
                                let now = ui.input(|i| i.time);
                                if self.last_click.get(kind).is_some_and(|t| now - t < 0.5) {
                                    reset = true;
                                    self.last_click.remove(kind);
                                } else {
                                    self.last_click.insert(kind, now);
                                }
                            }
                            if r.double_clicked() {
                                reset = true;
                            }
                        }
                        if closable && ui.small_button("x").clicked() {
                            dismiss = true;
                        }
                    });
                    body(ui);
                });
            });
        let rect = out.response.rect;
        self.size.insert(kind, rect.size());
        if reset {
            self.moved.remove(kind);
        } else if drag != Vec2::ZERO {
            self.moved
                .insert(kind, clamp_inside(pos + drag, rect.size(), screen));
        }
        DockOut { dismiss, rect }
    }
}

impl super::App {
    /// The middle of a body on the screen (the place a moved panel points to).
    pub(super) fn body_center_screen(&self, body: rolidium_doc::BodyId, rect: Rect) -> Option<Pos2> {
        let m = self.entry(body)?;
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &m.mesh.positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if lo.x > hi.x {
            return None;
        }
        self.project((lo + hi) * 0.5, rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_never_leaves_the_window() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 600.0));
        let size = Vec2::new(250.0, 200.0);
        assert_eq!(
            clamp_inside(Pos2::new(-500.0, -50.0), size, screen),
            Pos2::new(2.0, 2.0)
        );
        assert_eq!(
            clamp_inside(Pos2::new(5000.0, 5000.0), size, screen),
            Pos2::new(748.0, 398.0)
        );
        assert_eq!(
            clamp_inside(Pos2::new(300.0, 100.0), size, screen),
            Pos2::new(300.0, 100.0)
        );
        // a window smaller than the panel: the panel sticks to the top-left corner
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        assert_eq!(
            clamp_inside(Pos2::new(50.0, 50.0), size, tiny),
            Pos2::new(2.0, 2.0)
        );
    }

    #[test]
    fn the_position_is_remembered_per_kind_and_reset_by_removing_it() {
        let mut d = Docks::default();
        assert_eq!(d.position("move", Pos2::new(10.0, 10.0)), Pos2::new(10.0, 10.0));
        d.moved.insert("move", Pos2::new(300.0, 200.0));
        assert_eq!(d.position("move", Pos2::new(10.0, 10.0)), Pos2::new(300.0, 200.0));
        assert_eq!(d.position("loft", Pos2::new(10.0, 10.0)), Pos2::new(10.0, 10.0));
        d.moved.remove("move");
        assert!(!d.is_moved("move"));
    }
}
