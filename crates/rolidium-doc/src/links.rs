// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Links: objects of the document that remember a source, parameters and the bodies they made, and make the result again when the parameters or the
//! source change (a pattern along a line or around a circle, a mirror). They are not a history of the model: a link has no steps and no order of
//! features, only its own parameters, and the bodies it made are ordinary bodies (D44, D47, D48).
//!
//! The bodies of a link are *derived*: an edit of one by hand (a pull, a round, a move) is refused by the document with [`DocError::Derived`]; the program
//! asks the person whether to remove the link (the bodies become ordinary) or to leave the body alone. Names, colours, materials and visibility can be
//! changed freely.

use super::*;

pub type LinkId = u64;
pub type GroupId = u64;

/// The most pieces one link makes (a guard against a typing mistake such as 100000 pieces).
pub const MAX_PIECES: usize = 1000;

/// A line in space: a point and a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub origin: DVec3,
    pub dir: DVec3,
}

/// A line or an auxiliary axis of a sketch as a line in space (a point and the unit direction); `None` for any other shape.
pub fn sketch_line_in_space(plane: &rolidium_sketch::PlaneDef, e: &Entity) -> Option<Axis> {
    let (a, d) = e.axis_line()?;
    let o = plane.to_3d(a);
    let dir = (plane.to_3d(a + d) - o).normalize_or_zero();
    (dir != DVec3::ZERO).then_some(Axis { origin: o, dir })
}

/// The mirror plane that a line or an auxiliary axis of a sketch gives: the plane through the line that stands square to the plane of the sketch
/// (`dir` is the normal of the plane).
pub fn sketch_mirror_plane(plane: &rolidium_sketch::PlaneDef, e: &Entity) -> Option<Axis> {
    let l = sketch_line_in_space(plane, e)?;
    let normal = l.dir.cross(plane.normal.normalize()).normalize_or_zero();
    (normal != DVec3::ZERO).then_some(Axis {
        origin: l.origin,
        dir: normal,
    })
}

/// Where the pieces of a pattern go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternMode {
    /// New bodies (copies of the sources).
    New,
    /// The sources and their copies are added to this body.
    Add(BodyId),
    /// The sources and their copies are taken out of this body (a row of holes in a plate from one cylinder).
    Cut(BodyId),
}

impl PatternMode {
    pub fn target(self) -> Option<BodyId> {
        match self {
            PatternMode::New => None,
            PatternMode::Add(b) | PatternMode::Cut(b) => Some(b),
        }
    }
}

