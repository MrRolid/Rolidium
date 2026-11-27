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
    /// Along a curve of any shape (an arc, a circle, a spline, a helix, a chain of lines and arcs, the edges of a body).
    Path,
}

/// What the person chose in the first step of the Pattern tool (after the object): what the second click is on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Want {
    /// A line or a curve: a row along it.
    Curve,
    /// A point or an axis: round it.
    Point,
    /// A face: its corners or a grid on it.
    Face,
}

/// What the next click in the view sets.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LinkPick {
    /// The place of the second piece of a row: a point on a face.
    Point,
    Direction,
    Direction2,
    Axis,
    Plane,
    Target,
    /// The centre of a circular pattern: any point that can be snapped to (a vertex, the centre of a circle or an arc, the middle of a line, a point of a sketch).
    Centre,
}

/// What the second click of the Pattern and Mirror tools is on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hovered {
    Nothing,
    /// A straight line: an edge, a line or an auxiliary axis of a sketch.
    Line,
    /// A circle, an arc, a round edge, a cylindrical or conical face.
    Round,
    /// A flat face.
    Flat,
    /// Something that gives neither a direction nor a centre nor a plane.
    Other,
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
    /// Along a curve: the curve as a polyline, whether it is closed, a name for the panel, the distance between the pieces (0: by the count), the distances left
    /// free at the ends, and whether the pieces turn with the tangent.
    pub path: Vec<DVec3>,
    pub path_closed: bool,
    pub path_label: String,
    pub pitch: f64,
    pub start: f64,
    pub end: f64,
    pub orient: bool,
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
            path: Vec::new(),
            path_closed: false,
            path_label: String::new(),
            pitch: 0.0,
            start: 0.0,
            end: 0.0,
            orient: true,
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
    /// A rectangle of a sketch the corners or the grid follow (instead of a face).
    pub rect_sketch: Option<(SketchId, rolidium_doc::rolidium_sketch::EntityId)>,
    /// A link that follows a rectangle is being changed: the distances of the pieces to the nearest edges (along the two sides).
    pub near: Option<(f64, f64)>,
    pub grid: bool,
    pub columns: u32,
    pub rows: u32,
    pub margin: f64,
    /// The mouse handle of the spacing is being dragged.
    pub handle_drag: bool,
    /// The second click was made (a kind is chosen): the preview is shown and the panel has the numbers. Enter or a click on nothing applies.
    pub armed: bool,
    /// What the pointer is over, and what a click there would do (the text at the cursor).
    pub cursor_hint: String,
    /// While the place of the second piece is chosen: the point under the pointer on the face, its live distances to the nearest edges, and the typed ones.
    pub second: Option<SecondHover>,
    pub second_edit: super::edgeedit::EdgeEdit,
    /// The panel of patterns and mirrors of the shapes of the sketch that is open (`Linear`, `Circular` or `Mirror`).
    pub sk_open: Option<LinkMode>,
    /// What the person chose after the object: a line or a curve, a point or an axis, a face (the wizard of the Pattern tool).
    pub want: Option<Want>,
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
    /// The pattern of a sketch repeats all its shapes (a choice in the panel), instead of the ones that are selected.
    pub sk_all: bool,
    /// What the person chose in the first step in a sketch, and whether the guide was clicked (the numbers are shown then).
    pub sk_want: Option<Want>,
    pub sk_guided: bool,
    /// A grid: the second direction (degrees), its pieces and its spacing.
    pub sk_second: bool,
    pub sk_count2: u32,
    pub sk_spacing2: f64,
    /// The text of the guide that was clicked ("a line of the sketch", ...).
    pub sk_guide_label: String,
}

/// The lines and axes of a sketch (name, a point, the angle) and the centres it offers (name, point).
type SketchRefs = (Vec<(String, glam::DVec2, f64)>, Vec<(String, glam::DVec2)>);

