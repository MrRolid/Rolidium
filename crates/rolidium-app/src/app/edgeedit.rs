// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Live dimensions to the two nearest straight edges: the typed values (Tab switches, Enter confirms, Esc cancels) and how they are drawn. Used while a
//! circle is placed in a sketch, while a hole or a pin is moved on its face, and while the second piece of a pattern is placed.

use super::*;
use crate::i18n::parse_num;
use rolidium_doc::rolidium_sketch::edgedims::{nearest_edges, EdgeDims, EdgeSnap};

pub use crate::sketchmode::EdgeEdit;

impl App {
    /// Draws the two distances of the point `p` (a point of `plane`) to the nearest edges: a line to each edge with the number on it (the typed text
    /// when there is one), and the marks of the snaps.
    pub(super) fn paint_edge_dims(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        plane: &PlaneDef,
        p: DVec2,
        dims: &EdgeDims,
        edit: &EdgeEdit,
        snap: Option<EdgeSnap>,
    ) {
        let painter = ui.painter().with_clip_rect(rect);
        let col = crate::theme::rgb(255, 150, 60);
        let font = egui::FontId::proportional(13.0);
        let Some(ps) = self.project(plane.to_3d(p).as_vec3(), rect) else {
            return;
        };
        for (i, e) in [Some(dims.first), dims.second].into_iter().enumerate() {
            let Some(e) = e else { continue };
            let Some(fs) = self.project(plane.to_3d(e.foot).as_vec3(), rect) else {
                continue;
            };
            painter.line_segment([ps, fs], Stroke::new(1.6, col));
            painter.circle_filled(fs, 3.0, col);
            let mid = ps + (fs - ps) * 0.5;
            let typed = !edit.text[i].is_empty();
            let txt = if typed {
                edit.text[i].clone()
            } else {
                format!("{:.2}", e.dist)
            };
            let w = 9.0 + 7.5 * txt.chars().count().max(4) as f32;
            let r = Rect::from_center_size(mid, Vec2::new(w, 19.0));
            painter.rect_filled(r, 3.0, Color32::from_black_alpha(215));
            let active = edit.on && edit.active == i;
            painter.rect_stroke(
                r,
                3.0,
                Stroke::new(
                    1.5,
                    if active {
                        crate::theme::rgb(255, 215, 90)
                    } else if snap == Some(EdgeSnap::Equal) {
                        crate::theme::rgb(120, 255, 160)
                    } else {
                        Color32::from_gray(90)
                    },
                ),
                egui::StrokeKind::Inside,
            );
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                txt,
                font.clone(),
                if typed {
                    crate::theme::rgb(255, 235, 120)
                } else {
                    Color32::from_gray(225)
                },
            );
            if snap == Some(EdgeSnap::Middle(i)) {
                if let Some(ms) = self.project(plane.to_3d(e.middle()).as_vec3(), rect) {
                    let g = crate::theme::rgb(120, 255, 160);
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            ms + Vec2::new(0.0, -7.0),
                            ms + Vec2::new(6.0, 5.0),
                            ms + Vec2::new(-6.0, 5.0),
                        ],
                        g,
                        Stroke::new(1.0, Color32::BLACK),
                    ));
                }
            }
        }
        if let Some(k) = snap {
            let text = match k {
                EdgeSnap::Equal => tr("equal distance"),
                EdgeSnap::Middle(_) => tr("edge middle"),
            };
            painter.text(
                ps + Vec2::new(14.0, 12.0),
                egui::Align2::LEFT_TOP,
                text,
                font,
                crate::theme::rgb(120, 255, 160),
            );
        }
    }
}

/// What the distances of the selection are measured for.
#[derive(Clone)]
pub enum EdgeSubject {
    /// A circle of a sketch (moved by changing its centre).
    Circle {
        sketch: SketchId,
        entity: rolidium_doc::rolidium_sketch::EntityId,
        old: Entity,
    },
    /// A hole or a pin of a body (moved with `MoveFeature`); `centre` is where its axis meets the plane of the face it stands in.
    Hole { body: BodyId, face: FaceId },
}

/// The selected circle or hole with its two nearest straight edges.
#[derive(Clone)]
pub struct EdgeCtx {
    pub subject: EdgeSubject,
    pub plane: PlaneDef,
    pub centre: DVec2,
    pub dims: EdgeDims,
}

