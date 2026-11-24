// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Pattern and Mirror tools, and the panel that changes a link after it was made. A link (a pattern or a live mirror) remembers its source,
//! its parameters and the bodies it made; the panel shows a translucent preview of the pieces while the parameters change, and a click on Apply is
//! one step of Undo.

use super::*;
use crate::i18n::{tr, trf};
use rolidium_doc::{
    Axis, CircularParams, Direction2, LinearParams, LinkId, LinkKind, MirrorParams, PatternMode,
};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum LinkMode {
    #[default]
    Linear,
    Circular,
    Mirror,
    /// A hole or a pin repeated into the corners (or a grid) of a flat rectangular face.
    Fill,
}

/// What the next click in the view sets.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LinkPick {
    /// The place of the second piece of a row: a point on a face.
    Point,
    /// The flat rectangular face the corners or the grid are made on.
    RectFace,
    Direction,
    Direction2,
    Axis,
    Plane,
    Target,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum MirrorHow {
    InPlace,
    #[default]
    Keep,
    Merge,
    Live,
}

/// The parameters in the panel (what changes the preview).
#[derive(Clone, PartialEq, Debug)]
pub struct LinkParams {
    pub mode: LinkMode,
    pub dir: DVec3,
    pub dir_label: String,
    pub count: u32,
    pub spacing: f64,
    pub by_total: bool,
    pub second: bool,
    pub dir2: DVec3,
    pub dir2_label: String,
    pub count2: u32,
    pub spacing2: f64,
    pub by_total2: bool,
    pub axis: Axis,
    pub axis_label: String,
    pub angle: f64,
    pub full: bool,
    /// Circular pattern: the copies keep the orientation of the original instead of turning round the axis.
    pub keep_orientation: bool,
    /// 0 new bodies, 1 add to a body, 2 cut from a body.
    pub result: u8,
    pub target: Option<BodyId>,
    pub plane: Axis,
    pub plane_label: String,
    pub how: MirrorHow,
}

impl Default for LinkParams {
    fn default() -> Self {
        LinkParams {
            mode: LinkMode::Linear,
            dir: DVec3::X,
            dir_label: "X".into(),
            count: 3,
            spacing: 20.0,
            by_total: false,
            second: false,
            dir2: DVec3::Y,
            dir2_label: "Y".into(),
            count2: 3,
            spacing2: 20.0,
            by_total2: false,
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::Z,
            },
            axis_label: "Z".into(),
            angle: 60.0,
            full: true,
            keep_orientation: false,
            result: 0,
            target: None,
            plane: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            plane_label: "YZ".into(),
            how: MirrorHow::Keep,
        }
    }
}

#[derive(Default)]
pub struct LinkUi {
    pub p: LinkParams,
    /// The link that is being changed (the panel of an existing link), else a new one is made.
    pub editing: Option<LinkId>,
    pub pick: Option<LinkPick>,
    /// The pieces of the preview as triangles (normal, and those that may overlap or lie outside), and a remark about them.
    pub note: std::cell::RefCell<String>,
    pub plane_sketch: usize,
    /// A hole or a pin that is repeated in its own body (the body and the cylindrical face), instead of whole bodies.
    pub feature: Option<(BodyId, u32)>,
    /// The flat rectangular face of the corners or the grid, and what is made on it.
    pub rect_face: Option<u32>,
    pub grid: bool,
    pub columns: u32,
    pub rows: u32,
    pub margin: f64,
    /// The mouse handle of the spacing is being dragged.
    pub handle_drag: bool,
    /// The panel of patterns and mirrors of the shapes of the sketch that is open (`Linear`, `Circular` or `Mirror`).
    pub sk_open: Option<LinkMode>,
    pub sk_count: u32,
    pub sk_spacing: f64,
    pub sk_by_total: bool,
    /// Direction of a row, or of the mirror line, in degrees from the X axis of the sketch.
    pub sk_angle: f64,
    pub sk_cx: f64,
    pub sk_cy: f64,
    pub sk_step_angle: f64,
    pub sk_full: bool,
    pub sk_keep: bool,
    /// The preview of the copies in the model area (line pairs in 3D) and what it was made for.
    pub sk_preview: Vec<Vec3>,
    pub sk_key: String,
}

fn dv(v: Vec3) -> DVec3 {
    v.as_dvec3()
}

/// The bounding box of a mesh transformed by `f`.
fn moved_box(lo: DVec3, hi: DVec3, f: &dyn Fn(DVec3) -> DVec3) -> (DVec3, DVec3) {
    let mut a = DVec3::splat(f64::MAX);
    let mut b = DVec3::splat(f64::MIN);
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let p = f(c);
        a = a.min(p);
        b = b.max(p);
    }
    (a, b)
}

fn boxes_meet(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmplt(b.1 - DVec3::splat(1e-6)).all() && b.0.cmplt(a.1 - DVec3::splat(1e-6)).all()
}

impl LinkParams {
    /// The kind of link these parameters make.
    pub fn kind(&self) -> LinkKind {
        let mode = match (self.result, self.target) {
            (1, Some(t)) => PatternMode::Add(t),
            (2, Some(t)) => PatternMode::Cut(t),
            _ => PatternMode::New,
        };
        match self.mode {
            // the corners and the grid are made from the face (see `AddFeatureLink`); the numbers here are not used
            LinkMode::Fill => LinkKind::Linear(
                LinearParams {
                    axis: Axis {
                        origin: DVec3::ZERO,
                        dir: DVec3::X,
                    },
                    count: 1,
                    spacing: 0.0,
                    by_total: false,
                    second: None,
                },
                PatternMode::New,
            ),
            LinkMode::Linear => LinkKind::Linear(
                LinearParams {
                    axis: Axis {
                        origin: DVec3::ZERO,
                        dir: self.dir,
                    },
                    count: self.count,
                    spacing: self.spacing,
                    by_total: self.by_total,
                    second: self.second.then_some(Direction2 {
                        axis: Axis {
                            origin: DVec3::ZERO,
                            dir: self.dir2,
                        },
                        count: self.count2,
                        spacing: self.spacing2,
                        by_total: self.by_total2,
                    }),
                },
                mode,
            ),
            LinkMode::Circular => LinkKind::Circular(
                CircularParams {
                    axis: self.axis,
                    count: self.count,
                    angle_deg: self.angle,
                    full: self.full,
                    keep_orientation: self.keep_orientation,
                },
                mode,
            ),
            LinkMode::Mirror => LinkKind::Mirror(MirrorParams {
                origin: self.plane.origin,
                normal: self.plane.dir,
                into: PatternMode::New,
            }),
        }
    }

    /// The parameters of an existing link, for its panel.
    pub fn from_link(l: &rolidium_doc::Link) -> LinkParams {
        let mut p = LinkParams::default();
        let result = |m: PatternMode| match m {
            PatternMode::New => (0, None),
            PatternMode::Add(t) => (1, Some(t)),
            PatternMode::Cut(t) => (2, Some(t)),
        };
        match &l.kind {
            LinkKind::Linear(q, m) => {
                p.mode = LinkMode::Linear;
                p.dir = q.axis.dir;
                p.dir_label = tr("as made");
                p.count = q.count;
                p.spacing = q.spacing;
                p.by_total = q.by_total;
                if let Some(s) = &q.second {
                    p.second = true;
                    p.dir2 = s.axis.dir;
                    p.dir2_label = tr("as made");
                    p.count2 = s.count;
                    p.spacing2 = s.spacing;
                    p.by_total2 = s.by_total;
                }
                (p.result, p.target) = result(*m);
            }
            LinkKind::Circular(q, m) => {
                p.mode = LinkMode::Circular;
                p.axis = q.axis;
                p.axis_label = tr("as made");
                p.count = q.count;
                p.angle = q.angle_deg;
                p.full = q.full;
                p.keep_orientation = q.keep_orientation;
                (p.result, p.target) = result(*m);
            }
            LinkKind::Mirror(m) => {
                p.mode = LinkMode::Mirror;
                p.plane = Axis {
                    origin: m.origin,
                    dir: m.normal,
                };
                p.plane_label = tr("as made");
                p.how = MirrorHow::Live;
            }
            // a loft and a sweep have no panel of parameters: their profiles and path are edited where they are
            LinkKind::Loft { .. } | LinkKind::Sweep { .. } => {}
        }
        p
    }
}

impl App {
    /// The bodies the tool works on: the bodies marked in the view or in the list.
    pub(super) fn link_sources(&self) -> Vec<BodyId> {
        if !self.move_sel.is_empty() {
            return self.move_sel.clone();
        }
        self.body_sel.into_iter().collect()
    }