/// The point where the second piece would go.
#[derive(Clone)]
pub struct SecondHover {
    pub plane: PlaneDef,
    /// The place (after the snaps and the typed distances), in the plane of the face.
    pub at: DVec2,
    pub dims: Option<rolidium_doc::rolidium_sketch::edgedims::EdgeDims>,
    pub snap: Option<rolidium_doc::rolidium_sketch::edgedims::EdgeSnap>,
    pub body: BodyId,
    pub face: u32,
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
            LinkMode::Path => LinkKind::Path(
                rolidium_doc::PathParams {
                    points: self.path.clone(),
                    closed: self.path_closed,
                    count: self.count,
                    pitch: self.pitch,
                    start: self.start,
                    end: self.end,
                    orient: self.orient,
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
            LinkKind::Path(q, m) => {
                p.mode = LinkMode::Path;
                p.path = q.points.clone();
                p.path_closed = q.closed;
                p.path_label = tr("a curve");
                p.count = q.count;
                p.pitch = q.pitch;
                p.start = q.start;
                p.end = q.end;
                p.orient = q.orient;
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
            LinkKind::Loft { .. } | LinkKind::Sweep { .. } | LinkKind::Shell(_) | LinkKind::Text(_) => {}
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
            u.sk_want = None;
            u.sk_guided = false;
            u.sk_all = false;
            u.sk_second = false;
            u.sk_guide_label.clear();
            if u.sk_count2 == 0 {
                (u.sk_count2, u.sk_spacing2) = (3, 20.0);
            }
            // the shapes are chosen with the Select of the sketch (a click, Ctrl, Shift, a rectangle)
            self.sketch_select_tool();
            self.link_ui.sk_open = Some(mode);
            return;
        }
        if self.doc.bodies().is_empty() && self.doc.meshes().is_empty() {
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
        self.link_ui.want = None;
        self.link_ui.feature = None;
        self.link_ui.armed = false;
        self.link_ui.rect_face = None;
        self.link_ui.rect_sketch = None;
        self.link_ui.near = None;
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
        let sources = self.link_sources();
        let extent = |dir: DVec3| -> f64 {
            let dir = dir.normalize_or_zero();
            let mut ext = 0.0f64;
            for b in &sources {
                if let Some(m) = self.entry(*b) {
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
            ext
        };
        let (e1, e2) = (extent(self.link_ui.p.dir), extent(self.link_ui.p.dir2));
        if e1 > 1e-6 {
            self.link_ui.p.spacing = (e1 * 1.2 * 10.0).round() / 10.0;
        }
        if e2 > 1e-6 {
            self.link_ui.p.spacing2 = (e2 * 1.2 * 10.0).round() / 10.0;
        } else if e1 > 1e-6 {
            self.link_ui.p.spacing2 = self.link_ui.p.spacing;
        }
    }

    /// Opens the panel of a link (a click on it in the list of the document).
    pub(super) fn open_link(&mut self, id: LinkId) {
        let Some(l) = self.doc.state().links.iter().find(|l| l.id == id).cloned() else {
            return;
        };
        if matches!(l.kind, LinkKind::Shell(_)) {
            return self.open_shell_link(id);
        }
        if matches!(l.kind, LinkKind::Text(_)) {
            return self.open_text_link(id);
        }
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
        self.link_ui.near = l.fill.as_ref().map(|f| (f.sides.near_a, f.sides.near_b));
        if l.fill.is_some() {
            self.link_ui.p.mode = LinkMode::Fill;
        }
        if let Some(f) = &l.fill {
            match f.fill {
                rolidium_doc::RectFill::Corners => self.link_ui.grid = false,
                rolidium_doc::RectFill::Grid {
                    columns,
                    rows,
                    margin,
                } => {
                    (
                        self.link_ui.grid,
                        self.link_ui.columns,
                        self.link_ui.rows,
                        self.link_ui.margin,
                    ) = (true, columns, rows, margin)
                }
            }
        }
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
        self.link_ui.armed = false;
        self.link_ui.feature = None;
        self.link_sel = None;
        self.move_sel.clear();
        self.tool = Tool::Select;
        self.layers_changed = true;
    }

    /// What the pointer is over, for the second click: a curve or an axis first, else the face.
    pub(super) fn hovered_second(&self) -> Hovered {
        match self.hover_obj {
            Some(Obj::Entity(s, id)) => {
                let Some((plane, e)) = self
                    .doc
                    .sketch(s)
                    .and_then(|sk| sk.sketch.entity(id).map(|e| (sk.plane, *e)))
                else {
                    return Hovered::Other;
                };
                if rolidium_doc::sketch_line_in_space(&plane, &e).is_some() {
                    return Hovered::Line;
                }
                return match e {
                    Entity::Circle { .. } | Entity::Arc { .. } => Hovered::Round,
                    // a rectangle of a sketch: the corners of a hole follow it
                    Entity::Rect { .. } if self.link_ui.feature.is_some() => Hovered::Flat,
                    _ => Hovered::Other,
                };
            }
            Some(Obj::Edge(b, e)) => {
                if let Some(i) = self
                    .doc
                    .body(b)
                    .and_then(|body| body.solid.edge_info(rolidium_kernel::EdgeId(e)).ok())
                {
                    return match i.kind {
                        rolidium_kernel::EdgeKind::Line => Hovered::Line,
                        rolidium_kernel::EdgeKind::Circle => Hovered::Round,
                        rolidium_kernel::EdgeKind::Other => Hovered::Other,
                    };
                }
                return Hovered::Other;
            }
            Some(Obj::Curve(_)) => return Hovered::Other,
            None => {}
        }
        let Some((b, f)) = self.hovered_face() else {
            return Hovered::Nothing;
        };
        match self
            .entry(b)
            .and_then(|m| m.faces.get(f as usize))
            .map(|m| m.kind)
        {
            Some(FaceKind::Plane) => Hovered::Flat,
            Some(FaceKind::Cylinder | FaceKind::Cone) => Hovered::Round,
            _ => Hovered::Other,
        }
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
        self.update_second_hover(resp);
        let ch = if resp.hovered() {
            self.cursor_hint_text()
        } else {
            String::new()
        };
        if ch != self.link_ui.cursor_hint {
            self.link_ui.cursor_hint = ch;
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
            Some(LinkPick::Centre) => {
                if self.pick_centre_point(resp) {
                    self.link_ui.pick = None;
                    self.layers_changed = true;
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
                let have = self.link_ui.feature.is_some() || !self.link_sources().is_empty();
                let ctrl = resp.ctx.input(|i| i.modifiers.command);
                if have && !ctrl {
                    // the second click: what is under the pointer decides the kind
                    if self.second_click(resp) {
                        return;
                    }
                    if self.hovered_second() == Hovered::Nothing {
                        // a click on nothing confirms what the preview shows
                        if self.link_ui.armed {
                            self.apply_link();
                        } else {
                            self.set_status("Click a curve, an axis or a face (Esc cancels)", false);
                        }
                    }
                    return;
                }
                // the round face of a hole or a pin is a thing to repeat in its own body
                if !have {
                    if let Some((b, f)) = self.hovered_face() {
                        if self.is_feature_face(b, f) {
                            self.link_ui.feature = Some((b, f));
                            self.move_sel.clear();
                            self.selection = vec![(b, f)];
                            self.link_ui.rect_face = None;
                            self.link_ui.armed = false;
                            self.set_status(
                                "The hole or the pin is the source: now click a curve, an axis or a face",
                                false,
                            );
                            self.layers_changed = true;
                            return;
                        }
                    }
                }
                if let Some(b) = hover {
                    self.link_ui.feature = None;
                    if let Some(i) = self.move_sel.iter().position(|x| *x == b) {
                        self.move_sel.remove(i);
                    } else {
                        self.move_sel.push(b);
                    }
                    self.link_ui.armed = false;
                    self.default_spacing();
                    self.layers_changed = true;
                }
            }
        }
    }

    /// The second click of the Pattern and Mirror tools: a circle, an arc, a round edge or a round face means round its centre, a straight line, edge or
    /// axis a row along it, a flat face the corners of it (a hole) or a grid; for the Mirror tool a flat face, a straight edge or a line is the plane.
    /// `true` when the click was taken.
    fn second_click(&mut self, resp: &egui::Response) -> bool {
        let mirror = self.tool == Tool::Mirror;
        let h = self.hovered_second();
        let before = self.link_ui.p.mode;
        // the person chose "along a line or a curve": any curve under the pointer is the path (a straight line stays a row)
        let curve_wanted = !mirror && self.link_ui.want == Some(Want::Curve);
        if curve_wanted
            && matches!(
                self.hover_obj,
                Some(Obj::Entity(..) | Obj::Curve(_) | Obj::Edge(..))
            )
        {
            let straight = h == Hovered::Line;
            if !straight && self.pick_path_guide() {
                let p = &mut self.link_ui.p;
                if !self.link_ui.armed || before != LinkMode::Path {
                    p.count = 5;
                    p.pitch = 0.0;
                    p.start = 0.0;
                    p.end = 0.0;
                }
                self.link_ui.armed = true;
                self.link_ui.pick = None;
                self.layers_changed = true;
                self.set_status(
                    "A pattern along a curve: set the numbers in the panel, Enter applies, Esc cancels",
                    false,
                );
                return true;
            }
        }
        // the person chose "round a point": a circle or a round face is the centre as before, an axis or a line is the axis, any other point of a body or a sketch is the centre
        if !mirror && self.link_ui.want == Some(Want::Point) && h != Hovered::Round && h != Hovered::Nothing {
            let taken = if h == Hovered::Line {
                self.link_ui.p.mode = LinkMode::Circular;
                self.pick_link_geometry(LinkPick::Axis)
            } else {
                self.link_ui.p.mode = LinkMode::Circular;
                let suggested = self.suggested_axis();
                self.link_ui.p.axis.dir = suggested;
                self.pick_centre_point(resp)
            };
            if taken {
                let p = &mut self.link_ui.p;
                if !self.link_ui.armed {
                    (p.count, p.full) = (6, true);
                }
                self.link_ui.armed = true;
                self.link_ui.pick = None;
                self.layers_changed = true;
                self.set_status(
                    "Round a centre: set the number in the panel, Enter applies, Esc cancels",
                    false,
                );
            } else {
                self.link_ui.p.mode = before;
            }
            return true;
        }
        let ok = match (mirror, h) {
            (_, Hovered::Nothing) => return false,
            (true, Hovered::Line) => self.pick_link_geometry(LinkPick::Plane),
            (true, Hovered::Flat) => self.pick_link_geometry(LinkPick::Plane) || self.pick_mesh_plane(),
            (true, _) => {
                self.set_status(
                    "Click a flat face, a straight edge, an axis or a line of a sketch",
                    true,
                );
                return true;
            }
            (false, Hovered::Line) => {
                self.link_ui.p.mode = LinkMode::Linear;
                self.link_ui.p.second = false;
                self.pick_link_geometry(LinkPick::Direction)
            }
            (false, Hovered::Round) => {
                self.link_ui.p.mode = LinkMode::Circular;
                self.pick_link_geometry(LinkPick::Axis)
            }
            (false, Hovered::Flat) => self.second_flat(),
            (false, Hovered::Other) => {
                self.set_status("Click a curve, an axis or a flat face", true);
                return true;
            }
        };
        if !ok {
            self.link_ui.p.mode = before;
            return true;
        }
        if mirror {
            self.link_ui.p.mode = LinkMode::Mirror;
        }
        let p = &mut self.link_ui.p;
        if p.mode != before || !self.link_ui.armed {
            // the usual number of pieces of each kind: 3 in a row, 6 round an axis
            match p.mode {
                LinkMode::Circular => (p.count, p.full) = (6, true),
                LinkMode::Linear if p.count == 6 || !self.link_ui.armed => p.count = 3,
                _ => {}
            }
        }
        self.link_ui.armed = true;
        self.link_ui.pick = None;
        self.default_spacing();
        self.layers_changed = true;
        let say = match (mirror, self.link_ui.p.mode, self.link_ui.p.second) {
            (true, _, _) => "Mirror: change the options in the panel, Enter applies, Esc cancels",
            (_, LinkMode::Circular, _) => {
                "Round a centre: set the number in the panel, Enter applies, Esc cancels"
            }
            (_, LinkMode::Fill, _) => {
                "Into the corners: change the options in the panel, Enter applies, Esc cancels"
            }
            (_, _, true) => "A grid: set the numbers in the panel, Enter applies, Esc cancels",
            _ => "A row: set the number in the panel, Enter applies, Esc cancels",
        };
        self.set_status(say, false);
        true
    }

    /// A curve under the pointer as a polyline in space (and whether it is closed): the shapes of a sketch that continue each other, a curve, an edge of a body.
    fn guide_polyline(&self) -> Option<(Vec<DVec3>, bool, String)> {
        use rolidium_doc::SweepPath;
        match self.hover_obj? {
            Obj::Entity(s, e) => {
                let sk = self.doc.sketch(s)?;
                let chain = rolidium_doc::sketch_chain(&sk.sketch, e);
                let (pts, closed) = self
                    .doc
                    .path_polyline(&SweepPath::Sketch {
                        sketch: s,
                        entities: chain,
                    })
                    .ok()?;
                Some((pts, closed, tr("a curve of a sketch")))
            }
            Obj::Curve(c) => {
                let (pts, closed) = self.doc.path_polyline(&SweepPath::Curve(c)).ok()?;
                Some((pts, closed, tr("a curve")))
            }
            Obj::Edge(b, e) => {
                let body = self.doc.body(b)?;
                let chain = body.solid.smooth_chain(&[rolidium_kernel::EdgeId(e)], 20.0);
                let ids: Vec<u32> = chain.iter().map(|x| x.0).collect();
                let (pts, closed) = self
                    .doc
                    .path_polyline(&SweepPath::Edges { body: b, edges: ids })
                    .ok()?;
                Some((pts, closed, tr("an edge")))
            }
        }
    }

    /// The curve under the pointer becomes the path of the pattern. `true` when the click took one.
    fn pick_path_guide(&mut self) -> bool {
        let Some((pts, closed, label)) = self.guide_polyline() else {
            return false;
        };
        let p = &mut self.link_ui.p;
        p.path = pts;
        p.path_closed = closed;
        p.path_label = label;
        p.mode = LinkMode::Path;
        true
    }

    /// The point for the centre of a circular pattern from a click: a vertex of a body, the centre of a round edge, a point of a shape of a sketch (an end point, a middle,
    /// a centre), or else the point of the face under the pointer. The direction of the axis stays (it was suggested from the face the object lies on).
    fn pick_centre_point(&mut self, resp: &egui::Response) -> bool {
        let (Some(pointer), rect) = (resp.hover_pos(), resp.rect) else {
            return false;
        };
        let mut best: Option<(f32, DVec3, String)> = None;
        let offer = |d: f32, p: DVec3, what: String, best: &mut Option<(f32, DVec3, String)>| {
            if d <= 12.0 && best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
                *best = Some((d, p, what));
            }
        };
        // the vertices of the bodies
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                for q in [e.points.first(), e.points.last()].into_iter().flatten() {
                    if let Some(sp) = self.project(*q, rect) {
                        offer((sp - pointer).length(), q.as_dvec3(), tr("a vertex"), &mut best);
                    }
                }
            }
        }
        // the shapes of the sketches: end points, middles and centres
        for s in self.doc.sketches().iter().filter(|s| s.visible) {
            for (_, e) in s.sketch.entities() {
                for (q, _) in e.snap_points() {
                    let q3 = s.plane.to_3d(q);
                    if let Some(sp) = self.project(q3.as_vec3(), rect) {
                        offer((sp - pointer).length(), q3, tr("a point of a sketch"), &mut best);
                    }
                }
            }
        }
        // the centre of a round edge
        if let Some(Obj::Edge(b, e)) = self.hover_obj {
            if let Some(pl) = self.edge_polyline(b, e).filter(|p| p.len() >= 8) {
                let n = pl.len();
                let (a, m, c) = (dv(pl[0]), dv(pl[n / 3]), dv(pl[2 * n / 3]));
                let (u, v) = (m - a, c - a);
                if u.cross(v).length() > 1e-9 {
                    let (uu, vv, uv) = (u.dot(u), v.dot(v), u.dot(v));
                    let k = 2.0 * (uu * vv - uv * uv);
                    if k.abs() > 1e-12 {
                        let centre = a + u * (vv * (uu - uv) / k) + v * (uu * (vv - uv) / k);
                        // the circle itself is under the pointer: its centre is offered at once (distance 0)
                        offer(0.0, centre, tr("the centre of a round edge"), &mut best);
                    }
                }
            }
        }
        let picked = match best {
            Some((_, p, what)) => Some((p, what)),
            None => {
                let aspect = rect.width() / rect.height().max(1.0);
                let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
                self.hovered_face()
                    .and_then(|(b, f)| self.ray_face_point(b, f, o, d))
                    .map(|p| (p, tr("a point of a face")))
            }
        };
        let Some((point, what)) = picked else {
            self.set_status("Click a vertex, a centre, a point of a sketch or a face", true);
            return false;
        };
        let p = &mut self.link_ui.p;
        p.axis.origin = point;
        p.axis_label = what;
        true
    }

    /// The axis a circular pattern of the source suggests: the normal of the biggest flat face of the first source (the face it lies on), else Z.
    pub(super) fn suggested_axis(&self) -> DVec3 {
        let Some(b) = self.link_sources().first().copied() else {
            return DVec3::Z;
        };
        let Some(body) = self.doc.body(b) else {
            return DVec3::Z;
        };
        let mut best: Option<(f64, DVec3)> = None;
        for f in body.solid.faces() {
            if let Ok(i) = body.solid.face_info(f) {
                if i.kind == FaceKind::Plane && best.is_none_or(|(a, _)| i.area > a) {
                    best = Some((i.area, i.normal.normalize()));
                }
            }
        }
        best.map_or(DVec3::Z, |(_, n)| n)
    }

    /// A flat face as the second click of the Pattern tool: into the corners of it for a hole or a pin, else a grid on it.
    fn second_flat(&mut self) -> bool {
        if let (Some(Obj::Entity(s, e)), Some(_)) = (self.hover_obj, self.link_ui.feature) {
            self.link_ui.p.mode = LinkMode::Fill;
            self.link_ui.rect_sketch = Some((s, e));
            self.link_ui.rect_face = None;
            self.link_ui.grid = false;
            return true;
        }
        let Some((b, f)) = self.hovered_face() else {
            return false;
        };
        if let Some((fb, _)) = self.link_ui.feature {
            if fb != b {
                self.set_status(
                    "Click a flat face of the same body the hole or the pin is in",
                    true,
                );
                return false;
            }
            self.link_ui.p.mode = LinkMode::Fill;
            self.link_ui.rect_face = Some(f);
            self.link_ui.rect_sketch = None;
            self.link_ui.grid = false;
            return true;
        }
        let Some((normal, _)) = self.face_plane(b, f) else {
            return false;
        };
        let n = normal.normalize_or_zero();
        if n == DVec3::ZERO {
            return false;
        }
        // the two world axes that lie most in the plane of the face
        let mut axes = [
            (DVec3::X, n.x.abs()),
            (DVec3::Y, n.y.abs()),
            (DVec3::Z, n.z.abs()),
        ];
        axes.sort_by(|a, b| a.1.total_cmp(&b.1));
        let proj = |a: DVec3| (a - n * a.dot(n)).normalize_or_zero();
        let (u, v) = (proj(axes[0].0), proj(axes[1].0));
        let p = &mut self.link_ui.p;
        p.mode = LinkMode::Linear;
        p.second = true;
        (p.dir, p.dir_label) = (u, tr("a face"));
        (p.dir2, p.dir2_label) = (v, tr("a face"));
        (p.count, p.count2) = (3, 3);
        (p.by_total, p.by_total2) = (false, false);
        true
    }

    /// The origin and the normal of a flat face (of a solid or a wall of a mesh).
    fn face_plane(&self, b: BodyId, f: u32) -> Option<(DVec3, DVec3)> {
        let m = self.entry(b)?.faces.get(f as usize)?;
        (m.kind == FaceKind::Plane).then_some((m.normal, m.origin))
    }

    /// A wall of a mesh as the mirror plane.
    fn pick_mesh_plane(&mut self) -> bool {
        let Some((b, f)) = self.hovered_face() else {
            return false;
        };
        let Some((n, o)) = self.face_plane(b, f) else {
            return false;
        };
        self.link_ui.p.plane = Axis { origin: o, dir: n };
        self.link_ui.p.plane_label = tr("a face");
        true
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
                LinkPick::Plane => tr("Click a flat face, a straight edge, an axis or a line of a sketch, or the plane of a sketch for the mirror plane."),
                LinkPick::Axis => tr("Click a round edge, a cylindrical face, a straight edge, a face, or an axis or a line of a sketch for the axis (or press X, Y, Z)."),
                LinkPick::Target => tr("Click the body the pieces are added to or cut out of."),
                LinkPick::Centre => tr("Click a point: a vertex, the centre of a circle or an arc, the middle of a line, or a point of a sketch."),
                LinkPick::Direction | LinkPick::Direction2 => tr("Click a straight edge, a face, or a line of a sketch for the direction."),
            };
        }
        if !have {
            return if self.tool == Tool::Mirror {
                tr("Mirror: click what is mirrored: a body, or the round face of a hole or a pin.")
            } else {
                tr("Click what you want to repeat: a body (Ctrl adds more), or the round face of a hole or a pin.")
            };
        }
        if u.armed {
            return tr("Set the numbers in the panel; Enter or a click on nothing applies, Esc cancels. A click on another curve or face changes the kind.");
        }
        if self.tool == Tool::Mirror {
            tr("Mirror: click the plane: a flat face, a straight edge, an axis or a line of a sketch.")
        } else {
            tr("Pattern: click a curve, an axis or a face: a circle or a round face repeats round its centre, a line or an edge along it, a flat face into the corners or a grid.")
        }
    }