/// A second direction of a linear pattern (a grid).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Direction2 {
    pub axis: Axis,
    pub count: u32,
    pub spacing: f64,
    /// `spacing` is the whole length (from the first piece to the last), not the distance between two pieces.
    pub by_total: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearParams {
    pub axis: Axis,
    /// The number of pieces in the direction, the original included.
    pub count: u32,
    pub spacing: f64,
    /// `spacing` is the whole length (first piece to last), not the distance between two pieces.
    pub by_total: bool,
    pub second: Option<Direction2>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircularParams {
    pub axis: Axis,
    /// The number of pieces, the original included.
    pub count: u32,
    /// The angle between two pieces, degrees (ignored when `full`).
    pub angle_deg: f64,
    /// The pieces are spread evenly round the whole circle (the angle is 360 / count).
    pub full: bool,
    /// The pieces keep the orientation of the original (they are only carried round the axis) instead of turning with it.
    pub keep_orientation: bool,
}

/// Copies along any curve: the curve is kept as a polyline (the lines and arcs of a sketch, a spline, a helix, the edges of a body), the pieces stand at equal
/// distances along it, from `start` mm from its beginning to `end` mm before its end (a closed curve is shared out evenly over the whole length), and turn with
/// the tangent when `orient` is set. The original (piece 0) stands where the curve begins (after `start`).
#[derive(Debug, Clone, PartialEq)]
pub struct PathParams {
    pub points: Vec<DVec3>,
    pub closed: bool,
    /// The number of pieces, the original included (ignored when `pitch` is set).
    pub count: u32,
    /// The distance between two pieces along the curve; 0: the pieces are shared out by `count`.
    pub pitch: f64,
    pub start: f64,
    pub end: f64,
    pub orient: bool,
    /// The curve the pieces follow (a curve of the document, a chain of a sketch, edges of a body): `points` are calculated from it again whenever it changes.
    /// `None`: the polyline is all there is (a curve that is not kept).
    pub source: Option<SweepPath>,
    /// A piece at the start and at the end of an open curve (by the count); without them the pieces are spread between the two ends.
    pub ends: bool,
    /// The point of the source that stands on the places of the curve (the centre of the source): the pieces then lie on the curve itself and the source stays
    /// where it is. `None`: the first piece is the source and the others are moved by the distances along the curve from the first place.
    pub anchor: Option<DVec3>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirrorParams {
    pub origin: DVec3,
    pub normal: DVec3,
    /// Where the mirror images go: new bodies, or added to / cut from a body (a hole or a pin made again on the other side of a plane).
    pub into: PatternMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    Linear(LinearParams, PatternMode),
    Circular(CircularParams, PatternMode),
    /// Along a curve of any shape (a line, an arc, a spline, a helix, a chain of them).
    Path(PathParams, PatternMode),
    Mirror(MirrorParams),
    /// A loft through the profiles (regions of sketches, faces or edge loops of bodies); it follows the sketches and bodies it was made from.
    Loft {
        profiles: Vec<LoftRef>,
        ruled: bool,
        mode: PatternMode,
    },
    /// The body made hollow (see `bodyfeat`): it works on the body `target`, whose solid before the shell is the `base` of the link.
    Shell(ShellParams),
    /// Text raised from a flat face of a body or engraved into it.
    Text(TextParams),
    /// A profile swept along a path (a curve, a chain of a sketch or edges of a body); it follows the profile and the path.
    Sweep {
        profile: LoftRef,
        path: SweepPath,
        rounded: bool,
        mode: PatternMode,
    },
}

impl LinkKind {
    pub fn mode(&self) -> PatternMode {
        match self {
            LinkKind::Linear(_, m) | LinkKind::Circular(_, m) | LinkKind::Path(_, m) => *m,
            LinkKind::Loft { mode, .. } | LinkKind::Sweep { mode, .. } => *mode,
            LinkKind::Shell(p) => PatternMode::Add(p.target),
            LinkKind::Text(p) => PatternMode::Add(p.target),
            LinkKind::Mirror(m) => m.into,
        }
    }

    /// The same link with another place for the pieces (the modes of a loft and a sweep are changed too).
    pub fn with_mode(&self, mode: PatternMode) -> LinkKind {
        let mut k = self.clone();
        match &mut k {
            LinkKind::Linear(_, m) | LinkKind::Circular(_, m) | LinkKind::Path(_, m) => *m = mode,
            LinkKind::Mirror(p) => p.into = mode,
            LinkKind::Loft { mode: m, .. } | LinkKind::Sweep { mode: m, .. } => *m = mode,
            LinkKind::Shell(_) | LinkKind::Text(_) => {}
        }
        k
    }

    /// Whether the link makes its solid from sketches, curves and faces (a loft or a sweep) and not from copies of bodies.
    pub fn generates(&self) -> bool {
        matches!(
            self,
            LinkKind::Loft { .. } | LinkKind::Sweep { .. } | LinkKind::Shell(_) | LinkKind::Text(_)
        )
    }

    /// A link that changes the one body it names (a shell, a text) instead of making bodies of its own; links of this kind on one body follow each other in
    /// the order they were made.
    pub fn works_in_place(&self) -> bool {
        matches!(self, LinkKind::Shell(_) | LinkKind::Text(_))
    }

    /// The default name of a new link.
    pub fn title(&self) -> &'static str {
        match self {
            LinkKind::Linear(p, _) if p.second.is_some() => "Grid",
            LinkKind::Linear(..) => "Linear pattern",
            LinkKind::Circular(..) => "Circular pattern",
            LinkKind::Path(..) => "Pattern along a curve",
            LinkKind::Mirror(_) => "Mirror",
            LinkKind::Loft { .. } => "Loft",
            LinkKind::Sweep { .. } => "Sweep",
            LinkKind::Shell(_) => "Shell",
            LinkKind::Text(_) => "Text",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: LinkId,
    pub name: String,
    pub kind: LinkKind,
    /// The bodies the pieces are made of.
    pub sources: Vec<BodyId>,
    /// The bodies the link made: the copies (several per source, in order), or the one target body of a pattern that adds or cuts.
    pub results: Vec<BodyId>,
    /// For a pattern that adds to or cuts from a body: the solid of the body before the pattern, which every recalculation starts from.
    pub base: Option<Solid>,
    /// The solids of the sources when the link was last calculated (kept alive, so that their identity can be compared).
    pub seen: Vec<Solid>,
    /// Why the last calculation failed (the bodies are then what the last good calculation made).
    pub error: Option<String>,
    /// Remarks of the last calculation (pieces that overlap or lie outside).
    pub warnings: Vec<String>,
    /// For a loft or a sweep: the state of the sketches and curves it depends on when it was last calculated (see `generate::stamp`).
    pub stamp: u64,
    /// For a pattern into the corners (or a grid) of a rectangle: what it follows, so that it is made again when the rectangle changes.
    pub fill: Option<FillRule>,
    /// For a shell or a text: the solid this link made last (kept alive, so that it can be told whether another link changed the body since).
    pub produced: Option<Solid>,
}

/// A group of bodies in the right column. Groups nest; a body belongs to one group at most.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub parent: Option<GroupId>,
    pub visible: bool,
    pub bodies: Vec<BodyId>,
}

/// One piece of a pattern: how the source moves.
#[derive(Debug, Clone, Copy)]
struct Placement {
    translation: DVec3,
    pivot: DVec3,
    axis: DVec3,
    angle: f64,
}

fn step_of(count: u32, spacing: f64, by_total: bool) -> f64 {
    if by_total && count > 1 {
        spacing / (count - 1) as f64
    } else {
        spacing
    }
}

fn bad(msg: impl Into<String>) -> DocError {
    DocError::Kernel(KernelError::Invalid(msg.into()))
}

/// Where the pieces of a pattern along a curve stand: the place and the direction of the curve there, the original first.
pub fn path_places(p: &PathParams) -> Result<Vec<(DVec3, DVec3)>, DocError> {
    if p.points.len() < 2 || p.points.iter().any(|q| !q.is_finite()) {
        return Err(bad("The curve of the pattern has no length"));
    }
    let mut pts = p.points.clone();
    if p.closed && pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
    }
    let total = *cum.last().unwrap_or(&0.0);
    if total.is_nan() || total <= 1e-9 {
        return Err(bad("The curve of the pattern has no length"));
    }
    if !(p.start.is_finite() && p.end.is_finite() && p.pitch.is_finite())
        || p.start < 0.0
        || p.end < 0.0
        || p.pitch < 0.0
    {
        return Err(bad("The distances along the curve must be numbers, not negative"));
    }
    let usable = if p.closed { total } else { total - p.start - p.end };
    if usable.is_nan() || usable <= 1e-9 {
        return Err(bad(
            "The curve is shorter than the distances left free at its ends",
        ));
    }
    let (n, step) = if p.pitch > 1e-9 {
        let n = if p.closed {
            (total / p.pitch).floor() as usize
        } else {
            ((usable + 1e-9) / p.pitch).floor() as usize + 1
        };
        (n.max(1), p.pitch)
    } else {
        let n = p.count.max(1) as usize;
        let step = if p.closed {
            total / n as f64
        } else if !p.ends {
            usable / (n + 1) as f64
        } else if n > 1 {
            usable / (n - 1) as f64
        } else {
            0.0
        };
        (n, step)
    };
    // without a piece at the ends of an open curve the first place is one step in
    let skip = if !p.closed && !p.ends && p.pitch <= 1e-9 {
        1.0
    } else {
        0.0
    };
    if n > MAX_PIECES {
        return Err(bad(format!("A pattern can have at most {MAX_PIECES} pieces")));
    }
    let at = |s: f64| -> (DVec3, DVec3) {
        let s = if p.closed {
            s.rem_euclid(total)
        } else {
            s.clamp(0.0, total)
        };
        let i = cum
            .partition_point(|c| *c <= s)
            .saturating_sub(1)
            .min(pts.len() - 2);
        // skip a segment of no length
        let (mut a, mut b) = (pts[i], pts[i + 1]);
        let mut j = i;
        while a.distance(b) < 1e-12 && j + 2 < pts.len() {
            j += 1;
            (a, b) = (pts[j], pts[j + 1]);
        }
        let seg = cum[i + 1] - cum[i];
        let t = if seg > 1e-12 { (s - cum[i]) / seg } else { 0.0 };
        (a + (b - a) * t.clamp(0.0, 1.0), (b - a).normalize_or_zero())
    };
    let first = p.start + step * skip;
    Ok((0..n).map(|i| at(first + step * i as f64)).collect())
}

/// The placements of the pieces other than the original (the original is piece 0 and is not moved).
fn placements(kind: &LinkKind) -> Result<Vec<Placement>, DocError> {
    let mut out = Vec::new();
    match kind {
        LinkKind::Linear(p, _) => {
            if p.count < 1 || p.count > MAX_PIECES as u32 {
                return Err(bad(format!(
                    "The number of pieces must be between 1 and {MAX_PIECES}"
                )));
            }
            let d = p.axis.dir;
            if !d.is_finite() || d.length() < 1e-9 {
                return Err(bad("The direction of the pattern has no length"));
            }
            let d = d.normalize();
            let step = step_of(p.count, p.spacing, p.by_total);
            let (c2, d2, s2) = match &p.second {
                Some(s) => {
                    if s.count < 1 || s.count > MAX_PIECES as u32 {
                        return Err(bad(format!(
                            "The number of pieces must be between 1 and {MAX_PIECES}"
                        )));
                    }
                    if !s.axis.dir.is_finite() || s.axis.dir.length() < 1e-9 {
                        return Err(bad("The second direction of the grid has no length"));
                    }
                    (
                        s.count,
                        s.axis.dir.normalize(),
                        step_of(s.count, s.spacing, s.by_total),
                    )
                }
                None => (1, DVec3::Z, 0.0),
            };
            if (p.count as usize) * (c2 as usize) > MAX_PIECES {
                return Err(bad(format!("A pattern can have at most {MAX_PIECES} pieces")));
            }
            if !step.is_finite() || !s2.is_finite() {
                return Err(bad("The spacing must be a number"));
            }
            for j in 0..c2 {
                for i in 0..p.count {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    out.push(Placement {
                        translation: d * step * i as f64 + d2 * s2 * j as f64,
                        pivot: DVec3::ZERO,
                        axis: DVec3::Z,
                        angle: 0.0,
                    });
                }
            }
        }
        LinkKind::Circular(p, _) => {
            if p.count < 1 || p.count > MAX_PIECES as u32 {
                return Err(bad(format!(
                    "The number of pieces must be between 1 and {MAX_PIECES}"
                )));
            }
            if !p.axis.dir.is_finite() || p.axis.dir.length() < 1e-9 {
                return Err(bad("The axis of the pattern has no direction"));
            }
            let step = if p.full {
                std::f64::consts::TAU / p.count as f64
            } else {
                p.angle_deg.to_radians()
            };
            if !step.is_finite() {
                return Err(bad("The angle must be a number"));
            }
            for i in 1..p.count {
                out.push(Placement {
                    translation: DVec3::ZERO,
                    pivot: p.axis.origin,
                    axis: p.axis.dir.normalize(),
                    angle: step * i as f64,
                });
            }
        }
        LinkKind::Path(p, _) => {
            let places = path_places(p)?;
            let (mut p0, t0) = places[0];
            // the pieces stand on the curve itself: the centre of the source goes to every place (the source stays where it is)
            let from = if p.anchor.is_some() { 0 } else { 1 };
            if let Some(a) = p.anchor {
                p0 = a;
            }
            for (pos, tan) in places.into_iter().skip(from) {
                let (axis, angle) = if p.orient {
                    let c = t0.cross(tan);
                    if c.length() < 1e-9 {
                        // the same or the opposite direction: a half turn about any square axis
                        if t0.dot(tan) < 0.0 {
                            (t0.any_orthonormal_vector(), std::f64::consts::PI)
                        } else {
                            (DVec3::Z, 0.0)
                        }
                    } else {
                        (c.normalize(), t0.dot(tan).clamp(-1.0, 1.0).acos())
                    }
                } else {
                    (DVec3::Z, 0.0)
                };
                if p.anchor.is_some() && (pos - p0).length() < 1e-9 && angle.abs() < 1e-9 {
                    continue; // the source stands there already
                }
                out.push(Placement {
                    translation: pos - p0,
                    pivot: p0,
                    axis,
                    angle,
                });
            }
        }
        LinkKind::Mirror(m) => {
            if !m.normal.is_finite() || m.normal.length() < 1e-9 || !m.origin.is_finite() {
                return Err(bad("The mirror plane has no normal"));
            }
        }
        LinkKind::Loft { .. } | LinkKind::Sweep { .. } | LinkKind::Shell(_) | LinkKind::Text(_) => {}
    }
    Ok(out)
}

/// The pieces of one source: for a pattern the copies at the placements (the original is not among them), for a mirror the mirror image.
fn pieces_of(kind: &LinkKind, plan: &[Placement], source: &Solid) -> Result<Vec<(Solid, History)>, DocError> {
    match kind {
        LinkKind::Mirror(m) => {
            let r = source.mirrored(m.origin, m.normal)?;
            Ok(vec![(r.solid, r.history)])
        }
        LinkKind::Circular(c, _) if c.keep_orientation => {
            // carried round the axis without turning: the translation that takes the centre of the source to where the turn takes it
            let (lo, hi) = source.bounding_box()?;
            let centre = (lo + hi) * 0.5;
            plan.iter()
                .map(|p| {
                    let turned = glam::DQuat::from_axis_angle(p.axis, p.angle) * (centre - p.pivot) + p.pivot;
                    let r = source.moved(turned - centre, DVec3::ZERO, DVec3::Z, 0.0)?;
                    Ok((r.solid, r.history))
                })
                .collect()
        }
        _ => plan
            .iter()
            .map(|p| {
                let r = source.moved(p.translation, p.pivot, p.axis, p.angle)?;
                Ok((r.solid, r.history))
            })
            .collect(),
    }
}

/// What a calculation gives: the new solids of the copies (with the history of their faces and the index of the source), or the new solid of the target.
struct Outcome {
    copies: Vec<(usize, Solid, History)>,
    target: Option<Solid>,
    warnings: Vec<String>,
}

fn boxes_meet(a: &Solid, b: &Solid) -> bool {
    match (a.bounding_box(), b.bounding_box()) {
        (Ok((a0, a1)), Ok((b0, b1))) => {
            a0.cmple(b1 + DVec3::splat(1e-6)).all() && b0.cmple(a1 + DVec3::splat(1e-6)).all()
        }
        _ => true,
    }
}

fn common_volume(a: &Solid, b: &Solid) -> f64 {
    if !boxes_meet(a, b) {
        return 0.0;
    }
    a.boolean(b, BooleanOp::Common)
        .ok()
        .and_then(|r| r.solid.volume().ok())
        .unwrap_or(0.0)
}

/// A loft or a sweep: the tool from the profiles and the path as they are now, then the one new body, or added to / cut from the body
/// the link works on. The faces of that body that a profile names are the faces of the body as it was before the link (`base`).
fn calculate_generated(st: &State, link: &Link) -> Result<Outcome, DocError> {
    let mut at = st.clone();
    if let (Some(t), Some(base)) = (link.kind.mode().target(), &link.base) {
        if let Some(b) = at.bodies.iter_mut().find(|b| b.id == t) {
            b.solid = base.clone();
        }
    }
    if link.kind.works_in_place() {
        return calculate_in_place(st, link);
    }
    let made = match &link.kind {
        LinkKind::Loft { profiles, ruled, .. } => generate::loft(&at, profiles, *ruled)?,
        LinkKind::Sweep {
            profile,
            path,
            rounded,
            ..
        } => generate::sweep(&at, profile, path, *rounded)?,
        _ => return Err(bad("This link does not make its solid from profiles")),
    };
    let warnings: Vec<String> = made.notice.into_iter().collect();
    match link.kind.mode() {
        PatternMode::New => Ok(Outcome {
            copies: vec![(0, made.tool, History::default())],
            target: None,
            warnings,
        }),
        PatternMode::Add(_) | PatternMode::Cut(_) => {
            let cut = matches!(link.kind.mode(), PatternMode::Cut(_));
            let base = link
                .base
                .clone()
                .ok_or_else(|| bad("The link lost the body it works on"))?;
            let r = base.boolean_checked(&made.tool, if cut { BooleanOp::Cut } else { BooleanOp::Fuse })?;
            Ok(Outcome {
                copies: Vec::new(),
                target: Some(r.solid),
                warnings,
            })
        }
    }
}

/// A shell or a text: the body before it (`base`) with the shell or the letters.
fn calculate_in_place(st: &State, link: &Link) -> Result<Outcome, DocError> {
    let base = link
        .base
        .clone()
        .ok_or_else(|| bad("The link lost the body it works on"))?;
    let mut warnings = Vec::new();
    let solid = match &link.kind {
        LinkKind::Text(p) => bodyfeat::apply_text(&base, p)?,
        LinkKind::Shell(p) => {
            // the text that was put on the body before this link: when the shell cannot follow it, it is taken off and put back
            let before: Vec<&Link> = st
                .links
                .iter()
                .filter(|l| {
                    l.id < link.id && l.results.contains(&p.target) && matches!(l.kind, LinkKind::Text(_))
                })
                .collect();
            let texts: Vec<TextParams> = before
                .iter()
                .filter_map(|l| match &l.kind {
                    LinkKind::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect();
            let plain = before.first().and_then(|l| l.base.as_ref());
            let (s, notes) = bodyfeat::shell_with_details(&base, p, plain, &texts)?;
            warnings.extend(notes);
            s
        }
        _ => return Err(bad("This link does not work in place")),
    };
    Ok(Outcome {
        copies: Vec::new(),
        target: Some(solid),
        warnings,
    })
}

/// Calculates a link from the sources in `st` (and `base` for a pattern that adds or cuts).
fn calculate(st: &State, link: &Link) -> Result<Outcome, DocError> {
    if link.kind.generates() {
        return calculate_generated(st, link);
    }
    let plan = placements(&link.kind)?;
    let sources: Vec<&Body> = link
        .sources
        .iter()
        .map(|s| st.bodies.iter().find(|b| b.id == *s).ok_or(DocError::NoSuchBody))
        .collect::<Result<_, _>>()?;
    if sources.is_empty() {
        return Err(bad("The link has no source body"));
    }
    let mut warnings = Vec::new();
    let mut copies = Vec::new();
    for (si, b) in sources.iter().enumerate() {
        for (solid, history) in pieces_of(&link.kind, &plan, &b.solid)? {
            copies.push((si, solid, history));
        }
    }
    let mut target = None;
    match link.kind.mode() {
        PatternMode::New => {
            // pieces that overlap each other (the original included): only the pairs whose boxes meet are tested
            let mut all: Vec<&Solid> = sources.iter().map(|b| &b.solid).collect();
            all.extend(copies.iter().map(|c| &c.1));
            let mut overlaps = 0;
            let mut tested = 0;
            'outer: for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    if boxes_meet(all[i], all[j]) {
                        tested += 1;
                        if tested > 300 {
                            break 'outer;
                        }
                        if common_volume(all[i], all[j]) > 1e-6 {
                            overlaps += 1;
                        }
                    }
                }
            }
            if overlaps > 0 {
                warnings.push(format!("{overlaps} pair(s) of pieces overlap"));
            }
        }
        PatternMode::Add(t) | PatternMode::Cut(t) => {
            let cut = matches!(link.kind.mode(), PatternMode::Cut(_));
            let mut cur = link
                .base
                .clone()
                .ok_or_else(|| bad("The link lost the body it works on"))?;
            let tb = st.bodies.iter().find(|b| b.id == t).ok_or(DocError::NoSuchBody)?;
            let _ = tb;
            let mut outside = 0;
            let pieces: Vec<&Solid> = sources
                .iter()
                .map(|b| &b.solid)
                .chain(copies.iter().map(|c| &c.1))
                .collect();
            // pieces that cut (or join) without touching each other go in one boolean operation with one tool, not one after the other
            let relevant: Vec<&Solid> = pieces.iter().copied().filter(|p| boxes_meet(&cur, p)).collect();
            let apart = relevant
                .iter()
                .enumerate()
                .all(|(i, a)| relevant[i + 1..].iter().all(|b| !boxes_meet(a, b)));
            if cut && relevant.len() > 1 && apart {
                let tools = Solid::compound(&relevant)?;
                outside += pieces.len() - relevant.len();
                match cur.boolean_checked(&tools, BooleanOp::Cut) {
                    Ok(r) => {
                        cur = r.solid;
                        if outside > 0 {
                            warnings.push(format!("{outside} piece(s) do not cut into the body"));
                        }
                        target = Some(cur);
                        return Ok(Outcome {
                            copies,
                            target,
                            warnings,
                        });
                    }
                    // one operation did not work: the pieces are tried one after the other below
                    Err(_) => outside = 0,
                }
            }
            for p in pieces {
                let meets = if cut {
                    common_volume(&cur, p) > 1e-9
                } else {
                    boxes_meet(&cur, p) && cur.distance_to(p).map(|d| d < 1e-6).unwrap_or(true)
                };
                if !meets {
                    outside += 1;
                    continue;
                }
                let r = cur.boolean_checked(p, if cut { BooleanOp::Cut } else { BooleanOp::Fuse })?;
                cur = r.solid;
            }
            if outside > 0 {
                warnings.push(format!(
                    "{outside} piece(s) {} the body",
                    if cut { "do not cut into" } else { "do not touch" }
                ));
            }
            target = Some(cur);
        }
    }
    Ok(Outcome {
        copies,
        target,
        warnings,
    })
}