impl App {
    /// The straight edges (pairs of points of `plane`) of the faces of `body` that lie in the plane, and the straight shapes of the sketches in it.
    fn plane_straight_edges(&self, plane: &PlaneDef, body: Option<BodyId>) -> Vec<(DVec2, DVec2)> {
        let mut out = self.sketch_segments(plane, body);
        if let Some(solid) = body.and_then(|b| self.doc.body(b)).map(|b| &b.solid) {
            let n = plane.normal.normalize();
            for f in solid.faces() {
                let Ok(i) = solid.face_info(f) else { continue };
                if i.kind != FaceKind::Plane
                    || i.normal.normalize().dot(n).abs() < 1.0 - 1e-6
                    || (i.origin - plane.origin).dot(n).abs() > 1e-5
                {
                    continue;
                }
                if let Ok(es) = solid.face_edges(f) {
                    out.extend(
                        es.into_iter()
                            .filter_map(|e| solid.edge_info(e).ok())
                            .filter(|i| i.kind == rolidium_kernel::EdgeKind::Line)
                            .map(|i| (plane.to_2d(i.start), plane.to_2d(i.end))),
                    );
                }
            }
        }
        out
    }

    /// The selected circle of a sketch or the selected hole or pin, with the distances to its two nearest straight edges.
    fn edge_ctx_of_selection(&self) -> Option<EdgeCtx> {
        if let Some((s, id)) = self.entity_sel {
            let sk = self.doc.sketch(s)?;
            let e = *sk.sketch.entity(id)?;
            let Entity::Circle { center, .. } = e else {
                return None;
            };
            let segs = self.plane_straight_edges(&sk.plane, sk.target);
            let dims = nearest_edges(center, &segs)?;
            return Some(EdgeCtx {
                subject: EdgeSubject::Circle {
                    sketch: s,
                    entity: id,
                    old: e,
                },
                plane: sk.plane,
                centre: center,
                dims,
            });
        }
        let [(b, f)] = self.selection[..] else {
            return None;
        };
        let solid = &self.doc.body(b)?.solid;
        if solid.face_count() <= 3
            || solid.face_count() > 400
            || solid.cylinder_info(FaceId(f)).ok().flatten().is_none()
        {
            return None;
        }
        let rim = solid.feature_rim(FaceId(f)).ok().flatten()?;
        let plane = PlaneDef::from_face(rim.origin, rim.normal);
        let segs: Vec<_> = rim
            .edges
            .iter()
            .map(|(a, b)| (plane.to_2d(*a), plane.to_2d(*b)))
            .collect();
        let centre = plane.to_2d(rim.centre);
        let dims = nearest_edges(centre, &segs)?;
        Some(EdgeCtx {
            subject: EdgeSubject::Hole {
                body: b,
                face: FaceId(f),
            },
            plane,
            centre,
            dims,
        })
    }

    /// The command that moves the subject so that its distances to the nearest edges are `d1` and `d2` (`None`: as they are).
    pub(super) fn edge_move_command(ctx: &EdgeCtx, d1: Option<f64>, d2: Option<f64>) -> Option<Command> {
        let at = ctx.dims.place(d1, d2);
        if (at - ctx.centre).length() < 1e-9 {
            return None;
        }
        Some(match &ctx.subject {
            EdgeSubject::Circle { sketch, entity, old } => {
                let Entity::Circle { radius, .. } = *old else {
                    return None;
                };
                Command::ReplaceEntity {
                    sketch: *sketch,
                    entity: *entity,
                    new: Entity::Circle { center: at, radius },
                }
            }
            EdgeSubject::Hole { body, face } => Command::MoveFeature {
                body: *body,
                face: *face,
                delta: ctx.plane.to_3d(at) - ctx.plane.to_3d(ctx.centre),
            },
        })
    }

