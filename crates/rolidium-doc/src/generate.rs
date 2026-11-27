// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The solids that a loft and a sweep make from the profiles and the path of the document. The commands `Loft` and `Sweep` use this once,
//! and the links of the same names (see `links.rs`) use it again each time a source changes.

use super::*;

/// What a loft or a sweep made from the document.
pub(crate) struct Made {
    pub tool: Solid,
    /// The body a profile lies on (what "add" and "cut" work on).
    pub target: Option<BodyId>,
    /// The curves of the sketches that the profiles used.
    pub used: Vec<(SketchId, Vec<rolidium_sketch::EntityId>)>,
    /// A remark about the way the sweep was made.
    pub notice: Option<String>,
}

fn body_of(st: &State, id: BodyId) -> Result<&Body, DocError> {
    st.bodies.iter().find(|b| b.id == id).ok_or(DocError::NoSuchBody)
}

/// One profile of a loft or a sweep: a region of a sketch (owned), or a face or an edge loop of a body.
enum Gathered {
    Region(Profile, PlaneDef),
    Face(BodyId, FaceId),
    Edge(BodyId, u32),
}

fn gather(
    st: &State,
    p: &LoftRef,
    used: &mut Vec<(SketchId, Vec<rolidium_sketch::EntityId>)>,
    target: &mut Option<BodyId>,
) -> Result<Gathered, DocError> {
    Ok(match *p {
        LoftRef::Region { sketch, region } => {
            let sd = st
                .sketches
                .iter()
                .find(|s| s.id == sketch)
                .ok_or(DocError::NoSuchSketch)?;
            let regions = sd.sketch.regions();
            let reg = regions.get(region).ok_or(DocError::NoSuchRegion)?;
            used.push((sketch, reg.exclusive_entities.clone()));
            *target = target.or(sd.target);
            Gathered::Region(region_profile(reg), sd.plane)
        }
        LoftRef::Face { body, face } => {
            body_of(st, body)?;
            *target = target.or(Some(body));
            Gathered::Face(body, face)
        }
        LoftRef::EdgeLoop { body, edge } => {
            body_of(st, body)?;
            *target = target.or(Some(body));
            Gathered::Edge(body, edge)
        }
    })
}

fn input<'a>(st: &'a State, g: &'a Gathered) -> Result<rolidium_kernel::LoftInput<'a>, DocError> {
    Ok(match g {
        Gathered::Region(pr, pl) => rolidium_kernel::LoftInput::Profile {
            profile: pr,
            origin: pl.origin,
            normal: pl.normal,
            x_dir: pl.x_dir,
        },
        Gathered::Face(b, f) => rolidium_kernel::LoftInput::Face {
            solid: &body_of(st, *b)?.solid,
            face: *f,
        },
        Gathered::Edge(b, e) => rolidium_kernel::LoftInput::Edge {
            solid: &body_of(st, *b)?.solid,
            edge: rolidium_kernel::EdgeId(*e),
        },
    })
}

/// The solid of a loft through `profiles`.
pub(crate) fn loft(st: &State, profiles: &[LoftRef], ruled: bool) -> Result<Made, DocError> {
    if profiles.len() < 2 {
        return Err(KernelError::Invalid("A loft needs at least two profiles".into()).into());
    }
    let mut used = Vec::new();
    let mut target = None;
    let gathered: Vec<Gathered> = profiles
        .iter()
        .map(|p| gather(st, p, &mut used, &mut target))
        .collect::<Result<_, _>>()?;
    let inputs: Vec<rolidium_kernel::LoftInput> =
        gathered.iter().map(|g| input(st, g)).collect::<Result<_, _>>()?;
    let tool = Solid::loft(&inputs, ruled).map_err(|e| match e {
        KernelError::Native(m) => KernelError::Invalid(format!("The loft failed: {m}")),
        e => e,
    })?;
    Ok(Made {
        tool,
        target,
        used,
        notice: None,
    })
}