/// Puts the outcome into the state: the copies take the ids of the bodies the link made before (in order), new bodies get new ids, surplus bodies go.
fn apply(st: &mut State, link: &mut Link, out: Outcome, next: &mut u64) {
    link.warnings = out.warnings;
    link.error = None;
    if let (Some(solid), Some(t)) = (out.target, link.kind.mode().target()) {
        if link.kind.works_in_place() {
            link.produced = Some(solid.clone());
        }
        if let Some(b) = st.bodies.iter_mut().find(|b| b.id == t) {
            b.solid = solid;
        }
        link.results = vec![t];
        return;
    }
    if link.kind.generates() {
        // the one body of a loft or a sweep: the same body again, or a new one
        let old = std::mem::take(&mut link.results);
        let Some((_, solid, _)) = out.copies.into_iter().next() else {
            link.results = old;
            return;
        };
        let id = match old
            .first()
            .copied()
            .filter(|id| st.bodies.iter().any(|b| b.id == *id))
        {
            Some(id) => {
                if let Some(b) = st.bodies.iter_mut().find(|b| b.id == id) {
                    b.solid = solid;
                }
                id
            }
            None => {
                let id = *next;
                *next += 1;
                st.body_counter += 1;
                st.bodies.push(Body {
                    id,
                    name: format!("Body {}", st.body_counter),
                    solid,
                    color: [0.72, 0.76, 0.82],
                    visible: true,
                    features: Vec::new(),
                    material: None,
                });
                id
            }
        };
        link.results = vec![id];
        return;
    }
    let old = std::mem::take(&mut link.results);
    let mut ids = Vec::new();
    let mut per_source = vec![0usize; link.sources.len()];
    for (k, (si, solid, history)) in out.copies.into_iter().enumerate() {
        per_source[si] += 1;
        let src = st.bodies.iter().find(|b| b.id == link.sources[si]).cloned();
        let Some(src) = src else { continue };
        let mut features = src.features.clone();
        remap_features(&mut features, &history);
        if let Some(id) = old
            .get(k)
            .copied()
            .filter(|id| st.bodies.iter().any(|b| b.id == *id))
        {
            let b = st.bodies.iter_mut().find(|b| b.id == id).unwrap();
            b.solid = solid;
            b.features = features;
            ids.push(id);
        } else {
            let id = *next;
            *next += 1;
            st.bodies.push(Body {
                id,
                name: format!("{} {}", src.name, per_source[si] + 1),
                solid,
                color: src.color,
                visible: src.visible,
                features,
                material: src.material.clone(),
            });
            ids.push(id);
        }
    }
    // bodies the link made before and does not need now
    for id in old.iter().skip(ids.len()) {
        st.bodies.retain(|b| b.id != *id);
        remove_from_groups(st, *id);
    }
    link.results = ids;
}