    /// The two distances of the selected circle or hole to its nearest edges as fields in the right column (Enter moves it).
    #[allow(deprecated)]
    pub(super) fn edge_distance_ui(&mut self, ui: &mut egui::Ui) {
        let key = format!(
            "{:?}|{:?}|{}",
            self.entity_sel,
            self.selection,
            self.doc.revision()
        );
        if self.dim.edge_key != key {
            self.dim.edge_key = key;
            self.dim.edge_ctx = self.edge_ctx_of_selection();
            self.dim.edge_text = match &self.dim.edge_ctx {
                Some(c) => [
                    fmt_num(c.dims.first.dist),
                    c.dims.second.map(|e| fmt_num(e.dist)).unwrap_or_default(),
                ],
                None => Default::default(),
            };
        }
        let Some(ctx) = self.dim.edge_ctx.clone() else {
            return;
        };
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr("Distance to the nearest edges")).strong());
        let mut apply = false;
        for i in 0..2 {
            if i == 1 && ctx.dims.second.is_none() {
                break;
            }
            ui.horizontal(|ui| {
                ui.label(if i == 0 { tr("Edge 1") } else { tr("Edge 2") });
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.dim.edge_text[i])
                        .id(egui::Id::new(("edge_dist", i)))
                        .desired_width(60.0),
                );
                ui.label("mm");
                self.probe_widget(if i == 0 { "edge_dist_0" } else { "edge_dist_1" }, r.rect);
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
            });
        }
        ui.weak(tr(
            "Type the distances and press Enter: the circle or the hole moves there.",
        ));
        if apply {
            let v = |s: &str| parse_num(s).filter(|x| *x >= 0.0);
            let (a, b) = (v(&self.dim.edge_text[0]), v(&self.dim.edge_text[1]));
            match Self::edge_move_command(&ctx, a, b) {
                Some(cmd) => self.submit(cmd),
                None => self.set_status("Please enter a number", true),
            }
        }
    }
}

/// A hole or a pin that is dragged on the face it stands in.
pub struct HoleDrag {
    pub body: BodyId,
    pub face: FaceId,
    pub plane: PlaneDef,
    /// Where the axis of the feature meets the plane when the drag began, and where the pointer took hold of the plane.
    pub centre0: DVec2,
    pub grab: DVec2,
    pub radius: f64,
    pub edges: Vec<(DVec2, DVec2)>,
    /// The place the feature goes to now (the pointer, the snaps and the typed distances together) and its live distances.
    pub cur: DVec2,
    pub dims: Option<EdgeDims>,
    pub snap: Option<EdgeSnap>,
    pub edit: EdgeEdit,
}