    pub(super) fn activate_link_tool(&mut self, mode: LinkMode) {
        if self.sk.as_ref().is_some_and(|s| s.sketch.is_some()) {
            // in a sketch the same buttons repeat and mirror the shapes of the sketch
            let u = &mut self.link_ui;
            if u.sk_count == 0 {
                (u.sk_count, u.sk_spacing, u.sk_step_angle, u.sk_full, u.sk_keep) =
                    (3, 20.0, 60.0, true, true);
            }
            u.sk_open = Some(mode);
            return;
        }
        if self.doc.bodies().is_empty() {
            self.set_status("There is no body to repeat or mirror", true);
            return;
        }
        self.leave_sketch();
        self.tool = if mode == LinkMode::Mirror {
            Tool::Mirror
        } else {
            Tool::Pattern
        };
        if self.move_sel.is_empty() {
            // the body of a picked face, or the body picked in the list
            if let Some(b) = self.body_sel.or_else(|| self.selection.last().map(|s| s.0)) {
                self.move_sel = vec![b];
            }
        }
        self.link_ui.editing = None;
        self.link_ui.feature = None;
        self.link_ui.rect_face = None;
        if self.link_ui.columns == 0 {
            (self.link_ui.columns, self.link_ui.rows, self.link_ui.margin) = (3, 2, 5.0);
        }
        let old = std::mem::take(&mut self.link_ui.p);
        self.link_ui.p = LinkParams {
            mode,
            ..LinkParams::default()
        };
        if mode == LinkMode::Circular {
            self.link_ui.p.count = 6;
        }
        // the same defaults after the first time: the number of pieces and the kind of result are not remembered, the spacing follows the body
        let _ = old;
        self.link_ui.pick = None;
        self.default_spacing();
        self.layers_changed = true;
    }

    /// A spacing that leaves a gap between the pieces: the size of the sources along the direction and a fifth more.
    fn default_spacing(&mut self) {
        let dir = self.link_ui.p.dir.normalize_or_zero();
        let mut ext = 0.0f64;
        for b in self.link_sources() {
            if let Some(m) = self.entry(b) {
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                for p in &m.mesh.positions {
                    let d = dv(*p).dot(dir);
                    lo = lo.min(d);
                    hi = hi.max(d);
                }
                if lo <= hi {
                    ext = ext.max(hi - lo);
                }
            }
        }
        if ext > 1e-6 {
            let s = (ext * 1.2 * 10.0).round() / 10.0;
            self.link_ui.p.spacing = s;
            self.link_ui.p.spacing2 = s;
        }
    }

    /// Opens the panel of a link (a click on it in the list of the document).
    pub(super) fn open_link(&mut self, id: LinkId) {
        let Some(l) = self.doc.state().links.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        if l.kind.generates() {
            // a loft or a sweep has no panel of numbers: its body is selected, and the Selection panel names its sources
            self.clear_selection();
            self.body_sel = l.results.first().copied();
            self.link_sel = Some(id);
            self.layers_changed = true;
            return;
        }
        self.leave_sketch();
        self.link_ui.p = LinkParams::from_link(&l);
        self.link_ui.editing = Some(id);
        self.link_ui.pick = None;
        self.tool = if l.kind.title() == "Mirror" {
            Tool::Mirror
        } else {
            Tool::Pattern
        };
        self.move_sel = l.sources.clone();
        self.body_sel = None;
        self.selection.clear();
        self.link_sel = Some(id);
        self.layers_changed = true;
    }

    pub(super) fn close_link_tool(&mut self) {
        self.link_ui.editing = None;
        self.link_ui.pick = None;
        self.link_sel = None;
        self.move_sel.clear();
        self.tool = Tool::Select;
        self.layers_changed = true;
    }

    /// A click in the view while the tool works: a body (a source, or the target when picking one), or a face or an edge that gives the direction.
    pub(super) fn link_input(&mut self, resp: &egui::Response) {
        let hover = if resp.hovered() {
            self.hovered_face().map(|(b, _)| b)
        } else {
            None
        };
        if hover != self.hover_body {
            self.hover_body = hover;
            self.layers_changed = true;
        }
        if self.spacing_handle(resp) {
            return;
        }
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        match self.link_ui.pick {
            Some(LinkPick::Target) => {
                if let Some(b) = hover {
                    if !self.move_sel.contains(&b) {
                        self.link_ui.p.target = Some(b);
                        self.link_ui.pick = None;
                        self.layers_changed = true;
                    } else {
                        self.set_status("The body the pieces go into cannot be one of the sources", true);
                    }
                }
            }
            Some(LinkPick::Point) => {
                if self.pick_second_point(resp) {
                    self.link_ui.pick = None;
                    self.layers_changed = true;
                }
            }
            Some(LinkPick::RectFace) => {
                let ok = self.hovered_face().and_then(|(b, f)| {
                    let (fb, _) = self.link_ui.feature?;
                    let solid = &self.doc.body(b)?.solid;
                    (b == fb && solid.face_info(FaceId(f)).ok()?.kind == FaceKind::Plane).then_some(f)
                });
                match ok {
                    Some(f) => {
                        self.link_ui.rect_face = Some(f);
                        self.link_ui.pick = None;
                        self.layers_changed = true;
                    }
                    None => self.set_status("Click the flat rectangular face of the same body", true),
                }
            }
            Some(p @ (LinkPick::Direction | LinkPick::Direction2 | LinkPick::Axis | LinkPick::Plane)) => {
                if self.pick_link_geometry(p) {
                    self.link_ui.pick = None;
                    self.layers_changed = true;
                }
            }
            None => {
                if self.link_ui.editing.is_some() {
                    return;
                }
                // a line or an axis of a sketch is the direction, the axis or the mirror plane at once, with no button
                if matches!(self.hover_obj, Some(Obj::Entity(..)))
                    && (self.link_ui.feature.is_some() || !self.link_sources().is_empty())
                {
                    let what = match self.link_ui.p.mode {
                        LinkMode::Linear => Some(LinkPick::Direction),
                        LinkMode::Circular => Some(LinkPick::Axis),
                        LinkMode::Mirror => Some(LinkPick::Plane),
                        LinkMode::Fill => None,
                    };
                    if let Some(w) = what {
                        if self.pick_sketch_line(w) {
                            self.layers_changed = true;
                            return;
                        }
                    }
                }
                // the round face of a hole or a pin is a thing to repeat in its own body
                if let Some((b, f)) = self.hovered_face() {
                    if self.is_feature_face(b, f) {
                        self.link_ui.feature = Some((b, f));
                        self.move_sel.clear();
                        self.selection = vec![(b, f)];
                        self.link_ui.rect_face = None;
                        self.set_status("The hole or the pin is the source: choose how to repeat it (a row, round an axis, a mirror or into the corners)", false);
                        self.layers_changed = true;
                        return;
                    }
                }
                if let Some(b) = hover {
                    self.link_ui.feature = None;
                    if let Some(i) = self.move_sel.iter().position(|x| *x == b) {
                        self.move_sel.remove(i);
                    } else {
                        self.move_sel.push(b);
                    }
                    self.default_spacing();
                    self.layers_changed = true;
                }
            }
        }
    }

    /// The one sentence of the status line: what to click next in the Pattern and Mirror tools.
    pub(super) fn link_hint(&self) -> String {
        let u = &self.link_ui;
        if u.editing.is_some() {
            return tr("Change the numbers in the panel, then Apply or Enter.");
        }
        let have = u.feature.is_some() || !self.link_sources().is_empty();
        if let Some(pick) = u.pick {
            return match pick {
                LinkPick::Point => tr("Click the place of the second piece on a face."),
                LinkPick::RectFace => tr("Click the flat rectangular face of the same body."),
                LinkPick::Plane => tr("Click a flat face, a straight edge, an axis or a line of a sketch, or the plane of a sketch for the mirror plane."),
                LinkPick::Axis => tr("Click a round edge, a cylindrical face, a straight edge, a face, or an axis or a line of a sketch for the axis (or press X, Y, Z)."),
                LinkPick::Target => tr("Click the body the pieces are added to or cut out of."),
                LinkPick::Direction | LinkPick::Direction2 => tr("Click a straight edge, a face, or a line of a sketch for the direction."),
            };
        }
        if !have {
            return if self.tool == Tool::Mirror {
                tr("Mirror: click what is mirrored: a body, or the round face of a hole or a pin.")
            } else {
                tr("Pattern: click what is repeated: a body (Ctrl adds more), or the round face of a hole or a pin.")
            };
        }
        match u.p.mode {
            LinkMode::Linear => tr("Row: click an edge or a face for the direction (or press X, Y, Z), or click the place of the second piece."),
            LinkMode::Circular => tr("Round an axis: click the centre: a round edge, a cylindrical face, a straight edge, an axis of a sketch (or press X, Y, Z), then set the number."),
            LinkMode::Mirror => tr("Mirror: click the plane: a flat face, a straight edge, an axis or a line of a sketch, or choose a base plane."),
            LinkMode::Fill => tr("Into a face: click the flat rectangular face (Pick the face), choose the corners or a grid, then Apply."),
        }
    }

    /// Whether a face is the round face of a hole or a pin of a body that has more than the round part itself (a plain cylinder is a body to repeat).
    fn is_feature_face(&self, body: BodyId, face: u32) -> bool {
        self.doc.body(body).is_some_and(|b| {
            b.solid.face_count() > 3 && b.solid.cylinder_info(FaceId(face)).ok().flatten().is_some()
        })
    }