    /// The short text at the cursor: what to click, and what a click on the thing under the pointer would do.
    pub(super) fn cursor_hint_text(&self) -> String {
        let u = &self.link_ui;
        if u.editing.is_some() || !matches!(self.tool, Tool::Pattern | Tool::Mirror) || u.pick.is_some() {
            return String::new();
        }
        if u.feature.is_none() && self.link_sources().is_empty() {
            return tr("Click what you want to repeat");
        }
        match (self.tool == Tool::Mirror, self.hovered_second()) {
            (false, Hovered::Round) => tr("around the centre"),
            (false, Hovered::Line) => tr("along a line"),
            (false, Hovered::Flat) if u.feature.is_some() => tr("into the corners"),
            (false, Hovered::Flat) => tr("into a grid"),
            (true, Hovered::Line | Hovered::Flat) => tr("mirror across it"),
            (_, Hovered::Nothing) if u.armed => tr("Enter or click here: apply"),
            _ => {
                if self.tool == Tool::Mirror {
                    tr("Click a plane, an axis or a line")
                } else {
                    tr("Click a curve, an axis or a face")
                }
            }
        }
    }

    /// Draws the text at the cursor.
    pub(super) fn link_cursor_paint(&self, ui: &egui::Ui, rect: Rect) {
        let t = &self.link_ui.cursor_hint;
        if t.is_empty() {
            return;
        }
        let Some(cur) = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)) else {
            return;
        };
        let painter = ui.painter_at(rect);
        let galley = painter.layout_no_wrap(
            t.clone(),
            egui::FontId::proportional(15.0),
            crate::theme::rgb(255, 230, 140),
        );
        let at = cur + Vec2::new(16.0, 18.0);
        painter.rect_filled(
            Rect::from_min_size(at, galley.size()).expand(4.0),
            4.0,
            egui::Color32::from_black_alpha(180),
        );
        painter.galley(at, galley, crate::theme::rgb(255, 230, 140));
    }

    /// Whether a face is the round face of a hole or a pin of a body that has more than the round part itself (a plain cylinder is a body to repeat).
    pub(super) fn is_feature_face(&self, body: BodyId, face: u32) -> bool {
        self.doc.body(body).is_some_and(|b| {
            b.solid.face_count() > 3 && b.solid.cylinder_info(FaceId(face)).ok().flatten().is_some()
        })
    }

    /// While the second piece is chosen: the place under the pointer with its live distances to the two nearest edges of the face, the snaps and the
    /// typed distances.
    pub(super) fn second_paint(&self, ui: &egui::Ui, rect: Rect) {
        if !matches!(self.tool, Tool::Pattern) || self.link_ui.pick != Some(LinkPick::Point) {
            return;
        }
        if let Some(h) = &self.link_ui.second {
            if let Some(d) = &h.dims {
                self.paint_edge_dims(ui, rect, &h.plane, h.at, d, &self.link_ui.second_edit, h.snap);
            }
        }
    }

    fn update_second_hover(&mut self, resp: &egui::Response) {
        use rolidium_doc::rolidium_sketch::edgedims::nearest_edges;
        if self.link_ui.pick != Some(LinkPick::Point) {
            if self.link_ui.second.take().is_some() {
                self.link_ui.second_edit.clear();
                self.layers_changed = true;
            }
            return;
        }
        let (Some((b, f)), Some(pointer)) = (self.hovered_face(), resp.hover_pos()) else {
            self.link_ui.second = None;
            return;
        };
        let rect = resp.rect;
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
        let Some(raw3) = self.ray_face_point(b, f, o, d) else {
            self.link_ui.second = None;
            return;
        };
        let Some(solid) = self.doc.body(b).map(|x| &x.solid) else {
            return;
        };
        let Ok(info) = solid.face_info(FaceId(f)) else {
            return;
        };
        if info.kind != FaceKind::Plane {
            // a curved face: the point only, no distances
            self.link_ui.second = None;
            return;
        }
        let plane = PlaneDef::from_face(info.origin, info.normal);
        let edges: Vec<(DVec2, DVec2)> = solid
            .face_edges(FaceId(f))
            .map(|es| {
                es.into_iter()
                    .filter_map(|e| solid.edge_info(e).ok())
                    .filter(|i| i.kind == rolidium_kernel::EdgeKind::Line)
                    .map(|i| (plane.to_2d(i.start), plane.to_2d(i.end)))
                    .collect()
            })
            .unwrap_or_default();
        let raw = plane.to_2d(raw3);
        let mpp = self.mm_per_px(&plane, raw, rect);
        let dims = nearest_edges(raw, &edges);
        let (mut at, mut snap) = (raw, None);
        if let Some(dm) = &dims {
            let (p, k) = dm.snap(raw, 8.0 * mpp);
            if k.is_some() {
                (at, snap) = (p, k);
            }
            let (ta, tb) = self.link_ui.second_edit.values();
            if ta.is_some() || tb.is_some() {
                at = dm.place(ta, tb);
                snap = None;
            }
        }
        let dims = if at != raw {
            nearest_edges(at, &edges)
        } else {
            dims
        };
        let now = SecondHover {
            plane,
            at,
            dims,
            snap,
            body: b,
            face: f,
        };
        self.link_ui.second = Some(now);
        self.layers_changed = true;
    }

    /// The point of the second piece of a row: the point of the face under the pointer; the direction and the spacing are read from it.
    fn pick_second_point(&mut self, resp: &egui::Response) -> bool {
        let (Some((b, f)), Some(pointer)) = (self.hovered_face(), resp.interact_pointer_pos()) else {
            return false;
        };
        let rect = resp.rect;
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
        let Some(mut to) = self.ray_face_point(b, f, o, d) else {
            return false;
        };
        // the snaps and the typed distances moved the place
        if let Some(h) = self
            .link_ui
            .second
            .as_ref()
            .filter(|h| h.body == b && h.face == f)
        {
            to = h.plane.to_3d(h.at);
        }
        self.link_ui.second_edit.clear();
        self.set_second_piece(to)
    }

    /// Enter with distances typed while the second piece is chosen: the piece goes where the live distances say.
    pub(super) fn confirm_second_typed(&mut self) -> bool {
        let Some(h) = self.link_ui.second.clone() else {
            return false;
        };
        if !self.link_ui.second_edit.any_typed() {
            return false;
        }
        self.link_ui.second_edit.clear();
        if self.set_second_piece(h.plane.to_3d(h.at)) {
            self.link_ui.pick = None;
            self.link_ui.second = None;
            self.layers_changed = true;
        }
        true
    }

    /// Reads the direction and the spacing of the row from the place of the second piece.
    fn set_second_piece(&mut self, to: DVec3) -> bool {
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
                LinkPick::Target | LinkPick::Point | LinkPick::Centre => return false,
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
                            LinkPick::Target | LinkPick::Point | LinkPick::Centre => {}
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
        if self.link_ui.sk_open.is_some() && self.sk.is_some() && !self.link_ui.sk_preview.is_empty() {
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
        // the preview starts with the second click (or with the panel of an existing link)
        let (ok, bad) = if self.link_ui.armed || self.link_ui.editing.is_some() {
            self.compute_link_preview()
        } else {
            (Vec::new(), Vec::new())
        };
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
        if let (Some(id), Some(near)) = (self.link_ui.editing, self.link_ui.near) {
            let u = &self.link_ui;
            return Some(Command::EditLinkFill {
                link: id,
                fill: if u.grid {
                    rolidium_doc::RectFill::Grid {
                        columns: u.columns.max(1),
                        rows: u.rows.max(1),
                        margin: u.margin,
                    }
                } else {
                    rolidium_doc::RectFill::Corners
                },
                near: Some(near),
            });
        }
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
                let target = match (u.rect_sketch, u.rect_face) {
                    (Some((s, e)), _) => rolidium_doc::RectTarget::Sketch(s, e),
                    (None, Some(f)) => rolidium_doc::RectTarget::Face(FaceId(f)),
                    (None, None) => return None,
                };
                let fill = if u.grid {
                    rolidium_doc::RectFill::Grid {
                        columns: u.columns.max(1),
                        rows: u.rows.max(1),
                        margin: u.margin,
                    }
                } else {
                    rolidium_doc::RectFill::Corners
                };
                Some((target, fill))
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
        let why = if self.link_ui.p.mode == LinkMode::Fill && !feature && self.link_ui.editing.is_none() {
            Some("Into a face needs a hole or a pin: click its round face first")
        } else if feature
            && self.link_ui.p.mode == LinkMode::Fill
            && self.link_ui.rect_face.is_none()
            && self.link_ui.rect_sketch.is_none()
        {
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
        // the holes (pins) of a pattern of a hole stay selected afterwards, all of them, so that P pulls them at once
        if let Command::AddFeatureLink { body, face, .. } = &cmd {
            let d = self
                .entry(*body)
                .and_then(|m| m.faces.get(face.0 as usize))
                .and_then(|f| f.cyl_diameter);
            self.select_holes = d.map(|d| (*body, d));
        }
        self.submit(cmd);
        self.set_status("Computing…", false);
        let editing = self.link_ui.editing.is_some();
        if !editing {
            self.move_sel.clear();
            self.link_ui.feature = None;
            self.link_ui.rect_face = None;
            self.link_ui.rect_sketch = None;
            self.link_ui.pick = None;
            self.link_ui.armed = false;
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
        let mut start = false;
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
            .filter_map(|b| {
                self.doc
                    .body(*b)
                    .map(|x| x.name.clone())
                    .or_else(|| self.doc.mesh(*b).map(|m| m.name.clone()))
            })
            .collect();
        let feature_label = self.link_ui.feature.and_then(|(b, _)| {
            self.doc
                .body(b)
                .map(|x| trf("a hole or a pin of {}", std::slice::from_ref(&x.name)))
        });
        let has_feature = feature_label.is_some();
        let rect_label = self
            .link_ui
            .rect_face
            .map(|f| format!("#{f}"))
            .or_else(|| self.link_ui.rect_sketch.map(|_| tr("a rectangle of a sketch")));
        let bodies = self.doc.bodies().to_vec();
        let sketches = self.doc.sketches().to_vec();
        let error = editing
            .and_then(|id| self.doc.state().links.iter().find(|l| l.id == id))
            .and_then(|l| l.error.clone());
        let armed = self.link_ui.armed;
        let hint_text = self.link_hint();
        let suggested = self.suggested_axis();
        let mut ui_state = std::mem::take(&mut self.link_ui);
        let probe_w: std::cell::RefCell<Vec<(&'static str, Rect)>> = std::cell::RefCell::new(Vec::new());
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
            if !armed && editing.is_none() {
                // before the second click: only what to click, no numbers yet
                if !has_feature && sources.is_empty() {
                    // nothing is chosen yet: the first thing to say
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(tr("Click what you want to repeat"))
                            .size(16.0)
                            .color(crate::theme::rgb(255, 200, 100)),
                    );
                    ui.add_space(4.0);
                }
                ui.colored_label(crate::theme::rgb(255, 200, 100), hint_text.as_str());
                if (has_feature || !sources.is_empty()) && p.mode != LinkMode::Mirror {
                    // the object is chosen: the three ways to repeat it, as big buttons; a click on a guide in the view without a choice still works (the kind is told by what it is)
                    ui.add_space(4.0);
                    for (want, label, name) in [
                        (Want::Curve, tr("Along a line or a curve"), "want_curve"),
                        (Want::Point, tr("Round a point"), "want_point"),
                        (Want::Face, tr("On a face"), "want_face"),
                    ] {
                        let on = ui_state.want == Some(want);
                        let r = ui.add_sized(
                            [ui.available_width(), 34.0],
                            egui::Button::new(egui::RichText::new(label).size(15.0)).selected(on),
                        );
                        probe_w.borrow_mut().push((name, r.rect));
                        if r.clicked() {
                            ui_state.want = Some(want);
                            ui_state.pick = None;
                            if want == Want::Point {
                                p.axis.dir = suggested;
                                p.axis_label = tr("the face the object lies on");
                            }
                        }
                    }
                    if let Some(want) = ui_state.want {
                        ui.colored_label(
                            crate::theme::rgb(120, 220, 255),
                            match want {
                                Want::Curve => tr("Click a line or a curve"),
                                Want::Point => tr("Click a point or an axis"),
                                Want::Face => tr("Click a face"),
                            },
                        );
                        if want == Want::Point {
                            ui.horizontal(|ui| {
                                let on = ui_state.pick == Some(LinkPick::Centre);
                                let r = ui.selectable_label(on, tr("Pick a point"));
                                probe_w.borrow_mut().push(("pick_centre", r.rect));
                                if r.clicked() {
                                    ui_state.pick = if on { None } else { Some(LinkPick::Centre) };
                                }
                            });
                        }
                    }
                }
                if has_feature || !sources.is_empty() {
                    // no curve to click: start with a base direction or a base plane
                    ui.horizontal_wrapped(|ui| {
                        if p.mode == LinkMode::Mirror {
                            ui.label(tr("Or choose the plane"));
                            for (v, n) in [(DVec3::X, "YZ"), (DVec3::Y, "XZ"), (DVec3::Z, "XY")] {
                                let r = ui.button(n);
                                probe_w.borrow_mut().push((
                                    match n {
                                        "YZ" => "start_yz",
                                        "XZ" => "start_xz",
                                        _ => "start_xy",
                                    },
                                    r.rect,
                                ));
                                if r.clicked() {
                                    p.plane = Axis { origin: DVec3::ZERO, dir: v };
                                    p.plane_label = n.into();
                                    start = true;
                                }
                            }
                        } else {
                            ui.label(tr("Or choose a direction"));
                            for (v, n) in [(DVec3::X, "X"), (DVec3::Y, "Y"), (DVec3::Z, "Z")] {
                                let r = ui.button(n);
                                probe_w.borrow_mut().push((
                                    match n {
                                        "X" => "start_x",
                                        "Y" => "start_y",
                                        _ => "start_z",
                                    },
                                    r.rect,
                                ));
                                if r.clicked() {
                                    (p.mode, p.second, p.dir, p.dir_label) = (LinkMode::Linear, false, v, n.into());
                                    p.count = 3;
                                    start = true;
                                }
                            }
                        }
                    });
                }
                if ui.button(tr("Cancel")).clicked() {
                    cancel = true;
                }
                return;
            }
            if editing.is_none() && p.mode != LinkMode::Mirror {
                // the kind (a click on the object chose it; here it can be changed)
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr("Kind"));
                    let line = p.mode == LinkMode::Linear && !p.second;
                    let round = p.mode == LinkMode::Circular;
                    let grid = p.mode == LinkMode::Fill || (p.mode == LinkMode::Linear && p.second);
                    let r = ui.selectable_label(line, tr("Along a line"));
                    probe_w.borrow_mut().push(("kind_line", r.rect));
                    if r.clicked() && !line {
                        p.mode = LinkMode::Linear;
                        p.second = false;
                    }
                    if !p.path.is_empty() {
                        let on = p.mode == LinkMode::Path;
                        let r = ui.selectable_label(on, tr("Along a curve"));
                        probe_w.borrow_mut().push(("kind_path", r.rect));
                        if r.clicked() && !on {
                            p.mode = LinkMode::Path;
                        }
                    }
                    let r = ui.selectable_label(round, tr("Round a centre"));
                    probe_w.borrow_mut().push(("kind_round", r.rect));
                    if r.clicked() && !round {
                        p.mode = LinkMode::Circular;
                    }
                    let r = ui.selectable_label(grid, if has_feature { tr("Into the corners or a grid") } else { tr("A grid") });
                    probe_w.borrow_mut().push(("kind_grid", r.rect));
                    if r.clicked() && !grid {
                        if has_feature {
                            p.mode = LinkMode::Fill;
                        } else {
                            p.mode = LinkMode::Linear;
                            p.second = true;
                        }
                    }
                });
            }
            let pick_btn = |ui: &mut egui::Ui, st: &mut Option<LinkPick>, what: LinkPick, label: &str| {
                let on = *st == Some(what);
                let r = ui.selectable_label(on, label).on_hover_text(tr("Then click a straight edge or a face in the view"));
                probe_w.borrow_mut().push((
                    match what {
                        LinkPick::Axis => "pick_axis",
                        LinkPick::Plane => "pick_plane",
                        LinkPick::Point => "pick_point",
                        LinkPick::Direction => "pick_direction",
                        _ => "pick_other",
                    },
                    r.rect,
                ));
                if r.clicked() {
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
                    let _ = (&st, what);
                    if ui.button("⇄").on_hover_text(tr("Reverse the direction")).clicked() {
                        *dir = -*dir;
                    }
                });
                ui.weak(format!("{}: ({:.2}, {:.2}, {:.2})", label, dir.x, dir.y, dir.z));
                // the direction as numbers (any direction, not only X, Y and Z): an extra choice
                egui::CollapsingHeader::new(tr("The direction as numbers"))
                    .id_salt(("dirnum", what as u8))
                    .show(ui, |ui| {
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
                    });
            };
            match p.mode {
                LinkMode::Linear => {
                    ui.label(tr("Direction"));
                    dir_row(ui, &mut ui_state.pick, LinkPick::Direction, &mut p.dir, &mut p.dir_label);
                    let more = egui::CollapsingHeader::new(tr("More"))
                        .id_salt("link-more")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                pick_btn(ui, &mut ui_state.pick, LinkPick::Point, &tr("Second piece here"));
                                ui.weak(tr("click the place of the second piece: the spacing is taken from it"));
                            });
                        });
                    probe_w.borrow_mut().push(("link_more", more.header_response.rect));
                    ui.horizontal(|ui| {
                        ui.label(tr("Pieces"));
                        ui.add(egui::DragValue::new(&mut p.count).range(1..=1000).speed(0.2));
                        ui.label(if p.by_total { tr("Length") } else { tr("Spacing") });
                        ui.add(egui::DragValue::new(&mut p.spacing).speed(0.5).suffix(" mm"));
                    });
                    ui.checkbox(&mut p.by_total, tr("The spacing is the whole length"));
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
                LinkMode::Path => {
                    ui.weak(trf("The curve: {}", std::slice::from_ref(&p.path_label)));
                    ui.horizontal(|ui| {
                        ui.label(tr("Pieces"));
                        ui.add_enabled(p.pitch <= 0.0, egui::DragValue::new(&mut p.count).range(1..=1000).speed(0.2));
                        ui.label(tr("or the distance between pieces"));
                        ui.add(egui::DragValue::new(&mut p.pitch).range(0.0..=100000.0).speed(0.5).suffix(" mm"));
                    });
                    if !p.path_closed {
                        ui.horizontal(|ui| {
                            ui.label(tr("Free at the start"));
                            ui.add(egui::DragValue::new(&mut p.start).range(0.0..=100000.0).speed(0.5).suffix(" mm"));
                            ui.label(tr("at the end"));
                            ui.add(egui::DragValue::new(&mut p.end).range(0.0..=100000.0).speed(0.5).suffix(" mm"));
                        });
                    } else {
                        ui.weak(tr("A closed curve: the pieces are spread evenly over the whole length"));
                    }
                    ui.checkbox(&mut p.orient, tr("The pieces turn with the curve"));
                }
                LinkMode::Circular => {
                    ui.label(tr("Axis"));
                    ui.horizontal_wrapped(|ui| {
                        let r = ui.selectable_label(ui_state.pick == Some(LinkPick::Centre), tr("Pick a point"));
                        probe_w.borrow_mut().push(("pick_centre", r.rect));
                        if r.clicked() {
                            ui_state.pick = if ui_state.pick == Some(LinkPick::Centre) { None } else { Some(LinkPick::Centre) };
                        }
                        let r = ui.selectable_label(false, tr("Axis by the face the object lies on"));
                        probe_w.borrow_mut().push(("axis_suggested", r.rect));
                        if r.clicked() {
                            p.axis.dir = suggested;
                            p.axis_label = tr("the face the object lies on");
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (v, n) in [(DVec3::X, "X"), (DVec3::Y, "Y"), (DVec3::Z, "Z")] {
                            if ui.selectable_label(p.axis_label == n, n).clicked() {
                                p.axis = Axis { origin: DVec3::ZERO, dir: v };
                                p.axis_label = n.into();
                            }
                        }
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
                    if let Some((na, nb)) = ui_state.near.as_mut() {
                        // a link that follows a rectangle: the distances of the pieces to the edges
                        ui.horizontal(|ui| {
                            ui.label(tr("Distance from the edges"));
                            ui.add(egui::DragValue::new(na).range(0.0..=10000.0).speed(0.1).suffix(" mm"));
                            ui.add(egui::DragValue::new(nb).range(0.0..=10000.0).speed(0.1).suffix(" mm"));
                        });
                        ui.weak(tr("The pieces follow the rectangle: pull an edge of the plate or change the rectangle of the sketch and the distances stay."));
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(tr("Face"));
                            ui.label(rect_label.clone().unwrap_or_else(|| tr("click the flat rectangular face, or a rectangle of a sketch")));
                        });
                        ui.weak(tr("The copies keep the distances from the two nearest edges that the hole has, also when the plate or the rectangle of the sketch is changed later."));
                    }
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
                let r = ui.button(tr("Apply"));
                probe_w.borrow_mut().push(("link_apply", r.rect));
                if r.clicked() {
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
        if start {
            self.link_ui.armed = true;
            self.default_spacing();
            self.layers_changed = true;
        }
        for (n, r) in probe_w.into_inner() {
            self.probe_widget(n, r);
        }
        // the usual number of pieces of each kind: 3 in a row, 6 round an axis
        if self.link_ui.p.mode != before.mode {
            let p = &mut self.link_ui.p;
            if p.mode == LinkMode::Circular && p.count == 3 {
                p.count = 6;
            } else if p.mode == LinkMode::Linear && p.count == 6 {
                p.count = 3;
            }
        }
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
                            self.probe_widget(&format!("link_row_{}", l.id), r.rect);
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
                let (name, visible) = match (
                    st.bodies.iter().find(|b| b.id == *bid),
                    st.meshes.iter().find(|m| m.id == *bid),
                ) {
                    (Some(b), _) => (b.name.clone(), b.visible),
                    (None, Some(m)) => (format!("▦ {}", m.name), m.visible),
                    _ => continue,
                };
                let on = self.marked_bodies().contains(bid);
                let r = ui.selectable_label(
                    on,
                    if visible {
                        name.clone()
                    } else {
                        format!("{}  ({})", name, tr("hidden"))
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
                    self.rename = Some((ItemId::Body(*bid), name.trim_start_matches("▦ ").to_string()));
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
        if let Some((b, f)) = self.link_ui.feature {
            // a hole or a pin: the point of its axis
            return self
                .doc
                .body(b)
                .and_then(|x| x.solid.cylinder_info(FaceId(f)).ok().flatten())
                .map(|c| c.base);
        }
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
        self.probe_widget("link_handle", Rect::from_center_size(b, Vec2::splat(12.0)));
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
        // a click on the dot (no drag) turns the arrow to the next direction: X, Y, Z, -X, -Y, -Z
        if resp.clicked_by(PointerButton::Primary) && over {
            let order = [
                (DVec3::X, "X"),
                (DVec3::Y, "Y"),
                (DVec3::Z, "Z"),
                (-DVec3::X, "-X"),
                (-DVec3::Y, "-Y"),
                (-DVec3::Z, "-Z"),
            ];
            let cur = order.iter().position(|(v, _)| v.dot(dir) > 0.9999);
            let (v, n) = order[cur.map_or(0, |i| (i + 1) % order.len())];
            let p = &mut self.link_ui.p;
            (p.dir, p.dir_label) = (v, n.to_string());
            self.layers_changed = true;
            return true;
        }
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
    /// Closes the sketch pattern panel and takes its copies off the screen.
    pub(super) fn close_sketch_pattern(&mut self) {
        let u = &mut self.link_ui;
        if u.sk_open.is_some() || !u.sk_preview.is_empty() {
            u.sk_open = None;
            u.sk_want = None;
            u.sk_guided = false;
            u.sk_preview.clear();
            u.sk_key.clear();
            self.layers_changed = true;
        }
    }

    /// The shapes of the open sketch that are selected (a click, Ctrl, Shift, a rectangle).
    pub(super) fn sketch_chosen(&self) -> Vec<rolidium_doc::rolidium_sketch::EntityId> {
        let Some(sk) = self.sk.as_ref().and_then(|s| s.sketch) else {
            return Vec::new();
        };
        let mut v: Vec<_> = self
            .entity_multi
            .iter()
            .filter(|(s, _)| *s == sk)
            .map(|(_, e)| *e)
            .collect();
        if let Some((s, e)) = self.entity_sel {
            if s == sk && !v.contains(&e) {
                v.push(e);
            }
        }
        v
    }

    /// The panel of patterns and mirrors of the shapes of the sketch: the same steps as in 3D (what to repeat, three ways, the guide),
    /// and the numbers. Repeating all the shapes of the sketch is a choice in the panel.
    #[allow(deprecated)]
    pub(super) fn sketch_pattern_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        let Some(mode) = self.link_ui.sk_open else { return };
        let Some(sk) = self.sk.as_ref().and_then(|s| s.sketch) else {
            self.close_sketch_pattern();
            return;
        };
        let chosen = self.sketch_chosen();
        let mirror = mode == LinkMode::Mirror;
        // the entities the command works on: empty means all of them
        let all = self.link_ui.sk_all || (mirror && chosen.is_empty());
        let entities: Vec<_> = if all { Vec::new() } else { chosen.clone() };
        // the lines and the auxiliary axes of the sketch (a mirror line, a direction), and the centres of its circles and arcs
        let (lines, centres): SketchRefs = self
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
        if !mirror && !all && chosen.is_empty() && (u.sk_guided || !u.sk_preview.is_empty()) {
            // what was chosen is gone (Undo, a deletion): back to the first step, and nothing is drawn for it
            u.sk_guided = false;
            u.sk_want = None;
            u.sk_preview.clear();
            u.sk_key.clear();
            self.layers_changed = true;
        }
        let mut docks = std::mem::take(&mut self.docks);
        let mut apply = false;
        let mut close = false;
        let title = if mirror { tr("Mirror") } else { tr("Pattern") };
        let have = all || !chosen.is_empty();
        let n_chosen = chosen.len();
        let probe_w: std::cell::RefCell<Vec<(&'static str, Rect)>> = std::cell::RefCell::new(Vec::new());
        docks.panel(
            ctx,
            "skpattern",
            Pos2::new(rect.max.x - 290.0, rect.min.y + 10.0),
            None,
            270.0,
            &title,
            false,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(crate::theme::rgb(242, 89, 242), tr("Source"));
                    ui.label(if all {
                        tr("All the shapes of the sketch")
                    } else if n_chosen == 0 {
                        tr("click a shape of the sketch")
                    } else {
                        trf("{} shapes of the sketch", &[n_chosen.to_string()])
                    });
                });
                if !mirror && !u.sk_guided {
                    // the first steps: what to repeat, then the three ways, then the guide
                    if !have {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(tr("Click what you want to repeat"))
                                .size(16.0)
                                .color(crate::theme::rgb(255, 200, 100)),
                        );
                        ui.colored_label(
                            crate::theme::rgb(255, 200, 100),
                            tr("One or more shapes: Ctrl or Shift adds, a rectangle takes everything in it"),
                        );
                        if ui.button(tr("All the shapes of the sketch")).clicked() {
                            u.sk_all = true;
                        }
                    } else {
                        ui.add_space(4.0);
                        for (want, label, name) in [
                            (Want::Curve, tr("Along a line or a curve"), "want_curve"),
                            (Want::Point, tr("Round a point"), "want_point"),
                            (Want::Face, tr("A grid"), "want_face"),
                        ] {
                            let on = u.sk_want == Some(want);
                            let r = ui.add_sized(
                                [ui.available_width(), 34.0],
                                egui::Button::new(egui::RichText::new(label).size(15.0)).selected(on),
                            );
                            probe_w.borrow_mut().push((name, r.rect));
                            if r.clicked() {
                                u.sk_want = Some(want);
                            }
                        }
                        if let Some(want) = u.sk_want {
                            ui.colored_label(
                                crate::theme::rgb(120, 220, 255),
                                match want {
                                    Want::Curve | Want::Face => {
                                        tr("Click a line, an arc, a circle or an edge: the direction of the row")
                                    }
                                    Want::Point => tr("Click a point of the sketch, a centre of a circle, or an axis"),
                                },
                            );
                        } else {
                            ui.colored_label(crate::theme::rgb(255, 200, 100), tr("Choose how to repeat it"));
                        }
                        if u.sk_all {
                            ui.weak(tr("The shapes you click replace \"all the shapes\""));
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                    return;
                }
                if !mirror {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut u.sk_open, Some(LinkMode::Linear), tr("Along a line"));
                        ui.selectable_value(&mut u.sk_open, Some(LinkMode::Circular), tr("Round a point"));
                    });
                    if !u.sk_guide_label.is_empty() {
                        ui.weak(trf("Guide: {}", std::slice::from_ref(&u.sk_guide_label)));
                    }
                    let mut all_now = u.sk_all;
                    if ui.checkbox(&mut all_now, tr("All the shapes of the sketch")).changed() {
                        u.sk_all = all_now;
                    }
                }
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
                        ui.checkbox(&mut u.sk_second, tr("A second row (a grid) at a right angle"));
                        if u.sk_second {
                            ui.horizontal(|ui| {
                                ui.label(tr("Pieces"));
                                ui.add(egui::DragValue::new(&mut u.sk_count2).range(1..=1000).speed(0.2));
                                ui.label(tr("Spacing"));
                                ui.add(egui::DragValue::new(&mut u.sk_spacing2).speed(0.5).suffix(" mm"));
                            });
                        }
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
                    if !mirror && ui.button(tr("Change the guide")).clicked() {
                        u.sk_guided = false;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        close = true;
                    }
                });
            },
        );
        self.docks = docks;
        for (n, r) in probe_w.into_inner() {
            self.probe_widget(n, r);
        }
        let mode = u.sk_open.unwrap_or(mode);
        let all = u.sk_all || (mirror && chosen.is_empty());
        let entities: Vec<_> = if all { Vec::new() } else { entities };
        // the preview in the model area follows the numbers (only once the guide is clicked, or for a mirror)
        let show = u.sk_guided || mirror;
        let key = format!(
            "{show}|{mode:?}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{entities:?}|{}|{}|{}|{}",
            u.sk_count,
            u.sk_spacing,
            u.sk_by_total,
            u.sk_angle,
            u.sk_cx,
            u.sk_cy,
            u.sk_step_angle,
            u.sk_full,
            u.sk_keep,
            u.sk_second,
            u.sk_count2,
            u.sk_spacing2,
            self.doc.revision()
        );
        if key != u.sk_key {
            u.sk_key = key;
            u.sk_preview = if show {
                self.sketch_pattern_preview(&u, sk, mode, &entities)
            } else {
                Vec::new()
            };
            self.layers_changed = true;
        }
        let cmd = if apply {
            let dir = glam::DVec2::new(u.sk_angle.to_radians().cos(), u.sk_angle.to_radians().sin());
            Some(match mode {
                LinkMode::Linear | LinkMode::Fill | LinkMode::Path => Command::SketchPattern {
                    sketch: sk,
                    entities,
                    pattern: rolidium_doc::SketchPattern::Linear {
                        dir,
                        count: u.sk_count,
                        spacing: u.sk_spacing,
                        by_total: u.sk_by_total,
                        second: u.sk_second.then(|| {
                            (
                                glam::DVec2::new(-dir.y, dir.x),
                                u.sk_count2,
                                u.sk_spacing2,
                                false,
                            )
                        }),
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
            u.sk_want = None;
            u.sk_guided = false;
            u.sk_all = false;
            u.sk_preview.clear();
            u.sk_key.clear();
            self.layers_changed = true;
        }
        if cmd.is_some() {
            u.sk_preview.clear();
            u.sk_key.clear();
            u.sk_open = None;
            u.sk_want = None;
            u.sk_guided = false;
            u.sk_all = false;
        }
        self.link_ui = u;
        if let Some(c) = cmd {
            self.submit(c);
        }
    }

    /// A click in the view of a sketch while the pattern wizard waits for the guide: a line gives the direction of the row, a circle or an arc
    /// its centre, a point of the sketch the centre. Returns `true` when the click was meant for the wizard.
    pub(super) fn sketch_guide_click(&mut self, hover: Option<Obj>) -> bool {
        let (Some(mode), Some(want)) = (self.link_ui.sk_open, self.link_ui.sk_want) else {
            return false;
        };
        if mode == LinkMode::Mirror || self.link_ui.sk_guided {
            return false;
        }
        let Some(sm) = &self.sk else { return false };
        let Some(sk) = sm.sketch else { return false };
        let plane = sm.plane;
        let snap = sm.snap.map(|s| s.pos);
        enum Guide {
            Direction(glam::DVec2, String),
            Centre(glam::DVec2, String),
        }
        let mut guide: Option<Guide> = None;
        match hover {
            Some(Obj::Entity(s, id)) => {
                if let Some(e) = self.doc.sketch(s).and_then(|d| d.sketch.entity(id).copied()) {
                    if let (Some((_, d)), true) = (e.axis_line(), want != Want::Point) {
                        guide = Some(Guide::Direction(d, tr("a line of the sketch")));
                    } else if let Entity::Circle { center, .. } | Entity::Arc { center, .. } = e {
                        guide = Some(Guide::Centre(center, tr("the centre of a circle of the sketch")));
                    } else if let Some((a, _)) = e.axis_line() {
                        // a line or an axis for "round a point": the point under the pointer on it, else its start
                        guide = Some(Guide::Centre(snap.unwrap_or(a), tr("a point of the sketch")));
                    }
                }
            }
            Some(Obj::Edge(b, e)) => {
                if let Some(pl) = self.edge_polyline(b, e) {
                    if pl.len() >= 2 && want != Want::Point {
                        let d3 = (pl[pl.len() - 1] - pl[0]).as_dvec3();
                        let d = plane.to_2d(plane.origin + d3) - plane.to_2d(plane.origin);
                        if d.length() > 1e-9 {
                            guide = Some(Guide::Direction(d.normalize(), tr("an edge of a body")));
                        }
                    } else if let Some(p) = snap {
                        guide = Some(Guide::Centre(p, tr("a point of the sketch")));
                    }
                }
            }
            _ => {}
        }
        if guide.is_none() && want == Want::Point {
            if let Some(p) = snap {
                guide = Some(Guide::Centre(p, tr("a point of the sketch")));
            }
        }
        let _ = sk;
        let Some(g) = guide else {
            self.set_status(
                if want == Want::Point {
                    "Click a point of the sketch, the centre of a circle, or an axis"
                } else {
                    "Click a line, an arc, a circle or an edge"
                },
                true,
            );
            return true;
        };
        let u = &mut self.link_ui;
        match g {
            Guide::Direction(d, what) => {
                u.sk_open = Some(LinkMode::Linear);
                u.sk_angle = d.y.atan2(d.x).to_degrees();
                u.sk_second = want == Want::Face;
                u.sk_guide_label = what;
            }
            Guide::Centre(c, what) => {
                u.sk_open = Some(LinkMode::Circular);
                (u.sk_cx, u.sk_cy) = (c.x, c.y);
                u.sk_second = false;
                u.sk_guide_label = what;
            }
        }
        u.sk_guided = true;
        u.sk_key.clear();
        self.layers_changed = true;
        true
    }
}

impl App {
    /// The copies that the numbers of the panel would make, as pairs of points in 3D for the preview.
    fn sketch_pattern_preview(
        &self,
        u: &LinkUi,
        sk: SketchId,
        mode: LinkMode,
        ids: &[rolidium_doc::rolidium_sketch::EntityId],
    ) -> Vec<Vec3> {
        let Some(sd) = self.doc.sketch(sk) else {
            return Vec::new();
        };
        let dir = glam::DVec2::new(u.sk_angle.to_radians().cos(), u.sk_angle.to_radians().sin());
        let made = match mode {
            LinkMode::Linear | LinkMode::Fill | LinkMode::Path => rolidium_doc::preview_pattern(
                sd,
                ids,
                &rolidium_doc::SketchPattern::Linear {
                    dir,
                    count: u.sk_count,
                    spacing: u.sk_spacing,
                    by_total: u.sk_by_total,
                    second: u
                        .sk_second
                        .then(|| (glam::DVec2::new(-dir.y, dir.x), u.sk_count2, u.sk_spacing2, false)),
                },
            ),
            LinkMode::Circular => rolidium_doc::preview_pattern(
                sd,
                ids,
                &rolidium_doc::SketchPattern::Circular {
                    center: glam::DVec2::new(u.sk_cx, u.sk_cy),
                    count: u.sk_count,
                    angle_deg: u.sk_step_angle,
                    full: u.sk_full,
                },
            ),
            LinkMode::Mirror => {
                let a = glam::DVec2::new(u.sk_cx, u.sk_cy);
                rolidium_doc::preview_mirror(sd, ids, a, a + dir, true)
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