impl App {
    /// A drag that starts on the round face of a hole or a pin takes the feature along the face it stands in (Select tool).
    pub(super) fn hole_drag_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        hovered_face: Option<(BodyId, u32)>,
    ) {
        if self.hole_drag.is_none() && resp.drag_started_by(PointerButton::Primary) && self.idle() {
            let Some((b, f)) = hovered_face.filter(|(b, f)| self.is_feature_face(*b, *f)) else {
                return;
            };
            if self.doc.state().links.iter().any(|l| l.results.contains(&b)) {
                self.set_status(
                    "This body is made by a link: remove the link to move a hole in it",
                    true,
                );
                return;
            }
            let Some(solid) = self.doc.body(b).map(|x| &x.solid) else {
                return;
            };
            if solid.face_count() > 400 {
                self.set_status("The body has too many faces to move a hole by dragging: type the distances in the right column", true);
                return;
            }
            let Ok(Some(rim)) = solid.feature_rim(FaceId(f)) else {
                self.set_status(
                    "This hole does not stand in a flat face: it cannot be dragged",
                    true,
                );
                return;
            };
            let radius = solid
                .cylinder_info(FaceId(f))
                .ok()
                .flatten()
                .map_or(1.0, |c| c.radius);
            let plane = PlaneDef::from_face(rim.origin, rim.normal);
            let aspect = rect.width() / rect.height().max(1.0);
            let press = resp.ctx.input(|i| i.pointer.press_origin()).or(pointer);
            let Some(p) = press else { return };
            let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
            let Some((grab, _)) = plane.ray_hit(o.as_dvec3(), d.as_dvec3()) else {
                return;
            };
            let edges = rim
                .edges
                .iter()
                .map(|(a, b)| (plane.to_2d(*a), plane.to_2d(*b)))
                .collect();
            let centre0 = plane.to_2d(rim.centre);
            self.selection = vec![(b, f)];
            self.body_sel = None;
            // the panel of the hole next to it has the keyboard focus: while the hole is dragged the typed numbers are the distances
            resp.ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            self.hole_drag = Some(HoleDrag {
                body: b,
                face: FaceId(f),
                plane,
                centre0,
                grab,
                radius,
                edges,
                cur: centre0,
                dims: None,
                snap: None,
                edit: EdgeEdit::default(),
            });
            self.set_status("Drag the hole or the pin, or type the distances to the two edges (Tab switches, Enter confirms, Esc cancels)", false);
            self.layers_changed = true;
        }
        if self.hole_drag.is_none() {
            return;
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let Some(p) = pointer {
                let aspect = rect.width() / rect.height().max(1.0);
                let (o, d) = self.camera.ray(Self::ndc(p, rect), aspect);
                let mpp_at = self.hole_drag.as_ref().map(|h| h.centre0);
                let mpp = mpp_at
                    .zip(self.hole_drag.as_ref().map(|h| h.plane))
                    .map_or(0.1, |(c, pl)| self.mm_per_px(&pl, c, rect));
                if let Some(h) = &mut self.hole_drag {
                    if let Some((hit, _)) = h.plane.ray_hit(o.as_dvec3(), d.as_dvec3()) {
                        let raw = h.centre0 + (hit - h.grab);
                        h.update(raw, 8.0 * mpp);
                    }
                }
            }
            self.layers_changed = true;
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            if self.hole_drag.as_ref().is_some_and(|h| h.edit.any_typed()) {
                self.set_status(
                    "Press Enter to put the hole at the typed distances, Esc to cancel",
                    false,
                );
            } else {
                self.commit_hole_drag();
            }
        }
    }

    pub(super) fn cancel_hole_drag(&mut self) {
        if self.hole_drag.take().is_some() {
            self.layers_changed = true;
            self.set_status("Cancelled", false);
        }
    }

    pub(super) fn commit_hole_drag(&mut self) {
        let Some(h) = self.hole_drag.take() else { return };
        self.layers_changed = true;
        let delta = h.plane.to_3d(h.cur) - h.plane.to_3d(h.centre0);
        if delta.length() < 1e-6 {
            self.set_status("The hole stays where it is", false);
            return;
        }
        self.submit(Command::MoveFeature {
            body: h.body,
            face: h.face,
            delta,
        });
    }

    /// The outline of the hole at the place it is dragged to (and the edges it is measured to).
    pub(super) fn hole_drag_layers(&self) -> Vec<PreviewDraw> {
        let Some(h) = &self.hole_drag else {
            return Vec::new();
        };
        let mut ring = Vec::new();
        let n = 48;
        for i in 0..n {
            let (a0, a1) = (
                i as f64 / n as f64 * std::f64::consts::TAU,
                (i + 1) as f64 / n as f64 * std::f64::consts::TAU,
            );
            for a in [a0, a1] {
                let p = h.cur + DVec2::new(a.cos(), a.sin()) * h.radius;
                ring.push((h.plane.to_3d(p) + h.plane.normal.normalize() * 0.02).as_vec3());
            }
        }
        vec![PreviewDraw {
            thick: ring,
            thick_color: [1.0, 0.85, 0.35, 1.0],
            ..Default::default()
        }]
    }

    pub(super) fn hole_drag_paint(&self, ui: &egui::Ui, rect: Rect) {
        let Some(h) = &self.hole_drag else { return };
        if let Some(d) = &h.dims {
            self.paint_edge_dims(ui, rect, &h.plane, h.cur, d, &h.edit, h.snap);
        }
    }
}

impl HoleDrag {
    /// Takes the raw place under the pointer: the snaps (equal distance, middle of an edge) and the typed distances give the place `cur`.
    pub fn update(&mut self, raw: DVec2, tol: f64) {
        let dims = nearest_edges(raw, &self.edges);
        let (mut pos, mut snap) = (raw, None);
        if let Some(d) = &dims {
            let (p, k) = d.snap(raw, tol);
            if k.is_some() {
                (pos, snap) = (p, k);
            }
            let (a, b) = self.edit.values();
            if a.is_some() || b.is_some() {
                pos = d.place(a, b);
                snap = None;
            }
        }
        self.cur = pos;
        self.snap = snap;
        self.dims = if pos != raw {
            nearest_edges(pos, &self.edges)
        } else {
            dims
        };
    }
}
