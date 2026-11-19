// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Document model. The document changes only through [`Command`]s; every command is computed
//! from an immutable snapshot (so it can run on a worker thread) and committed as a whole, so a
//! failed or panicking operation leaves the document exactly as it was.

use rolidium_kernel::{BooleanOp, DVec3, FaceId, History, KernelError, Profile, ProfileSegment, Solid};

mod curves;
pub use curves::{sketch_chain, sketch_path_segments, CurveDef, CurveDoc, CurveId};
use rolidium_sketch::{Contour, Entity, PlaneDef, Region, Segment, Sketch, SketchError};
use std::sync::Arc;

pub use rolidium_sketch;

pub mod file;

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error(transparent)]
    Kernel(#[from] KernelError),
    #[error(transparent)]
    Sketch(#[from] SketchError),
    #[error("The body no longer exists")]
    NoSuchBody,
    #[error("The sketch no longer exists")]
    NoSuchSketch,
    #[error("The curve no longer exists")]
    NoSuchCurve,
    #[error("That region no longer exists")]
    NoSuchRegion,
    #[error("The body this sketch was drawn on no longer exists")]
    TargetGone,
    #[error("The model was changed while the operation was running, so the result was discarded")]
    Stale,
    #[error("Internal error while computing the operation; the model was not changed ({0})")]
    Panic(String),
}

pub type BodyId = u64;
pub type SketchId = u64;

/// How a pull treats its direction. `Auto` adds material outwards and removes it inwards; `Add`
/// and `Cut` force one or the other whatever the sign of the distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PullMode {
    #[default]
    Auto,
    Add,
    Cut,
}

