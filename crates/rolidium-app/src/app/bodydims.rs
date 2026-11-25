// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The size of a selected body: three dimensions (like the Measure tool draws them) round its bounding box, in X, Y and Z. A click on a size opens it for
//! typing. The size is changed by moving the one flat wall at the end of that direction when there is one (a solid keeps its rounds, a mesh stretches its
//! neighbours), else by scaling in that direction. Several bodies are scaled as a whole.

#![allow(deprecated)]

use super::*;

#[derive(Default)]
pub struct BodyDimsUi {
    /// The direction (0 x, 1 y, 2 z) whose size is being typed, and the text.
    pub editing: Option<usize>,
    pub text: String,
    /// "Keep the proportions": the same scale in all directions.
    pub proportional: bool,
    /// "Symmetric about the centre": the middle stays where it is, else the end at the negative side does.
    pub symmetric: bool,
    focus: bool,
    /// The box of the bodies for the key it was made for.
    cache: Option<BoxCache>,
    /// The sizes shown, for the scripted tests.
    pub shown: Option<[f64; 3]>,
}

/// The key of a box that was made (which bodies, which version of each), and the box.
type BoxCache = (Vec<(BodyId, usize)>, [f64; 3], [f64; 3]);

/// The wall that ends a body in one direction.
enum EndWall {
    /// A face of a solid or a wall of a mesh (its number in the entry), with the normal pointing out of the body.
    Face {
        body: BodyId,
        face: u32,
        origin: DVec3,
        normal: Vec3,
    },
}

