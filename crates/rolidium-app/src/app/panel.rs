// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(deprecated)]
//! The small non-modal panel next to the selected object: sizes of sketch shapes, holes and pins,
//! rounds and chamfers, and for flat faces and sketch regions the move, mode and "up to" controls.

use super::snap::opposite_distance;
use super::*;
use crate::i18n::{parse_num, tr, trf};
use rolidium_doc::rolidium_sketch::EntityId;
use rolidium_doc::{BlendKind, PullMode};

#[derive(Default)]
pub struct DimPanel {
    key: String,
    fields: Vec<String>,
    dismissed: String,
    focus: bool,
}

enum Target {
    Entity {
        sketch: SketchId,
        entity: EntityId,
        e: Entity,
    },
    Edges {
        body: BodyId,
        edges: Vec<u32>,
        extra: usize,
        /// The edges lie where another body touches: the bodies are not combined.
        joint: bool,
    },
    Face {
        body: BodyId,
        face: u32,
        origin: DVec3,
        normal: Vec3,
        /// Complete hole or pin: (cylindrical face, diameter, is a pin).
        hole: Option<(u32, f64, bool)>,
        blend: Option<(BlendKind, f64)>,
        planar: bool,
        opposite: Option<f64>,
    },
    Region {
        sketch: SketchId,
        idx: usize,
        opposite: Option<f64>,
    },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    Entity(usize),
    EdgeSize,
    Diameter,
    BlendSize,
    MoveBy,
    Opposite,
}

enum Act {
    Entity(Vec<String>),
    EdgeSize(f64),
    Diameter(f64),
    BlendSize(f64),
    MoveBy(f64),
    Opposite(f64),
    UpTo,
    Revolve,
}