impl PullMode {
    /// The signed distance actually used (positive: outwards / adding, negative: inwards / cutting).
    pub fn apply(self, distance: f64) -> f64 {
        match self {
            PullMode::Auto => distance,
            PullMode::Add => distance.abs(),
            PullMode::Cut => -distance.abs(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendKind {
    Round,
    Chamfer,
}

/// A round or chamfer made by one command. Its faces are followed through the kernel history of
/// every later operation, so the feature can be recognised and edited again.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub kind: BlendKind,
    pub size: f64,
    pub faces: Vec<FaceId>,
    /// Chamfer angle in degrees (45: symmetric) and which face the distance is measured on.
    pub angle: f64,
    pub flip: bool,
}

#[derive(Debug, Clone)]
pub struct Body {
    pub id: BodyId,
    pub name: String,
    pub solid: Solid,
    pub color: [f32; 3],
    pub visible: bool,
    pub features: Vec<Feature>,
    /// `None`: the default material (steel) was never changed; such a body is saved exactly as before.
    pub material: Option<Material>,
}

/// What a body is made of (for its mass).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Name of a preset ("steel", "PLA", …) or "custom".
    pub name: String,
    /// g/cm³.
    pub density: f64,
}

/// The presets of the Mass row (g/cm³); the first is the default.
pub const MATERIALS: [(&str, f64); 7] = [
    ("steel", 7.85),
    ("aluminium", 2.70),
    ("brass", 8.5),
    ("copper", 8.96),
    ("PLA", 1.24),
    ("PETG", 1.27),
    ("ABS", 1.04),
];

impl Material {
    pub fn preset(i: usize) -> Material {
        Material {
            name: MATERIALS[i].0.into(),
            density: MATERIALS[i].1,
        }
    }
}

impl Body {
    /// The material, the default one when none was chosen.
    pub fn material_or_default(&self) -> Material {
        self.material.clone().unwrap_or_else(|| Material::preset(0))
    }

    /// Mass in grams (volume in mm³, density in g/cm³).
    pub fn mass_g(&self) -> Option<f64> {
        Some(self.solid.volume().ok()? / 1000.0 * self.material_or_default().density)
    }
}

#[derive(Debug, Clone)]
pub struct SketchDoc {
    pub id: SketchId,
    pub name: String,
    pub plane: PlaneDef,
    /// Body whose face the sketch was drawn on (`None`: base plane, pulling creates a new body).
    pub target: Option<BodyId>,
    pub sketch: Sketch,
    pub visible: bool,
}

/// Everything the document contains at one moment. Snapshots are cheap (`Arc`).
#[derive(Debug, Clone, Default)]
pub struct State {
    pub bodies: Vec<Body>,
    pub sketches: Vec<SketchDoc>,
    pub curves: Vec<CurveDoc>,
    /// Numbering of new bodies, sketches and curves, each with its own series.
    pub body_counter: u32,
    pub sketch_counter: u32,
    pub curve_counter: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombineOp {
    Merge,
    Cut,
    Intersect,
}

/// A profile of a loft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoftRef {
    Region {
        sketch: SketchId,
        region: usize,
    },
    Face {
        body: BodyId,
        face: FaceId,
    },
    /// A closed flat loop of one edge of a body.
    EdgeLoop {
        body: BodyId,
        edge: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoftMode {
    NewBody,
    Add,
    Cut,
}

/// A body or a sketch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemId {
    Body(BodyId),
    Sketch(SketchId),
    Curve(CurveId),
}

/// The path of a sweep.
#[derive(Debug, Clone, PartialEq)]
pub enum SweepPath {
    Curve(CurveId),
    /// Shapes of a sketch (lines and arcs that continue each other).
    Sketch {
        sketch: SketchId,
        entities: Vec<rolidium_sketch::EntityId>,
    },
    /// Edges of a body that form one chain.
    Edges {
        body: BodyId,
        edges: Vec<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepMode {
    /// Adds to the body the profile lies on, else makes a new body.
    Auto,
    NewBody,
    Add,
    Cut,
}

#[derive(Debug, Clone)]
pub enum SketchTarget {
    Existing(SketchId),
    New { plane: PlaneDef, body: Option<BodyId> },
}

#[derive(Debug, Clone)]
pub enum Command {
    AddBox {
        name: String,
        min: DVec3,
        max: DVec3,
    },
    PullFace {
        body: BodyId,
        face: FaceId,
        distance: f64,
        mode: PullMode,
    },
    AddEntity {
        target: SketchTarget,
        entity: Entity,
    },
    /// Pull a closed region of a sketch. `region` is the index in `Sketch::regions()`.
    PullRegion {
        sketch: SketchId,
        region: usize,
        distance: f64,
        mode: PullMode,
    },
    /// Round (`chamfer == false`) or chamfer edges of a body; `edges` are indices in the current solid.
    FilletEdges {
        body: BodyId,
        edges: Vec<u32>,
        size: f64,
        chamfer: bool,
    },
    /// Change the size of an existing round or chamfer; `face` is any face of it. Size 0 removes it.
    EditBlend {
        body: BodyId,
        face: FaceId,
        size: f64,
    },
    /// Chamfer with a distance and an angle (the second distance is distance * tan(angle)).
    ChamferAngled {
        body: BodyId,
        edges: Vec<u32>,
        distance: f64,
        angle_deg: f64,
        flip: bool,
    },
    /// Change the distance, angle and side of an existing chamfer (any face of it); distance 0 removes it.
    EditChamfer {
        body: BodyId,
        face: FaceId,
        distance: f64,
        angle_deg: f64,
        flip: bool,
    },
    /// Combine bodies: the target with the others (merge, cut, intersect).
    Combine {
        target: BodyId,
        tools: Vec<BodyId>,
        op: CombineOp,
        keep_tools: bool,
    },
    /// Rebuild a complete hole or pin with a new diameter.
    SetCylinderDiameter {
        body: BodyId,
        face: FaceId,
        diameter: f64,
    },
    /// Replace the geometry of a sketch shape (dimension edit).
    ReplaceEntity {
        sketch: SketchId,
        entity: rolidium_sketch::EntityId,
        new: Entity,
    },
    DeleteEntity {
        sketch: SketchId,
        entity: rolidium_sketch::EntityId,
    },
    /// Sets the material of a body (`None`: back to the default).
    SetMaterial {
        body: BodyId,
        material: Option<Material>,
    },
    DeleteBody {
        body: BodyId,
    },
    /// Delete the shapes that border only this region (shapes shared with another region stay).
    DeleteRegionShapes {
        sketch: SketchId,
        region: usize,
    },
    DeleteSketch {
        sketch: SketchId,
    },
    /// A curve (3D line or helix) as an object of the document.
    AddCurve {
        def: CurveDef,
    },
    DeleteCurve {
        curve: CurveId,
    },
    /// Rotate about the axis through `pivot` by `angle` (radians), then translate.
    MoveCurves {
        curves: Vec<CurveId>,
        translation: DVec3,
        pivot: DVec3,
        axis: DVec3,
        angle: f64,
    },
    /// Sweep a flat profile along a path.
    Sweep {
        profile: LoftRef,
        path: SweepPath,
        mode: SweepMode,
        rounded: bool,
    },
    SetVisible {
        target: ItemId,
        visible: bool,
    },
    Rename {
        target: ItemId,
        name: String,
    },
    /// Rotate about the axis through `pivot` by `angle` (radians), then translate. With `copy`, new
    /// bodies are made and the originals stay.
    Move {
        bodies: Vec<BodyId>,
        translation: DVec3,
        pivot: DVec3,
        axis: DVec3,
        angle: f64,
        copy: bool,
    },
    /// Revolve a closed sketch region about an axis (in the plane of the sketch).
    Revolve {
        sketch: SketchId,
        region: usize,
        axis_point: DVec3,
        axis_dir: DVec3,
        angle: f64,
        mode: PullMode,
    },
    /// Revolve a planar face of a body about an axis lying in its plane.
    RevolveFace {
        body: BodyId,
        face: FaceId,
        axis_point: DVec3,
        axis_dir: DVec3,
        angle: f64,
        mode: PullMode,
    },
    /// Join profiles (regions or flat faces) in the given order into a solid.
    Loft {
        profiles: Vec<LoftRef>,
        ruled: bool,
        mode: LoftMode,
    },
    /// Add solids (STEP import) as new bodies.
    AddSolids {
        solids: Vec<Solid>,
    },
}

/// Result of computing a command against a snapshot.
#[derive(Debug, Clone)]
pub struct Computed {
    pub state: State,
    /// For commands that modify one body: face history, so selection can be remapped.
    pub history: Option<(BodyId, History)>,
    pub label: String,
    pub created_sketch: Option<SketchId>,
    pub created_curve: Option<CurveId>,
    /// Body that was created or changed by the command, if any.
    pub touched_body: Option<BodyId>,
    /// A remark for the user about how the command was carried out (shown in the status bar).
    pub notice: Option<String>,
    base_revision: u64,
}

/// Everything a worker needs to compute a command; independent of the live document.
#[derive(Debug, Clone)]
pub struct Job {
    pub command: Command,
    base: Arc<State>,
    base_revision: u64,
    next_id: u64,
}

/// Converts a closed sketch contour into a kernel profile loop.
fn contour_loop(c: &Contour) -> Vec<ProfileSegment> {
    let p = |v: rolidium_sketch::DVec2| [v.x, v.y];
    c.segments
        .iter()
        .map(|s| match *s {
            Segment::Line { a, b } => ProfileSegment::Line { a: p(a), b: p(b) },
            Segment::Arc {
                center,
                radius,
                sweep,
                ..
            } if sweep.abs() >= std::f64::consts::TAU - 1e-9 => ProfileSegment::Circle {
                center: p(center),
                radius,
            },
            seg => ProfileSegment::Arc {
                a: p(seg.start_point()),
                mid: p(seg.mid_point()),
                b: p(seg.end_point()),
            },
        })
        .collect()
}

/// Moves the faces of every feature to the faces that descend from them.
fn remap_features(features: &mut Vec<Feature>, history: &History) {
    for f in features.iter_mut() {
        let mut faces: Vec<FaceId> = f.faces.iter().flat_map(|x| history.successors(0, *x)).collect();
        faces.sort();
        faces.dedup();
        f.faces = faces;
    }
    features.retain(|f| !f.faces.is_empty());
}

/// The axis of a revolve must lie in the plane of the profile and must not cut through it
/// (touching its boundary is fine).
fn check_axis(plane: &PlaneDef, reg: &Region, p: DVec3, d: DVec3) -> Result<(), DocError> {
    let err = |m: &str| DocError::Kernel(KernelError::Invalid(m.into()));
    if d.length() < 1e-9 {
        return Err(err("The axis has no direction"));
    }
    let n = plane.normal.normalize();
    if d.normalize().dot(n).abs() > 1e-6 || (p - plane.origin).dot(n).abs() > 1e-6 {
        return Err(err("The axis must lie in the plane of the profile"));
    }
    let a = plane.to_2d(p);
    let dir = plane.to_2d(p + d) - a;
    let len = dir.length();
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for q in reg.outer.polyline(0.05) {
        let side = dir.perp_dot(q - a) / len;
        lo = lo.min(side);
        hi = hi.max(side);
    }
    if lo < -1e-6 && hi > 1e-6 {
        return Err(err(
            "The axis passes through the profile; it must be beside it or along its edge",
        ));
    }
    Ok(())
}

/// Fuses or cuts `tool` with body `bid`; a fuse that does not touch the body makes a new body. Without a
/// target body the tool becomes a new body. Returns the body that changed or appeared and its history.
fn apply_tool(
    st: &mut State,
    bid: Option<BodyId>,
    tool: Solid,
    op: BooleanOp,
    next_id: u64,
) -> Result<(BodyId, Option<History>), DocError> {
    let new_body = |st: &mut State, solid: Solid| {
        st.body_counter += 1;
        st.bodies.push(Body {
            id: next_id,
            name: format!("Body {}", st.body_counter),
            solid,
            color: [0.72, 0.76, 0.82],
            visible: true,
            features: Vec::new(),
            material: None,
        });
        (next_id, None)
    };
    let Some(bid) = bid else {
        return Ok(new_body(st, tool));
    };
    let bpos = st
        .bodies
        .iter()
        .position(|b| b.id == bid)
        .ok_or(DocError::TargetGone)?;
    let before = st.bodies[bpos].solid.volume()?;
    match st.bodies[bpos].solid.boolean(&tool, op) {
        Err(KernelError::SplitResult) if op == BooleanOp::Fuse => Ok(new_body(st, tool)),
        Err(e) => Err(e.into()),
        Ok(r) => {
            let after = r.solid.volume()?;
            if (after - before).abs() < 1e-9 {
                return Err(KernelError::Invalid(if op == BooleanOp::Cut {
                    "There is nothing to remove here: the shape does not overlap the body".into()
                } else {
                    "the operation did not change the solid as expected".into()
                })
                .into());
            }
            let b = &mut st.bodies[bpos];
            remap_features(&mut b.features, &r.history);
            b.solid = r.solid;
            Ok((bid, Some(r.history)))
        }
    }
}

pub fn region_profile(r: &Region) -> Profile {
    let mut loops = vec![contour_loop(&r.outer)];
    loops.extend(r.holes.iter().map(contour_loop));
    Profile { loops }
}

impl Job {
    /// Runs the command. Native panics are converted into an error.
    pub fn run(self) -> Result<Computed, DocError> {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.compute()));
        match r {
            Ok(r) => r,
            Err(p) => {
                let msg = p
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                Err(DocError::Panic(msg))
            }
        }
    }

    fn done(&self, state: State, label: &str) -> Computed {
        Computed {
            state,
            history: None,
            label: label.into(),
            created_sketch: None,
            created_curve: None,
            touched_body: None,
            notice: None,
            base_revision: self.base_revision,
        }
    }

    fn compute(&self) -> Result<Computed, DocError> {
        let mut st: State = (*self.base).clone();
        match &self.command {
            Command::AddBox { name, min, max } => {
                let solid = Solid::cuboid(*min, *max)?;
                st.body_counter += 1;
                let name = if name.is_empty() {
                    format!("Body {}", st.body_counter)
                } else {
                    name.clone()
                };
                st.bodies.push(Body {
                    id: self.next_id,
                    name,
                    solid,
                    color: [0.72, 0.76, 0.82],
                    visible: true,
                    features: Vec::new(),
                    material: None,
                });
                let mut c = self.done(st, "Add box");
                c.touched_body = Some(self.next_id);
                Ok(c)
            }
            Command::PullFace {
                body,
                face,
                distance,
                mode,
            } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                let r = b.solid.pull_face(*face, mode.apply(*distance))?;
                remap_features(&mut b.features, &r.history);
                b.solid = r.solid;
                let mut c = self.done(st, "Pull");
                c.history = Some((*body, r.history));
                c.touched_body = Some(*body);
                Ok(c)
            }
            Command::AddEntity { target, entity } => {
                entity.validate()?;
                let mut created = None;
                let id = match target {
                    SketchTarget::Existing(id) => *id,
                    SketchTarget::New { plane, body } => {
                        st.sketch_counter += 1;
                        st.sketches.push(SketchDoc {
                            id: self.next_id,
                            name: format!("Sketch {}", st.sketch_counter),
                            plane: *plane,
                            target: *body,
                            sketch: Sketch::new(),
                            visible: true,
                        });
                        created = Some(self.next_id);
                        self.next_id
                    }
                };
                let sk = st
                    .sketches
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or(DocError::NoSuchSketch)?;
                sk.sketch.add(*entity)?;
                let mut c = self.done(st, "Draw");
                c.created_sketch = created;
                Ok(c)
            }
            Command::PullRegion {
                sketch,
                region,
                distance,
                mode,
            } => {
                let idx = st
                    .sketches
                    .iter()
                    .position(|s| s.id == *sketch)
                    .ok_or(DocError::NoSuchSketch)?;
                let regions = st.sketches[idx].sketch.regions();
                let reg = regions.get(*region).ok_or(DocError::NoSuchRegion)?;
                let plane = st.sketches[idx].plane;
                let target = st.sketches[idx].target;
                let profile = region_profile(reg);
                let distance = &(if target.is_some() {
                    mode.apply(*distance)
                } else {
                    *distance
                });
                if !distance.is_finite() || distance.abs() < 1e-6 {
                    return Err(KernelError::ZeroDistance.into());
                }
                let tool =
                    Solid::extrude_profile(&profile, plane.origin, plane.normal, plane.x_dir, *distance)?;
                let mut c;
                match target {
                    None => {
                        let id = self.next_id;
                        st.body_counter += 1;
                        st.bodies.push(Body {
                            id,
                            name: format!("Body {}", st.body_counter),
                            solid: tool,
                            color: [0.72, 0.76, 0.82],
                            visible: true,
                            features: Vec::new(),
                            material: None,
                        });
                        c = (id, None);
                    }
                    Some(bid) => {
                        let bpos = st
                            .bodies
                            .iter()
                            .position(|b| b.id == bid)
                            .ok_or(DocError::TargetGone)?;
                        let before = st.bodies[bpos].solid.volume()?;
                        let op = if *distance > 0.0 {
                            BooleanOp::Fuse
                        } else {
                            BooleanOp::Cut
                        };
                        match st.bodies[bpos].solid.boolean(&tool, op) {
                            // Fusing can only fall apart into two solids when the prism does not
                            // touch the body at all: that is a new, separate body.
                            Err(KernelError::SplitResult) if op == BooleanOp::Fuse => {
                                let id = self.next_id;
                                st.body_counter += 1;
                                st.bodies.push(Body {
                                    id,
                                    name: format!("Body {}", st.body_counter),
                                    solid: tool,
                                    color: [0.72, 0.76, 0.82],
                                    visible: true,
                                    features: Vec::new(),
                                    material: None,
                                });
                                c = (id, None);
                            }
                            Err(e) => return Err(e.into()),
                            Ok(r) => {
                                let after = r.solid.volume()?;
                                let ok = if op == BooleanOp::Fuse {
                                    after >= before - 1e-6
                                } else {
                                    after <= before + 1e-6
                                };
                                if op == BooleanOp::Cut && (after - before).abs() < 1e-9 {
                                    return Err(KernelError::Invalid(
                                        "There is nothing to remove here: the region does not overlap the body".into(),
                                    )
                                    .into());
                                }
                                if !ok || (after - before).abs() < 1e-9 {
                                    return Err(KernelError::Invalid(
                                        "the pull did not change the solid as expected".into(),
                                    )
                                    .into());
                                }
                                let b = &mut st.bodies[bpos];
                                remap_features(&mut b.features, &r.history);
                                b.solid = r.solid;
                                c = (bid, Some(r.history));
                            }
                        }
                    }
                }
                // The curves that only belonged to this region are used up.
                st.sketches[idx].sketch.remove(&reg.exclusive_entities);
                if st.sketches[idx].sketch.is_empty() {
                    st.sketches.remove(idx);
                }
                let mut out = self.done(st, "Pull from sketch");
                out.touched_body = Some(c.0);
                out.history = c.1.take().map(|h| (c.0, h));
                Ok(out)
            }
            Command::FilletEdges {
                body,
                edges,
                size,
                chamfer,
            } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                if edges.is_empty() {
                    return Err(KernelError::Invalid("no edge is selected".into()).into());
                }
                let ids: Vec<rolidium_kernel::EdgeId> =
                    edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
                let kind = if *chamfer {
                    BlendKind::Chamfer
                } else {
                    BlendKind::Round
                };
                // Rounds next to the new one that its tangent chain runs through are redone together with it,
                // so only the edges the user chose are rounded.
                let same_kind: Vec<usize> = (0..b.features.len())
                    .filter(|i| b.features[*i].kind == kind)
                    .collect();
                let inputs: Vec<rolidium_kernel::BlendInput> = same_kind
                    .iter()
                    .map(|i| rolidium_kernel::BlendInput {
                        faces: b.features[*i].faces.clone(),
                        size: b.features[*i].size,
                        chamfer: *chamfer,
                    })
                    .collect();
                let mut notice = None;
                let combo = match b.solid.blend_with_neighbours(&inputs, &ids, *size, *chamfer) {
                    Ok(c) => c,
                    Err(e) => {
                        notice = Some(format!(
                            "The chain of tangent edges could not be limited to the selected edges ({e}); the whole chain was treated"
                        ));
                        None
                    }
                };
                if let Some(combo) = combo {
                    let mut old = std::mem::take(&mut b.features);
                    let absorbed: Vec<usize> = combo
                        .groups
                        .iter()
                        .filter_map(|g| g.existing)
                        .map(|i| same_kind[i])
                        .collect();
                    // blends that were not touched follow the history; the redone ones get their new faces
                    let mut keep: Vec<Feature> = Vec::new();
                    for (i, f) in old.drain(..).enumerate() {
                        if !absorbed.contains(&i) {
                            keep.push(f);
                        }
                    }
                    let mut redone: Vec<Feature> = Vec::new();
                    for g in &combo.groups {
                        match g.existing {
                            Some(i) => {
                                let f = &inputs[i];
                                redone.push(Feature {
                                    kind,
                                    size: f.size,
                                    faces: g.faces.clone(),
                                    angle: 45.0,
                                    flip: false,
                                });
                            }
                            None => redone.push(Feature {
                                kind,
                                size: *size,
                                faces: g.faces.clone(),
                                angle: 45.0,
                                flip: false,
                            }),
                        }
                    }
                    remap_features(&mut keep, &combo.result.history);
                    keep.extend(redone);
                    b.features = keep;
                    b.solid = combo.result.solid;
                    let mut c = self.done(st, if *chamfer { "Chamfer" } else { "Round" });
                    c.history = Some((*body, combo.result.history));
                    c.touched_body = Some(*body);
                    return Ok(c);
                }
                let r = if *chamfer { b.solid.chamfer(&ids, *size) } else { b.solid.fillet(&ids, *size) }
                    .map_err(|e| match e {
                        KernelError::Native(_) if *chamfer => KernelError::Invalid(format!(
                            "The chamfer of {size} mm does not fit these edges (it is too large or the edges meet in a way that cannot be chamfered)"
                        )),
                        KernelError::Native(_) => KernelError::Invalid(format!(
                            "The radius of {size} mm does not fit these edges (it is too large or the edges meet in a way that cannot be rounded)"
                        )),
                        e => e,
                    })?;
                remap_features(&mut b.features, &r.history);
                let new_faces = r.history.new_faces(r.solid.face_count());
                if !new_faces.is_empty() {
                    b.features.push(Feature {
                        kind,
                        size: *size,
                        faces: new_faces,
                        angle: 45.0,
                        flip: false,
                    });
                }
                b.solid = r.solid;
                let mut c = self.done(st, if *chamfer { "Chamfer" } else { "Round" });
                c.history = Some((*body, r.history));
                c.touched_body = Some(*body);
                c.notice = notice;
                Ok(c)
            }
            Command::ChamferAngled {
                body,
                edges,
                distance,
                angle_deg,
                flip,
            } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                if edges.is_empty() {
                    return Err(KernelError::Invalid("no edge is selected".into()).into());
                }
                let ids: Vec<rolidium_kernel::EdgeId> =
                    edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
                let chain = b.solid.blend_chain(&ids, true)?;
                if chain.len() > ids.len() {
                    return Err(KernelError::Invalid(
                        "An angled chamfer cannot be made on edges that continue other edges tangentially; use the plain chamfer there".into(),
                    )
                    .into());
                }
                let r = b.solid.chamfer_angled(&ids, *distance, *angle_deg, *flip).map_err(|e| match e {
                    KernelError::Native(_) => KernelError::Invalid(format!(
                        "The chamfer of {distance} mm at {angle_deg} degrees does not fit these edges (too large, or the edges meet in a way that cannot be chamfered with two distances)"
                    )),
                    e => e,
                })?;
                remap_features(&mut b.features, &r.history);
                let faces = r.history.new_faces(r.solid.face_count());
                if !faces.is_empty() {
                    b.features.push(Feature {
                        kind: BlendKind::Chamfer,
                        size: *distance,
                        faces,
                        angle: *angle_deg,
                        flip: *flip,
                    });
                }
                b.solid = r.solid;
                let mut c = self.done(st, "Chamfer");
                c.history = Some((*body, r.history));
                c.touched_body = Some(*body);
                Ok(c)
            }
            Command::EditChamfer {
                body,
                face,
                distance,
                angle_deg,
                flip,
            } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                let fi = b
                    .features
                    .iter()
                    .position(|f| f.faces.contains(face) && f.kind == BlendKind::Chamfer)
                    .ok_or_else(|| KernelError::Invalid("this face is not a chamfer".into()))?;
                let feat = b.features.remove(fi);
                let r = b
                    .solid
                    .edit_blend_angled(&feat.faces, *distance, true, *angle_deg, *flip)?;
                remap_features(&mut b.features, &r.history);
                if *distance > 1e-9 {
                    let faces = r.history.new_faces(r.solid.face_count());
                    if !faces.is_empty() {
                        b.features.push(Feature {
                            kind: BlendKind::Chamfer,
                            size: *distance,
                            faces,
                            angle: *angle_deg,
                            flip: *flip,
                        });
                    }
                }
                b.solid = r.solid;
                let mut c = self.done(
                    st,
                    if *distance > 1e-9 {
                        "Change chamfer"
                    } else {
                        "Remove chamfer"
                    },
                );
                c.history = Some((*body, r.history));
                c.touched_body = Some(*body);
                Ok(c)
            }
            Command::Combine {
                target,
                tools,
                op,
                keep_tools,
            } => {
                let tpos = st
                    .bodies
                    .iter()
                    .position(|b| b.id == *target)
                    .ok_or(DocError::NoSuchBody)?;
                if tools.is_empty() {
                    return Err(KernelError::Invalid("select at least one more body".into()).into());
                }
                let mut tool_solids: Vec<(BodyId, Solid)> = Vec::new();
                for t in tools {
                    if t == target {
                        continue;
                    }
                    let tb = st
                        .bodies
                        .iter()
                        .find(|b| b.id == *t)
                        .ok_or(DocError::NoSuchBody)?;
                    tool_solids.push((*t, tb.solid.clone()));
                }
                let bop = match op {
                    CombineOp::Merge => BooleanOp::Fuse,
                    CombineOp::Cut => BooleanOp::Cut,
                    CombineOp::Intersect => BooleanOp::Common,
                };
                let mut features = st.bodies[tpos].features.clone();
                let mut cur = st.bodies[tpos].solid.clone();
                let mut pieces: Vec<Solid> = Vec::new();
                for (i, (_, tool)) in tool_solids.iter().enumerate() {
                    let before = cur.volume()?;
                    let tool_volume = tool.volume()?;
                    // "do not touch" is said only when the bodies really are apart
                    let apart = || cur.distance_to(tool).map(|d| d > 1e-6).unwrap_or(false);
                    let r = match cur.boolean_checked(tool, bop) {
                        Ok(r) => r,
                        Err(KernelError::SplitResult | KernelError::InvalidResult | KernelError::Native(_))
                            if bop != BooleanOp::Cut && apart() =>
                        {
                            if bop == BooleanOp::Fuse {
                                return Err(KernelError::Invalid("There is nothing to merge: the bodies do not touch or overlap".into()).into());
                            }
                            return Err(KernelError::Invalid("The bodies do not overlap, so there is no common part".into()).into());
                        }
                        Err(KernelError::SplitResult | KernelError::InvalidResult | KernelError::Native(_))
                            if bop != BooleanOp::Cut =>
                        {
                            return Err(KernelError::Invalid(CALC_FAILED.into()).into())
                        }
                        Err(KernelError::SplitResult) if bop == BooleanOp::Cut && i + 1 == tool_solids.len() => {
                            pieces = cur.boolean_pieces(tool, bop)?;
                            break;
                        }
                        Err(KernelError::SplitResult) => {
                            return Err(KernelError::Invalid("The body would fall into pieces before the last tool; cut with the tools one after another".into()).into())
                        }
                        Err(KernelError::InvalidResult) => {
                            return Err(KernelError::Invalid(CALC_FAILED.into()).into())
                        }
                        Err(e) => return Err(e.into()),
                    };
                    let after = r.solid.volume()?;
                    if !combine_volume_ok(bop, before, tool_volume, after) {
                        return Err(KernelError::Invalid(CALC_FAILED.into()).into());
                    }
                    match bop {
                        BooleanOp::Cut if (after - before).abs() < 1e-9 => {
                            // "do not overlap" only when they really are apart; otherwise the kernel lost the cut
                            let overlap = !apart()
                                && cur
                                    .boolean(tool, BooleanOp::Common)
                                    .ok()
                                    .and_then(|c| c.solid.volume().ok())
                                    .is_some_and(|v| v > 1e-6 * (before + tool_volume).max(1e-3));
                            let m = if !overlap {
                                "There is nothing to cut: the bodies do not overlap"
                            } else {
                                CALC_FAILED
                            };
                            return Err(KernelError::Invalid(m.into()).into());
                        }
                        BooleanOp::Common if after < 1e-9 => {
                            return Err(KernelError::Invalid(
                                "The bodies do not overlap, so there is no common part".into(),
                            )
                            .into())
                        }
                        _ => {}
                    }
                    remap_features(&mut features, &r.history);
                    cur = r.solid;
                }
                let consumed = |id: &BodyId| {
                    !(*keep_tools && *op != CombineOp::Merge) && tool_solids.iter().any(|(t, _)| t == id)
                };
                let first = *target;
                if pieces.is_empty() {
                    st.bodies[tpos].solid = cur;
                    st.bodies[tpos].features = features;
                } else {
                    // the target fell into several bodies: the first keeps the name, the others are new
                    let mut it = pieces.into_iter();
                    st.bodies[tpos].solid = it.next().unwrap();
                    st.bodies[tpos].features = Vec::new();
                    let color = st.bodies[tpos].color;
                    for (k, p) in it.enumerate() {
                        st.body_counter += 1;
                        st.bodies.push(Body {
                            id: self.next_id + k as u64,
                            name: format!("Body {}", st.body_counter),
                            solid: p,
                            color,
                            visible: true,
                            features: Vec::new(),
                            material: None,
                        });
                    }
                }
                st.bodies.retain(|b| !consumed(&b.id) || b.id == *target);
                for s in st.sketches.iter_mut() {
                    if s.target.is_some_and(|t| !st.bodies.iter().any(|b| b.id == t)) {
                        s.target = None;
                    }
                }
                let mut c = self.done(st, "Combine");
                c.touched_body = Some(first);
                Ok(c)
            }
            Command::EditBlend { body, face, size } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                let fi = b
                    .features
                    .iter()
                    .position(|f| f.faces.contains(face))
                    .ok_or_else(|| KernelError::Invalid("this face is not a round or a chamfer".into()))?;
                let feat = b.features.remove(fi);
                let chamfer = feat.kind == BlendKind::Chamfer;
                let r = b
                    .solid
                    .edit_blend_angled(&feat.faces, *size, chamfer, feat.angle, feat.flip)?;
                remap_features(&mut b.features, &r.history);
                if *size > 1e-9 {
                    let faces = r.history.new_faces(r.solid.face_count());
                    if !faces.is_empty() {
                        b.features.push(Feature {
                            kind: feat.kind,
                            size: *size,
                            faces,
                            angle: feat.angle,
                            flip: feat.flip,
                        });
                    }
                }
                b.solid = r.solid;
                let label = match (chamfer, *size > 1e-9) {
                    (false, true) => "Change round",
                    (true, true) => "Change chamfer",
                    (false, false) => "Remove round",
                    (true, false) => "Remove chamfer",
                };
                let mut c = self.done(st, label);
                c.history = Some((*body, r.history));
                c.touched_body = Some(*body);
                Ok(c)
            }
            Command::SetCylinderDiameter { body, face, diameter } => {
                let b = st
                    .bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                let r = b.solid.set_cylinder_diameter(*face, *diameter)?;
                remap_features(&mut b.features, &r.history);
                b.solid = r.solid;
                let mut c = self.done(st, "Change diameter");
                c.touched_body = Some(*body);
                Ok(c)
            }
            Command::ReplaceEntity { sketch, entity, new } => {
                let sk = st
                    .sketches
                    .iter_mut()
                    .find(|s| s.id == *sketch)
                    .ok_or(DocError::NoSuchSketch)?;
                sk.sketch.replace(*entity, *new)?;
                Ok(self.done(st, "Change size"))
            }
            Command::DeleteEntity { sketch, entity } => {
                let idx = st
                    .sketches
                    .iter()
                    .position(|s| s.id == *sketch)
                    .ok_or(DocError::NoSuchSketch)?;
                if st.sketches[idx].sketch.entity(*entity).is_none() {
                    return Err(SketchError::NoSuchEntity.into());
                }
                st.sketches[idx].sketch.remove(&[*entity]);
                if st.sketches[idx].sketch.is_empty() {
                    st.sketches.remove(idx);
                }
                Ok(self.done(st, "Delete shape"))
            }
            Command::DeleteRegionShapes { sketch, region } => {
                let idx = st
                    .sketches
                    .iter()
                    .position(|s| s.id == *sketch)
                    .ok_or(DocError::NoSuchSketch)?;
                let regions = st.sketches[idx].sketch.regions();
                let reg = regions.get(*region).ok_or(DocError::NoSuchRegion)?;
                if reg.exclusive_entities.is_empty() {
                    return Err(KernelError::Invalid(
                        "This region has no shape of its own (all its lines are shared with neighbouring regions); select the line itself to delete it".into(),
                    )
                    .into());
                }
                st.sketches[idx].sketch.remove(&reg.exclusive_entities.clone());
                if st.sketches[idx].sketch.is_empty() {
                    st.sketches.remove(idx);
                }
                Ok(self.done(st, "Delete shapes"))
            }
            Command::DeleteSketch { sketch } => {
                let n = st.sketches.len();
                st.sketches.retain(|s| s.id != *sketch);
                if st.sketches.len() == n {
                    return Err(DocError::NoSuchSketch);
                }
                Ok(self.done(st, "Delete sketch"))
            }
            Command::SetVisible { target, visible } => {
                match target {
                    ItemId::Body(id) => {
                        st.bodies
                            .iter_mut()
                            .find(|b| b.id == *id)
                            .ok_or(DocError::NoSuchBody)?
                            .visible = *visible
                    }
                    ItemId::Sketch(id) => {
                        st.sketches
                            .iter_mut()
                            .find(|s| s.id == *id)
                            .ok_or(DocError::NoSuchSketch)?
                            .visible = *visible
                    }
                    ItemId::Curve(id) => {
                        st.curves
                            .iter_mut()
                            .find(|c| c.id == *id)
                            .ok_or(DocError::NoSuchCurve)?
                            .visible = *visible
                    }
                }
                Ok(self.done(st, if *visible { "Show" } else { "Hide" }))
            }
            Command::Rename { target, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(KernelError::Invalid("the name must not be empty".into()).into());
                }
                match target {
                    ItemId::Body(id) => {
                        st.bodies
                            .iter_mut()
                            .find(|b| b.id == *id)
                            .ok_or(DocError::NoSuchBody)?
                            .name = name.to_string()
                    }
                    ItemId::Sketch(id) => {
                        st.sketches
                            .iter_mut()
                            .find(|s| s.id == *id)
                            .ok_or(DocError::NoSuchSketch)?
                            .name = name.to_string()
                    }
                    ItemId::Curve(id) => {
                        st.curves
                            .iter_mut()
                            .find(|c| c.id == *id)
                            .ok_or(DocError::NoSuchCurve)?
                            .name = name.to_string()
                    }
                }
                Ok(self.done(st, "Rename"))
            }
            Command::Move {
                bodies,
                translation,
                pivot,
                axis,
                angle,
                copy,
            } => {
                if bodies.is_empty() {
                    return Err(KernelError::Invalid("no body is selected".into()).into());
                }
                let q = if axis.length() > 1e-9 && angle.abs() > 1e-12 {
                    glam::DQuat::from_axis_angle(axis.normalize(), *angle)
                } else {
                    glam::DQuat::IDENTITY
                };
                let point = |p: DVec3| *pivot + q * (p - *pivot) + *translation;
                let vector = |v: DVec3| q * v;
                let mut first = None;
                let mut next = self.next_id;
                for bid in bodies {
                    let pos = st
                        .bodies
                        .iter()
                        .position(|b| b.id == *bid)
                        .ok_or(DocError::NoSuchBody)?;
                    let r = st.bodies[pos].solid.moved(*translation, *pivot, *axis, *angle)?;
                    if *copy {
                        st.body_counter += 1;
                        let mut features = st.bodies[pos].features.clone();
                        remap_features(&mut features, &r.history);
                        let id = next;
                        next += 1;
                        first.get_or_insert(id);
                        st.bodies.push(Body {
                            id,
                            name: format!("Body {}", st.body_counter),
                            solid: r.solid,
                            color: st.bodies[pos].color,
                            visible: true,
                            features,
                            material: st.bodies[pos].material.clone(),
                        });
                    } else {
                        first.get_or_insert(*bid);
                        let b = &mut st.bodies[pos];
                        remap_features(&mut b.features, &r.history);
                        b.solid = r.solid;
                        // sketches drawn on this body travel with it
                        for sk in st.sketches.iter_mut().filter(|s| s.target == Some(*bid)) {
                            sk.plane = PlaneDef {
                                origin: point(sk.plane.origin),
                                normal: vector(sk.plane.normal),
                                x_dir: vector(sk.plane.x_dir),
                            };
                        }
                    }
                }
                let label = if *copy {
                    "Copy"
                } else if angle.abs() > 1e-12 && translation.length() < 1e-12 {
                    "Rotate"
                } else {
                    "Move"
                };
                let mut c = self.done(st, label);
                c.touched_body = first;
                Ok(c)
            }
            Command::Revolve {
                sketch,
                region,
                axis_point,
                axis_dir,
                angle,
                mode,
            } => {
                let idx = st
                    .sketches
                    .iter()
                    .position(|s| s.id == *sketch)
                    .ok_or(DocError::NoSuchSketch)?;
                let regions = st.sketches[idx].sketch.regions();
                let reg = regions.get(*region).ok_or(DocError::NoSuchRegion)?;
                let plane = st.sketches[idx].plane;
                let target = st.sketches[idx].target;
                check_axis(&plane, reg, *axis_point, *axis_dir)?;
                let tool = Solid::revolve_profile(
                    &region_profile(reg),
                    plane.origin,
                    plane.normal,
                    plane.x_dir,
                    *axis_point,
                    *axis_dir,
                    *angle,
                )?;
                let op = if *mode == PullMode::Cut {
                    BooleanOp::Cut
                } else {
                    BooleanOp::Fuse
                };
                let touched = match (target, op) {
                    // a sketch on a base plane that cuts: the first body the shape overlaps
                    (None, BooleanOp::Cut) => {
                        let ids: Vec<BodyId> = st.bodies.iter().map(|b| b.id).collect();
                        let mut done = None;
                        let mut last =
                            DocError::Kernel(KernelError::Invalid("There is no body to cut from".into()));
                        for b in ids {
                            match apply_tool(&mut st, Some(b), tool.clone(), op, self.next_id) {
                                Ok(t) => {
                                    done = Some(t);
                                    break;
                                }
                                Err(e) => last = e,
                            }
                        }
                        done.ok_or(last)?
                    }
                    (t, _) => apply_tool(&mut st, t, tool, op, self.next_id)?,
                };
                st.sketches[idx].sketch.remove(&reg.exclusive_entities);
                if st.sketches[idx].sketch.is_empty() {
                    st.sketches.remove(idx);
                }
                let mut out = self.done(st, "Revolve");
                out.touched_body = Some(touched.0);
                out.history = touched.1.map(|h| (touched.0, h));
                Ok(out)
            }
            Command::RevolveFace {
                body,
                face,
                axis_point,
                axis_dir,
                angle,
                mode,
            } => {
                let bpos = st
                    .bodies
                    .iter()
                    .position(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?;
                let solid = st.bodies[bpos].solid.clone();
                let info = solid.face_info(*face)?;
                if info.kind != rolidium_kernel::FaceKind::Plane {
                    return Err(KernelError::NotPlanar.into());
                }
                let err = |m: &str| DocError::Kernel(KernelError::Invalid(m.into()));
                let n = info.normal.normalize();
                if axis_dir.length() < 1e-9 {
                    return Err(err("The axis has no direction"));
                }
                if axis_dir.normalize().dot(n).abs() > 1e-6 || (*axis_point - info.origin).dot(n).abs() > 1e-6
                {
                    return Err(err("The axis must lie in the plane of the profile"));
                }
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                let u = axis_dir.normalize();
                if let Some(outer) = solid.face_outline(*face, 0.05)?.first() {
                    for p in outer {
                        let side = u.cross(p.as_dvec3() - *axis_point).dot(n);
                        lo = lo.min(side);
                        hi = hi.max(side);
                    }
                }
                if lo < -1e-6 && hi > 1e-6 {
                    return Err(err(
                        "The axis passes through the profile; it must be beside it or along its edge",
                    ));
                }
                let tool = solid.revolve_face(*face, *axis_point, *axis_dir, *angle)?;
                let op = if *mode == PullMode::Cut {
                    BooleanOp::Cut
                } else {
                    BooleanOp::Fuse
                };
                let touched = apply_tool(&mut st, Some(*body), tool, op, self.next_id)?;
                let mut out = self.done(st, "Revolve");
                out.touched_body = Some(touched.0);
                out.history = touched.1.map(|h| (touched.0, h));
                Ok(out)
            }
            Command::Loft {
                profiles,
                ruled,
                mode,
            } => {
                if profiles.len() < 2 {
                    return Err(KernelError::Invalid("A loft needs at least two profiles".into()).into());
                }
                // gather the profiles
                let mut owned: Vec<(Profile, PlaneDef)> = Vec::new();
                let mut kinds: Vec<Option<usize>> = Vec::new(); // index into owned, or None for a face
                let mut face_inputs: Vec<(BodyId, Option<FaceId>, u32)> = Vec::new();
                let mut used: Vec<(SketchId, Vec<rolidium_sketch::EntityId>)> = Vec::new();
                let mut target: Option<BodyId> = None;
                for p in profiles {
                    match *p {
                        LoftRef::Region { sketch, region } => {
                            let sd = st
                                .sketches
                                .iter()
                                .find(|s| s.id == sketch)
                                .ok_or(DocError::NoSuchSketch)?;
                            let regions = sd.sketch.regions();
                            let reg = regions.get(region).ok_or(DocError::NoSuchRegion)?;
                            owned.push((region_profile(reg), sd.plane));
                            kinds.push(Some(owned.len() - 1));
                            used.push((sketch, reg.exclusive_entities.clone()));
                            target = target.or(sd.target);
                        }
                        LoftRef::Face { body, face } => {
                            if !st.bodies.iter().any(|b| b.id == body) {
                                return Err(DocError::NoSuchBody);
                            }
                            kinds.push(None);
                            face_inputs.push((body, Some(face), 0));
                            target = target.or(Some(body));
                        }
                        LoftRef::EdgeLoop { body, edge } => {
                            if !st.bodies.iter().any(|b| b.id == body) {
                                return Err(DocError::NoSuchBody);
                            }
                            kinds.push(None);
                            face_inputs.push((body, None, edge));
                            target = target.or(Some(body));
                        }
                    }
                }
                let mut fi = 0;
                let mut inputs: Vec<rolidium_kernel::LoftInput> = Vec::new();
                for k in &kinds {
                    match k {
                        Some(i) => {
                            let (pr, pl) = &owned[*i];
                            inputs.push(rolidium_kernel::LoftInput::Profile {
                                profile: pr,
                                origin: pl.origin,
                                normal: pl.normal,
                                x_dir: pl.x_dir,
                            });
                        }
                        None => {
                            let (b, f, e) = face_inputs[fi];
                            fi += 1;
                            let solid = &st.bodies.iter().find(|x| x.id == b).unwrap().solid;
                            inputs.push(match f {
                                Some(face) => rolidium_kernel::LoftInput::Face { solid, face },
                                None => rolidium_kernel::LoftInput::Edge {
                                    solid,
                                    edge: rolidium_kernel::EdgeId(e),
                                },
                            });
                        }
                    }
                }
                let tool = Solid::loft(&inputs, *ruled).map_err(|e| match e {
                    KernelError::Native(m) => KernelError::Invalid(format!("The loft failed: {m}")),
                    e => e,
                })?;
                let touched = match mode {
                    LoftMode::NewBody => apply_tool(&mut st, None, tool, BooleanOp::Fuse, self.next_id)?,
                    LoftMode::Add => {
                        let t = target.ok_or_else(|| {
                            KernelError::Invalid(
                                "There is no body to add to: the profiles are not on a body".into(),
                            )
                        })?;
                        apply_tool(&mut st, Some(t), tool, BooleanOp::Fuse, self.next_id)?
                    }
                    LoftMode::Cut => {
                        let ids: Vec<BodyId> =
                            target.into_iter().chain(st.bodies.iter().map(|b| b.id)).collect();
                        let mut done = None;
                        let mut last =
                            DocError::Kernel(KernelError::Invalid("There is no body to cut from".into()));
                        for b in ids {
                            match apply_tool(&mut st, Some(b), tool.clone(), BooleanOp::Cut, self.next_id) {
                                Ok(t) => {
                                    done = Some(t);
                                    break;
                                }
                                Err(e) => last = e,
                            }
                        }
                        done.ok_or(last)?
                    }
                };
                // the curves of the used regions are consumed
                for (sk, ents) in used {
                    if let Some(i) = st.sketches.iter().position(|s| s.id == sk) {
                        st.sketches[i].sketch.remove(&ents);
                        if st.sketches[i].sketch.is_empty() {
                            st.sketches.remove(i);
                        }
                    }
                }
                let mut out = self.done(st, "Loft");
                out.touched_body = Some(touched.0);
                out.history = touched.1.map(|h| (touched.0, h));
                Ok(out)
            }
            Command::AddCurve { def } => {
                def.path()?; // the curve must be buildable
                st.curve_counter += 1;
                let id = self.next_id;
                st.curves.push(CurveDoc {
                    id,
                    name: format!("Curve {}", st.curve_counter),
                    def: def.clone(),
                    visible: true,
                });
                let mut c = self.done(st, "Add curve");
                c.created_curve = Some(id);
                Ok(c)
            }
            Command::DeleteCurve { curve } => {
                let n = st.curves.len();
                st.curves.retain(|c| c.id != *curve);
                if st.curves.len() == n {
                    return Err(DocError::NoSuchCurve);
                }
                Ok(self.done(st, "Delete curve"))
            }
            Command::MoveCurves {
                curves,
                translation,
                pivot,
                axis,
                angle,
            } => {
                if curves.is_empty() {
                    return Err(KernelError::Invalid("no curve is selected".into()).into());
                }
                for id in curves {
                    let c = st
                        .curves
                        .iter_mut()
                        .find(|c| c.id == *id)
                        .ok_or(DocError::NoSuchCurve)?;
                    c.def = c.def.moved(*translation, *pivot, *axis, *angle);
                }
                Ok(self.done(st, "Move"))
            }
            Command::Sweep {
                profile,
                path,
                mode,
                rounded,
            } => {
                // the profile
                let mut owned: Option<(Profile, PlaneDef)> = None;
                let mut face_input: Option<(BodyId, Option<FaceId>, u32)> = None;
                let mut used: Option<(SketchId, Vec<rolidium_sketch::EntityId>)> = None;
                let target: Option<BodyId>;
                match *profile {
                    LoftRef::Region { sketch, region } => {
                        let sd = st
                            .sketches
                            .iter()
                            .find(|s| s.id == sketch)
                            .ok_or(DocError::NoSuchSketch)?;
                        let regions = sd.sketch.regions();
                        let reg = regions.get(region).ok_or(DocError::NoSuchRegion)?;
                        owned = Some((region_profile(reg), sd.plane));
                        used = Some((sketch, reg.exclusive_entities.clone()));
                        target = sd.target;
                    }
                    LoftRef::Face { body, face } => {
                        if !st.bodies.iter().any(|b| b.id == body) {
                            return Err(DocError::NoSuchBody);
                        }
                        face_input = Some((body, Some(face), 0));
                        target = Some(body);
                    }
                    LoftRef::EdgeLoop { body, edge } => {
                        if !st.bodies.iter().any(|b| b.id == body) {
                            return Err(DocError::NoSuchBody);
                        }
                        face_input = Some((body, None, edge));
                        target = Some(body);
                    }
                }
                // the path
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
                        let b = st
                            .bodies
                            .iter()
                            .find(|b| b.id == *body)
                            .ok_or(DocError::NoSuchBody)?;
                        let ids: Vec<rolidium_kernel::EdgeId> =
                            edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
                        rolidium_kernel::Path::build(rolidium_kernel::PathSource::Edges {
                            solid: &b.solid,
                            edges: &ids,
                        })?
                    }
                };
                let input = match (&owned, &face_input) {
                    (Some((pr, pl)), _) => rolidium_kernel::LoftInput::Profile {
                        profile: pr,
                        origin: pl.origin,
                        normal: pl.normal,
                        x_dir: pl.x_dir,
                    },
                    (_, Some((b, f, e))) => {
                        let solid = &st
                            .bodies
                            .iter()
                            .find(|x| x.id == *b)
                            .ok_or(DocError::NoSuchBody)?
                            .solid;
                        match f {
                            Some(face) => rolidium_kernel::LoftInput::Face { solid, face: *face },
                            None => rolidium_kernel::LoftInput::Edge {
                                solid,
                                edge: rolidium_kernel::EdgeId(*e),
                            },
                        }
                    }
                    _ => return Err(KernelError::Invalid("There is no profile".into()).into()),
                };
                let swept = Solid::sweep(
                    &input,
                    &kpath,
                    &rolidium_kernel::SweepOptions { rounded: *rounded },
                )?;
                let tool = swept.solid;
                let mut notice = None;
                if swept
                    .warnings
                    .contains(&rolidium_kernel::SweepWarning::ProfileNotAtPath)
                {
                    notice = Some(
                        "The profile does not lie at the start of the path; the sweep was made anyway"
                            .to_string(),
                    );
                } else if swept
                    .warnings
                    .contains(&rolidium_kernel::SweepWarning::ProfileNotPerpendicular)
                {
                    notice = Some("The profile is not perpendicular to the path where it starts; the sweep was made anyway".to_string());
                }
                let effective = match mode {
                    SweepMode::Auto if target.is_some() => SweepMode::Add,
                    SweepMode::Auto => SweepMode::NewBody,
                    m => *m,
                };
                let touched = match effective {
                    SweepMode::Add => {
                        let t = target.ok_or_else(|| {
                            KernelError::Invalid(
                                "There is no body to add to: the profile is not on a body".into(),
                            )
                        })?;
                        apply_tool(&mut st, Some(t), tool, BooleanOp::Fuse, self.next_id)?
                    }
                    SweepMode::Cut => {
                        let ids: Vec<BodyId> =
                            target.into_iter().chain(st.bodies.iter().map(|b| b.id)).collect();
                        let mut done = None;
                        let mut last =
                            DocError::Kernel(KernelError::Invalid("There is no body to cut from".into()));
                        for b in ids {
                            match apply_tool(&mut st, Some(b), tool.clone(), BooleanOp::Cut, self.next_id) {
                                Ok(t) => {
                                    done = Some(t);
                                    break;
                                }
                                Err(e) => last = e,
                            }
                        }
                        done.ok_or(last)?
                    }
                    _ => apply_tool(&mut st, None, tool, BooleanOp::Fuse, self.next_id)?,
                };
                if let Some((sk, ents)) = used {
                    if let Some(i) = st.sketches.iter().position(|s| s.id == sk) {
                        st.sketches[i].sketch.remove(&ents);
                        if st.sketches[i].sketch.is_empty() {
                            st.sketches.remove(i);
                        }
                    }
                }
                let mut out = self.done(st, "Sweep");
                out.touched_body = Some(touched.0);
                out.history = touched.1.map(|h| (touched.0, h));
                out.notice = notice;
                Ok(out)
            }
            Command::AddSolids { solids } => {
                if solids.is_empty() {
                    return Err(KernelError::Invalid("there is nothing to add".into()).into());
                }
                let mut first = None;
                for (i, solid) in solids.iter().enumerate() {
                    st.body_counter += 1;
                    let id = self.next_id + i as u64;
                    first.get_or_insert(id);
                    st.bodies.push(Body {
                        id,
                        name: format!("Body {}", st.body_counter),
                        solid: solid.clone(),
                        color: [0.72, 0.76, 0.82],
                        visible: true,
                        features: Vec::new(),
                        material: None,
                    });
                }
                let mut c = self.done(st, "Import STEP");
                c.touched_body = first;
                Ok(c)
            }
            Command::SetMaterial { body, material } => {
                if let Some(m) = material {
                    if !(m.density.is_finite() && m.density > 0.0 && m.density < 100.0) {
                        return Err(KernelError::Invalid(
                            "the density must be a positive number (g/cm³)".into(),
                        )
                        .into());
                    }
                }
                st.bodies
                    .iter_mut()
                    .find(|b| b.id == *body)
                    .ok_or(DocError::NoSuchBody)?
                    .material = material.clone();
                Ok(self.done(st, "Material"))
            }
            Command::DeleteBody { body } => {
                let n = st.bodies.len();
                st.bodies.retain(|b| b.id != *body);
                if st.bodies.len() == n {
                    return Err(DocError::NoSuchBody);
                }
                for s in st.sketches.iter_mut().filter(|s| s.target == Some(*body)) {
                    s.target = None; // the sketch stays, pulling it now makes a new body
                }
                Ok(self.done(st, "Delete body"))
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Document {
    current: Arc<State>,
    undo: Vec<(Arc<State>, String)>,
    redo: Vec<(Arc<State>, String)>,
    next_id: u64,
    revision: u64,
    /// The state that is on disk (or the empty start state); `None` after loading a recovery file.
    saved: Option<Arc<State>>,
    /// File content this program does not know; written back unchanged on save.
    pub extras: file::Extras,
}

impl Document {
    pub fn new() -> Self {
        let mut d = Document {
            next_id: 1,
            ..Default::default()
        };
        d.saved = Some(d.current.clone());
        d
    }

    /// True if the model differs from what was last saved or opened.
    pub fn is_dirty(&self) -> bool {
        !self.saved.as_ref().is_some_and(|s| Arc::ptr_eq(s, &self.current))
    }

    /// Declares the current state as saved.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.current.clone());
    }

    /// Declares that `state` (a snapshot taken when saving started) is what is on disk; the document stays
    /// changed if it moved on meanwhile.
    pub fn mark_saved_state(&mut self, state: Arc<State>) {
        self.saved = Some(state);
    }

    /// Forgets that the current state is saved (a recovered document counts as changed).
    pub fn mark_dirty(&mut self) {
        self.saved = None;
    }

    pub fn state(&self) -> &Arc<State> {
        &self.current
    }
    pub fn bodies(&self) -> &[Body] {
        &self.current.bodies
    }
    pub fn curves(&self) -> &[CurveDoc] {
        &self.current.curves
    }

    pub fn curve(&self, id: CurveId) -> Option<&CurveDoc> {
        self.current.curves.iter().find(|c| c.id == id)
    }

    pub fn sketches(&self) -> &[SketchDoc] {
        &self.current.sketches
    }
    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.current.bodies.iter().find(|b| b.id == id)
    }
    pub fn sketch(&self, id: SketchId) -> Option<&SketchDoc> {
        self.current.sketches.iter().find(|s| s.id == id)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(_, l)| l.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(_, l)| l.as_str())
    }

    /// Prepares a command for (possibly off-thread) execution.
    pub fn plan(&self, command: Command) -> Job {
        Job {
            command,
            base: self.current.clone(),
            base_revision: self.revision,
            next_id: self.next_id,
        }
    }

    /// Applies a computed result. Fails with [`DocError::Stale`] if the document changed meanwhile.
    pub fn commit(&mut self, c: Computed) -> Result<(), DocError> {
        if c.base_revision != self.revision {
            return Err(DocError::Stale);
        }
        let max_id = c
            .state
            .bodies
            .iter()
            .map(|b| b.id)
            .chain(c.state.sketches.iter().map(|s| s.id))
            .chain(c.state.curves.iter().map(|s| s.id))
            .max()
            .unwrap_or(0);
        self.undo
            .push((std::mem::replace(&mut self.current, Arc::new(c.state)), c.label));
        self.redo.clear();
        self.next_id = self.next_id.max(max_id + 1);
        self.revision += 1;
        Ok(())
    }

    /// Synchronous convenience: plan, run and commit.
    pub fn apply(&mut self, command: Command) -> Result<Computed, DocError> {
        let c = self.plan(command).run()?;
        self.commit(c.clone())?;
        Ok(c)
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some((prev, label)) => {
                self.redo
                    .push((std::mem::replace(&mut self.current, prev), label));
                self.revision += 1;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some((next, label)) => {
                self.undo
                    .push((std::mem::replace(&mut self.current, next), label));
                self.revision += 1;
                true
            }
            None => false,
        }
    }
}