fn fmt_size(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl App {
    /// The bodies (and meshes) whose size is shown: those selected as bodies, when nothing finer is selected and the Select tool is on.
    fn dims_bodies(&self) -> Vec<BodyId> {
        if self.tool != Tool::Select
            || !self.settings.body_dims
            || self.sk.is_some()
            || self.pull.is_some()
            || self.move_ui.drag.is_some()
            || self.shape_drag.is_some()
            || self.hole_drag.is_some()
            || !self.selection.is_empty()
            || !self.edge_sel.is_empty()
            || self.region_sel.is_some()
            || self.entity_sel.is_some()
        {
            return Vec::new();
        }
        let mut v: Vec<BodyId> = self.move_sel.clone();
        if let Some(b) = self.body_sel {
            if !v.contains(&b) {
                v.push(b);
            }
        }
        v.retain(|b| {
            self.doc.body(*b).is_some_and(|x| x.visible) || self.doc.mesh(*b).is_some_and(|x| x.visible)
        });
        v
    }

    /// The box round the bodies: from the display entries (faces' boxes) of the solids, from the vertices of the meshes (kept while the mesh is the same).
    fn dims_box(&mut self, bodies: &[BodyId]) -> Option<([f64; 3], [f64; 3])> {
        let key: Vec<(BodyId, usize)> = bodies
            .iter()
            .map(|b| {
                let k = match self.doc.mesh(*b) {
                    Some(m) => Arc::as_ptr(&m.mesh) as usize,
                    None => self.entry(*b).map_or(0, |e| e.stamp as usize),
                };
                (*b, k)
            })
            .collect();
        if let Some((k, lo, hi)) = &self.body_dims.cache {
            if *k == key {
                return Some((*lo, *hi));
            }
        }
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for b in bodies {
            let (l, h) = if let Some(m) = self.doc.mesh(*b) {
                m.mesh.bounding_box()?
            } else {
                // the exact box of the solid (the display mesh of a round body is a little smaller than the body)
                let (l, h) = self.doc.body(*b)?.solid.bounding_box().ok()?;
                ([l.x, l.y, l.z], [h.x, h.y, h.z])
            };
            for i in 0..3 {
                lo[i] = lo[i].min(l[i]);
                hi[i] = hi[i].max(h[i]);
            }
        }
        if lo
            .iter()
            .chain(hi.iter())
            .any(|v| !v.is_finite() || v.abs() > 1e12)
        {
            return None;
        }
        self.body_dims.cache = Some((key, lo, hi));
        Some((lo, hi))
    }

    /// Whether the bodies have round faces (a change of the scale in one direction turns them into ellipses).
    fn dims_have_round(&self, bodies: &[BodyId]) -> bool {
        bodies.iter().any(|b| {
            self.entry(*b).is_some_and(|e| {
                e.faces.iter().any(|f| {
                    matches!(
                        f.kind,
                        FaceKind::Cylinder | FaceKind::Sphere | FaceKind::Cone | FaceKind::Torus
                    )
                })
            })
        })
    }

    /// The one flat wall that ends the body at the `plus` or the minus side in direction `a`, if there is exactly one.
    fn dims_end_wall(
        &self,
        body: BodyId,
        a: usize,
        lo: [f64; 3],
        hi: [f64; 3],
        plus: bool,
    ) -> Option<EndWall> {
        let e = self.entry(body)?;
        let size = (0..3).map(|i| hi[i] - lo[i]).fold(0.0, f64::max);
        let tol = size * 1e-4 + 1e-6;
        let end = if plus { hi[a] } else { lo[a] };
        let mut found = None;
        for (i, f) in e.faces.iter().enumerate() {
            if f.kind != FaceKind::Plane {
                continue;
            }
            let n = f.normal.normalize_or_zero();
            let na = [n.x, n.y, n.z][a];
            if na.abs() < 0.999 || (na > 0.0) != plus {
                continue;
            }
            let at = [f.origin.x, f.origin.y, f.origin.z][a];
            if (at - end).abs() > tol {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(EndWall::Face {
                body,
                face: i as u32,
                origin: f.origin,
                normal: n.as_vec3(),
            });
        }
        found
    }

    /// The sizes, the lines and the fields: once a frame, after the 3D view.
    pub(super) fn body_dims_ui(&mut self, ctx: &egui::Context, rect: Rect) {
        let bodies = self.dims_bodies();
        if bodies.is_empty() {
            self.body_dims.editing = None;
            self.body_dims.shown = None;
            return;
        }
        let Some((lo, hi)) = self.dims_box(&bodies) else {
            return;
        };
        self.body_dims.shown = Some([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]);
        let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
        let off = diag * 0.07;
        let p = |x: f64, y: f64, z: f64| Vec3::new(x as f32, y as f32, z as f32);
        // line a of the dimension, the two points it runs between and where the body's corners are that the extension lines start at
        let lines: [(Vec3, Vec3, Vec3, Vec3); 3] = [
            (
                p(lo[0], lo[1] - off, lo[2]),
                p(hi[0], lo[1] - off, lo[2]),
                p(lo[0], lo[1], lo[2]),
                p(hi[0], lo[1], lo[2]),
            ),
            (
                p(hi[0] + off, lo[1], lo[2]),
                p(hi[0] + off, hi[1], lo[2]),
                p(hi[0], lo[1], lo[2]),
                p(hi[0], hi[1], lo[2]),
            ),
            (
                p(lo[0] - off, lo[1] - off, lo[2]),
                p(lo[0] - off, lo[1] - off, hi[2]),
                p(lo[0], lo[1], lo[2]),
                p(lo[0], lo[1], hi[2]),
            ),
        ];
        let painter = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("body_dims_lines"),
            ))
            .with_clip_rect(rect);
        let col = crate::theme::rgb(255, 214, 90);
        let mut label_pos: [Option<Pos2>; 3] = [None; 3];
        for (a, (p0, p1, c0, c1)) in lines.iter().enumerate() {
            let (Some(s0), Some(s1)) = (self.project(*p0, rect), self.project(*p1, rect)) else {
                continue;
            };
            // the extension lines run from the body to the dimension line (the third, upright one from the two ends of the edge)
            let ext0 = if a == 2 { lines[2].2 } else { *c0 };
            let ext1 = if a == 2 { lines[2].3 } else { *c1 };
            let thin = Stroke::new(1.0, col.gamma_multiply(0.7));
            if let (Some(e0), Some(e1)) = (self.project(ext0, rect), self.project(ext1, rect)) {
                painter.line_segment([e0, s0], thin);
                painter.line_segment([e1, s1], thin);
            }
            Self::arrow(&painter, s0, s1, col);
            label_pos[a] = Some(s0.lerp(s1, 0.5) + Vec2::new(0.0, -14.0));
        }
        let sizes = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let names = ["x", "y", "z"];
        let mut start_edit: Option<usize> = None;
        let mut apply: Option<(usize, f64)> = None;
        let mut cancel = false;
        for a in 0..3 {
            let Some(at) = label_pos[a] else { continue };
            let editing = self.body_dims.editing == Some(a);
            egui::Area::new(egui::Id::new(("body_dim", a)))
                .fixed_pos(at - Vec2::new(36.0, 11.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style())
                        .inner_margin(egui::Margin::symmetric(5, 2))
                        .show(ui, |ui| {
                            if editing {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}:", names[a].to_uppercase()));
                                    let r = ui.add(
                                        egui::TextEdit::singleline(&mut self.body_dims.text)
                                            .desired_width(70.0)
                                            .id(egui::Id::new("body_dim_field")),
                                    );
                                    self.probe_widget("body_dim_field", r.rect);
                                    if self.body_dims.focus {
                                        r.request_focus();
                                        self.body_dims.focus = false;
                                    }
                                    ui.label("mm");
                                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                        match parse_num(&self.body_dims.text) {
                                            Some(v) if v.is_finite() && v > 0.0 => apply = Some((a, v)),
                                            _ => self.set_status("The size must be a positive number", true),
                                        }
                                    } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                                        cancel = true;
                                    }
                                });
                            } else {
                                let r = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{} mm", fmt_size(sizes[a]))).color(col),
                                    )
                                    .sense(Sense::click()),
                                );
                                self.probe_widget(&format!("body_dim_{}", names[a]), r.rect);
                                let r = r.on_hover_text(tr("Click to type a new size"));
                                if r.clicked() {
                                    start_edit = Some(a);
                                }
                            }
                        });
                });
        }
        if let Some(a) = self.body_dims.editing {
            if let Some(at) = label_pos[a] {
                let single = bodies.len() == 1;
                let wall = single && self.dims_end_wall(bodies[0], a, lo, hi, true).is_some();
                let round = self.dims_have_round(&bodies);
                egui::Area::new(egui::Id::new("body_dim_options"))
                    .fixed_pos(at + Vec2::new(-36.0, 18.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            let r = ui.checkbox(&mut self.body_dims.proportional, tr("Keep the proportions"));
                            self.probe_widget("body_dim_proportional", r.rect);
                            r.on_hover_text(tr("The same scale in all directions"));
                            let r = ui.checkbox(&mut self.body_dims.symmetric, tr("Symmetric about the centre"));
                            self.probe_widget("body_dim_symmetric", r.rect);
                            let moves = single && wall && !self.body_dims.proportional;
                            ui.weak(if moves {
                                tr("The wall at the end is moved")
                            } else {
                                tr("The body is scaled in this direction")
                            });
                            if round && !moves && !self.body_dims.proportional {
                                ui.colored_label(
                                    Color32::from_rgb(230, 170, 60),
                                    tr("Circles of the body become ellipses (tick “Keep the proportions” to avoid it)"),
                                );
                            }
                        });
                    });
            }
        }
        if let Some(a) = start_edit {
            self.body_dims.editing = Some(a);
            self.body_dims.text = fmt_size(sizes[a]);
            self.body_dims.focus = true;
        }
        if cancel {
            self.body_dims.editing = None;
        }
        if let Some((a, v)) = apply {
            self.body_dims.editing = None;
            self.apply_body_dim(&bodies, a, v, lo, hi);
        }
    }

    /// Changes the size of the bodies in direction `a` to `v`.
    fn apply_body_dim(&mut self, bodies: &[BodyId], a: usize, v: f64, lo: [f64; 3], hi: [f64; 3]) {
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        let s = hi[a] - lo[a];
        if s < 1e-9 || (v - s).abs() < 1e-9 {
            return;
        }
        let (symmetric, proportional) = (self.body_dims.symmetric, self.body_dims.proportional);
        let delta = v - s;
        let axis_v = |k: f64| {
            let mut x = [0.0; 3];
            x[a] = k;
            DVec3::from_array(x)
        };
        // one flat wall at the end: it is moved (a solid keeps its rounds, a mesh stretches its neighbours)
        if bodies.len() == 1 && !proportional {
            let b = bodies[0];
            let wall = self
                .dims_end_wall(b, a, lo, hi, true)
                .map(|w| (w, true))
                .or_else(|| {
                    if symmetric {
                        None
                    } else {
                        self.dims_end_wall(b, a, lo, hi, false).map(|w| (w, false))
                    }
                });
            if let Some((
                EndWall::Face {
                    body,
                    face,
                    origin,
                    normal,
                },
                plus,
            )) = wall
            {
                let mut doable = true;
                if self.doc.mesh(body).is_some() {
                    // the mesh: only when the pull can be made (neighbours square to the wall, not through the opposite wall)
                    doable = self.mesh_pull_check(body, face, delta).is_none()
                        && rolidium_doc::display::regions_of(
                            &self.doc.mesh(body).map(|m| m.mesh.clone()).unwrap(),
                        )
                        .is_some();
                }
                if doable {
                    let kind = PullKind::Face { body, face };
                    self.pending = None;
                    self.submit_pull(kind, origin, normal, delta, PullMode::Auto);
                    if symmetric && plus && !self.idle() {
                        // the middle stays: the whole body moves back by half of the change
                        self.after_unlink = Some(Command::Move {
                            bodies: vec![body],
                            translation: axis_v(-delta / 2.0),
                            pivot: DVec3::ZERO,
                            axis: DVec3::Z,
                            angle: 0.0,
                            copy: false,
                        });
                    }
                    return;
                }
            }
        }
        // else: scaled (the whole group, or one direction)
        let f = v / s;
        let factors = if proportional {
            DVec3::splat(f)
        } else {
            let mut x = [1.0; 3];
            x[a] = f;
            DVec3::from_array(x)
        };
        let mut c = [0.0; 3];
        for i in 0..3 {
            c[i] = if symmetric { (lo[i] + hi[i]) / 2.0 } else { lo[i] };
        }
        self.pending = None;
        crate::logging::log(&format!(
            "size of bodies {bodies:?} in direction {a} to {v} mm: scaled by {factors:?} about {c:?} (symmetric {symmetric}, proportional {proportional}), box {lo:?} {hi:?}"
        ));
        self.submit(Command::ScaleBodies {
            bodies: bodies.to_vec(),
            centre: DVec3::from_array(c),
            factors,
        });
    }
}