    /// The point of the second piece of a row: the point of the face under the pointer; the direction and the spacing are read from it.
    fn pick_second_point(&mut self, resp: &egui::Response) -> bool {
        let (Some((b, f)), Some(pointer)) = (self.hovered_face(), resp.interact_pointer_pos()) else {
            return false;
        };
        let rect = resp.rect;
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
        let Some(to) = self.ray_face_point(b, f, o, d) else {
            return false;
        };
        // the place of the first piece: the axis of the hole, or the centre of the box of the sources
        let from = match self.link_ui.feature {
            Some((fb, ff)) => self
                .doc
                .body(fb)
                .and_then(|x| x.solid.cylinder_info(FaceId(ff)).ok().flatten())
                .map(|c| c.base),
            None => {
                let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for s in self.link_sources() {
                    if let Some((a, c)) = self.doc.body(s).and_then(|x| x.solid.bounding_box().ok()) {
                        lo = lo.min(a);
                        hi = hi.max(c);
                    }
                }
                (lo.x <= hi.x).then(|| (lo + hi) * 0.5)
            }
        };
        let Some(from) = from else {
            self.set_status("Click what is repeated first", true);
            return false;
        };
        // a hole is drilled along its axis: the point is taken on the plane through the axis point, square to the axis of the face the point lies on
        let count = self.link_ui.p.count.max(2);
        match rolidium_doc::linear_through(from, to, count) {
            Ok(q) => {
                let p = &mut self.link_ui.p;
                (p.dir, p.dir_label) = (q.axis.dir, tr("the second piece"));
                p.spacing = q.spacing;
                p.by_total = false;
                p.count = count;
                true
            }
            Err(e) => {
                self.set_status(e.to_string(), true);
                false
            }
        }
    }

    /// A line or an auxiliary axis of a sketch under the pointer as a direction, an axis or a mirror plane (the plane through the line
    /// that stands square to the plane of the sketch); the centre of a circle or an arc of a sketch as an axis square to the sketch.
    fn pick_sketch_line(&mut self, what: LinkPick) -> bool {
        let Some(Obj::Entity(s, id)) = self.hover_obj else {
            return false;
        };
        let Some((plane, e)) = self
            .doc
            .sketch(s)
            .and_then(|sk| sk.sketch.entity(id).map(|e| (sk.plane, *e)))
        else {
            return false;
        };
        let n = plane.normal.normalize();
        let p = &mut self.link_ui.p;
        if let Some(line) = rolidium_doc::sketch_line_in_space(&plane, &e) {
            let label = if e.is_axis() {
                tr("an auxiliary axis")
            } else {
                tr("a line of a sketch")
            };
            match what {
                LinkPick::Direction => (p.dir, p.dir_label) = (line.dir, label),
                LinkPick::Direction2 => (p.dir2, p.dir2_label) = (line.dir, label),
                LinkPick::Axis => (p.axis, p.axis_label) = (line, label),
                LinkPick::Plane => {
                    let Some(pl) = rolidium_doc::sketch_mirror_plane(&plane, &e) else {
                        return false;
                    };
                    (p.plane, p.plane_label) = (pl, label);
                }
                LinkPick::Target | LinkPick::Point | LinkPick::RectFace => return false,
            }
            return true;
        }
        if let (LinkPick::Axis, Entity::Circle { center, .. } | Entity::Arc { center, .. }) = (what, e) {
            (p.axis, p.axis_label) = (
                Axis {
                    origin: plane.to_3d(center),
                    dir: n,
                },
                tr("the centre of a circle of a sketch"),
            );
            return true;
        }
        false
    }

    /// Sets a direction, an axis or a plane from the edge or the face under the pointer. `true` when something was taken.
    fn pick_link_geometry(&mut self, what: LinkPick) -> bool {
        if self.pick_sketch_line(what) {
            return true;
        }
        // a round edge: its centre and the axis of the circle (an axis of a pattern)
        if let (Some(Obj::Edge(b, e)), LinkPick::Axis) = (self.hover_obj, what) {
            if let Some(pl) = self.edge_polyline(b, e).filter(|p| p.len() >= 8) {
                let n = pl.len();
                let (a, m, c) = (dv(pl[0]), dv(pl[n / 3]), dv(pl[2 * n / 3]));
                let (u, v) = (m - a, c - a);
                let normal = u.cross(v);
                if normal.length() > 1e-9 {
                    // the centre of the circle through the three points
                    let w = normal.normalize();
                    let (uu, vv, uv) = (u.dot(u), v.dot(v), u.dot(v));
                    let k = 2.0 * (uu * vv - uv * uv);
                    if k.abs() > 1e-12 {
                        let (s1, s2) = (vv * (uu - uv) / k, uu * (vv - uv) / k);
                        let centre = a + u * s1 + v * s2;
                        self.link_ui.p.axis = Axis {
                            origin: centre,
                            dir: w,
                        };
                        self.link_ui.p.axis_label = tr("a round edge");
                        return true;
                    }
                }
            }
        }
        // an edge: a straight one gives a direction and a point
        if let Some(Obj::Edge(b, e)) = self.hover_obj {
            if let Some(body) = self.doc.body(b) {
                if let Ok(i) = body.solid.edge_info(rolidium_kernel::EdgeId(e)) {
                    if i.kind == rolidium_kernel::EdgeKind::Line && (i.end - i.start).length() > 1e-9 {
                        let d = (i.end - i.start).normalize();
                        let p = &mut self.link_ui.p;
                        match what {
                            LinkPick::Direction => (p.dir, p.dir_label) = (d, tr("an edge")),
                            LinkPick::Direction2 => (p.dir2, p.dir2_label) = (d, tr("an edge")),
                            LinkPick::Axis => {
                                (p.axis, p.axis_label) = (
                                    Axis {
                                        origin: i.start,
                                        dir: d,
                                    },
                                    tr("an edge"),
                                )
                            }
                            LinkPick::Plane => {
                                // the plane through the middle of the edge, square to it
                                (p.plane, p.plane_label) = (
                                    Axis {
                                        origin: (i.start + i.end) * 0.5,
                                        dir: d,
                                    },
                                    tr("the middle of an edge"),
                                )
                            }
                            LinkPick::Target | LinkPick::Point | LinkPick::RectFace => {}
                        }
                        return true;
                    }
                }
            }
        }
        let Some((b, f)) = self.hovered_face() else {
            return false;
        };
        let Some(info) = self
            .doc
            .body(b)
            .and_then(|body| body.solid.face_info(FaceId(f)).ok())
        else {
            return false;
        };
        let p = &mut self.link_ui.p;
        match (what, info.kind) {
            (LinkPick::Direction, FaceKind::Plane) => (p.dir, p.dir_label) = (info.normal, tr("a face")),
            (LinkPick::Direction2, FaceKind::Plane) => (p.dir2, p.dir2_label) = (info.normal, tr("a face")),
            (LinkPick::Direction, FaceKind::Cylinder | FaceKind::Cone) => {
                (p.dir, p.dir_label) = (info.normal, tr("a face"))
            }
            (LinkPick::Direction2, FaceKind::Cylinder | FaceKind::Cone) => {
                (p.dir2, p.dir2_label) = (info.normal, tr("a face"))
            }
            (LinkPick::Axis, FaceKind::Cylinder | FaceKind::Cone) => {
                (p.axis, p.axis_label) = (
                    Axis {
                        origin: info.origin,
                        dir: info.normal,
                    },
                    tr("the axis of a hole or a shaft"),
                )
            }
            (LinkPick::Axis, FaceKind::Plane) => {
                (p.axis, p.axis_label) = (
                    Axis {
                        origin: info.origin,
                        dir: info.normal,
                    },
                    tr("a face"),
                )
            }
            (LinkPick::Plane, FaceKind::Plane) => {
                (p.plane, p.plane_label) = (
                    Axis {
                        origin: info.origin,
                        dir: info.normal,
                    },
                    tr("a face"),
                )
            }
            (LinkPick::Plane, _) => {
                self.set_status("Click a flat face for the mirror plane", true);
                return false;
            }
            _ => {
                self.set_status("Click a flat or a round face, or a straight edge", true);
                return false;
            }
        }
        true
    }

