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
    /// A second click on the selected shape: the field next to it takes the keyboard again.
    pub(super) refocus: bool,
    /// The fields of the right column for the selected shape, and what they belong to.
    sel_key: String,
    sel_fields: Vec<String>,
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
        /// An end of a plain cone, frustum or cylinder: its diameter.
        cap: Option<f64>,
        /// An end of a loft whose profile is a circle: (sketch, circle, centre, diameter). The diameter is the circle of the sketch.
        end: Option<(SketchId, EntityId, DVec2, f64)>,
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
    CapDiameter,
    EndDiameter,
}

enum Act {
    Entity(Vec<String>),
    EdgeSize(f64),
    Diameter(f64),
    BlendSize(f64),
    CapDiameter(f64),
    EndDiameter(f64),
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
                Entity::Circle { .. } => vec![
                    (Row::Entity(0), tr("Diameter")),
                    (Row::Entity(1), tr("Centre X")),
                    (Row::Entity(2), tr("Centre Y")),
                ],
                Entity::Arc { .. } => vec![(Row::Entity(0), tr("Radius")), (Row::Entity(1), tr("Angle"))],
                Entity::Rect { .. } => vec![(Row::Entity(0), tr("Width")), (Row::Entity(1), tr("Height"))],
                Entity::Line { .. } | Entity::Axis { .. } => {
                    vec![(Row::Entity(0), tr("Length")), (Row::Entity(1), tr("Angle"))]
                }
            },
            Target::Edges { .. } => vec![(Row::EdgeSize, tr("Size"))],
            Target::Face {
                hole,
                blend,
                planar,
                opposite,
                cap,
                end,
                ..
            } => {
                let mut v = Vec::new();
                if hole.is_some() {
                    v.push((Row::Diameter, tr("Diameter")));
                }
                if end.is_some() {
                    v.push((Row::EndDiameter, tr("Diameter")));
                } else if cap.is_some() && hole.is_none() {
                    v.push((Row::CapDiameter, tr("Diameter")));
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
                Entity::Axis { .. } => tr("Auxiliary axis"),
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
            (Target::Entity { e, .. }, Row::Entity(i)) => rolidium_doc::entity_dims(e)
                .get(i)
                .map_or(String::new(), |v| fmt(*v)),
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
                    end: Some((_, _, _, d)),
                    ..
                },
                Row::EndDiameter,
            ) => fmt(*d),
            (Target::Face { cap: Some(d), .. }, Row::CapDiameter) => fmt(*d),
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
                Entity::Line { a, b } | Entity::Rect { a, b } | Entity::Axis { a, b } => (a + b) * 0.5,
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
            // an end of a loft whose profile is one circle: the diameter is the circle of the sketch
            let end = self.doc.end_face(b, FaceId(f)).and_then(|ef| {
                self.doc
                    .end_circle(&ef)
                    .map(|(e, c, r)| (ef.sketch, e, c, 2.0 * r))
            });
            let cap = if end.is_none() { meta.cap } else { None };
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
                    cap,
                    end,
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
        let v: Vec<Option<f64>> = f.iter().map(|s| parse_num(s)).collect();
        // an empty or unreadable field is not a number (the first one must be); the others keep the value they have
        let new = if v.first().is_some_and(Option::is_some) {
            let unreadable = f.iter().zip(&v).any(|(t, n)| !t.trim().is_empty() && n.is_none());
            if unreadable {
                None
            } else {
                rolidium_doc::entity_with_dims(&old, &v)
            }
        } else {
            None
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

    /// Makes a line an auxiliary axis, or an axis an ordinary line.
    pub(super) fn set_entity_axis(&mut self, sketch: SketchId, entity: EntityId, axis: bool) {
        let Some(old) = self
            .doc
            .sketch(sketch)
            .and_then(|s| s.sketch.entity(entity).copied())
        else {
            return;
        };
        let new = if axis { old.as_axis() } else { old.as_line() };
        if new == old {
            return;
        }
        match self.doc.apply(Command::ReplaceEntity { sketch, entity, new }) {
            Ok(_) => {
                self.layers_changed = true;
                if axis {
                    self.set_status("The line is an auxiliary axis now", false);
                } else {
                    self.set_status("The axis is an ordinary line now", false);
                }
            }
            Err(e) => self.set_status(
                trf("{}. The sketch was not changed.", &[tr(&e.to_string())]),
                true,
            ),
        }
    }

    /// A click on a shape of a sketch: the first click selects it, a second click on the selected shape (or a double click) puts the cursor into the
    /// field of its size next to it (a closed panel opens again).
    pub(super) fn pick_entity(&mut self, s: SketchId, e: EntityId) {
        if self.entity_sel == Some((s, e)) {
            self.dim.refocus = true;
            self.dim.dismissed.clear();
        } else {
            self.clear_selection();
            self.entity_sel = Some((s, e));
        }
    }

    /// Whether a body has a flat face on the plane of the sketch: the body was probably made from the sketch (or stands on it).
    fn sketch_has_body(&self, s: SketchId) -> bool {
        let Some(sk) = self.doc.sketch(s) else {
            return false;
        };
        let n = sk.plane.normal.normalize();
        self.meshes.iter().any(|m| {
            m.faces.iter().any(|f| {
                f.kind == FaceKind::Plane
                    && f.normal.normalize_or_zero().dot(n).abs() > 0.9999
                    && (f.origin - sk.plane.origin).dot(n).abs() < 1e-6
            })
        })
    }

    /// The sizes of the selected shape of a sketch as fields in the right column (Enter applies, like the panel next to the shape).
    pub(super) fn selection_dims_ui(&mut self, ui: &mut egui::Ui) {
        let Some((key, target, _)) = self.dim_target() else {
            return;
        };
        let Target::Entity { sketch, .. } = &target else {
            return;
        };
        let sketch = *sketch;
        let rows = target.rows();
        if self.dim.sel_key != key || self.dim.sel_fields.len() != rows.len() {
            self.dim.sel_key = key;
            self.dim.sel_fields = rows
                .iter()
                .map(|(r, _)| target.initial(*r, &self.edge_size_text))
                .collect();
        }
        let mut apply = false;
        for (i, (row, label)) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(label.as_str());
                let unit = if *label == tr("Angle") { "°" } else { "mm" };
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.dim.sel_fields[i])
                        .id(egui::Id::new(("sel_dim", i)))
                        .desired_width(60.0),
                );
                ui.label(unit);
                let _ = row;
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
            });
        }
        // a line and an auxiliary axis are the same line drawn two ways
        if let Target::Entity { entity, e, .. } = &target {
            if matches!(e, Entity::Line { .. } | Entity::Axis { .. }) {
                let mut axis = e.is_axis();
                if ui
                    .checkbox(&mut axis, tr("Auxiliary axis (dash-dotted, no shape)"))
                    .on_hover_text(tr("An auxiliary axis forms no region and no path; it is the axis of a mirror, a pattern or a revolve"))
                    .changed()
                {
                    self.set_entity_axis(sketch, *entity, axis);
                }
            }
        }
        if let Some(body) = self.body_following_sketch(sketch) {
            ui.weak(trf(
                "The body '{}' follows this sketch and is made again with every change.",
                &[body],
            ));
        } else if self.sketch_has_body(sketch) {
            ui.weak(tr(
                "Bodies already made from this sketch do not change; change them with Pull.",
            ));
        }
        if apply {
            let f = self.dim.sel_fields.clone();
            let act = if f.first().is_some_and(|t| parse_num(t).is_some()) {
                Act::Entity(f)
            } else {
                Act::Entity(vec![String::new()])
            };
            self.apply_panel_act(&target, act);
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
        let mut whole_chain = self.blend_chain;
        let mut keep_angle = self.cap_keep_angle;
        let mut angle_text = self.chamfer_angle_text.clone();
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(ctx, "dim", pos, Some(anchor_screen), 250.0, &title, true, |ui| {
            if let Target::Edges { joint: true, .. } = &target {
                ui.colored_label(
                    crate::theme::rgb(255, 190, 90),
                    tr("These edges lie where two separate bodies meet. The bodies are not joined, so a round or chamfer would not add material across the joint. Combine the bodies first."),
                );
            }
            if let Target::Edges { extra, .. } = &target {
                if *extra > 0 {
                    ui.colored_label(
                        crate::theme::rgb(217, 128, 255),
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
                    if (first && i == 0 && self.dim.focus) || (i == 0 && self.dim.refocus) {
                        ctx.memory_mut(|m| m.request_focus(id));
                        // the text is selected, so a number typed at once replaces it
                        if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
                            let n = self.dim.fields[i].chars().count();
                            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(n),
                            )));
                            st.store(ctx, id);
                        }
                    }
                    ui.label(if *label == tr("Angle") { "°" } else { "mm" });
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
                            Row::CapDiameter => parse_num(&text).map(Act::CapDiameter),
                            Row::EndDiameter => parse_num(&text).map(Act::EndDiameter),
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
            if matches!(&target, Target::Face { cap: Some(_), end: None, hole: None, .. }) {
                ui.checkbox(&mut keep_angle, tr("Keep the angle (the other end follows)"))
                    .on_hover_text(tr(
                        "Off: the other end keeps its diameter and the angle of the cone changes. On: the angle stays and the other end changes by the same amount.",
                    ));
            }
            if matches!(&target, Target::Face { blend: Some(_), .. }) {
                ui.checkbox(&mut whole_chain, tr("Whole chain"))
                    .on_hover_text(tr(
                        "Change all rounds of the same size that touch this one, not only the selected one",
                    ));
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
        self.dim.refocus = false;
        if dock.dismiss {
            dismiss = true;
        }
        self.edge_chamfer = chamfer;
        self.pull_mode = mode;
        self.chamfer_flip = flip;
        self.blend_chain = whole_chain;
        self.cap_keep_angle = keep_angle;
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
                    end: Some((sketch, entity, centre, _)),
                    ..
                },
                Act::EndDiameter(d),
            ) => {
                // the end is the circle of the sketch of its profile: the body follows
                self.pending = None;
                self.submit(Command::ReplaceEntity {
                    sketch: *sketch,
                    entity: *entity,
                    new: Entity::Circle {
                        center: *centre,
                        radius: d / 2.0,
                    },
                });
                self.set_status(tr("Rebuilding…"), false);
            }
            (Target::Face { body, face, .. }, Act::CapDiameter(d)) => {
                self.pending = None;
                self.submit(Command::SetCapDiameter {
                    body: *body,
                    face: FaceId(*face),
                    diameter: d,
                    keep_angle: self.cap_keep_angle,
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
                        chain: self.blend_chain,
                    });
                    self.set_status(tr("Computing…"), false);
                    return;
                }
                self.submit(Command::EditBlend {
                    body: *body,
                    face: FaceId(*face),
                    size: v,
                    chain: self.blend_chain,
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