/// A curve (a curve of the document, the shapes of a sketch that continue each other, edges of a body) as a polyline, and whether it is closed.
pub(crate) fn path_polyline(st: &State, path: &SweepPath) -> Result<(Vec<DVec3>, bool), DocError> {
    let kpath = match path {
        SweepPath::Curve(id) => st
            .curves
            .iter()
            .find(|c| c.id == *id)
            .ok_or(DocError::NoSuchCurve)?
            .def
            .path()?,
        SweepPath::Sketch { sketch, entities } => {
            let sd = st
                .sketches
                .iter()
                .find(|s| s.id == *sketch)
                .ok_or(DocError::NoSuchSketch)?;
            let segs = sketch_path_segments(&sd.plane, &sd.sketch, entities)?;
            rolidium_kernel::Path::build(rolidium_kernel::PathSource::Segments(&segs))?
        }
        SweepPath::Edges { body, edges } => {
            let b = body_of(st, *body)?;
            let ids: Vec<rolidium_kernel::EdgeId> =
                edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
            rolidium_kernel::Path::build(rolidium_kernel::PathSource::Edges {
                solid: &b.solid,
                edges: &ids,
            })?
        }
    };
    let pts = kpath.polyline(0.02)?;
    Ok((pts, kpath.closed))
}

/// The solid of a sweep of `profile` along `path`.
pub(crate) fn sweep(
    st: &State,
    profile: &LoftRef,
    path: &SweepPath,
    rounded: bool,
) -> Result<Made, DocError> {
    let mut used = Vec::new();
    let mut target = None;
    let g = gather(st, profile, &mut used, &mut target)?;
    let kpath = match path {
        SweepPath::Curve(id) => st
            .curves
            .iter()
            .find(|c| c.id == *id)
            .ok_or(DocError::NoSuchCurve)?
            .def
            .path()?,
        SweepPath::Sketch { sketch, entities } => {
            let sd = st
                .sketches
                .iter()
                .find(|s| s.id == *sketch)
                .ok_or(DocError::NoSuchSketch)?;
            let segs = sketch_path_segments(&sd.plane, &sd.sketch, entities)?;
            rolidium_kernel::Path::build(rolidium_kernel::PathSource::Segments(&segs))?
        }
        SweepPath::Edges { body, edges } => {
            let b = body_of(st, *body)?;
            let ids: Vec<rolidium_kernel::EdgeId> =
                edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
            rolidium_kernel::Path::build(rolidium_kernel::PathSource::Edges {
                solid: &b.solid,
                edges: &ids,
            })?
        }
    };
    let swept = Solid::sweep(
        &input(st, &g)?,
        &kpath,
        &rolidium_kernel::SweepOptions { rounded },
    )?;
    let notice = if swept
        .warnings
        .contains(&rolidium_kernel::SweepWarning::ProfileNotAtPath)
    {
        Some("The profile does not lie at the start of the path; the sweep was made anyway".to_string())
    } else if swept
        .warnings
        .contains(&rolidium_kernel::SweepWarning::ProfileNotPerpendicular)
    {
        Some(
            "The profile is not perpendicular to the path where it starts; the sweep was made anyway"
                .to_string(),
        )
    } else {
        None
    };
    Ok(Made {
        tool: swept.solid,
        target,
        used,
        notice,
    })
}

/// The bodies, sketches and curves that a loft or a sweep depends on.
pub(crate) struct Deps {
    pub bodies: Vec<BodyId>,
    pub sketches: Vec<SketchId>,
    pub curves: Vec<CurveId>,
}

fn add_ref(d: &mut Deps, r: &LoftRef) {
    match r {
        LoftRef::Region { sketch, .. } => d.sketches.push(*sketch),
        LoftRef::Face { body, .. } | LoftRef::EdgeLoop { body, .. } => d.bodies.push(*body),
    }
}