    /// The translucent pieces of the preview, and a remark about those that may overlap or lie outside (from the boxes of the bodies).
    fn compute_link_preview(&self) -> (Vec<Vec3>, Vec<Vec3>) {
        let (mut ok, mut badv) = (Vec::new(), Vec::new());
        self.link_ui.note.borrow_mut().clear();
        let sources = self.link_sources();
        if sources.is_empty() || !matches!(self.tool, Tool::Pattern | Tool::Mirror) {
            return (ok, badv);
        }
        let kind = self.link_ui.p.kind();
        let mirror = self.link_ui.p.mode == LinkMode::Mirror;
        let plan = match rolidium_doc::placements_of(&kind) {
            Ok(p) => p,
            Err(e) => {
                *self.link_ui.note.borrow_mut() = e;
                return (ok, badv);
            }
        };
        let transforms: Vec<Box<dyn Fn(DVec3) -> DVec3>> = if mirror {
            let (o, n) = (
                self.link_ui.p.plane.origin,
                self.link_ui.p.plane.dir.normalize_or_zero(),
            );
            vec![Box::new(move |p| p - n * (2.0 * (p - o).dot(n)))]
        } else {
            plan.iter()
                .map(|(t, pivot, axis, angle)| {
                    let q = if axis.length() > 1e-9 && angle.abs() > 1e-12 {
                        glam::DQuat::from_axis_angle(axis.normalize(), *angle)
                    } else {
                        glam::DQuat::IDENTITY
                    };
                    let (t, pivot) = (*t, *pivot);
                    Box::new(move |p: DVec3| pivot + q * (p - pivot) + t) as Box<dyn Fn(DVec3) -> DVec3>
                })
                .collect()
        };
        let mut tris_budget = 600_000usize;
        let mut boxes: Vec<(DVec3, DVec3)> = Vec::new();
        let mut originals: Vec<(DVec3, DVec3)> = Vec::new();
        let mut per_piece: Vec<(usize, usize)> = Vec::new(); // (source, transform) of each piece
        for (si, b) in sources.iter().enumerate() {
            let Some(m) = self.entry(*b) else { continue };
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for p in &m.mesh.positions {
                lo = lo.min(dv(*p));
                hi = hi.max(dv(*p));
            }
            if lo.x > hi.x {
                continue;
            }
            originals.push((lo, hi));
            for (ti, f) in transforms.iter().enumerate() {
                boxes.push(moved_box(lo, hi, f.as_ref()));
                per_piece.push((si, ti));
            }
        }
        // which pieces are doubtful
        let mut bad = vec![false; boxes.len()];
        let mut overlaps = 0usize;
        let all: Vec<(DVec3, DVec3)> = originals.iter().copied().chain(boxes.iter().copied()).collect();
        if matches!(kind.mode(), PatternMode::New) {
            for i in 0..boxes.len() {
                let me = &boxes[i];
                for (j, other) in all.iter().enumerate() {
                    // the other pieces and the originals, not itself
                    if j == originals.len() + i {
                        continue;
                    }
                    if boxes_meet(*me, *other) {
                        bad[i] = true;
                        overlaps += 1;
                        break;
                    }
                }
            }
            if overlaps > 0 {
                *self.link_ui.note.borrow_mut() =
                    format!("{overlaps} {}", tr("piece(s) may overlap another piece"));
            }
        } else if let Some(t) = kind.mode().target() {
            if let Some(m) = self.entry(t) {
                let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for p in &m.mesh.positions {
                    lo = lo.min(dv(*p));
                    hi = hi.max(dv(*p));
                }
                let mut outside = 0;
                for i in 0..boxes.len() {
                    let touches = boxes[i].0.cmple(hi + DVec3::splat(1e-6)).all()
                        && lo.cmple(boxes[i].1 + DVec3::splat(1e-6)).all();
                    if !touches {
                        bad[i] = true;
                        outside += 1;
                    }
                }
                if outside > 0 {
                    *self.link_ui.note.borrow_mut() =
                        format!("{outside} {}", tr("piece(s) lie outside the body"));
                }
            }
        }
        for (k, (si, ti)) in per_piece.iter().enumerate() {
            let Some(m) = self.entry(sources[*si]) else {
                continue;
            };
            let n = m.mesh.indices.len();
            if n > tris_budget {
                // too many triangles to draw every piece: the box of the piece instead
                let (a, b) = boxes[k];
                let dst = if bad[k] { &mut badv } else { &mut ok };
                box_triangles(a.as_vec3(), b.as_vec3(), dst);
                continue;
            }
            tris_budget -= n;
            let f = &transforms[*ti];
            let dst = if bad[k] { &mut badv } else { &mut ok };
            let flip = mirror;
            for t in m.mesh.indices.as_chunks::<3>().0.iter().map(|c| c.as_slice()) {
                let p: Vec<Vec3> = t
                    .iter()
                    .map(|i| f(dv(m.mesh.positions[*i as usize])).as_vec3())
                    .collect();
                if flip {
                    dst.extend_from_slice(&[p[0], p[2], p[1]]);
                } else {
                    dst.extend_from_slice(&[p[0], p[1], p[2]]);
                }
            }
        }
        (ok, badv)
    }

    pub(super) fn link_layers(&self) -> Vec<PreviewDraw> {
        let mut out = Vec::new();
        if self.link_ui.sk_open.is_some() && !self.link_ui.sk_preview.is_empty() {
            // the copies of the shapes of the sketch (with the mirror line drawn through the point)
            out.push(PreviewDraw {
                lines: self.link_ui.sk_preview.clone(),
                line_color: [0.35, 0.65, 1.0, 1.0],
                ..Default::default()
            });
        }
        if !matches!(self.tool, Tool::Pattern | Tool::Mirror) {
            return out;
        }
        let (ok, bad) = self.compute_link_preview();
        for b in self.link_sources() {
            out.push(PreviewDraw {
                thick: self.body_edge_lines(b),
                thick_color: [0.95, 0.35, 0.95, 1.0],
                ..Default::default()
            });
        }
        if let Some(t) = self
            .link_ui
            .p
            .target
            .filter(|_| self.link_ui.p.result != 0 && self.link_ui.p.mode != LinkMode::Mirror)
        {
            out.push(PreviewDraw {
                thick: self.body_edge_lines(t),
                thick_color: [0.25, 0.85, 1.0, 1.0],
                ..Default::default()
            });
        }
        if !ok.is_empty() {
            out.push(PreviewDraw {
                triangles: ok,
                color: [0.35, 0.65, 1.0, 0.35],
                ..Default::default()
            });
        }
        if !bad.is_empty() {
            out.push(PreviewDraw {
                triangles: bad,
                color: [1.0, 0.35, 0.3, 0.45],
                ..Default::default()
            });
        }
        out
    }

    /// The command that the panel makes: a new link, a change of the link, or a plain mirror.
    fn link_command(&self) -> Option<Command> {
        let sources = self.link_sources();
        let p = &self.link_ui.p;
        if let Some(id) = self.link_ui.editing {
            return Some(Command::EditLink {
                link: id,
                kind: Some(p.kind()),
                name: None,
            });
        }
        if let Some((body, face)) = self.link_ui.feature {
            let u = &self.link_ui;
            let rect = if p.mode == LinkMode::Fill {
                let f = u.rect_face?;
                let fill = if u.grid {
                    rolidium_doc::RectFill::Grid {
                        columns: u.columns.max(1),
                        rows: u.rows.max(1),
                        margin: u.margin,
                    }
                } else {
                    rolidium_doc::RectFill::Corners
                };
                Some((rolidium_doc::RectTarget::Face(FaceId(f)), fill))
            } else {
                None
            };
            return Some(Command::AddFeatureLink {
                body,
                face: FaceId(face),
                kind: p.kind(),
                name: String::new(),
                rect,
            });
        }
        if sources.is_empty() {
            return None;
        }
        if p.mode == LinkMode::Mirror {
            return Some(match p.how {
                MirrorHow::Live => Command::AddLink {
                    kind: p.kind(),
                    sources,
                    name: String::new(),
                },
                how => Command::Mirror {
                    bodies: sources,
                    origin: p.plane.origin,
                    normal: p.plane.dir,
                    keep: how != MirrorHow::InPlace,
                    merge: how == MirrorHow::Merge,
                },
            });
        }
        Some(Command::AddLink {
            kind: p.kind(),
            sources,
            name: String::new(),
        })
    }

    pub(super) fn apply_link(&mut self) {
        if !self.idle() {
            return;
        }
        let feature = self.link_ui.feature.is_some() && self.link_ui.editing.is_none();
        let why = if feature && self.link_ui.p.mode == LinkMode::Fill && self.link_ui.rect_face.is_none() {
            Some("Click the flat rectangular face the copies go into (Pick the face)")
        } else if feature {
            None
        } else if self.link_ui.editing.is_none() && self.link_sources().is_empty() {
            Some("Click the body to repeat or mirror")
        } else if self.link_ui.p.result != 0
            && self.link_ui.p.target.is_none()
            && self.link_ui.p.mode != LinkMode::Mirror
        {
            Some("Click the body the pieces are added to or cut out of")
        } else {
            None
        };
        if let Some(w) = why {
            self.set_status(w, true);
            return;
        }
        let Some(cmd) = self.link_command() else { return };
        self.pending = None;
        self.submit(cmd);
        self.set_status("Computing…", false);
        let editing = self.link_ui.editing.is_some();
        if !editing {
            self.move_sel.clear();
            self.link_ui.feature = None;
            self.link_ui.rect_face = None;
            self.link_ui.pick = None;
            self.tool = Tool::Select;
        }
        self.layers_changed = true;
    }