/// What Combine says when the kernel gave a result that cannot be right (or none, although the bodies overlap).
const CALC_FAILED: &str = "The result of the operation does not add up (its volume is impossible for these bodies), so nothing was changed. Try moving one body by a hundredth of a millimetre, or combine the bodies in another order";

/// Safety net for Combine: the volume of the result must be possible for the volumes of the inputs.
/// A merge is at least the larger input and at most the sum; a cut is at most the target (and positive);
/// an intersection is at most the smaller input (and positive).
pub fn combine_volume_ok(op: BooleanOp, target: f64, tool: f64, result: f64) -> bool {
    let tol = 1e-5 * (target + tool).max(1e-3);
    if !result.is_finite() {
        return false;
    }
    match op {
        BooleanOp::Fuse => result >= target.max(tool) - tol && result <= target + tool + tol,
        BooleanOp::Cut => result > 0.0 && result <= target + tol,
        BooleanOp::Common => result > 0.0 && result <= target.min(tool) + tol,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_box() -> (Document, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        (d, id)
    }

    fn top(d: &Document, id: BodyId) -> FaceId {
        let s = &d.body(id).unwrap().solid;
        s.faces()
            .find(|f| s.face_info(*f).unwrap().normal.z > 0.99)
            .unwrap()
    }

    #[test]
    fn pull_undo_redo() {
        let (mut d, id) = doc_with_box();
        let f = top(&d, id);
        d.apply(Command::PullFace {
            body: id,
            face: f,
            distance: 10.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 36000.0).abs() < 1e-6);
        assert!(d.undo());
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 24000.0).abs() < 1e-6);
        assert!(d.redo());
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 36000.0).abs() < 1e-6);
        assert!(d.undo() && d.undo());
        assert!(!d.undo());
        assert!(d.bodies().is_empty());
    }

    #[test]
    fn many_undo_steps_unbounded() {
        let (mut d, id) = doc_with_box();
        for i in 0..30 {
            let f = top(&d, id);
            let dist = if i % 2 == 0 { 3.0 } else { -2.0 };
            d.apply(Command::PullFace {
                body: id,
                face: f,
                distance: dist,
                mode: PullMode::Auto,
            })
            .unwrap();
        }
        let mut n = 0;
        while d.undo() {
            n += 1;
        }
        assert_eq!(n, 31);
    }

    #[test]
    fn failed_command_leaves_document_unchanged() {
        let (mut d, id) = doc_with_box();
        let rev = d.revision();
        let f = top(&d, id);
        assert!(d
            .apply(Command::PullFace {
                body: id,
                face: f,
                distance: -50.0,
                mode: PullMode::Auto,
            })
            .is_err());
        assert!(d
            .apply(Command::PullFace {
                body: 999,
                face: f,
                distance: 1.0,
                mode: PullMode::Auto,
            })
            .is_err());
        assert_eq!(d.revision(), rev);
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 24000.0).abs() < 1e-6);
    }

    #[test]
    fn stale_result_is_rejected() {
        let (mut d, id) = doc_with_box();
        let f = top(&d, id);
        let job = d.plan(Command::PullFace {
            body: id,
            face: f,
            distance: 5.0,
            mode: PullMode::Auto,
        });
        d.apply(Command::AddBox {
            name: "B2".into(),
            min: DVec3::splat(100.0),
            max: DVec3::splat(110.0),
        })
        .unwrap();
        let c = job.run().unwrap();
        assert!(matches!(d.commit(c), Err(DocError::Stale)));
    }

    #[test]
    fn worker_thread_roundtrip() {
        let (mut d, id) = doc_with_box();
        let f = top(&d, id);
        let job = d.plan(Command::PullFace {
            body: id,
            face: f,
            distance: 5.0,
            mode: PullMode::Auto,
        });
        let c = std::thread::spawn(move || job.run()).join().unwrap().unwrap();
        d.commit(c).unwrap();
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 30000.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod sketch_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    fn doc_box() -> (Document, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        (d, id)
    }

    fn top_plane() -> PlaneDef {
        PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z)
    }

    fn circle_on_top(d: &mut Document, body: BodyId) -> SketchId {
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: top_plane(),
                    body: Some(body),
                },
                entity: Entity::Circle {
                    center: DVec2::new(20.0, 15.0),
                    radius: 5.0,
                },
            })
            .unwrap();
        c.created_sketch.unwrap()
    }

    #[test]
    fn reference_part_through_hole() {
        let (mut d, id) = doc_box();
        let sk = circle_on_top(&mut d, id);
        let t = std::time::Instant::now();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -20.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        eprintln!("pull from region: {:?}", t.elapsed());
        let v = d.body(id).unwrap().solid.volume().unwrap();
        let expect = 24000.0 - std::f64::consts::PI * 25.0 * 20.0;
        assert!((v - expect).abs() < 1e-6, "{v} vs {expect}");
        assert!((v - 22429.2).abs() < 0.1);
        assert!(d.body(id).unwrap().solid.is_valid());
        assert!(
            d.sketches().is_empty(),
            "used-up curves and the empty sketch disappear"
        );
        // undo brings back the sketch and the solid box
        assert!(d.undo());
        assert_eq!(d.sketches().len(), 1);
        assert!((d.body(id).unwrap().solid.volume().unwrap() - 24000.0).abs() < 1e-6);
        // undo drawing, redo drawing
        assert!(d.undo() && d.sketches().is_empty());
        assert!(d.redo() && d.sketches().len() == 1);
    }

    #[test]
    fn blind_pocket_and_boss() {
        let (mut d, id) = doc_box();
        let sk = circle_on_top(&mut d, id);
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: 8.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let boss = std::f64::consts::PI * 25.0 * 8.0;
        assert!((d.body(id).unwrap().solid.volume().unwrap() - (24000.0 + boss)).abs() < 1e-6);
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::from_face(DVec3::new(0.0, 0.0, 28.0), DVec3::Z),
                    body: Some(id),
                },
                entity: Entity::Circle {
                    center: DVec2::new(20.0, 15.0),
                    radius: 3.0,
                },
            })
            .unwrap();
        let sk = c.created_sketch.unwrap();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -3.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let v = d.body(id).unwrap().solid.volume().unwrap();
        assert!(
            (v - (24000.0 + boss - std::f64::consts::PI * 9.0 * 3.0)).abs() < 1e-6,
            "{v}"
        );
    }

    #[test]
    fn base_plane_region_makes_a_new_body() {
        let mut d = Document::new();
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::base(),
                    body: None,
                },
                entity: Entity::Rect {
                    a: DVec2::ZERO,
                    b: DVec2::new(40.0, 30.0),
                },
            })
            .unwrap();
        let sk = c.created_sketch.unwrap();
        d.apply(Command::AddEntity {
            target: SketchTarget::Existing(sk),
            entity: Entity::Circle {
                center: DVec2::new(20.0, 15.0),
                radius: 5.0,
            },
        })
        .unwrap();
        // regions: ring and disc; pull the ring (the one with a hole)
        let regs = d.sketch(sk).unwrap().sketch.regions();
        let ring = regs.iter().position(|r| !r.holes.is_empty()).unwrap();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: ring,
            distance: 10.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        assert_eq!(d.bodies().len(), 1);
        let v = d.bodies()[0].solid.volume().unwrap();
        assert!((v - (1200.0 - std::f64::consts::PI * 25.0) * 10.0).abs() < 1e-6);
        // the rectangle is used up, the circle still borders the disc and stays
        let left = d.sketch(sk).unwrap();
        assert_eq!(left.sketch.entities().len(), 1);
        assert_eq!(left.sketch.regions().len(), 1);
    }

    #[test]
    fn region_failures_leave_document_unchanged() {
        let (mut d, id) = doc_box();
        let sk = circle_on_top(&mut d, id);
        let rev = d.revision();
        assert!(d
            .apply(Command::PullRegion {
                sketch: sk,
                region: 5,
                distance: 1.0,
                mode: PullMode::Auto,
            })
            .is_err());
        assert!(d
            .apply(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: 0.0,
                mode: PullMode::Auto,
            })
            .is_err());
        assert!(d
            .apply(Command::PullRegion {
                sketch: 999,
                region: 0,
                distance: 1.0,
                mode: PullMode::Auto,
            })
            .is_err());
        assert!(d
            .apply(Command::AddEntity {
                target: SketchTarget::Existing(sk),
                entity: Entity::Line {
                    a: DVec2::ZERO,
                    b: DVec2::ZERO
                }
            })
            .is_err());
        assert_eq!(d.revision(), rev);
        assert_eq!(d.sketches().len(), 1);
    }
}