pub(crate) fn remove_from_groups(st: &mut State, body: BodyId) {
    for g in st.groups.iter_mut() {
        g.bodies.retain(|b| *b != body);
    }
}

/// Makes a link: calculates it, adds the bodies and remembers everything. The link has the id `*next`.
pub(crate) fn create(
    st: &mut State,
    kind: LinkKind,
    sources: Vec<BodyId>,
    name: &str,
    next: &mut u64,
) -> Result<LinkId, DocError> {
    create_with(st, kind, sources, name, next, None)
}

/// A number that changes when something a link depends on besides its source bodies changes (the sketches and curves of a loft or a sweep,
/// the rectangle a pattern into the corners follows).
pub(crate) fn stamp_of(st: &State, link: &Link) -> u64 {
    generate::stamp(st, &link.kind) ^ rule_stamp(st, link).rotate_left(17)
}

/// Makes a link that follows a rectangle (see [`FillRule`]).
pub(crate) fn create_with(
    st: &mut State,
    kind: LinkKind,
    sources: Vec<BodyId>,
    name: &str,
    next: &mut u64,
    fill: Option<FillRule>,
) -> Result<LinkId, DocError> {
    // a loft or a sweep depends on the bodies, sketches and curves its profiles and path name, not on the bodies the caller lists
    let sources = if kind.generates() {
        let d = generate::deps(&kind);
        let target = kind.mode().target();
        for s in &d.sketches {
            if !st.sketches.iter().any(|x| x.id == *s) {
                return Err(DocError::NoSuchSketch);
            }
        }
        for c in &d.curves {
            if !st.curves.iter().any(|x| x.id == *c) {
                return Err(DocError::NoSuchCurve);
            }
        }
        d.bodies.into_iter().filter(|b| Some(*b) != target).collect()
    } else {
        sources
    };
    for s in &sources {
        if !st.bodies.iter().any(|b| b.id == *s) {
            return Err(DocError::NoSuchBody);
        }
        if st
            .links
            .iter()
            .any(|l| l.results.contains(s) && !matches!(l.kind.mode(), PatternMode::New))
        {
            // a body that is the target of another pattern is not a source of a new one (it would change with it)
        }
    }
    if let Some(t) = kind.mode().target() {
        if sources.contains(&t) && !kind.generates() {
            return Err(bad("The body the pattern works on cannot be its own source"));
        }
        if !st.bodies.iter().any(|b| b.id == t) {
            return Err(DocError::NoSuchBody);
        }
    }
    let id = *next;
    *next += 1;
    st.link_counter += 1;
    let name = if name.trim().is_empty() {
        format!("{} {}", kind.title(), st.link_counter)
    } else {
        name.trim().to_string()
    };
    let base = kind
        .mode()
        .target()
        .and_then(|t| st.bodies.iter().find(|b| b.id == t))
        .map(|b| b.solid.clone());
    let mut link = Link {
        id,
        name,
        kind,
        sources,
        results: Vec::new(),
        base,
        seen: Vec::new(),
        error: None,
        warnings: Vec::new(),
        stamp: 0,
        fill,
        produced: None,
    };
    let out = calculate(st, &link)?;
    apply(st, &mut link, out, next);
    link.stamp = stamp_of(st, &link);
    // the sketches a loft or a sweep was made from are its sources: they are hidden, the link keeps them (Delete link shows them again)
    for sk in generate::deps(&link.kind).sketches {
        if let Some(s) = st.sketches.iter_mut().find(|s| s.id == sk) {
            s.visible = false;
        }
    }
    // the sources of a pattern that adds or cuts are tools: they are hidden, the result is the target
    if link.kind.mode().target().is_some() && !link.kind.generates() {
        for s in &link.sources {
            if let Some(b) = st.bodies.iter_mut().find(|b| b.id == *s) {
                b.visible = false;
            }
        }
    }
    link.seen = link
        .sources
        .iter()
        .filter_map(|s| st.bodies.iter().find(|b| b.id == *s))
        .map(|b| b.solid.clone())
        .collect();
    st.links.push(link);
    Ok(id)
}