fn fmt(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl Target {
    fn rows(&self) -> Vec<(Row, String)> {
        match self {
            Target::Entity { e, .. } => match e {
                Entity::Circle { .. } => vec![(Row::Entity(0), tr("Diameter"))],
                Entity::Arc { .. } => vec![(Row::Entity(0), tr("Radius"))],
                Entity::Rect { .. } => vec![(Row::Entity(0), tr("Width")), (Row::Entity(1), tr("Height"))],
                Entity::Line { .. } => vec![(Row::Entity(0), tr("Length"))],
            },
            Target::Edges { .. } => vec![(Row::EdgeSize, tr("Size"))],
            Target::Face {
                hole,
                blend,
                planar,
                opposite,
                ..
            } => {
                let mut v = Vec::new();
                if hole.is_some() {
                    v.push((Row::Diameter, tr("Diameter")));
                }
                if let Some((k, _)) = blend {
                    v.push((
                        Row::BlendSize,
                        tr(if *k == BlendKind::Round {
                            "Radius"
                        } else {
                            "Distance"
                        }),
                    ));
                }
                if *planar {
                    v.push((Row::MoveBy, tr("Move by")));
                    if opposite.is_some() {
                        v.push((Row::Opposite, tr("Distance to opposite face")));
                    }
                }
                v
            }
            Target::Region { opposite, .. } => {
                let mut v = vec![(Row::MoveBy, tr("Move by"))];
                if opposite.is_some() {
                    v.push((Row::Opposite, tr("Distance to opposite face")));
                }
                v
            }
        }
    }

    fn title(&self) -> String {
        match self {
            Target::Entity { e, .. } => match e {
                Entity::Circle { .. } => tr("Circle"),
                Entity::Arc { .. } => tr("Arc"),
                Entity::Rect { .. } => tr("Rectangle"),
                Entity::Line { .. } => tr("Line"),
            },
            Target::Edges { edges, .. } => trf("Edges ({})", &[edges.len().to_string()]),
            Target::Face { hole, blend, .. } => match (blend, hole) {
                (Some((BlendKind::Round, _)), _) => tr("Round"),
                (Some((BlendKind::Chamfer, _)), _) => tr("Chamfer"),
                (None, Some((_, _, pin))) => tr(if *pin { "Pin" } else { "Hole" }),
                _ => tr("Face"),
            },
            Target::Region { .. } => tr("Sketch region"),
        }
    }

    fn has_pull_controls(&self) -> bool {
        match self {
            Target::Face { planar, .. } => *planar,
            Target::Region { .. } => true,
            _ => false,
        }
    }

    fn initial(&self, row: Row, edge_text: &str) -> String {
        match (self, row) {
            (Target::Entity { e, .. }, Row::Entity(i)) => match (*e, i) {
                (Entity::Circle { radius, .. }, _) => fmt(radius * 2.0),
                (Entity::Arc { radius, .. }, _) => fmt(radius),
                (Entity::Rect { a, b }, 0) => fmt((b.x - a.x).abs()),
                (Entity::Rect { a, b }, _) => fmt((b.y - a.y).abs()),
                (Entity::Line { a, b }, _) => fmt(a.distance(b)),
            },
            (_, Row::EdgeSize) => edge_text.to_string(),
            (
                Target::Face {
                    hole: Some((_, d, _)),
                    ..
                },
                Row::Diameter,
            ) => fmt(*d),
            (
                Target::Face {
                    blend: Some((_, s)), ..
                },
                Row::BlendSize,
            ) => fmt(*s),
            (
                Target::Face {
                    opposite: Some(d), ..
                }
                | Target::Region {
                    opposite: Some(d), ..
                },
                Row::Opposite,
            ) => fmt(*d),
            (_, Row::MoveBy) => "0".into(),
            _ => String::new(),
        }
    }
}

impl App {
    fn dim_target(&self) -> Option<(String, Target, Vec3)> {
        // the Measure tool only looks: its selection is not edited
        if self.tool == Tool::Measure {
            return None;
        }
        let rev = self.doc.revision();
        if let Some((s, id)) = self.entity_sel {
            let sk = self.doc.sketch(s)?;
            let e = *sk.sketch.entity(id)?;
            let anchor = match e {
                Entity::Circle { center, .. } | Entity::Arc { center, .. } => center,
                Entity::Line { a, b } | Entity::Rect { a, b } => (a + b) * 0.5,
            };
            let key = format!("e:{s}:{id}:{e:?}");
            return Some((
                key,
                Target::Entity {
                    sketch: s,
                    entity: id,
                    e,
                },
                sk.plane.to_3d(anchor).as_vec3(),
            ));
        }
        if let [(b, f)] = self.selection[..] {
            let entry = self.entry(b)?;
            let meta = entry.faces.get(f as usize)?;
            let hole = match (meta.cyl_diameter, meta.disc) {
                (Some(d), _) => Some((f, d, true)),
                (None, Some((cyl, d))) => Some((cyl, d, false)),
                _ => None,
            }
            .map(|(face, d, _)| {
                let pin = entry
                    .faces
                    .get(face as usize)
                    .map(|m| m.cyl_diameter.is_some())
                    .unwrap_or(false);
                (face, d, pin)
            });
            let blend = self
                .doc
                .body(b)
                .and_then(|bd| bd.features.iter().find(|ft| ft.faces.contains(&FaceId(f))))
                .map(|ft| (ft.kind, ft.size));
            let planar = meta.kind == FaceKind::Plane;
            if hole.is_none() && blend.is_none() && !planar {
                return None;
            }
            let opposite = if planar {
                opposite_distance(&entry.faces, Some(f as usize), meta.origin, meta.normal)
            } else {
                None
            };
            let key = format!("f:{b}:{f}:{rev}");
            return Some((
                key,
                Target::Face {
                    body: b,
                    face: f,
                    origin: meta.origin,
                    normal: meta.normal.as_vec3().normalize_or_zero(),
                    hole,
                    blend,
                    planar,
                    opposite,
                },
                meta.centroid,
            ));
        }
        if let Some((s, i)) = self.region_sel {
            let sr = self.regions.iter().find(|r| r.id == s)?;
            let g = sr.geoms.get(i)?;
            let target = self.doc.sketch(s)?.target;
            // Only when the region lies on the body and there is material below it.
            let inside = target
                .and_then(|t| self.doc.body(t))
                .and_then(|b| {
                    let c = g.centroid.as_dvec3() - sr.plane.normal.normalize() * 1e-3;
                    b.solid.contains_point(c).ok()
                })
                .unwrap_or(false);
            let opposite = target
                .filter(|_| inside)
                .and_then(|t| self.entry(t))
                .and_then(|m| opposite_distance(&m.faces, None, sr.plane.origin, sr.plane.normal));
            return Some((
                format!("r:{s}:{i}:{rev}"),
                Target::Region {
                    sketch: s,
                    idx: i,
                    opposite,
                },
                g.centroid,
            ));
        }
        if let Some(&(b, e0)) = self.edge_sel.first() {
            let edges: Vec<u32> = self
                .edge_sel
                .iter()
                .filter(|(bb, _)| *bb == b)
                .map(|(_, e)| *e)
                .collect();
            let pl = self.edge_polyline(b, e0)?;
            let key = format!("x:{b}:{edges:?}");
            let extra = self.chain_extra(b, &edges).len();
            let joint = self.edges_at_body_joint(b, &edges);
            return Some((
                key,
                Target::Edges {
                    body: b,
                    edges,
                    extra,
                    joint,
                },
                pl[pl.len() / 2],
            ));
        }
        None
    }

    /// Labels of the rows the panel shows for the current selection (used by the self-test).
    pub(super) fn panel_rows(&self) -> Vec<String> {
        self.dim_target()
            .map(|(_, t, _)| t.rows().into_iter().map(|(_, l)| l).collect())
            .unwrap_or_default()
    }

    pub(super) fn apply_entity(&mut self, sketch: SketchId, entity: EntityId, old: Entity, f: &[String]) {
        let num = |i: usize| f.get(i).and_then(|s| parse_num(s));
        let new = match old {
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => num(0).and_then(|r| {
                // the ends and the side stay, only the radius changes
                let seg = rolidium_doc::rolidium_sketch::Segment::Arc {
                    center,
                    radius,
                    start,
                    sweep,
                };
                Entity::arc_chord_radius(seg.start_point(), seg.end_point(), r.abs(), seg.mid_point())
            }),
            Entity::Circle { center, .. } => num(0).map(|d| Entity::Circle {
                center,
                radius: d.abs() / 2.0,
            }),
            Entity::Rect { a, b } => match (num(0), num(1)) {
                (Some(w), Some(h)) => {
                    let sx = if b.x >= a.x { 1.0 } else { -1.0 };
                    let sy = if b.y >= a.y { 1.0 } else { -1.0 };
                    Some(Entity::Rect {
                        a,
                        b: a + DVec2::new(sx * w.abs(), sy * h.abs()),
                    })
                }
                _ => None,
            },
            Entity::Line { a, b } => num(0).map(|l| {
                let dir = (b - a).normalize_or_zero();
                Entity::Line {
                    a,
                    b: a + dir * l.abs(),
                }
            }),
        };
        let Some(new) = new else {
            self.set_status(tr("Please enter a number"), true);
            return;
        };
        match self.doc.apply(Command::ReplaceEntity { sketch, entity, new }) {
            Ok(_) => {
                self.layers_changed = true;
                self.set_status(tr("Size changed"), false);
            }
            Err(e) => self.set_status(
                trf("{}. The sketch was not changed.", &[tr(&e.to_string())]),
                true,
            ),
        }
    }

    pub(super) fn show_dim_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some((key, target, anchor)) = self.dim_target() else {
            self.dim.key.clear();
            return;
        };
        if key == self.dim.dismissed {
            return;
        }
        let rows = target.rows();
        let first = self.dim.key != key;
        if first {
            self.dim.key = key.clone();
            self.dim.focus = match &target {
                Target::Edges { .. } | Target::Region { .. } => false,
                Target::Face { hole, blend, .. } => {
                    (hole.is_some() || blend.is_some()) && self.tool != Tool::Pull
                }
                Target::Entity { .. } => true,
            };
            self.dim.fields = rows
                .iter()
                .map(|(r, _)| target.initial(*r, &self.edge_size_text))
                .collect();
        }
        let anchor_screen = self.project(anchor, rect).unwrap_or(rect.center());
        let pos = anchor_screen;
        let pos = Pos2::new(
            (pos.x + 24.0).clamp(rect.min.x + 4.0, (rect.max.x - 230.0).max(rect.min.x + 4.0)),
            (pos.y + 16.0).clamp(rect.min.y + 4.0, (rect.max.y - 230.0).max(rect.min.y + 4.0)),
        );
        let title = target.title();
        let is_edges = matches!(target, Target::Edges { .. });
        let pull_controls = target.has_pull_controls();
        let mut acts: Vec<Act> = Vec::new();
        let mut dismiss = false;
        let mut chamfer = self.edge_chamfer;
        let mut mode = self.pull_mode;
        // New body belongs to the pull of a sketch region; a face of a body is pulled in Auto, Add or Cut
        let is_region = matches!(target, Target::Region { .. });
        if !is_region && mode == PullMode::New {
            mode = PullMode::Auto;
        }
        let mut edge_text = None;
        let show_angle = (is_edges && chamfer)
            || matches!(
                &target,
                Target::Face {
                    blend: Some((BlendKind::Chamfer, _)),
                    ..
                }
            );
        if first {
            if let Target::Face {
                body,
                face,
                blend: Some((BlendKind::Chamfer, _)),
                ..
            } = &target
            {
                if let Some(f) = self
                    .doc
                    .body(*body)
                    .and_then(|b| b.features.iter().find(|f| f.faces.contains(&FaceId(*face))))
                {
                    self.chamfer_angle_text = fmt(f.angle);
                    self.chamfer_flip = f.flip;
                }
            }
        }
        let size_row = rows
            .iter()
            .position(|(r, _)| matches!(r, Row::EdgeSize | Row::BlendSize));
        let mut flip = self.chamfer_flip;
        let mut angle_text = self.chamfer_angle_text.clone();
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(ctx, "dim", pos, Some(anchor_screen), 250.0, &title, true, |ui| {
            if let Target::Edges { joint: true, .. } = &target {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 190, 90),
                    tr("These edges lie where two separate bodies meet. The bodies are not joined, so a round or chamfer would not add material across the joint. Combine the bodies first."),
                );
            }
            if let Target::Edges { extra, .. } = &target {
                if *extra > 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(217, 128, 255),
                        trf(
                            "{} more edge(s) continue them tangentially and are treated too",
                            &[extra.to_string()],
                        ),
                    );
                }
            }
            if is_edges {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut chamfer, false, tr("Round"));
                    ui.selectable_value(&mut chamfer, true, tr("Chamfer"));
                });
            }
            for (i, (row, label)) in rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(label.as_str());
                    let id = egui::Id::new(("dim_field", i));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.dim.fields[i])
                            .id(id)
                            .desired_width(60.0),
                    );
                    if first && i == 0 && self.dim.focus {
                        ctx.memory_mut(|m| m.request_focus(id));
                    }
                    ui.label("mm");
                    if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                        dismiss = true;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let text = self.dim.fields[i].clone();
                        let a = match row {
                            Row::Entity(_) => Some(Act::Entity(self.dim.fields.clone())),
                            Row::EdgeSize => parse_num(&text).map(Act::EdgeSize),
                            Row::Diameter => parse_num(&text).map(Act::Diameter),
                            Row::BlendSize => parse_num(&text).map(Act::BlendSize),
                            Row::MoveBy => parse_num(&text).map(Act::MoveBy),
                            Row::Opposite => parse_num(&text).map(Act::Opposite),
                        };
                        match a {
                            Some(a) => acts.push(a),
                            None => acts.push(Act::Entity(vec![String::new()])),
                        }
                    }
                });
            }
            if show_angle {
                ui.horizontal(|ui| {
                    ui.label(tr("Angle"));
                    let r = ui.add(egui::TextEdit::singleline(&mut angle_text).desired_width(60.0));
                    ui.label("°");
                    ui.checkbox(&mut flip, tr("Flip"))
                        .on_hover_text(tr("Measure the distance on the other face"));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        if let Some(i) = size_row {
                            let a = match rows[i].0 {
                                Row::EdgeSize => parse_num(&self.dim.fields[i]).map(Act::EdgeSize),
                                _ => parse_num(&self.dim.fields[i]).map(Act::BlendSize),
                            };
                            if let Some(a) = a {
                                acts.push(a);
                            }
                        }
                    }
                });
            }
            if pull_controls {
                ui.horizontal(|ui| {
                    ui.label(tr("Mode"));
                    let modes = pull_modes(is_region);
                    for (m, name) in modes {
                        ui.selectable_value(&mut mode, *m, tr(name));
                    }
                })
                .response
                .on_hover_text(tr(
                    "Auto adds material outwards and removes it inwards. Add and Cut force one of them whichever way you pull.",
                ));
                ui.horizontal(|ui| {
                    if ui.button(format!("{} (U)", tr("Up to"))).clicked() {
                        acts.push(Act::UpTo);
                    }
                    if ui.button(tr("Revolve")).clicked() {
                        acts.push(Act::Revolve);
                    }
                });
            }
            if is_edges {
                edge_text = Some(self.dim.fields[0].clone());
                if ui.button(tr("Apply")).clicked() {
                    acts.push(parse_num(&self.dim.fields[0]).map_or(
                        Act::Entity(vec![String::new()]),
                        Act::EdgeSize,
                    ));
                }
            }
        });
        self.docks = docks;
        if dock.dismiss {
            dismiss = true;
        }
        self.edge_chamfer = chamfer;
        self.pull_mode = mode;
        self.chamfer_flip = flip;
        self.chamfer_angle_text = angle_text;
        if let Some(t) = edge_text {
            self.edge_size_text = t;
        }
        if dismiss {
            self.dim.dismissed = key;
            return;
        }
        for act in acts {
            self.apply_panel_act(&target, act);
        }
    }

    fn apply_panel_act(&mut self, target: &Target, act: Act) {
        // An empty entity action stands for "the text was not a number".
        if let Act::Entity(f) = &act {
            if f.len() == 1 && f[0].is_empty() {
                return self.set_status(tr("Please enter a number"), true);
            }
        }
        match (target, act) {
            (Target::Entity { sketch, entity, e }, Act::Entity(f)) => {
                self.apply_entity(*sketch, *entity, *e, &f)
            }
            (Target::Edges { body, edges, .. }, Act::EdgeSize(v)) => {
                self.pending = None;
                let angle = if self.chamfer_angle_text.trim().is_empty() {
                    45.0
                } else {
                    parse_num(&self.chamfer_angle_text).unwrap_or(f64::NAN)
                };
                if self.edge_chamfer && ((angle - 45.0).abs() > 1e-9 || self.chamfer_flip) {
                    self.submit(Command::ChamferAngled {
                        body: *body,
                        edges: edges.clone(),
                        distance: v,
                        angle_deg: angle,
                        flip: self.chamfer_flip,
                    });
                    self.set_status(tr("Computing…"), false);
                    return;
                }
                self.submit(Command::FilletEdges {
                    body: *body,
                    edges: edges.clone(),
                    size: v,
                    chamfer: self.edge_chamfer,
                });
                self.set_status(tr("Computing…"), false);
            }
            (
                Target::Face {
                    body,
                    hole: Some((cyl, ..)),
                    ..
                },
                Act::Diameter(d),
            ) => {
                self.pending = None;
                self.submit(Command::SetCylinderDiameter {
                    body: *body,
                    face: FaceId(*cyl),
                    diameter: d,
                });
                self.set_status(tr("Rebuilding…"), false);
            }
            (
                Target::Face {
                    body,
                    face,
                    blend: Some(_),
                    ..
                },
                Act::BlendSize(v),
            ) => {
                self.pending = None;
                self.reselect_blend = Some(*body);
                let is_chamfer = matches!(
                    target,
                    Target::Face {
                        blend: Some((BlendKind::Chamfer, _)),
                        ..
                    }
                );
                if is_chamfer {
                    let angle = if self.chamfer_angle_text.trim().is_empty() {
                        45.0
                    } else {
                        parse_num(&self.chamfer_angle_text).unwrap_or(f64::NAN)
                    };
                    self.submit(Command::EditChamfer {
                        body: *body,
                        face: FaceId(*face),
                        distance: v,
                        angle_deg: angle,
                        flip: self.chamfer_flip,
                    });
                    self.set_status(tr("Computing…"), false);
                    return;
                }
                self.submit(Command::EditBlend {
                    body: *body,
                    face: FaceId(*face),
                    size: v,
                });
                self.set_status(tr("Computing…"), false);
            }
            (
                Target::Face {
                    body,
                    face,
                    origin,
                    normal,
                    ..
                },
                Act::MoveBy(v),
            ) => {
                let kind = PullKind::Face {
                    body: *body,
                    face: *face,
                };
                self.submit_pull(kind, *origin, *normal, v, self.pull_mode);
            }
            (
                Target::Face {
                    body,
                    face,
                    origin,
                    normal,
                    opposite: Some(d),
                    ..
                },
                Act::Opposite(v),
            ) => {
                let kind = PullKind::Face {
                    body: *body,
                    face: *face,
                };
                self.submit_pull(kind, *origin, *normal, v - d, PullMode::Auto);
            }
            (Target::Region { sketch, idx, .. }, Act::MoveBy(v)) => {
                let kind = PullKind::Region {
                    sketch: *sketch,
                    idx: *idx,
                };
                self.submit_pull(kind, DVec3::ZERO, Vec3::Z, v, self.pull_mode);
            }
            (Target::Region { sketch, idx, .. }, Act::Opposite(v)) => {
                // The depth of the cut from the sketch plane to the opposite face.
                let kind = PullKind::Region {
                    sketch: *sketch,
                    idx: *idx,
                };
                self.submit_pull(kind, DVec3::ZERO, Vec3::Z, -v.abs(), PullMode::Cut);
            }
            (_, Act::UpTo) => self.begin_up_to(),
            (_, Act::Revolve) => self.begin_revolve(),
            _ => {}
        }
    }
}

/// The modes the Pull panel offers: New body only for the region of a sketch, not for a face of a body.
pub(super) fn pull_modes(region: bool) -> &'static [(PullMode, &'static str)] {
    if region {
        &[
            (PullMode::Auto, "Auto"),
            (PullMode::New, "New body"),
            (PullMode::Add, "Add"),
            (PullMode::Cut, "Cut"),
        ]
    } else {
        &[
            (PullMode::Auto, "Auto"),
            (PullMode::Add, "Add"),
            (PullMode::Cut, "Cut"),
        ]
    }
}

#[cfg(test)]
mod pull_mode_tests {
    use super::*;

    #[test]
    fn new_body_is_offered_for_a_sketch_region_and_not_for_a_face() {
        assert!(pull_modes(true).iter().any(|(m, _)| *m == PullMode::New));
        assert!(!pull_modes(false).iter().any(|(m, _)| *m == PullMode::New));
        assert_eq!(pull_modes(false).len(), 3);
        assert_eq!(pull_modes(true)[0].0, PullMode::Auto);
    }
}