#[cfg(test)]
mod phase1b_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    fn doc_box() -> (Document, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        (d, id)
    }

    #[test]
    fn numbering_series_and_empty_sketches_vanish() {
        let (mut d, id) = doc_box();
        assert_eq!(d.bodies()[0].name, "Body 1");
        let plane = PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
        let mut ids = vec![];
        for r in [3.0, 4.0] {
            let c = d
                .apply(Command::AddEntity {
                    target: SketchTarget::New {
                        plane,
                        body: Some(id),
                    },
                    entity: Entity::Circle {
                        center: DVec2::new(10.0, 10.0),
                        radius: r,
                    },
                })
                .unwrap();
            ids.push(c.created_sketch.unwrap());
        }
        assert_eq!(d.sketches()[0].name, "Sketch 1");
        assert_eq!(d.sketches()[1].name, "Sketch 2");
        let ent = d.sketches()[0].sketch.entities()[0].0;
        d.apply(Command::DeleteEntity {
            sketch: ids[0],
            entity: ent,
        })
        .unwrap();
        assert_eq!(d.sketches().len(), 1, "a sketch without shapes disappears");
        assert!(d.undo());
        assert_eq!(d.sketches().len(), 2);
        d.apply(Command::DeleteBody { body: id }).unwrap();
        assert!(d.bodies().is_empty());
        assert!(d.sketches().iter().all(|s| s.target.is_none()));
        assert!(d.undo());
        assert_eq!(d.bodies().len(), 1);
    }

    #[test]
    fn replace_entity_and_commands_on_solids() {
        let (mut d, id) = doc_box();
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z),
                    body: Some(id),
                },
                entity: Entity::Circle {
                    center: DVec2::new(20.0, 15.0),
                    radius: 5.0,
                },
            })
            .unwrap();
        let sk = c.created_sketch.unwrap();
        let ent = d.sketch(sk).unwrap().sketch.entities()[0].0;
        d.apply(Command::ReplaceEntity {
            sketch: sk,
            entity: ent,
            new: Entity::Circle {
                center: DVec2::new(20.0, 15.0),
                radius: 6.0,
            },
        })
        .unwrap();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -20.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let s = &d.body(id).unwrap().solid;
        assert!((s.volume().unwrap() - (24000.0 - std::f64::consts::PI * 36.0 * 20.0)).abs() < 1e-6);
        let cyl = s
            .faces()
            .find(|f| s.face_info(*f).unwrap().kind == rolidium_kernel::FaceKind::Cylinder)
            .unwrap();
        d.apply(
            Command::SetCylinderDiameter {
                body: id,
                face: cyl,
                diameter: 12.0,
            }
            .clone(),
        )
        .unwrap_err();
        d.apply(Command::SetCylinderDiameter {
            body: id,
            face: cyl,
            diameter: 10.0,
        })
        .unwrap();
        let v = d.body(id).unwrap().solid.volume().unwrap();
        assert!((v - (24000.0 - std::f64::consts::PI * 25.0 * 20.0)).abs() < 1e-6);
        // fillet/chamfer through the document; failures keep the document unchanged
        let s = d.body(id).unwrap().solid.clone();
        let e = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.length > 19.0
                    && i.start.x < 1.0
                    && i.start.y < 1.0
            })
            .unwrap();
        let rev = d.revision();
        assert!(d
            .apply(Command::FilletEdges {
                body: id,
                edges: vec![e.0],
                size: 500.0,
                chamfer: false
            })
            .is_err());
        assert_eq!(d.revision(), rev);
        d.apply(Command::FilletEdges {
            body: id,
            edges: vec![e.0],
            size: 5.0,
            chamfer: true,
        })
        .unwrap();
        let v2 = d.body(id).unwrap().solid.volume().unwrap();
        assert!((v - v2 - 250.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod blend_tests {
    use super::*;

    fn doc_box() -> (Document, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        (d, id)
    }

    fn vol(d: &Document, id: BodyId) -> f64 {
        d.body(id).unwrap().solid.volume().unwrap()
    }

    fn corner_edge(d: &Document, id: BodyId) -> u32 {
        let s = &d.body(id).unwrap().solid;
        s.edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.start.x > 39.0
                    && i.start.y > 29.0
            })
            .unwrap()
            .0
    }

    fn top(d: &Document, id: BodyId) -> FaceId {
        let s = &d.body(id).unwrap().solid;
        s.faces()
            .find(|f| s.face_info(*f).unwrap().normal.z > 0.99)
            .unwrap()
    }

    fn round(d: &mut Document, id: BodyId, size: f64, chamfer: bool) {
        let e = corner_edge(d, id);
        d.apply(Command::FilletEdges {
            body: id,
            edges: vec![e],
            size,
            chamfer,
        })
        .unwrap();
    }

    fn edit(d: &mut Document, id: BodyId, size: f64) -> Result<Computed, DocError> {
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditBlend { body: id, face, size })
    }

    #[test]
    fn edit_round_repeatedly_and_remove() {
        let (mut d, id) = doc_box();
        round(&mut d, id, 5.0, false);
        assert_eq!(d.body(id).unwrap().features.len(), 1);
        edit(&mut d, id, 8.0).unwrap();
        assert!((vol(&d, id) - 23725.31).abs() < 0.01);
        assert_eq!(d.body(id).unwrap().features[0].size, 8.0);
        edit(&mut d, id, 3.0).unwrap(); // again, smaller
        assert!((vol(&d, id) - (24000.0 - 20.0 * (1.0 - std::f64::consts::PI / 4.0) * 9.0)).abs() < 0.01);
        edit(&mut d, id, 0.0).unwrap();
        assert!((vol(&d, id) - 24000.0).abs() < 1e-6);
        assert!(d.body(id).unwrap().features.is_empty());
        // Undo goes back through every step.
        assert!(d.undo());
        assert_eq!(d.body(id).unwrap().features.len(), 1);
    }

    #[test]
    fn edit_chamfer_values() {
        let (mut d, id) = doc_box();
        round(&mut d, id, 5.0, true);
        edit(&mut d, id, 3.0).unwrap();
        assert!((vol(&d, id) - 23910.0).abs() < 1e-6);
        edit(&mut d, id, 0.0).unwrap();
        assert!((vol(&d, id) - 24000.0).abs() < 1e-6);
    }

    #[test]
    fn marking_survives_other_edits_and_failure_changes_nothing() {
        let (mut d, id) = doc_box();
        round(&mut d, id, 5.0, false);
        let f = top(&d, id);
        d.apply(Command::PullFace {
            body: id,
            face: f,
            distance: 10.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        assert_eq!(d.body(id).unwrap().features.len(), 1, "round is still recognised");
        let rev = d.revision();
        let v = vol(&d, id);
        let err = edit(&mut d, id, 500.0).unwrap_err();
        assert!(!err.to_string().is_empty());
        assert_eq!(d.revision(), rev);
        assert_eq!(vol(&d, id), v);
        edit(&mut d, id, 8.0).unwrap();
        assert!(vol(&d, id) < v);
        // A face that is no feature is refused.
        let f = top(&d, id);
        assert!(d
            .apply(Command::EditBlend {
                body: id,
                face: f,
                size: 2.0
            })
            .is_err());
    }

    #[test]
    fn pull_modes_force_the_direction() {
        let (mut d, id) = doc_box();
        let f = top(&d, id);
        // Cut with a positive (outward) distance still removes material.
        d.apply(Command::PullFace {
            body: id,
            face: f,
            distance: 5.0,
            mode: PullMode::Cut,
        })
        .unwrap();
        assert!((vol(&d, id) - 40.0 * 30.0 * 15.0).abs() < 1e-6);
        // Add with a negative distance adds.
        let f = top(&d, id);
        d.apply(Command::PullFace {
            body: id,
            face: f,
            distance: -3.0,
            mode: PullMode::Add,
        })
        .unwrap();
        assert!((vol(&d, id) - 40.0 * 30.0 * 18.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod detached_region_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    fn setup() -> (Document, BodyId, SketchId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z),
                    body: Some(id),
                },
                entity: Entity::Circle {
                    center: DVec2::new(60.0, 15.0),
                    radius: 5.0,
                },
            })
            .unwrap();
        (d, id, c.created_sketch.unwrap())
    }

    #[test]
    fn region_beside_the_body_becomes_a_new_body() {
        for mode in [PullMode::Auto, PullMode::Add] {
            let (mut d, id, sk) = setup();
            d.apply(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: 10.0,
                mode,
            })
            .unwrap();
            assert_eq!(d.bodies().len(), 2);
            assert!((d.body(id).unwrap().solid.volume().unwrap() - 24000.0).abs() < 1e-6);
            let v = d.bodies()[1].solid.volume().unwrap();
            assert!((v - std::f64::consts::PI * 25.0 * 10.0).abs() < 1e-6);
            assert!(d.sketches().is_empty(), "the used-up sketch is gone");
        }
    }

    #[test]
    fn cut_without_overlap_says_there_is_nothing_to_remove() {
        let (mut d, _, sk) = setup();
        let rev = d.revision();
        let e = d
            .apply(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: -10.0,
                mode: PullMode::Cut,
            })
            .unwrap_err();
        assert!(e.to_string().contains("nothing to remove"), "{e}");
        assert_eq!(d.revision(), rev);
        assert_eq!(d.bodies().len(), 1);
    }
}