/// Changes the parameters of a link (or only its name) and calculates it again. On failure the state is not changed.
pub(crate) fn edit(
    st: &mut State,
    id: LinkId,
    kind: Option<LinkKind>,
    name: Option<&str>,
    next: &mut u64,
) -> Result<(), DocError> {
    let pos = st
        .links
        .iter()
        .position(|l| l.id == id)
        .ok_or(DocError::NoSuchLink)?;
    let mut link = st.links[pos].clone();
    if let Some(n) = name {
        let n = n.trim();
        if n.is_empty() {
            return Err(bad("The name must not be empty"));
        }
        link.name = n.to_string();
    }
    if let Some(k) = kind {
        // the way of working (new bodies, add, cut) and the target stay: a different one is a different link
        if std::mem::discriminant(&k) != std::mem::discriminant(&link.kind) || k.mode() != link.kind.mode() {
            return Err(bad("The kind of the link and the body it works on cannot be changed; remove the link and make a new one"));
        }
        link.kind = k;
        // numbers typed by hand: the link no longer follows a rectangle
        link.fill = None;
        let out = calculate(st, &link)?;
        apply(st, &mut link, out, next);
        link.stamp = stamp_of(st, &link);
        link.seen = link
            .sources
            .iter()
            .filter_map(|s| st.bodies.iter().find(|b| b.id == *s))
            .map(|b| b.solid.clone())
            .collect();
    }
    st.links[pos] = link;
    Ok(())
}

/// Changes what a pattern into the corners (or a grid) keeps and calculates it again. On failure the state is not changed.
pub(crate) fn edit_fill(
    st: &mut State,
    id: LinkId,
    fill: RectFill,
    near: Option<(f64, f64)>,
    next: &mut u64,
) -> Result<(), DocError> {
    let pos = st
        .links
        .iter()
        .position(|l| l.id == id)
        .ok_or(DocError::NoSuchLink)?;
    let mut link = st.links[pos].clone();
    let Some(rule) = link.fill.as_mut() else {
        return Err(bad("This link does not follow a rectangle"));
    };
    rule.fill = fill;
    if let Some((a, b)) = near {
        if !(a.is_finite() && b.is_finite()) || a < 0.0 || b < 0.0 {
            return Err(bad(
                "The distances from the edges must be numbers that are not negative",
            ));
        }
        rule.sides.near_a = a;
        rule.sides.near_b = b;
    }
    let mut trial = st.clone();
    follow(&mut trial, &mut link)?;
    let out = calculate(&trial, &link)?;
    apply(&mut trial, &mut link, out, next);
    link.stamp = stamp_of(&trial, &link);
    link.seen = link
        .sources
        .iter()
        .filter_map(|s| trial.bodies.iter().find(|b| b.id == *s))
        .map(|b| b.solid.clone())
        .collect();
    trial.links[pos] = link;
    *st = trial;
    Ok(())
}

/// Removes the link; its bodies stay as ordinary bodies.
pub(crate) fn unlink(st: &mut State, id: LinkId) -> Result<(), DocError> {
    let n = st.links.len();
    st.links.retain(|l| l.id != id);
    if st.links.len() == n {
        return Err(DocError::NoSuchLink);
    }
    Ok(())
}