    /// The panel of the Pattern and Mirror tools and of an existing link.
    #[allow(deprecated)]
    pub(super) fn link_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if !matches!(self.tool, Tool::Pattern | Tool::Mirror) {
            return;
        }
        let before = self.link_ui.p.clone();
        let mut apply = false;
        let mut cancel = false;
        let mut unlink = None;
        let mut delete = None;
        let mut docks = std::mem::take(&mut self.docks);
        let editing = self.link_ui.editing;
        let title = match (editing, self.link_ui.p.mode) {
            (Some(id), _) => self
                .doc
                .state()
                .links
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            (None, LinkMode::Mirror) => tr("Mirror"),
            (None, _) => tr("Pattern"),
        };
        let sources: Vec<String> = self
            .link_sources()
            .iter()
            .filter_map(|b| self.doc.body(*b))
            .map(|b| b.name.clone())
            .collect();
        let feature_label = self.link_ui.feature.and_then(|(b, _)| {
            self.doc
                .body(b)
                .map(|x| trf("a hole or a pin of {}", std::slice::from_ref(&x.name)))
        });
        let has_feature = feature_label.is_some();
        let rect_label = self.link_ui.rect_face.map(|f| format!("#{f}"));
        let bodies = self.doc.bodies().to_vec();
        let sketches = self.doc.sketches().to_vec();
        let error = editing
            .and_then(|id| self.doc.state().links.iter().find(|l| l.id == id))
            .and_then(|l| l.error.clone());
        let mut ui_state = std::mem::take(&mut self.link_ui);
        docks.panel(ctx, "link", Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0), None, 270.0, &title, false, |ui| {
            let p = &mut ui_state.p;
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(crate::theme::rgb(242, 89, 242), tr("Source"));
                ui.label(match (&feature_label, sources.is_empty()) {
                    (Some(f), _) => f.clone(),
                    (None, true) => tr("click a body, or the round face of a hole or a pin"),
                    (None, false) => sources.join(", "),
                });
            });
            if editing.is_none() && p.mode != LinkMode::Mirror {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut p.mode, LinkMode::Linear, tr("Along a line"));
                    ui.selectable_value(&mut p.mode, LinkMode::Circular, tr("Round an axis"));
                    if has_feature {
                        ui.selectable_value(&mut p.mode, LinkMode::Fill, tr("Into a face"));
                    }
                });
            }
            let pick_btn = |ui: &mut egui::Ui, st: &mut Option<LinkPick>, what: LinkPick, label: &str| {
                let on = *st == Some(what);
                if ui.selectable_label(on, label).on_hover_text(tr("Then click a straight edge or a face in the view")).clicked() {
                    *st = if on { None } else { Some(what) };
                }
            };
            let dir_row = |ui: &mut egui::Ui, st: &mut Option<LinkPick>, what: LinkPick, dir: &mut DVec3, label: &mut String| {
                ui.horizontal_wrapped(|ui| {
                    for (v, n) in [(DVec3::X, "X"), (DVec3::Y, "Y"), (DVec3::Z, "Z")] {
                        if ui.selectable_label(*label == n, n).clicked() {
                            *dir = v;
                            *label = n.into();
                        }
                    }
                    pick_btn(ui, st, what, &tr("Pick"));
                    if ui.button("⇄").on_hover_text(tr("Reverse the direction")).clicked() {
                        *dir = -*dir;
                    }
                });
                ui.weak(format!("{}: ({:.2}, {:.2}, {:.2})", label, dir.x, dir.y, dir.z));
                // the direction as numbers (any direction, not only X, Y and Z)
                ui.horizontal(|ui| {
                    let mut v = [dir.x, dir.y, dir.z];
                    let mut changed = false;
                    for (k, n) in ["dx", "dy", "dz"].iter().enumerate() {
                        ui.label(*n);
                        changed |= ui.add(egui::DragValue::new(&mut v[k]).speed(0.02).max_decimals(4)).changed();
                    }
                    if changed && v.iter().any(|x| x.abs() > 1e-9) {
                        *dir = DVec3::new(v[0], v[1], v[2]);
                        *label = tr("typed");
                    }
                });
            };
            match p.mode {
                LinkMode::Linear => {
                    ui.label(tr("Direction"));
                    dir_row(ui, &mut ui_state.pick, LinkPick::Direction, &mut p.dir, &mut p.dir_label);
                    ui.horizontal(|ui| {
                        pick_btn(ui, &mut ui_state.pick, LinkPick::Point, &tr("Second piece here"));
                        ui.weak(tr("click the place of the second piece: the spacing is taken from it"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Pieces"));
                        ui.add(egui::DragValue::new(&mut p.count).range(1..=1000).speed(0.2));
                        ui.label(if p.by_total { tr("Length") } else { tr("Spacing") });
                        ui.add(egui::DragValue::new(&mut p.spacing).speed(0.5).suffix(" mm"));
                    });
                    ui.checkbox(&mut p.by_total, tr("The spacing is the whole length"));
                    ui.checkbox(&mut p.second, tr("A second direction (a grid)"));
                    if p.second {
                        dir_row(ui, &mut ui_state.pick, LinkPick::Direction2, &mut p.dir2, &mut p.dir2_label);
                        ui.horizontal(|ui| {
                            ui.label(tr("Pieces"));
                            ui.add(egui::DragValue::new(&mut p.count2).range(1..=1000).speed(0.2));
                            ui.label(if p.by_total2 { tr("Length") } else { tr("Spacing") });
                            ui.add(egui::DragValue::new(&mut p.spacing2).speed(0.5).suffix(" mm"));
                        });
                        ui.checkbox(&mut p.by_total2, tr("The spacing is the whole length"));
                    }
                }
                LinkMode::Circular => {
                    ui.label(tr("Axis"));
                    ui.horizontal_wrapped(|ui| {
                        for (v, n) in [(DVec3::X, "X"), (DVec3::Y, "Y"), (DVec3::Z, "Z")] {
                            if ui.selectable_label(p.axis_label == n, n).clicked() {
                                p.axis = Axis { origin: DVec3::ZERO, dir: v };
                                p.axis_label = n.into();
                            }
                        }
                        pick_btn(ui, &mut ui_state.pick, LinkPick::Axis, &tr("Pick"));
                    });
                    ui.weak(format!(
                        "{}: ({:.1}, {:.1}, {:.1}) → ({:.2}, {:.2}, {:.2})",
                        p.axis_label, p.axis.origin.x, p.axis.origin.y, p.axis.origin.z, p.axis.dir.x, p.axis.dir.y, p.axis.dir.z
                    ));
                    ui.horizontal(|ui| {
                        ui.label(tr("Pieces"));
                        ui.add(egui::DragValue::new(&mut p.count).range(1..=1000).speed(0.2));
                        ui.checkbox(&mut p.full, tr("Round the whole circle"));
                    });
                    ui.checkbox(&mut p.keep_orientation, tr("Keep the orientation of the copies"));
                    ui.add_enabled_ui(!p.full, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(tr("Angle between pieces"));
                            ui.add(egui::DragValue::new(&mut p.angle).speed(0.5).suffix("°"));
                        });
                    });
                }
                LinkMode::Fill => {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut ui_state.grid, false, tr("The four corners"));
                        ui.radio_value(&mut ui_state.grid, true, tr("A grid"));
                    });
                    if ui_state.grid {
                        ui.horizontal(|ui| {
                            ui.label(tr("Columns"));
                            ui.add(egui::DragValue::new(&mut ui_state.columns).range(1..=200).speed(0.2));
                            ui.label(tr("Rows"));
                            ui.add(egui::DragValue::new(&mut ui_state.rows).range(1..=200).speed(0.2));
                            ui.label(tr("Margin"));
                            ui.add(egui::DragValue::new(&mut ui_state.margin).range(0.0..=1000.0).speed(0.2).suffix(" mm"));
                        });
                    }
                    ui.horizontal(|ui| {
                        pick_btn(ui, &mut ui_state.pick, LinkPick::RectFace, &tr("Pick the face"));
                        ui.label(rect_label.clone().unwrap_or_else(|| tr("click the flat rectangular face")));
                    });
                    ui.weak(tr("The copies keep the distances from the two nearest edges that the hole has. The face is read when you press Apply: a later change of the plate does not move them."));
                }
                LinkMode::Mirror => {
                    ui.label(tr("Mirror plane"));
                    ui.horizontal_wrapped(|ui| {
                        for (v, n, name) in [(DVec3::X, "YZ", "X"), (DVec3::Y, "XZ", "Y"), (DVec3::Z, "XY", "Z")] {
                            let _ = name;
                            if ui.selectable_label(p.plane_label == n, n).clicked() {
                                p.plane = Axis { origin: DVec3::ZERO, dir: v };
                                p.plane_label = n.into();
                            }
                        }
                        pick_btn(ui, &mut ui_state.pick, LinkPick::Plane, &tr("Flat face"));
                    });
                    if !sketches.is_empty() {
                        egui::ComboBox::from_id_salt("mirror-sketch")
                            .selected_text(tr("Plane of a sketch"))
                            .show_ui(ui, |ui| {
                                for (i, s) in sketches.iter().enumerate() {
                                    if ui.selectable_label(ui_state.plane_sketch == i + 1 && p.plane_label == s.name, &s.name).clicked() {
                                        ui_state.plane_sketch = i + 1;
                                        p.plane = Axis { origin: s.plane.origin, dir: s.plane.normal };
                                        p.plane_label = s.name.clone();
                                    }
                                }
                            });
                    }
                    ui.weak(format!(
                        "{}: ({:.1}, {:.1}, {:.1}) ⟂ ({:.2}, {:.2}, {:.2})",
                        p.plane_label, p.plane.origin.x, p.plane.origin.y, p.plane.origin.z, p.plane.dir.x, p.plane.dir.y, p.plane.dir.z
                    ));
                    ui.horizontal(|ui| {
                        ui.label(tr("Offset"));
                        let n = p.plane.dir.normalize_or_zero();
                        let mut off = p.plane.origin.dot(n);
                        if ui.add(egui::DragValue::new(&mut off).speed(0.5).suffix(" mm")).changed() {
                            p.plane.origin = n * off + (p.plane.origin - n * p.plane.origin.dot(n));
                            p.plane_label = tr("as set");
                        }
                    });
                    if editing.is_none() && !has_feature {
                        ui.separator();
                        for (h, n, tip) in [
                            (MirrorHow::InPlace, "Mirror the body itself", "The body is mirrored where it is."),
                            (MirrorHow::Keep, "Keep the original", "The mirror image is a new body."),
                            (MirrorHow::Merge, "Merge into one body", "The image is fused with the original into one symmetric body (the body must touch the plane)."),
                            (MirrorHow::Live, "Live mirror", "The mirror image follows when you change the original. A merged body cannot be live: an edit of one half could not be carried to the other reliably."),
                        ] {
                            ui.radio_value(&mut p.how, h, tr(n)).on_hover_text(tr(tip));
                        }
                    }
                }
            }
            if p.mode != LinkMode::Mirror && p.mode != LinkMode::Fill && !has_feature {
                ui.separator();
                let locked = editing.is_some();
                ui.add_enabled_ui(!locked, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(&mut p.result, 0, tr("New bodies"));
                        ui.radio_value(&mut p.result, 1, tr("Add to a body"));
                        ui.radio_value(&mut p.result, 2, tr("Cut from a body"));
                    });
                    if p.result != 0 {
                        ui.horizontal(|ui| {
                            let name = p.target.and_then(|t| bodies.iter().find(|b| b.id == t)).map(|b| b.name.clone());
                            let on = ui_state.pick == Some(LinkPick::Target);
                            if ui.selectable_label(on, tr("Pick the body")).clicked() {
                                ui_state.pick = if on { None } else { Some(LinkPick::Target) };
                            }
                            ui.label(name.unwrap_or_else(|| tr("click the body in the view")));
                        });
                    }
                });
            }
            if let Some(pk) = ui_state.pick {
                ui.colored_label(
                    crate::theme::rgb(255, 200, 100),
                    match pk {
                        LinkPick::Target => tr("Click the body in the view"),
                        LinkPick::Plane => tr("Click a flat face, a straight edge, an axis or a line of a sketch in the view"),
                        LinkPick::Point => tr("Click the place of the second piece on a face"),
                        LinkPick::RectFace => tr("Click the flat rectangular face of the same body"),
                        _ => tr("Click a straight edge or a face in the view"),
                    },
                );
            }
            if !ui_state.note.borrow().is_empty() {
                ui.colored_label(crate::theme::rgb(255, 150, 120), ui_state.note.borrow().as_str());
            }
            if let Some(e) = &error {
                ui.colored_label(crate::theme::rgb(255, 120, 110), format!("{}: {e}", tr("The last calculation failed; the link keeps its last result")));
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Apply")).clicked() {
                    apply = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
            });
            if let Some(id) = editing {
                ui.horizontal(|ui| {
                    if ui.button(tr("Remove the link")).on_hover_text(tr("The bodies stay as ordinary bodies")).clicked() {
                        unlink = Some(id);
                    }
                    if ui.button(tr("Delete with its bodies")).on_hover_text(tr("The copies go; a body that was added to or cut from gets its old shape back")).clicked() {
                        delete = Some(id);
                    }
                });
            }
        });
        self.link_ui = ui_state;
        self.docks = docks;
        if self.link_ui.p != before {
            self.layers_changed = true;
        }
        if apply {
            self.apply_link();
        }
        if let Some(id) = unlink {
            self.close_link_tool();
            self.submit(Command::Unlink { link: id });
        } else if let Some(id) = delete {
            self.close_link_tool();
            self.submit(Command::DeleteLink { link: id });
        } else if cancel {
            self.close_link_tool();
        }
    }
}