pub(crate) fn deps(kind: &LinkKind) -> Deps {
    let mut d = Deps {
        bodies: Vec::new(),
        sketches: Vec::new(),
        curves: Vec::new(),
    };
    match kind {
        LinkKind::Shell(_) | LinkKind::Text(_) => {}
        LinkKind::Loft { profiles, .. } => profiles.iter().for_each(|p| add_ref(&mut d, p)),
        LinkKind::Sweep { profile, path, .. } => {
            add_ref(&mut d, profile);
            match path {
                SweepPath::Curve(c) => d.curves.push(*c),
                SweepPath::Sketch { sketch, .. } => d.sketches.push(*sketch),
                SweepPath::Edges { body, .. } => d.bodies.push(*body),
            }
        }
        _ => {}
    }
    d.bodies.sort_unstable();
    d.bodies.dedup();
    d.sketches.sort_unstable();
    d.sketches.dedup();
    d
}

/// A number that changes when a sketch or a curve the link depends on changes (its shapes, its plane).
pub(crate) fn stamp(st: &State, kind: &LinkKind) -> u64 {
    use std::hash::{Hash, Hasher};
    let d = deps(kind);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for id in &d.sketches {
        match st.sketches.iter().find(|s| s.id == *id) {
            Some(s) => format!("{:?}{:?}", s.plane, s.sketch).hash(&mut h),
            None => (u64::MAX, *id).hash(&mut h),
        }
    }
    for id in &d.curves {
        match st.curves.iter().find(|c| c.id == *id) {
            Some(c) => format!("{:?}", c.def).hash(&mut h),
            None => (u64::MAX - 1, *id).hash(&mut h),
        }
    }
    h.finish()
}

/// How the solid of a loft or a sweep joins the document: as a new body, added to the body a profile lies on, or cut from a body.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    New,
    Add,
    Cut,
}

fn pattern_mode(st: &State, made: &Made, way: Way) -> Result<PatternMode, DocError> {
    match way {
        Way::New => Ok(PatternMode::New),
        Way::Add => made.target.map(PatternMode::Add).ok_or_else(|| {
            KernelError::Invalid("There is no body to add to: the profiles are not on a body".into()).into()
        }),
        Way::Cut => {
            // the body the profile lies on first, then the others: the first one the tool cuts into
            let ids: Vec<BodyId> = made
                .target
                .into_iter()
                .chain(st.bodies.iter().map(|b| b.id))
                .collect();
            let mut last = DocError::Kernel(KernelError::Invalid("There is no body to cut from".into()));
            for id in ids {
                let Some(b) = st.bodies.iter().find(|b| b.id == id) else {
                    continue;
                };
                match b.solid.boolean_checked(&made.tool, BooleanOp::Cut) {
                    Ok(r) => {
                        let (v0, v1) = (b.solid.volume()?, r.solid.volume()?);
                        if (v0 - v1).abs() > 1e-9 {
                            return Ok(PatternMode::Cut(id));
                        }
                    }
                    Err(e) => last = e.into(),
                }
            }
            Err(last)
        }
    }
}

/// The kind of link for a loft through `profiles` that is added with `mode` (the body it works on is found as `Command::Loft` finds it).
pub(crate) fn loft_kind(
    st: &State,
    profiles: &[LoftRef],
    ruled: bool,
    mode: LoftMode,
) -> Result<LinkKind, DocError> {
    let made = loft(st, profiles, ruled)?;
    let way = match mode {
        LoftMode::NewBody => Way::New,
        LoftMode::Add => Way::Add,
        LoftMode::Cut => Way::Cut,
    };
    Ok(LinkKind::Loft {
        profiles: profiles.to_vec(),
        ruled,
        mode: pattern_mode(st, &made, way)?,
    })
}

/// The kind of link for a sweep of `profile` along `path`.
pub(crate) fn sweep_kind(
    st: &State,
    profile: &LoftRef,
    path: &SweepPath,
    rounded: bool,
    mode: SweepMode,
) -> Result<LinkKind, DocError> {
    let made = sweep(st, profile, path, rounded)?;
    let way = match mode {
        SweepMode::Auto if made.target.is_some() => Way::Add,
        SweepMode::Auto | SweepMode::NewBody => Way::New,
        SweepMode::Add => Way::Add,
        SweepMode::Cut => Way::Cut,
    };
    Ok(LinkKind::Sweep {
        profile: *profile,
        path: path.clone(),
        rounded,
        mode: pattern_mode(st, &made, way)?,
    })
}