/// Removes the link and what it made: the copies go, a target that was added to or cut from gets its old solid back.
pub(crate) fn delete(st: &mut State, id: LinkId) -> Result<(), DocError> {
    let link = st
        .links
        .iter()
        .find(|l| l.id == id)
        .cloned()
        .ok_or(DocError::NoSuchLink)?;
    match (link.kind.mode().target(), &link.base) {
        (Some(t), Some(base)) => {
            if let Some(b) = st.bodies.iter_mut().find(|b| b.id == t) {
                b.solid = base.clone();
            }
            for s in &link.sources {
                if let Some(b) = st.bodies.iter_mut().find(|b| b.id == *s) {
                    b.visible = true;
                }
            }
        }
        _ => {
            for r in &link.results {
                st.bodies.retain(|b| b.id != *r);
                remove_from_groups(st, *r);
            }
        }
    }
    for sk in generate::deps(&link.kind).sketches {
        if let Some(s) = st.sketches.iter_mut().find(|s| s.id == sk) {
            s.visible = true;
        }
    }
    st.links.retain(|l| l.id != id);
    Ok(())
}

/// Every link whose sources changed since it was calculated is calculated again, in the order the links were made. A calculation that fails
/// leaves the bodies as they were and puts the reason into the link; the remarks are returned for the status bar.
pub(crate) fn refresh(st: &mut State, next: &mut u64) -> Vec<String> {
    let mut notes = Vec::new();
    for i in 0..st.links.len() {
        let link = &st.links[i];
        let now: Vec<Option<&Solid>> = link
            .sources
            .iter()
            .map(|s| st.bodies.iter().find(|b| b.id == *s).map(|b| &b.solid))
            .collect();
        let missing = now.iter().any(|o| o.is_none());
        // a shell or a text works on a body that another link of the same kind has changed since (it was made before this one, and has been calculated again):
        // this link starts from the body as it is now
        let chain_base: Option<Solid> = if link.kind.works_in_place() {
            let t = link.kind.mode().target();
            let cur = t
                .and_then(|t| st.bodies.iter().find(|b| b.id == t))
                .map(|b| b.solid.clone());
            let earlier = st.links[..i]
                .iter()
                .any(|l| l.kind.works_in_place() && l.kind.mode().target() == t);
            match (&link.produced, cur) {
                (Some(p), Some(c)) if earlier && p.shape_id() != c.shape_id() => Some(c),
                _ => None,
            }
        } else {
            None
        };
        let same = !missing
            && chain_base.is_none()
            && stamp_of(st, link) == link.stamp
            && now.len() == link.seen.len()
            && now
                .iter()
                .zip(&link.seen)
                .all(|(a, b)| a.is_some_and(|a| a.shape_id() == b.shape_id()));
        if same {
            continue;
        }
        let mut link = st.links[i].clone();
        if missing {
            link.error = Some("A source body of the link was deleted".into());
            link.seen = Vec::new();
            notes.push(format!(
                "{}: a source body was deleted, the link keeps its last result",
                link.name
            ));
            st.links[i] = link;
            continue;
        }
        if let Some(b) = chain_base {
            link.base = Some(b);
        }
        let followed = follow(st, &mut link).and_then(|_| calculate(st, &link));
        match followed {
            Ok(out) => {
                apply(st, &mut link, out, next);
                if !link.warnings.is_empty() {
                    notes.push(format!("{}: {}", link.name, link.warnings.join("; ")));
                }
            }
            Err(e) => {
                link.error = Some(e.to_string());
                notes.push(format!(
                    "{} could not be recalculated: {e}. It keeps its last result",
                    link.name
                ));
            }
        }
        link.seen = link
            .sources
            .iter()
            .filter_map(|s| st.bodies.iter().find(|b| b.id == *s))
            .map(|b| b.solid.clone())
            .collect();
        link.stamp = stamp_of(st, &link);
        st.links[i] = link;
    }
    // bodies that are gone are not in a group
    let alive: std::collections::HashSet<BodyId> = st
        .bodies
        .iter()
        .map(|b| b.id)
        .chain(st.meshes.iter().map(|m| m.id))
        .collect();
    for g in st.groups.iter_mut() {
        g.bodies.retain(|b| alive.contains(b));
    }
    notes
}

/// The first body of `after` that is a result of a link and was changed or removed by the command (compared with `before`): a hand edit of a derived body.
pub(crate) fn derived_edit(before: &State, after: &State) -> Option<(LinkId, String, BodyId, String)> {
    for l in &after.links {
        for r in &l.results {
            let was = before.bodies.iter().find(|b| b.id == *r);
            let is = after.bodies.iter().find(|b| b.id == *r);
            let changed = match (was, is) {
                (Some(a), Some(b)) => a.solid.shape_id() != b.solid.shape_id(),
                (Some(_), None) => true,
                _ => false,
            };
            if changed {
                let name = was.map(|b| b.name.clone()).unwrap_or_default();
                return Some((l.id, l.name.clone(), *r, name));
            }
        }
    }
    None
}

/// A shell or a text link calculated on a copy of the state and thrown away: the solid the body would get, and the remarks. `editing`: the link whose
/// numbers are being changed (`kind` replaces them), otherwise `kind` is a new link. For the live preview of the tools.
pub fn trial(st: &State, kind: LinkKind, editing: Option<LinkId>) -> Result<(Solid, Vec<String>), DocError> {
    let target = kind.mode().target().ok_or(DocError::NoSuchBody)?;
    let mut s = st.clone();
    let mut next = s
        .bodies
        .iter()
        .map(|b| b.id)
        .chain(s.sketches.iter().map(|x| x.id))
        .chain(s.curves.iter().map(|x| x.id))
        .chain(s.links.iter().map(|l| l.id))
        .chain(s.groups.iter().map(|g| g.id))
        .max()
        .unwrap_or(0)
        + 1000;
    let id = match editing {
        Some(id) => {
            edit(&mut s, id, Some(kind), None, &mut next)?;
            id
        }
        None => create(&mut s, kind, Vec::new(), "", &mut next)?,
    };
    refresh(&mut s, &mut next);
    let warnings = s
        .links
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.warnings.clone())
        .unwrap_or_default();
    let solid = s
        .bodies
        .iter()
        .find(|b| b.id == target)
        .ok_or(DocError::NoSuchBody)?
        .solid
        .clone();
    Ok((solid, warnings))
}

/// The link that made this body, if any.
pub fn link_of_result(st: &State, body: BodyId) -> Option<&Link> {
    st.links.iter().find(|l| l.results.contains(&body))
}

/// How the pieces of a pattern are placed, for a preview: `(translation, pivot, axis, angle)` for each piece other than the original (rotate about the axis
/// through the pivot by the angle, then translate). Empty for a mirror. The error says what is wrong with the parameters.
pub fn placements_of(kind: &LinkKind) -> Result<Vec<(DVec3, DVec3, DVec3, f64)>, String> {
    placements(kind)
        .map(|v| {
            v.into_iter()
                .map(|p| (p.translation, p.pivot, p.axis, p.angle))
                .collect()
        })
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- where the pieces go: from a point, into the corners of a face

/// A linear pattern whose second piece lies at `to` (the first, the original, at `from`): the direction and the spacing are read from the two points, and
/// `count` pieces are made (so that a click on the place of the second piece gives the pattern).
pub fn linear_through(from: DVec3, to: DVec3, count: u32) -> Result<LinearParams, DocError> {
    let d = to - from;
    if !d.is_finite() || d.length() < 1e-9 {
        return Err(bad(
            "The second piece would lie on the first one: click another point",
        ));
    }
    Ok(LinearParams {
        axis: Axis {
            origin: from,
            dir: d.normalize(),
        },
        count: count.max(2),
        spacing: d.length(),
        by_total: false,
        second: None,
    })
}

/// How many pieces a pattern into a rectangle makes along each side: the four corners, or a grid with a margin from the edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RectFill {
    /// Four copies, each with the distances from the two nearest edges that the original has (symmetric).
    Corners,
    /// `columns` x `rows` pieces from the original to a margin of `margin` mm from the far edges.
    Grid { columns: u32, rows: u32, margin: f64 },
}