/// The 12 triangles of a box.
fn box_triangles(lo: Vec3, hi: Vec3, out: &mut Vec<Vec3>) {
    let c = |i: u32| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    for q in [
        [0u32, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        out.extend_from_slice(&[c(q[0]), c(q[1]), c(q[2]), c(q[0]), c(q[2]), c(q[3])]);
    }
}

impl App {
    /// The question after a hand edit of a body that a link made: remove the link and make the edit, or leave everything as it is.
    #[allow(deprecated)]
    pub(super) fn derived_window(&mut self, ctx: &egui::Context) {
        let Some((_, _, link_name, body_name)) = &self.derived_ask else {
            return;
        };
        let text = trf(
            "The body '{}' is made by the link '{}'. Editing it by hand would break the link.",
            &[body_name.clone(), link_name.clone()],
        );
        let mut answer = None;
        egui::Window::new(tr("The body is made by a link"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(text);
                ui.weak(tr("Removing the link keeps all its bodies as ordinary bodies, then the edit is made. Ctrl+Z brings the link back."));
                ui.horizontal(|ui| {
                    if ui.button(tr("Remove the link and edit")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                if let Some((cmd, link, _, _)) = self.derived_ask.take() {
                    self.after_unlink = Some(cmd);
                    self.submit(Command::Unlink { link });
                }
            }
            Some(false) => self.derived_ask = None,
            None => {}
        }
    }
}

/// What is dragged in the lists of the document: a body or a group.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dnd {
    Body(BodyId),
    Group(rolidium_doc::GroupId),
}

type TreeCmd = Option<(Command, &'static str)>;

impl App {
    /// The bodies that are marked now (in the view or in the list).
    pub(super) fn marked_bodies(&self) -> Vec<BodyId> {
        let mut v: Vec<BodyId> = self.move_sel.clone();
        for b in self
            .body_sel
            .into_iter()
            .chain(self.selection.iter().map(|s| s.0))
        {
            if !v.contains(&b) {
                v.push(b);
            }
        }
        v
    }

    /// Ctrl+G: the marked bodies become a group (or, with a group row marked and no bodies, nothing happens).
    pub(super) fn group_marked(&mut self) {
        let bodies = self.marked_bodies();
        if bodies.is_empty() {
            self.set_status("Select the bodies to group first", true);
            return;
        }
        let name = tr("Group");
        let n = self.doc.state().groups.len() + 1;
        self.run_doc_command(
            Command::CreateGroup {
                bodies,
                name: format!("{name} {n}"),
                parent: None,
            },
            "Group made",
        );
    }

    /// The lists of links and of groups under the list of bodies. `cmd` receives the command that a click asks for.
    pub(super) fn links_and_groups_tree(&mut self, ui: &mut egui::Ui, cmd: &mut TreeCmd) {
        let st = self.doc.state().clone();
        if !st.links.is_empty() {
            egui::CollapsingHeader::new(tr("Links"))
                .default_open(true)
                .show(ui, |ui| {
                    for l in &st.links {
                        ui.horizontal(|ui| {
                            let shown = l
                                .results
                                .iter()
                                .any(|r| st.bodies.iter().any(|b| b.id == *r && b.visible));
                            if icons::eye_button(ui, shown).clicked() {
                                *cmd = Some((
                                    Command::SetVisible {
                                        target: ItemId::Link(l.id),
                                        visible: !shown,
                                    },
                                    if shown { "Hidden" } else { "Shown" },
                                ));
                            }
                            let mut t =
                                trf("{}  ({} bodies)", &[l.name.clone(), l.results.len().to_string()]);
                            if l.error.is_some() {
                                t = format!("⚠ {t}");
                            }
                            let r = ui.selectable_label(self.link_sel == Some(l.id), t);
                            let r = match (&l.error, l.warnings.first()) {
                                (Some(e), _) => r.on_hover_text(format!(
                                    "{}: {e}",
                                    tr("The last calculation failed; the link keeps its last result")
                                )),
                                (None, Some(w)) => r.on_hover_text(w),
                                _ => r.on_hover_text(tr("Click to change the numbers of the link")),
                            };
                            if r.clicked() {
                                self.open_link(l.id);
                            }
                            if r.double_clicked() {
                                self.rename = Some((ItemId::Link(l.id), l.name.clone()));
                            }
                            r.context_menu(|ui| {
                                if ui.button(tr("Change the link")).clicked() {
                                    self.open_link(l.id);
                                    ui.close();
                                }
                                if ui.button(tr("Rename")).clicked() {
                                    self.rename = Some((ItemId::Link(l.id), l.name.clone()));
                                    ui.close();
                                }
                                if ui
                                    .button(tr("Remove the link"))
                                    .on_hover_text(tr("The bodies stay as ordinary bodies"))
                                    .clicked()
                                {
                                    *cmd = Some((Command::Unlink { link: l.id }, "The link was removed"));
                                    ui.close();
                                }
                                if ui.button(tr("Delete with its bodies")).clicked() {
                                    *cmd = Some((
                                        Command::DeleteLink { link: l.id },
                                        "Deleted (Ctrl+Z brings it back)",
                                    ));
                                    ui.close();
                                }
                            });
                        });
                    }
                });
        }
        egui::CollapsingHeader::new(tr("Groups"))
            .default_open(true)
            .show(ui, |ui| {
                if ui
                    .button(tr("New group from the marked bodies"))
                    .on_hover_text(tr(
                        "Ctrl+G. Drag bodies and groups onto a group to move them into it.",
                    ))
                    .clicked()
                {
                    self.group_marked();
                }
                let dragging = egui::DragAndDrop::has_any_payload(ui.ctx());
                if dragging && !st.groups.is_empty() {
                    let r = ui.add(
                        egui::Label::new(egui::RichText::new(tr("Out of every group")).weak())
                            .sense(egui::Sense::hover()),
                    );
                    let r = ui.interact(r.rect.expand(2.0), r.id.with("out"), egui::Sense::hover());
                    if let Some(p) = r.dnd_release_payload::<Dnd>() {
                        *cmd = Some(match *p {
                            Dnd::Body(b) => (
                                Command::MoveToGroup {
                                    bodies: vec![b],
                                    group: None,
                                },
                                "Moved",
                            ),
                            Dnd::Group(g) => (
                                Command::MoveGroup {
                                    group: g,
                                    parent: None,
                                },
                                "Moved",
                            ),
                        });
                    }
                }
                for g in st.groups.iter().filter(|g| g.parent.is_none()) {
                    self.group_rows(ui, &st, g, cmd, 0);
                }
                if st.groups.is_empty() {
                    ui.weak(tr("(none)"));
                }
            });
    }

    fn group_rows(
        &mut self,
        ui: &mut egui::Ui,
        st: &rolidium_doc::State,
        g: &rolidium_doc::Group,
        cmd: &mut TreeCmd,
        depth: usize,
    ) {
        if depth > 32 {
            return;
        }
        let id = ui.make_persistent_id(("group", g.id));
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
        let header = state.show_header(ui, |ui| {
            if icons::eye_button(ui, g.visible).clicked() {
                *cmd = Some((
                    Command::SetVisible {
                        target: ItemId::Group(g.id),
                        visible: !g.visible,
                    },
                    if g.visible { "Hidden" } else { "Shown" },
                ));
            }
            let all = self.group_members(st, g.id);
            let marked = !all.is_empty() && all.iter().all(|b| self.marked_bodies().contains(b));
            let r = ui.selectable_label(marked, format!("{}  ({})", g.name, all.len()));
            let r = r.interact(egui::Sense::click_and_drag());
            if r.drag_started() {
                r.dnd_set_drag_payload(Dnd::Group(g.id));
            }
            if let Some(p) = r.dnd_release_payload::<Dnd>() {
                *cmd = Some(match *p {
                    Dnd::Body(b) => (
                        Command::MoveToGroup {
                            bodies: vec![b],
                            group: Some(g.id),
                        },
                        "Moved",
                    ),
                    Dnd::Group(x) => (
                        Command::MoveGroup {
                            group: x,
                            parent: Some(g.id),
                        },
                        "Moved",
                    ),
                });
            }
            if r.clicked() {
                // a click on the group marks all its bodies
                self.clear_selection();
                self.move_sel = all.clone();
                self.body_sel = None;
                self.layers_changed = true;
            }
            if r.double_clicked() {
                self.rename = Some((ItemId::Group(g.id), g.name.clone()));
            }
            r.context_menu(|ui| {
                if ui.button(tr("Rename")).clicked() {
                    self.rename = Some((ItemId::Group(g.id), g.name.clone()));
                    ui.close();
                }
                if ui
                    .button(tr("Group inside"))
                    .on_hover_text(tr("Makes a group of the marked bodies inside this group"))
                    .clicked()
                {
                    let bodies = self.marked_bodies();
                    if !bodies.is_empty() {
                        let name = format!("{} {}", tr("Group"), st.groups.len() + 1);
                        *cmd = Some((
                            Command::CreateGroup {
                                bodies,
                                name,
                                parent: Some(g.id),
                            },
                            "Group made",
                        ));
                    }
                    ui.close();
                }
                if ui
                    .button(tr("Dissolve the group"))
                    .on_hover_text(tr("The bodies stay, one level up"))
                    .clicked()
                {
                    *cmd = Some((Command::Ungroup { group: g.id }, "Group dissolved"));
                    ui.close();
                }
            });
        });
        header.body(|ui| {
            for sub in st.groups.iter().filter(|x| x.parent == Some(g.id)) {
                self.group_rows(ui, st, sub, cmd, depth + 1);
            }
            for bid in &g.bodies {
                let Some(b) = st.bodies.iter().find(|b| b.id == *bid) else {
                    continue;
                };
                let on = self.marked_bodies().contains(bid);
                let r = ui.selectable_label(
                    on,
                    if b.visible {
                        b.name.clone()
                    } else {
                        format!("{}  ({})", b.name, tr("hidden"))
                    },
                );
                let r = r.interact(egui::Sense::click_and_drag());
                if r.drag_started() {
                    r.dnd_set_drag_payload(Dnd::Body(*bid));
                }
                if r.clicked() {
                    self.clear_selection();
                    self.body_sel = Some(*bid);
                    self.move_sel.clear();
                    self.layers_changed = true;
                }
                if r.double_clicked() {
                    self.rename = Some((ItemId::Body(*bid), b.name.clone()));
                }
            }
        });
    }

    /// The bodies of a group and of the groups inside it.
    fn group_members(&self, st: &rolidium_doc::State, g: rolidium_doc::GroupId) -> Vec<BodyId> {
        let mut out = Vec::new();
        let mut todo = vec![g];
        let mut guard = 0;
        while let Some(x) = todo.pop() {
            guard += 1;
            if guard > 10_000 {
                break;
            }
            for gr in st.groups.iter().filter(|gr| gr.id == x) {
                out.extend(gr.bodies.iter().copied());
            }
            todo.extend(st.groups.iter().filter(|gr| gr.parent == Some(x)).map(|gr| gr.id));
        }
        out
    }
}

impl App {
    /// The centre of the sources (for the handle), in the model.
    fn sources_centre(&self) -> Option<DVec3> {
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for b in self.link_sources() {
            if let Some(m) = self.entry(b) {
                for p in &m.mesh.positions {
                    lo = lo.min(dv(*p));
                    hi = hi.max(dv(*p));
                }
            }
        }
        (lo.x <= hi.x).then(|| (lo + hi) * 0.5)
    }

    /// A handle at the first copy of a linear pattern: drag it along the direction to change the spacing. `true` while it is used.
    #[allow(deprecated)]
    fn spacing_handle(&mut self, resp: &egui::Response) -> bool {
        if self.tool != Tool::Pattern
            || self.link_ui.p.mode != LinkMode::Linear
            || self.link_ui.pick.is_some()
        {
            return false;
        }
        let Some(c) = self.sources_centre() else {
            return false;
        };
        let rect = resp.rect;
        let dir = self.link_ui.p.dir.normalize_or_zero();
        if dir == DVec3::ZERO {
            return false;
        }
        let step = if self.link_ui.p.by_total && self.link_ui.p.count > 1 {
            self.link_ui.p.spacing / (self.link_ui.p.count - 1) as f64
        } else {
            self.link_ui.p.spacing
        };
        let (Some(a), Some(b)) = (
            self.project(c.as_vec3(), rect),
            self.project((c + dir * step).as_vec3(), rect),
        ) else {
            return false;
        };
        let painter = resp.ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("link-handle"),
        ));
        let over = resp.hover_pos().is_some_and(|p| p.distance(b) < 11.0);
        painter.line_segment([a, b], egui::Stroke::new(1.5, crate::theme::rgb(255, 200, 100)));
        painter.circle(
            b,
            if over || self.link_ui.handle_drag {
                8.0
            } else {
                6.0
            },
            crate::theme::rgb(255, 200, 100),
            egui::Stroke::new(1.5, Color32::BLACK),
        );
        if resp.drag_started_by(PointerButton::Primary) && over {
            self.link_ui.handle_drag = true;
        }
        if !resp.dragged_by(PointerButton::Primary) {
            self.link_ui.handle_drag = false;
        }
        if self.link_ui.handle_drag {
            if let Some(p) = resp.interact_pointer_pos() {
                let screen = b - a;
                let px_per_mm = screen.length() as f64 / step.max(1e-9);
                if px_per_mm > 1e-6 {
                    let along = ((p - a).dot(screen.normalized()) as f64) / px_per_mm;
                    let mut mm = along.max(0.1);
                    // snaps to whole millimetres (Shift: free)
                    if !resp.ctx.input(|i| i.modifiers.shift) {
                        mm = mm.round().max(1.0);
                    }
                    let p = &mut self.link_ui.p;
                    p.spacing = if p.by_total && p.count > 1 {
                        mm * (p.count - 1) as f64
                    } else {
                        mm
                    };
                    self.layers_changed = true;
                }
            }
            resp.ctx.request_repaint();
            return true;
        }
        false
    }
}

impl App {
    /// The panel of patterns and mirrors of the shapes of the sketch (all the shapes, or the one that is selected).
    #[allow(deprecated)]
    pub(super) fn sketch_pattern_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(mode) = self.link_ui.sk_open else { return };
        let Some(sk) = self.sk.as_ref().and_then(|s| s.sketch) else {
            self.link_ui.sk_open = None;
            return;
        };
        let one = self.entity_sel.filter(|(s, _)| *s == sk).map(|(_, e)| e);
        // the lines and the auxiliary axes of the sketch (a mirror line, a direction), and the centres of its circles and arcs
        let (lines, centres): (Vec<(String, glam::DVec2, f64)>, Vec<(String, glam::DVec2)>) = self
            .doc
            .sketch(sk)
            .map(|sd| {
                let mut lines = Vec::new();
                let mut centres = Vec::new();
                for (id, e) in sd.sketch.entities() {
                    if let Some((a, d)) = e.axis_line() {
                        let name = if e.is_axis() {
                            tr("Auxiliary axis")
                        } else {
                            tr("Line")
                        };
                        lines.push((format!("{name} {id}"), a, d.y.atan2(d.x).to_degrees()));
                        if e.is_axis() {
                            centres.push((format!("{name} {id}"), a));
                        }
                    }
                    if let Entity::Circle { center, .. } | Entity::Arc { center, .. } = e {
                        centres.push((format!("{} {id}", tr("Circle")), *center));
                    }
                }
                (lines, centres)
            })
            .unwrap_or_default();
        let mut u = std::mem::take(&mut self.link_ui);
        let mut docks = std::mem::take(&mut self.docks);
        let mut apply = false;
        let mut close = false;
        let title = if mode == LinkMode::Mirror {
            tr("Mirror")
        } else {
            tr("Pattern")
        };
        docks.panel(
            ctx,
            "skpattern",
            Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0),
            None,
            250.0,
            &title,
            false,
            |ui| {
                ui.label(if one.is_some() {
                    tr("The selected shape")
                } else {
                    tr("All the shapes of the sketch")
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut u.sk_open, Some(LinkMode::Linear), tr("Along a line"));
                    ui.selectable_value(&mut u.sk_open, Some(LinkMode::Circular), tr("Round a point"));
                    ui.selectable_value(&mut u.sk_open, Some(LinkMode::Mirror), tr("Mirror"));
                });
                match u.sk_open {
                    Some(LinkMode::Linear) => {
                        ui.horizontal(|ui| {
                            ui.label(tr("Direction"));
                            ui.add(egui::DragValue::new(&mut u.sk_angle).speed(1.0).suffix("°"));
                            if !lines.is_empty() {
                                egui::ComboBox::from_id_salt("sk_dir_from")
                                    .selected_text(tr("From a line"))
                                    .show_ui(ui, |ui| {
                                        for (n, _, ang) in &lines {
                                            if ui.selectable_label(false, n).clicked() {
                                                u.sk_angle = *ang;
                                            }
                                        }
                                    });
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr("Pieces"));
                            ui.add(egui::DragValue::new(&mut u.sk_count).range(1..=1000).speed(0.2));
                            ui.label(if u.sk_by_total {
                                tr("Length")
                            } else {
                                tr("Spacing")
                            });
                            ui.add(egui::DragValue::new(&mut u.sk_spacing).speed(0.5).suffix(" mm"));
                        });
                        ui.checkbox(&mut u.sk_by_total, tr("The spacing is the whole length"));
                    }
                    Some(LinkMode::Circular) => {
                        ui.horizontal(|ui| {
                            ui.label(tr("Centre"));
                            ui.add(egui::DragValue::new(&mut u.sk_cx).speed(0.5));
                            ui.add(egui::DragValue::new(&mut u.sk_cy).speed(0.5));
                            if !centres.is_empty() {
                                egui::ComboBox::from_id_salt("sk_centre_from")
                                    .selected_text(tr("From a shape"))
                                    .show_ui(ui, |ui| {
                                        for (n, c) in &centres {
                                            if ui.selectable_label(false, n).clicked() {
                                                (u.sk_cx, u.sk_cy) = (c.x, c.y);
                                            }
                                        }
                                    });
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr("Pieces"));
                            ui.add(egui::DragValue::new(&mut u.sk_count).range(1..=1000).speed(0.2));
                            ui.checkbox(&mut u.sk_full, tr("Round the whole circle"));
                        });
                        ui.add_enabled_ui(!u.sk_full, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(tr("Angle between pieces"));
                                ui.add(egui::DragValue::new(&mut u.sk_step_angle).speed(0.5).suffix("°"));
                            });
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.label(tr("Mirror line through"));
                            ui.add(egui::DragValue::new(&mut u.sk_cx).speed(0.5));
                            ui.add(egui::DragValue::new(&mut u.sk_cy).speed(0.5));
                        });
                        ui.horizontal(|ui| {
                            ui.label(tr("Direction"));
                            ui.add(egui::DragValue::new(&mut u.sk_angle).speed(1.0).suffix("°"));
                        });
                        if !lines.is_empty() {
                            egui::ComboBox::from_id_salt("sk_mirror_from")
                                .selected_text(tr("Mirror line: a line or an axis of the sketch"))
                                .show_ui(ui, |ui| {
                                    for (n, a, ang) in &lines {
                                        if ui.selectable_label(false, n).clicked() {
                                            (u.sk_cx, u.sk_cy, u.sk_angle) = (a.x, a.y, *ang);
                                        }
                                    }
                                });
                        }
                        ui.checkbox(&mut u.sk_keep, tr("Keep the original"));
                    }
                }
                ui.weak(tr(
                    "The copies are ordinary shapes of the sketch; Undo takes them back.",
                ));
                ui.horizontal(|ui| {
                    if ui.button(tr("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                });
            },
        );
        self.docks = docks;
        let mode = u.sk_open.unwrap_or(mode);
        // the preview in the model area follows the numbers
        let key = format!(
            "{mode:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{one:?}|{}",
            u.sk_count,
            u.sk_spacing,
            u.sk_by_total,
            u.sk_angle,
            u.sk_cx,
            u.sk_cy,
            u.sk_step_angle,
            u.sk_full,
            u.sk_keep,
            self.doc.revision()
        );
        if key != u.sk_key {
            u.sk_key = key;
            u.sk_preview = self.sketch_pattern_preview(&u, sk, mode, one);
            self.layers_changed = true;
        }
        let cmd = if apply {
            let dir = glam::DVec2::new(u.sk_angle.to_radians().cos(), u.sk_angle.to_radians().sin());
            let entities: Vec<_> = one.into_iter().collect();
            Some(match mode {
                LinkMode::Linear | LinkMode::Fill => Command::SketchPattern {
                    sketch: sk,
                    entities,
                    pattern: rolidium_doc::SketchPattern::Linear {
                        dir,
                        count: u.sk_count,
                        spacing: u.sk_spacing,
                        by_total: u.sk_by_total,
                        second: None,
                    },
                },
                LinkMode::Circular => Command::SketchPattern {
                    sketch: sk,
                    entities,
                    pattern: rolidium_doc::SketchPattern::Circular {
                        center: glam::DVec2::new(u.sk_cx, u.sk_cy),
                        count: u.sk_count,
                        angle_deg: u.sk_step_angle,
                        full: u.sk_full,
                    },
                },
                LinkMode::Mirror => {
                    let a = glam::DVec2::new(u.sk_cx, u.sk_cy);
                    Command::SketchMirror {
                        sketch: sk,
                        entities,
                        a,
                        b: a + dir,
                        keep: u.sk_keep,
                    }
                }
            })
        } else {
            None
        };
        if close {
            u.sk_open = None;
            u.sk_preview.clear();
            u.sk_key.clear();
            self.layers_changed = true;
        }
        if cmd.is_some() {
            u.sk_preview.clear();
            u.sk_key.clear();
        }
        self.link_ui = u;
        if let Some(c) = cmd {
            self.submit(c);
        }
    }
}

