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
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirrorParams {
    pub origin: DVec3,
    pub normal: DVec3,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    Linear(LinearParams, PatternMode),
    Circular(CircularParams, PatternMode),
    Mirror(MirrorParams),
}

impl LinkKind {
    pub fn mode(&self) -> PatternMode {
        match self {
            LinkKind::Linear(_, m) | LinkKind::Circular(_, m) => *m,
            LinkKind::Mirror(_) => PatternMode::New,
        }
    }

    /// The default name of a new link.
    pub fn title(&self) -> &'static str {
        match self {
            LinkKind::Linear(p, _) if p.second.is_some() => "Grid",
            LinkKind::Linear(..) => "Linear pattern",
            LinkKind::Circular(..) => "Circular pattern",
            LinkKind::Mirror(_) => "Mirror",
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
        LinkKind::Mirror(m) => {
            if !m.normal.is_finite() || m.normal.length() < 1e-9 || !m.origin.is_finite() {
                return Err(bad("The mirror plane has no normal"));
            }
        }
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

/// Calculates a link from the sources in `st` (and `base` for a pattern that adds or cuts).
fn calculate(st: &State, link: &Link) -> Result<Outcome, DocError> {
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
        if let Some(b) = st.bodies.iter_mut().find(|b| b.id == t) {
            b.solid = solid;
        }
        link.results = vec![t];
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
        if sources.contains(&t) {
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
    };
    let out = calculate(st, &link)?;
    apply(st, &mut link, out, next);
    // the sources of a pattern that adds or cuts are tools: they are hidden, the result is the target
    if link.kind.mode().target().is_some() {
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
        let out = calculate(st, &link)?;
        apply(st, &mut link, out, next);
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
        let same = !missing
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
        match calculate(st, &link) {
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
        st.links[i] = link;
    }
    // bodies that are gone are not in a group
    let alive: std::collections::HashSet<BodyId> = st.bodies.iter().map(|b| b.id).collect();
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