/// A rectangle in space: the corner `o`, the unit directions `a` and `b` (`b` = normal x `a`) and the extent along them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RectGeom {
    pub o: DVec3,
    pub a: DVec3,
    pub b: DVec3,
    pub n: DVec3,
    pub amin: f64,
    pub amax: f64,
    pub bmin: f64,
    pub bmax: f64,
}

/// A direction with the sign that makes its biggest component positive (the same direction whichever way an edge runs).
fn canonical(d: DVec3) -> DVec3 {
    let d = d.normalize();
    let k = if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() {
        d.x
    } else if d.y.abs() >= d.z.abs() {
        d.y
    } else {
        d.z
    };
    if k < 0.0 {
        -d
    } else {
        d
    }
}

/// The index of the biggest component of a direction.
fn dominant(d: DVec3) -> usize {
    if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() {
        0
    } else if d.y.abs() >= d.z.abs() {
        1
    } else {
        2
    }
}

/// The two directions of a rectangle in a fixed order, so that the same rectangle gives the same `a` and `b` after it was changed.
fn rect_axes(n: DVec3, side: DVec3) -> (DVec3, DVec3) {
    let a0 = canonical(side);
    let b0 = canonical(n.cross(a0));
    let a = if dominant(b0) < dominant(a0) { b0 } else { a0 };
    (a, n.cross(a).normalize())
}

/// The rectangle of a flat face with four straight sides (a hole in it does not matter).
pub(crate) fn rect_of_face(solid: &Solid, face: FaceId) -> Result<RectGeom, DocError> {
    let info = solid.face_info(face)?;
    if info.kind != rolidium_kernel::FaceKind::Plane {
        return Err(bad("Click a flat rectangular face (or a rectangle of a sketch)"));
    }
    let n = info.normal.normalize();
    let lines: Vec<_> = solid
        .face_edges(face)?
        .into_iter()
        .filter_map(|e| solid.edge_info(e).ok())
        .filter(|i| i.kind == rolidium_kernel::EdgeKind::Line)
        .collect();
    if lines.len() != 4 {
        return Err(bad(
            "The face is not a rectangle (it needs four straight sides). Click a rectangular face",
        ));
    }
    let (a, b) = rect_axes(n, lines[0].end - lines[0].start);
    // the four sides must be along a and b, and the face must fill the rectangle they make
    let pts: Vec<DVec3> = lines.iter().flat_map(|l| [l.start, l.end]).collect();
    let o = pts[0];
    let (mut amin, mut amax, mut bmin, mut bmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in &pts {
        let (x, y) = ((*p - o).dot(a), (*p - o).dot(b));
        amin = amin.min(x);
        amax = amax.max(x);
        bmin = bmin.min(y);
        bmax = bmax.max(y);
    }
    for l in &lines {
        let d = (l.end - l.start).normalize();
        if d.dot(a).abs().max(d.dot(b).abs()) < 1.0 - 1e-6 {
            return Err(bad(
                "The face is not a rectangle (its sides are not square to each other)",
            ));
        }
    }
    let area = (amax - amin) * (bmax - bmin);
    // a hole in the face (the hole that is repeated) makes the area smaller: the outer rectangle is what counts, so only the sides are checked
    if area <= 1e-9 || info.area > area * (1.0 + 1e-6) {
        return Err(bad("The face is not a rectangle"));
    }
    Ok(RectGeom {
        o,
        a,
        b,
        n,
        amin,
        amax,
        bmin,
        bmax,
    })
}

/// The rectangle of a sketch (an axis-aligned rectangle of the plane of the sketch).
pub(crate) fn rect_of_sketch(
    st: &State,
    sketch: SketchId,
    entity: rolidium_sketch::EntityId,
) -> Result<RectGeom, DocError> {
    let sk = st
        .sketches
        .iter()
        .find(|s| s.id == sketch)
        .ok_or(DocError::NoSuchSketch)?;
    let Some(Entity::Rect { a, b }) = sk.sketch.entity(entity).copied() else {
        return Err(bad(
            "The rectangle of the sketch is gone (or it is not a rectangle)",
        ));
    };
    let corners: Vec<DVec3> = Entity::rect_corners(a, b)
        .iter()
        .map(|c| sk.plane.to_3d(*c))
        .collect();
    let n = sk.plane.normal.normalize();
    let (ax, bx) = rect_axes(n, corners[1] - corners[0]);
    let o = corners[0];
    let (mut amin, mut amax, mut bmin, mut bmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in &corners {
        let (x, y) = ((*p - o).dot(ax), (*p - o).dot(bx));
        amin = amin.min(x);
        amax = amax.max(x);
        bmin = bmin.min(y);
        bmax = bmax.max(y);
    }
    Ok(RectGeom {
        o,
        a: ax,
        b: bx,
        n,
        amin,
        amax,
        bmin,
        bmax,
    })
}

/// Where the nearest edges of a rectangle are to a point, and the sides they are on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSides {
    /// +1: the nearest edge along `a` is the one at the smallest coordinate, -1: at the biggest.
    pub sign_a: f64,
    pub near_a: f64,
    pub sign_b: f64,
    pub near_b: f64,
}

impl RectGeom {
    /// The distances of a point to the nearest edges along `a` and along `b`.
    pub(crate) fn sides_of(&self, p: DVec3) -> Result<EdgeSides, DocError> {
        let (sa, sb) = ((p - self.o).dot(self.a), (p - self.o).dot(self.b));
        let (da0, da1, db0, db1) = (sa - self.amin, self.amax - sa, sb - self.bmin, self.bmax - sb);
        if da0.min(da1) < -1e-6 || db0.min(db1) < -1e-6 {
            return Err(bad(
                "The hole does not lie inside the face: click the face the hole is in",
            ));
        }
        let (sign_a, near_a) = if da0 <= da1 { (1.0, da0) } else { (-1.0, da1) };
        let (sign_b, near_b) = if db0 <= db1 { (1.0, db0) } else { (-1.0, db1) };
        Ok(EdgeSides {
            sign_a,
            near_a,
            sign_b,
            near_b,
        })
    }

    /// The point of the plane of the rectangle that has the given distances to the nearest edges (the height above the plane is that of `like`).
    pub(crate) fn place(&self, sides: EdgeSides, like: DVec3) -> DVec3 {
        let sa = if sides.sign_a > 0.0 {
            self.amin + sides.near_a
        } else {
            self.amax - sides.near_a
        };
        let sb = if sides.sign_b > 0.0 {
            self.bmin + sides.near_b
        } else {
            self.bmax - sides.near_b
        };
        self.o + self.a * sa + self.b * sb + self.n * (like - self.o).dot(self.n)
    }

    /// The pattern that puts the copies of something at `source` (with these distances to the nearest edges) into the corners of the rectangle or into a grid.
    pub(crate) fn fill_kind(
        &self,
        source: DVec3,
        sides: EdgeSides,
        fill: RectFill,
    ) -> Result<LinkKind, DocError> {
        let EdgeSides {
            sign_a,
            near_a,
            sign_b,
            near_b,
        } = sides;
        let (sa, sb) = ((source - self.o).dot(self.a), (source - self.o).dot(self.b));
        let (amin, amax, bmin, bmax) = (self.amin, self.amax, self.bmin, self.bmax);
        let (nc, nr, len_a, len_b) = match fill {
            RectFill::Corners => (2, 2, (amax - amin) - 2.0 * near_a, (bmax - bmin) - 2.0 * near_b),
            RectFill::Grid {
                columns,
                rows,
                margin,
            } => {
                let la = if sign_a > 0.0 {
                    (amax - margin) - sa
                } else {
                    sa - (amin + margin)
                };
                let lb = if sign_b > 0.0 {
                    (bmax - margin) - sb
                } else {
                    sb - (bmin + margin)
                };
                (columns.max(1), rows.max(1), la, lb)
            }
        };
        // the columns run along the longer side of the rectangle, the rows along the shorter
        let (nc, nr) = if matches!(fill, RectFill::Grid { .. }) && (amax - amin) < (bmax - bmin) {
            (nr, nc)
        } else {
            (nc, nr)
        };
        if (nc > 1 && len_a <= 1e-9) || (nr > 1 && len_b <= 1e-9) {
            return Err(bad(
                "The pieces do not fit into the face with this distance from the edges",
            ));
        }
        Ok(LinkKind::Linear(
            LinearParams {
                axis: Axis {
                    origin: source,
                    dir: self.a * sign_a,
                },
                count: nc,
                spacing: len_a,
                by_total: true,
                second: Some(Direction2 {
                    axis: Axis {
                        origin: source,
                        dir: self.b * sign_b,
                    },
                    count: nr,
                    spacing: len_b,
                    by_total: true,
                }),
            },
            PatternMode::New,
        ))
    }
}

/// The pattern that puts the copies of something that lies at `source` into the corners of the flat rectangular `face` (or in a grid over it): the
/// original keeps its place and the copies are placed with the distances to the two nearest edges that it has. `source` may lie anywhere (it is projected
/// on the plane of the face).
pub fn rect_fill(solid: &Solid, face: FaceId, source: DVec3, fill: RectFill) -> Result<LinkKind, DocError> {
    let g = rect_of_face(solid, face)?;
    let sides = g.sides_of(source)?;
    g.fill_kind(source, sides, fill)
}

/// The rectangle a pattern into the corners (or a grid) is made for, as the person points at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RectTarget {
    /// A flat rectangular face of the body the feature is in.
    Face(FaceId),
    /// A rectangle of a sketch.
    Sketch(SketchId, rolidium_sketch::EntityId),
}