impl App {
    /// The copies that the numbers of the panel would make, as pairs of points in 3D for the preview.
    fn sketch_pattern_preview(
        &self,
        u: &LinkUi,
        sk: SketchId,
        mode: LinkMode,
        one: Option<rolidium_doc::rolidium_sketch::EntityId>,
    ) -> Vec<Vec3> {
        let Some(sd) = self.doc.sketch(sk) else {
            return Vec::new();
        };
        let dir = glam::DVec2::new(u.sk_angle.to_radians().cos(), u.sk_angle.to_radians().sin());
        let ids: Vec<_> = one.into_iter().collect();
        let made = match mode {
            LinkMode::Linear | LinkMode::Fill => rolidium_doc::preview_pattern(
                sd,
                &ids,
                &rolidium_doc::SketchPattern::Linear {
                    dir,
                    count: u.sk_count,
                    spacing: u.sk_spacing,
                    by_total: u.sk_by_total,
                    second: None,
                },
            ),
            LinkMode::Circular => rolidium_doc::preview_pattern(
                sd,
                &ids,
                &rolidium_doc::SketchPattern::Circular {
                    center: glam::DVec2::new(u.sk_cx, u.sk_cy),
                    count: u.sk_count,
                    angle_deg: u.sk_step_angle,
                    full: u.sk_full,
                },
            ),
            LinkMode::Mirror => {
                let a = glam::DVec2::new(u.sk_cx, u.sk_cy);
                rolidium_doc::preview_mirror(sd, &ids, a, a + dir, true)
            }
        };
        let mut out = Vec::new();
        for e in made.unwrap_or_default() {
            for pl in e.polylines() {
                for w in pl.windows(2) {
                    out.push(sd.plane.to_3d(w[0]).as_vec3());
                    out.push(sd.plane.to_3d(w[1]).as_vec3());
                }
            }
        }
        out
    }
}