#[cfg(test)]
mod neighbour_blend_tests {
    use super::*;

    fn edge(s: &Solid, f: impl Fn(&rolidium_kernel::EdgeInfo) -> bool) -> u32 {
        s.edges().find(|e| f(&s.edge_info(*e).unwrap())).unwrap().0
    }

    #[test]
    fn round_next_to_rounds_leaves_other_edges_sharp_and_all_stay_editable() {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "C".into(),
            min: DVec3::ZERO,
            max: DVec3::splat(20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let s0 = d.body(id).unwrap().solid.clone();
        let front = edge(&s0, |i| {
            (i.end - i.start).x.abs() > 1.0 && i.start.y < 1.0 && i.start.z > 19.9
        });
        let back = edge(&s0, |i| {
            (i.end - i.start).x.abs() > 1.0 && i.start.y > 19.0 && i.start.z > 19.9
        });
        let left = edge(&s0, |i| {
            (i.end - i.start).y.abs() > 1.0 && i.start.x < 1.0 && i.start.z > 19.9
        });
        // reference: the three edges at once
        let mut r = Document::new();
        r.apply(Command::AddBox {
            name: "C".into(),
            min: DVec3::ZERO,
            max: DVec3::splat(20.0),
        })
        .unwrap();
        r.apply(Command::FilletEdges {
            body: r.bodies()[0].id,
            edges: vec![front, back, left],
            size: 3.0,
            chamfer: false,
        })
        .unwrap();
        let want = r.bodies()[0].solid.volume().unwrap();
        // two rounds, then the edge between them
        d.apply(Command::FilletEdges {
            body: id,
            edges: vec![front, back],
            size: 3.0,
            chamfer: false,
        })
        .unwrap();
        let s1 = d.body(id).unwrap().solid.clone();
        let mid = edge(&s1, |i| {
            (i.end - i.start).y.abs() > 1.0 && i.start.x < 1.0 && i.start.z > 19.9
        });
        let c = d
            .apply(Command::FilletEdges {
                body: id,
                edges: vec![mid],
                size: 3.0,
                chamfer: false,
            })
            .unwrap();
        assert!(c.notice.is_none());
        let b = d.body(id).unwrap();
        assert!((b.solid.volume().unwrap() - want).abs() < 1e-3);
        assert_eq!(
            b.features.len(),
            2,
            "the old round and the new one are separate features"
        );
        // the new one can be changed and removed on its own
        let new_face = b.features.iter().find(|f| f.faces.len() < 3).unwrap().faces[0];
        let before = d.body(id).unwrap().solid.volume().unwrap();
        d.apply(Command::EditBlend {
            body: id,
            face: new_face,
            size: 0.0,
        })
        .unwrap();
        assert!(d.body(id).unwrap().solid.volume().unwrap() > before);
        assert_eq!(d.body(id).unwrap().features.len(), 1);
        // one undo step for the whole combined operation
        d.undo();
        d.undo();
        assert_eq!(d.body(id).unwrap().features.len(), 1);
        assert!(
            (d.body(id).unwrap().solid.volume().unwrap()
                - (8000.0 - 2.0 * 9.0 * (1.0 - std::f64::consts::PI / 4.0) * 20.0))
                .abs()
                < 1e-3
        );
    }
}

#[cfg(test)]
mod phase1g_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    fn doc_box() -> (Document, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        (d, id)
    }