/// What a pattern into the corners (or a grid) of a rectangle remembers, so that it follows the rectangle: the distances of the original to the nearest
/// edges and on which sides they are.
#[derive(Debug, Clone, PartialEq)]
pub struct FillRule {
    pub rect: RectRef,
    pub fill: RectFill,
    pub sides: EdgeSides,
    /// A point of the source (the tool) at the last calculation: where it stands.
    pub at: DVec3,
}

/// The rectangle a [`FillRule`] follows.
#[derive(Debug, Clone, PartialEq)]
pub enum RectRef {
    /// A flat rectangular face of the body the pieces go into: found again by its plane (a point and the normal) and the point on it.
    Face {
        origin: DVec3,
        normal: DVec3,
        near: DVec3,
    },
    /// A rectangle of a sketch.
    Sketch {
        sketch: SketchId,
        entity: rolidium_sketch::EntityId,
    },
}

/// The rectangle of the rule in the current state: a flat rectangular face of the solid `base` on the plane of the rule (the one nearest to the point),
/// or the rectangle of the sketch.
pub(crate) fn rect_of_rule(st: &State, base: Option<&Solid>, r: &RectRef) -> Result<RectGeom, DocError> {
    match r {
        RectRef::Sketch { sketch, entity } => rect_of_sketch(st, *sketch, *entity),
        RectRef::Face { origin, normal, near } => {
            let solid = base.ok_or_else(|| bad("The link lost the body it works on"))?;
            let n = normal.normalize();
            let mut best: Option<(f64, RectGeom)> = None;
            for f in solid.faces() {
                let Ok(i) = solid.face_info(f) else { continue };
                if i.kind != rolidium_kernel::FaceKind::Plane
                    || i.normal.normalize().dot(n) < 1.0 - 1e-6
                    || (i.origin - *origin).dot(n).abs() > 1e-5
                {
                    continue;
                }
                let Ok(g) = rect_of_face(solid, f) else { continue };
                let (x, y) = ((*near - g.o).dot(g.a), (*near - g.o).dot(g.b));
                let out = (g.amin - x)
                    .max(x - g.amax)
                    .max(0.0)
                    .hypot((g.bmin - y).max(y - g.bmax).max(0.0));
                if best.as_ref().is_none_or(|(d, _)| out < *d) {
                    best = Some((out, g));
                }
            }
            best.map(|b| b.1)
                .ok_or_else(|| bad("The rectangular face the pieces follow is gone"))
        }
    }
}

/// Places the source of a link with a [`FillRule`] and makes the pattern again from the rectangle as it is now.
pub(crate) fn follow(st: &mut State, link: &mut Link) -> Result<(), DocError> {
    // a pattern along a curve that is kept: the polyline is read from the curve again
    if let LinkKind::Path(p, m) = &link.kind {
        if let Some(src) = &p.source {
            let (pts, closed) = generate::path_polyline(st, src)?;
            let mut q = p.clone();
            q.points = pts;
            q.closed = closed;
            link.kind = LinkKind::Path(q, *m);
        }
    }
    let Some(mut rule) = link.fill.clone() else {
        return Ok(());
    };
    let g = rect_of_rule(st, link.base.as_ref(), &rule.rect)?;
    let want = g.place(rule.sides, rule.at);
    let shift = want - rule.at;
    if shift.length() > 1e-9 {
        for s in &link.sources {
            if let Some(b) = st.bodies.iter_mut().find(|b| b.id == *s) {
                b.solid = b.solid.moved(shift, DVec3::ZERO, DVec3::Z, 0.0)?.solid;
            }
        }
    }
    rule.at = want;
    let kind = g.fill_kind(want, rule.sides, rule.fill)?;
    link.kind = kind.with_mode(link.kind.mode());
    link.fill = Some(rule);
    Ok(())
}

/// A number that changes when what a link follows besides its sources changes (the rectangle of a sketch).
pub(crate) fn rule_stamp(st: &State, link: &Link) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(FillRule {
        rect: RectRef::Sketch { sketch, entity },
        ..
    }) = &link.fill
    {
        let e = st
            .sketches
            .iter()
            .find(|s| s.id == *sketch)
            .and_then(|s| s.sketch.entity(*entity).copied());
        format!("{e:?}").hash(&mut h);
        st.sketches
            .iter()
            .find(|s| s.id == *sketch)
            .map(|s| format!("{:?}", s.plane))
            .hash(&mut h);
    }
    h.finish()
}