    fn vol(d: &Document, id: BodyId) -> f64 {
        d.body(id).unwrap().solid.volume().unwrap()
    }

    #[test]
    fn move_rotate_copy_and_sketches_travel() {
        let (mut d, id) = doc_box();
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z),
                    body: Some(id),
                },
                entity: Entity::Circle {
                    center: DVec2::new(20.0, 15.0),
                    radius: 5.0,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        let t = DVec3::new(100.0, 0.0, 5.0);
        d.apply(Command::Move {
            bodies: vec![id],
            translation: t,
            pivot: DVec3::ZERO,
            axis: DVec3::Z,
            angle: 0.0,
            copy: false,
        })
        .unwrap();
        let p = d.body(id).unwrap().solid.props().unwrap();
        assert!((p.min - t).length() < 1e-9);
        assert!((vol(&d, id) - 24000.0).abs() < 1e-6);
        // the sketch moved with the body: its plane is now at z = 25 and the region still pulls
        let plane = d.sketch(sk).unwrap().plane;
        assert!((plane.origin.z - 25.0).abs() < 1e-9 && plane.normal.z > 0.999);
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -25.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        assert!((vol(&d, id) - (24000.0 - std::f64::consts::PI * 25.0 * 20.0)).abs() < 1e-6);
        // rotate 90 degrees about Z through the centre: X and Y sizes swap
        let c = d.body(id).unwrap().solid.props().unwrap().centroid;
        d.apply(Command::Move {
            bodies: vec![id],
            translation: DVec3::ZERO,
            pivot: c,
            axis: DVec3::Z,
            angle: std::f64::consts::FRAC_PI_2,
            copy: false,
        })
        .unwrap();
        let p = d.body(id).unwrap().solid.props().unwrap();
        assert!((p.max.x - p.min.x - 30.0).abs() < 1e-6 && (p.max.y - p.min.y - 40.0).abs() < 1e-6);
        // copy: two bodies with the same volume, original stays
        let v = vol(&d, id);
        d.apply(Command::Move {
            bodies: vec![id],
            translation: DVec3::new(0.0, 100.0, 0.0),
            pivot: DVec3::ZERO,
            axis: DVec3::Z,
            angle: 0.0,
            copy: true,
        })
        .unwrap();
        assert_eq!(d.bodies().len(), 2);
        assert!((vol(&d, d.bodies()[1].id) - v).abs() < 1e-6 && (vol(&d, id) - v).abs() < 1e-6);
        // one undo step each
        assert!(d.undo());
        assert_eq!(d.bodies().len(), 1);
    }

    #[test]
    fn rounds_stay_editable_after_a_move() {
        let (mut d, id) = doc_box();
        let s = d.body(id).unwrap().solid.clone();
        let e = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.start.x > 39.0
                    && i.start.y > 29.0
            })
            .unwrap()
            .0;
        d.apply(Command::FilletEdges {
            body: id,
            edges: vec![e],
            size: 5.0,
            chamfer: false,
        })
        .unwrap();
        d.apply(Command::Move {
            bodies: vec![id],
            translation: DVec3::new(7.0, 8.0, 9.0),
            pivot: DVec3::ZERO,
            axis: DVec3::Z,
            angle: 0.0,
            copy: false,
        })
        .unwrap();
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditBlend {
            body: id,
            face,
            size: 8.0,
        })
        .unwrap();
        assert!((vol(&d, id) - 23725.31).abs() < 0.01);
    }

    #[test]
    fn delete_region_shapes_sketch_and_visibility() {
        let (mut d, id) = doc_box();
        let plane = PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane,
                    body: Some(id),
                },
                entity: Entity::Rect {
                    a: DVec2::new(2.0, 2.0),
                    b: DVec2::new(12.0, 12.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        // a second shape overlapping the first shares no whole boundary
        d.apply(Command::AddEntity {
            target: SketchTarget::Existing(sk),
            entity: Entity::Circle {
                center: DVec2::new(30.0, 20.0),
                radius: 4.0,
            },
        })
        .unwrap();
        let regions = d.sketch(sk).unwrap().sketch.regions();
        assert_eq!(regions.len(), 2);
        d.apply(Command::DeleteRegionShapes {
            sketch: sk,
            region: 0,
        })
        .unwrap();
        assert_eq!(d.sketch(sk).unwrap().sketch.entities().len(), 1);
        d.apply(Command::DeleteSketch { sketch: sk }).unwrap();
        assert!(d.sketches().is_empty());
        assert!(d.undo() && d.undo());
        assert_eq!(d.sketch(sk).unwrap().sketch.entities().len(), 2);
        d.apply(Command::SetVisible {
            target: ItemId::Body(id),
            visible: false,
        })
        .unwrap();
        assert!(!d.body(id).unwrap().visible);
        d.apply(Command::SetVisible {
            target: ItemId::Sketch(sk),
            visible: false,
        })
        .unwrap();
        d.apply(Command::Rename {
            target: ItemId::Body(id),
            name: "Plate".into(),
        })
        .unwrap();
        assert_eq!(d.body(id).unwrap().name, "Plate");
        assert!(d
            .apply(Command::Rename {
                target: ItemId::Body(id),
                name: " ".into()
            })
            .is_err());
        // saved and reopened, visibility is kept
        let e = Document::from_loaded(
            file::load_from_bytes(&file::save_to_bytes(&d, None, "t").unwrap()).unwrap(),
        );
        assert!(!e.body(id).unwrap().visible && !e.sketch(sk).unwrap().visible);
    }

    #[test]
    fn region_with_only_shared_lines_cannot_be_deleted_by_region() {
        let (mut d, id) = doc_box();
        let plane = PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
        // a rectangle with a line across it: two regions that share the line, each with its own sides
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane,
                    body: Some(id),
                },
                entity: Entity::Rect {
                    a: DVec2::new(2.0, 2.0),
                    b: DVec2::new(12.0, 12.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::AddEntity {
            target: SketchTarget::Existing(sk),
            entity: Entity::Line {
                a: DVec2::new(7.0, 0.0),
                b: DVec2::new(7.0, 14.0),
            },
        })
        .unwrap();
        // the rectangle is one shape shared by both regions; the line divides them
        let n = d.sketch(sk).unwrap().sketch.regions().len();
        assert_eq!(n, 2);
        let r = d.apply(Command::DeleteRegionShapes {
            sketch: sk,
            region: 0,
        });
        assert!(r.is_err(), "no shape of its own: the user must pick the curve");
        assert!(r.unwrap_err().to_string().contains("select the line"));
    }

    fn revolve_doc() -> (Document, SketchId) {
        let mut d = Document::new();
        // plane XZ (x_dir X, y_dir Z), axis = Z axis
        let plane = PlaneDef {
            origin: DVec3::ZERO,
            normal: -DVec3::Y,
            x_dir: DVec3::X,
        };
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Rect {
                    a: DVec2::new(5.0, 0.0),
                    b: DVec2::new(15.0, 20.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        (d, sk)
    }

    fn revolve(d: &mut Document, sk: SketchId, angle_deg: f64, mode: PullMode) -> Result<Computed, DocError> {
        d.apply(Command::Revolve {
            sketch: sk,
            region: 0,
            axis_point: DVec3::ZERO,
            axis_dir: DVec3::Z,
            angle: angle_deg.to_radians(),
            mode,
        })
    }

    #[test]
    fn revolve_reference_volumes_and_axis_checks() {
        let (mut d, sk) = revolve_doc();
        revolve(&mut d, sk, 360.0, PullMode::Auto).unwrap();
        assert!((vol(&d, d.bodies()[0].id) - 12566.37).abs() < 0.01);
        let (mut d, sk) = revolve_doc();
        revolve(&mut d, sk, 90.0, PullMode::Auto).unwrap();
        assert!((vol(&d, d.bodies()[0].id) - 3141.59).abs() < 0.01);
        // touching the axis
        let mut d = Document::new();
        let plane = PlaneDef {
            origin: DVec3::ZERO,
            normal: -DVec3::Y,
            x_dir: DVec3::X,
        };
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Rect {
                    a: DVec2::new(0.0, 0.0),
                    b: DVec2::new(10.0, 20.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        revolve(&mut d, sk, 360.0, PullMode::Auto).unwrap();
        assert!((vol(&d, d.bodies()[0].id) - 6283.19).abs() < 0.01);
        // an axis through the profile is refused and nothing changes
        let mut d = Document::new();
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Rect {
                    a: DVec2::new(-5.0, 0.0),
                    b: DVec2::new(5.0, 20.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        let rev = d.revision();
        let e = revolve(&mut d, sk, 360.0, PullMode::Auto).unwrap_err();
        assert!(e.to_string().contains("passes through"), "{e}");
        assert_eq!(d.revision(), rev);
        // an axis outside the plane is refused too
        assert!(d
            .apply(Command::Revolve {
                sketch: sk,
                region: 0,
                axis_point: DVec3::new(0.0, 5.0, 0.0),
                axis_dir: DVec3::Z,
                angle: 1.0,
                mode: PullMode::Auto
            })
            .is_err());
    }

    #[test]
    fn revolve_cut_makes_a_groove() {
        let mut d = Document::new();
        // cylinder d30 h20 from a base-plane circle
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef::base(),
                    body: None,
                },
                entity: Entity::Circle {
                    center: DVec2::ZERO,
                    radius: 15.0,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::PullRegion {
            sketch: c,
            region: 0,
            distance: 20.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let id = d.bodies()[0].id;
        // groove 2 x 4: radius 13 to 15, height 8 to 12
        let plane = PlaneDef {
            origin: DVec3::ZERO,
            normal: -DVec3::Y,
            x_dir: DVec3::X,
        };
        let g = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Rect {
                    a: DVec2::new(13.0, 8.0),
                    b: DVec2::new(15.0, 12.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        revolve(&mut d, g, 360.0, PullMode::Cut).unwrap();
        assert_eq!(d.bodies().len(), 1);
        assert!((vol(&d, id) - 13433.45).abs() < 0.01, "{}", vol(&d, id));
        // Cut with nothing to cut says so
        let mut e = Document::new();
        let g = e
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Rect {
                    a: DVec2::new(13.0, 8.0),
                    b: DVec2::new(15.0, 12.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        assert!(revolve(&mut e, g, 360.0, PullMode::Cut).is_err());
    }

    #[test]
    fn revolve_a_face_about_its_edge() {
        let (mut d, id) = doc_box();
        let solid = d.body(id).unwrap().solid.clone();
        let right = solid
            .faces()
            .find(|f| {
                let i = solid.face_info(*f).unwrap();
                i.normal.x > 0.99
            })
            .unwrap();
        // the bottom edge of that face: along Y at x = 40, z = 0; the face is above it
        d.apply(Command::RevolveFace {
            body: id,
            face: right,
            axis_point: DVec3::new(40.0, 0.0, 0.0),
            axis_dir: DVec3::Y,
            angle: 90f64.to_radians(),
            mode: PullMode::Auto,
        })
        .unwrap();
        assert!((vol(&d, id) - (24000.0 + std::f64::consts::PI * 400.0 / 4.0 * 30.0)).abs() < 0.01);
        // an axis through the middle of the face is refused
        let (mut d, id) = doc_box();
        let solid = d.body(id).unwrap().solid.clone();
        let right = solid
            .faces()
            .find(|f| solid.face_info(*f).unwrap().normal.x > 0.99)
            .unwrap();
        let e = d.apply(Command::RevolveFace {
            body: id,
            face: right,
            axis_point: DVec3::new(40.0, 0.0, 10.0),
            axis_dir: DVec3::Y,
            angle: 1.0,
            mode: PullMode::Auto,
        });
        assert!(e.unwrap_err().to_string().contains("passes through"));
    }
}

#[cfg(test)]
mod phase1h_tests {
    use super::*;

    fn two_cubes() -> (Document, BodyId, BodyId) {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "A".into(),
            min: DVec3::ZERO,
            max: DVec3::splat(20.0),
        })
        .unwrap();
        d.apply(Command::AddBox {
            name: "B".into(),
            min: DVec3::new(10.0, 0.0, 0.0),
            max: DVec3::new(30.0, 20.0, 20.0),
        })
        .unwrap();
        (d.clone(), d.bodies()[0].id, d.bodies()[1].id)
    }

    fn total(d: &Document) -> f64 {
        d.bodies().iter().map(|b| b.solid.volume().unwrap()).sum()
    }

    #[test]
    fn combine_merge_cut_intersect_and_keep_tools() {
        for (op, keep, want, bodies) in [
            (CombineOp::Merge, false, 12000.0, 1),
            (CombineOp::Cut, false, 4000.0, 1),
            (CombineOp::Intersect, false, 4000.0, 1),
            (CombineOp::Cut, true, 4000.0 + 8000.0, 2),
        ] {
            let (mut d, a, b) = two_cubes();
            d.apply(Command::Combine {
                target: a,
                tools: vec![b],
                op,
                keep_tools: keep,
            })
            .unwrap();
            assert_eq!(d.bodies().len(), bodies, "{op:?} keep {keep}");
            assert!((total(&d) - want).abs() < 1e-6, "{op:?}: {}", total(&d));
            assert!(d.undo());
            assert_eq!(d.bodies().len(), 2);
        }
    }

    #[test]
    fn combine_refuses_bodies_that_do_not_meet_and_splits_pieces() {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "A".into(),
            min: DVec3::ZERO,
            max: DVec3::splat(10.0),
        })
        .unwrap();
        d.apply(Command::AddBox {
            name: "B".into(),
            min: DVec3::splat(50.0),
            max: DVec3::splat(60.0),
        })
        .unwrap();
        let (a, b) = (d.bodies()[0].id, d.bodies()[1].id);
        for op in [CombineOp::Merge, CombineOp::Cut, CombineOp::Intersect] {
            let rev = d.revision();
            let e = d
                .apply(Command::Combine {
                    target: a,
                    tools: vec![b],
                    op,
                    keep_tools: false,
                })
                .unwrap_err();
            assert!(
                e.to_string().contains("nothing to") || e.to_string().contains("do not overlap"),
                "{e}"
            );
            assert_eq!(d.revision(), rev);
        }
        // a slot through a plate: two bodies
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Plate".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 10.0, 5.0),
        })
        .unwrap();
        d.apply(Command::AddBox {
            name: "Slot".into(),
            min: DVec3::new(15.0, -1.0, -1.0),
            max: DVec3::new(25.0, 11.0, 6.0),
        })
        .unwrap();
        let (p, s) = (d.bodies()[0].id, d.bodies()[1].id);
        d.apply(Command::Combine {
            target: p,
            tools: vec![s],
            op: CombineOp::Cut,
            keep_tools: false,
        })
        .unwrap();
        assert_eq!(d.bodies().len(), 2);
        assert!((total(&d) - 1500.0).abs() < 1e-6);
    }

    #[test]
    fn combine_keeps_rounds_that_survive() {
        let (mut d, a, b) = two_cubes();
        let s = d.body(a).unwrap().solid.clone();
        let e = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.start.x < 1.0
                    && i.start.y < 1.0
            })
            .unwrap()
            .0;
        d.apply(Command::FilletEdges {
            body: a,
            edges: vec![e],
            size: 3.0,
            chamfer: false,
        })
        .unwrap();
        d.apply(Command::Combine {
            target: a,
            tools: vec![b],
            op: CombineOp::Merge,
            keep_tools: false,
        })
        .unwrap();
        assert_eq!(d.body(a).unwrap().features.len(), 1);
        let face = d.body(a).unwrap().features[0].faces[0];
        d.apply(Command::EditBlend {
            body: a,
            face,
            size: 5.0,
        })
        .unwrap();
    }

    #[test]
    fn angled_chamfer_is_remembered_edited_flipped_and_saved() {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Box".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let s = d.body(id).unwrap().solid.clone();
        let e = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9
                    && (i.start.y - i.end.y).abs() < 1e-9
                    && i.start.x > 39.0
                    && i.start.y > 29.0
            })
            .unwrap()
            .0;
        d.apply(Command::ChamferAngled {
            body: id,
            edges: vec![e],
            distance: 5.0,
            angle_deg: 30.0,
            flip: false,
        })
        .unwrap();
        let v = |d: &Document| d.body(id).unwrap().solid.volume().unwrap();
        assert!((v(&d) - 23855.66).abs() < 0.01);
        let f = &d.body(id).unwrap().features[0];
        assert_eq!((f.angle, f.flip, f.size), (30.0, false, 5.0));
        // reopen: angle and flip survive
        let e2 = Document::from_loaded(
            file::load_from_bytes(&file::save_to_bytes(&d, None, "t").unwrap()).unwrap(),
        );
        assert_eq!(e2.body(id).unwrap().features[0].angle, 30.0);
        // flip, then plain size change keeps the angle, then 60 degrees, then remove
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditChamfer {
            body: id,
            face,
            distance: 5.0,
            angle_deg: 30.0,
            flip: true,
        })
        .unwrap();
        assert!((v(&d) - 23855.66).abs() < 0.01);
        assert!(d.body(id).unwrap().features[0].flip);
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditBlend {
            body: id,
            face,
            size: 4.0,
        })
        .unwrap();
        assert_eq!(d.body(id).unwrap().features[0].angle, 30.0);
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditChamfer {
            body: id,
            face,
            distance: 5.0,
            angle_deg: 60.0,
            flip: false,
        })
        .unwrap();
        assert!((v(&d) - 23566.99).abs() < 0.01);
        let face = d.body(id).unwrap().features[0].faces[0];
        d.apply(Command::EditChamfer {
            body: id,
            face,
            distance: 0.0,
            angle_deg: 60.0,
            flip: false,
        })
        .unwrap();
        assert!((v(&d) - 24000.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod arc_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    #[test]
    fn oval_pull_revolve_edit_and_save() {
        let mut d = Document::new();
        let mut sk = None;
        for e in [
            Entity::Line {
                a: DVec2::new(0.0, 0.0),
                b: DVec2::new(20.0, 0.0),
            },
            Entity::Line {
                a: DVec2::new(0.0, 10.0),
                b: DVec2::new(20.0, 10.0),
            },
            Entity::arc_through(
                DVec2::new(20.0, 0.0),
                DVec2::new(25.0, 5.0),
                DVec2::new(20.0, 10.0),
            )
            .unwrap(),
            Entity::arc_through(DVec2::new(0.0, 10.0), DVec2::new(-5.0, 5.0), DVec2::new(0.0, 0.0)).unwrap(),
        ] {
            let target = match sk {
                None => SketchTarget::New {
                    plane: PlaneDef::base(),
                    body: None,
                },
                Some(id) => SketchTarget::Existing(id),
            };
            let c = d.apply(Command::AddEntity { target, entity: e }).unwrap();
            sk = sk.or(c.created_sketch);
        }
        let sk = sk.unwrap();
        // survives saving
        let e = Document::from_loaded(
            file::load_from_bytes(&file::save_to_bytes(&d, None, "t").unwrap()).unwrap(),
        );
        assert_eq!(e.sketch(sk).unwrap().sketch.regions().len(), 1);
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: 10.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let v = d.bodies()[0].solid.volume().unwrap();
        assert!((v - 2785.40).abs() < 0.01, "{v}");
        // a half disc revolved about its diameter is a ball
        let mut d = Document::new();
        let plane = PlaneDef::xz();
        let a = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Line {
                    a: DVec2::new(-5.0, 0.0),
                    b: DVec2::new(5.0, 0.0),
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::AddEntity {
            target: SketchTarget::Existing(a),
            entity: Entity::arc_through(DVec2::new(5.0, 0.0), DVec2::new(0.0, 5.0), DVec2::new(-5.0, 0.0))
                .unwrap(),
        })
        .unwrap();
        d.apply(Command::Revolve {
            sketch: a,
            region: 0,
            axis_point: DVec3::ZERO,
            axis_dir: DVec3::X,
            angle: std::f64::consts::TAU,
            mode: PullMode::Auto,
        })
        .unwrap();
        let v = d.bodies()[0].solid.volume().unwrap();
        assert!((v - 4.0 / 3.0 * std::f64::consts::PI * 125.0).abs() < 0.05, "{v}");
    }
}

#[cfg(test)]
mod loft_tests {
    use super::*;
    use rolidium_sketch::DVec2;

    fn circle_sketch(d: &mut Document, z: f64, r: f64, inner: Option<f64>) -> SketchId {
        let plane = PlaneDef {
            origin: DVec3::new(0.0, 0.0, z),
            normal: DVec3::Z,
            x_dir: DVec3::X,
        };
        let id = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Circle {
                    center: DVec2::ZERO,
                    radius: r,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        if let Some(ri) = inner {
            d.apply(Command::AddEntity {
                target: SketchTarget::Existing(id),
                entity: Entity::Circle {
                    center: DVec2::ZERO,
                    radius: ri,
                },
            })
            .unwrap();
        }
        id
    }

    fn region_with_holes(d: &Document, s: SketchId, holes: usize) -> usize {
        d.sketch(s)
            .unwrap()
            .sketch
            .regions()
            .iter()
            .position(|r| r.holes.len() == holes)
            .unwrap()
    }

    #[test]
    fn loft_cone_funnel_modes_and_errors() {
        let mut d = Document::new();
        let a = circle_sketch(&mut d, 0.0, 20.0, None);
        let b = circle_sketch(&mut d, 30.0, 10.0, None);
        d.apply(Command::Loft {
            profiles: vec![
                LoftRef::Region { sketch: a, region: 0 },
                LoftRef::Region { sketch: b, region: 0 },
            ],
            ruled: true,
            mode: LoftMode::NewBody,
        })
        .unwrap();
        assert!((d.bodies()[0].solid.volume().unwrap() - 21991.15).abs() < 0.05);
        assert!(d.sketches().is_empty(), "used-up sketches are gone");
        assert!(d.undo());
        assert_eq!(d.sketches().len(), 2);
        // rings: the funnel
        let mut d = Document::new();
        let a = circle_sketch(&mut d, 0.0, 20.0, Some(18.0));
        let b = circle_sketch(&mut d, 30.0, 10.0, Some(8.0));
        let (ra, rb) = (region_with_holes(&d, a, 1), region_with_holes(&d, b, 1));
        d.apply(Command::Loft {
            profiles: vec![
                LoftRef::Region {
                    sketch: a,
                    region: ra,
                },
                LoftRef::Region {
                    sketch: b,
                    region: rb,
                },
            ],
            ruled: true,
            mode: LoftMode::NewBody,
        })
        .unwrap();
        assert!((d.bodies()[0].solid.volume().unwrap() - 5277.88).abs() < 0.05);
        // a disc and a ring: different numbers of holes, nothing changes
        let mut d = Document::new();
        let a = circle_sketch(&mut d, 0.0, 20.0, Some(18.0));
        let b = circle_sketch(&mut d, 30.0, 10.0, None);
        let ra = region_with_holes(&d, a, 1);
        let rev = d.revision();
        let e = d
            .apply(Command::Loft {
                profiles: vec![
                    LoftRef::Region {
                        sketch: a,
                        region: ra,
                    },
                    LoftRef::Region { sketch: b, region: 0 },
                ],
                ruled: true,
                mode: LoftMode::NewBody,
            })
            .unwrap_err();
        assert!(e.to_string().contains("same number of holes"), "{e}");
        assert_eq!(d.revision(), rev);
        // add: from the top face of a box to a smaller circle above it
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "B".into(),
            min: DVec3::new(-20.0, -20.0, 0.0),
            max: DVec3::new(20.0, 20.0, 10.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let s = d.body(id).unwrap().solid.clone();
        let top = s
            .faces()
            .find(|f| s.face_info(*f).unwrap().normal.z > 0.99)
            .unwrap();
        let c = circle_sketch(&mut d, 40.0, 10.0, None);
        d.apply(Command::Loft {
            profiles: vec![
                LoftRef::Face { body: id, face: top },
                LoftRef::Region { sketch: c, region: 0 },
            ],
            ruled: true,
            mode: LoftMode::Add,
        })
        .unwrap();
        assert_eq!(d.bodies().len(), 1);
        let v = d.bodies()[0].solid.volume().unwrap();
        assert!(v > 16000.0 + 100.0, "{v}");
        // saved loft body stays valid
        let e = Document::from_loaded(
            file::load_from_bytes(&file::save_to_bytes(&d, None, "t").unwrap()).unwrap(),
        );
        assert!((e.bodies()[0].solid.volume().unwrap() - v).abs() < 1e-6);
    }
}

#[cfg(test)]
mod loft_edge_tests {
    use super::*;

    #[test]
    fn loft_from_the_circular_edge_of_a_cylinder() {
        let mut d = Document::new();
        let plane = PlaneDef::base();
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: None },
                entity: Entity::Circle {
                    center: rolidium_sketch::DVec2::ZERO,
                    radius: 10.0,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: 20.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let s = d.body(id).unwrap().solid.clone();
        let top = s
            .edges()
            .find(|e| {
                let i = s.edge_info(*e).unwrap();
                i.kind == rolidium_kernel::EdgeKind::Circle
                    && i.start.z > 19.9
                    && (i.start - i.end).length() < 1e-6
            })
            .unwrap();
        // a smaller circle above: loft from the edge of the cylinder to it
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane: PlaneDef {
                        origin: DVec3::new(0.0, 0.0, 50.0),
                        normal: DVec3::Z,
                        x_dir: DVec3::X,
                    },
                    body: None,
                },
                entity: Entity::Circle {
                    center: rolidium_sketch::DVec2::ZERO,
                    radius: 5.0,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::Loft {
            profiles: vec![
                LoftRef::EdgeLoop {
                    body: id,
                    edge: top.0,
                },
                LoftRef::Region { sketch: c, region: 0 },
            ],
            ruled: true,
            mode: LoftMode::Add,
        })
        .unwrap();
        let v = d.bodies()[0].solid.volume().unwrap();
        let cyl = std::f64::consts::PI * 100.0 * 20.0;
        let cone = std::f64::consts::PI * 30.0 / 3.0 * (100.0 + 50.0 + 25.0);
        assert!((v - (cyl + cone)).abs() < 0.1, "{v}");
    }
}
