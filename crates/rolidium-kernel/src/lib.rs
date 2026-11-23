// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Geometry kernel interface for Rolidium.
//!
//! This crate is the only place that touches Open CASCADE. Everything above it works with
//! the types defined here. Units are millimetres, `f64`.
//!
//! Reliability rules enforced here:
//! * no C++ exception crosses the FFI boundary (the bridge converts them to [`KernelError`]);
//! * every modelling operation validates its result and refuses to return an invalid body;
//! * shapes are immutable, so a failed operation can never damage its inputs.

mod blend;
mod feature;
mod frustum;
mod looptool;
mod measure;
mod mesh;
mod occt;
mod ops;
mod rounds;
mod sweep;
pub mod timing;

pub use blend::{largest_working_size, CHAIN_ANGLE_DEG};
pub use feature::FeatureTool;
pub use frustum::FrustumInfo;
pub use glam::{DVec3, Vec3};
pub use rounds::{BlendMap, RoundComponent, Rounding};
/// Angular deflection (rad) of the display meshes. It is only a safety limit: the density is set by the linear deflection (a fraction
/// of a pixel), so a shape that is small on the screen is not made denser than its size in pixels asks for.
pub const VIEW_ANGLE: f64 = 0.5;
/// The angle of the faces that are meshed finer than the rest because of their smallest feature (a thin wire): with 0.5 a circle of the profile
/// has 13 segments and the mesh covers 96 % of it, with this 31 and 99.3 %.
pub const THIN_ANGLE: f64 = 0.2;
/// The display mesh deviates from the exact shape by at most this fraction of a pixel.
pub const VIEW_PIXEL_FRACTION: f64 = 0.4;
/// The first display mesh after an operation is rough: its deflection is this fraction of the size of the part (about two pixels of a
/// window of 900 px); the finer mesh for the zoom is made afterwards in the background.
pub const FIRST_DISPLAY_FRACTION: f64 = 0.002;
/// At most this many triangles are made for the faces in view in one go: meshing them costs about 15 microseconds a triangle when few
/// faces are meshed at once (no parallelism), and the background work must be done in about a second and a half.
pub const FINE_TRIANGLE_BUDGET: usize = 60_000;
/// The most a pass of `view_mesh_faces` makes the mesh of a face finer (see there).
const MAX_STEP: f64 = 4.0;
/// A face is meshed so that the mesh is at most this fraction of its smallest radius of curvature from it (but not finer than `MIN_FACE_DEFLECTION`):
/// the thin wire of a thread on a big cone keeps its shape although the body is big. The deviation is then at most about 10 % of the radius.
const CAP_FRACTION: f64 = 0.1;
const MIN_FACE_DEFLECTION: f64 = 0.002;
/// The angle between neighbouring normals in the base mesh of a thin face: a wire of a few pixels has ten segments round it.
const BASE_THIN_ANGLE: f64 = 1.0;
/// A face is thin for the base mesh when its cap (a tenth of its smallest radius) is under this fraction of the base deflection: its radius is
/// then about five pixels at most. Wider wires keep the mesh made for their own radius (their shape is seen).
const BASE_THIN_RATIO: f64 = 0.3;

pub use looptool::{LoopEdge, LoopError, PlanarLoop};
pub use measure::{measure, Angle, Elem, Measurement};
pub use mesh::{EdgePolyline, Mesh};
pub use ops::{
    BlendCombo, BlendGroup, BlendInput, ConeInfo, CylinderInfo, LoftInput, Profile, ProfileSegment,
    SectionCurve,
};
pub use sweep::{
    estimated_sweep_seconds, polyline_segments, HelixDef, Path, PathSeg, PathSource, SweepOptions,
    SweepResult, SweepWarning, HELIX_MAX_TURNS, SWEEP_WARN_SECONDS,
};

use cxx::SharedPtr;
use occt::ffi as native;

/// Errors from kernel operations. The `Display` text is meant to be shown to users.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("{0}")]
    Native(String),
    #[error("The face is not planar, so it cannot be pulled this way")]
    NotPlanar,
    #[error("The distance is too small to change anything")]
    ZeroDistance,
    #[error("The operation produced an invalid solid, so it was not applied")]
    InvalidResult,
    #[error("The operation would split the body into several pieces, so it was not applied")]
    SplitResult,
    #[error("{0}")]
    Invalid(String),
    /// The user cancelled the operation; the model is unchanged.
    #[error("Cancelled")]
    Cancelled,
}

impl From<cxx::Exception> for KernelError {
    fn from(e: cxx::Exception) -> Self {
        let m = e.what().to_owned();
        if m.contains("cancelled") {
            KernelError::Cancelled
        } else {
            KernelError::Native(m)
        }
    }
}

pub mod par;

/// Time (nanoseconds, summed over the threads) spent reading BREP data (native, no lock) and checking the shapes read (behind the lock of the
/// kernel): for `rolidium-cli times open`, which tells what part of opening runs one thread at a time.
pub static READ_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CHECK_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many threads Open CASCADE may use inside one operation (0: its default, one per core; 1: no parallel mode).
pub(crate) fn set_native_threads(n: usize) {
    native::set_threads(n.min(256) as i32);
}

/// Asks the running operation (on any thread) to stop as soon as it can. The flag stays set until [`reset_cancel`].
pub fn request_cancel() {
    native::set_cancel(true);
}

/// Clears the flag; every job calls this before it starts.
pub fn reset_cancel() {
    native::set_cancel(false);
}

/// What the calling thread spent in native calls since the last ask: (calls, ms waiting for the lock, ms running, the dearest call in ms and its name).
pub fn take_thread_stats() -> (u32, f64, f64, f64, String) {
    let worst = native::take_thread_worst();
    let v = native::take_thread_stats();
    (v[0] as u32, v[1], v[2], v[3], worst)
}

pub fn cancel_requested() -> bool {
    native::cancel_requested()
}

pub type Result<T> = std::result::Result<T, KernelError>;

/// Index of a face inside one [`Solid`] version. Identity across operations is carried by [`History`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// Index of an edge inside one [`Solid`] version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceInfo {
    pub kind: FaceKind,
    /// Plane: outward unit normal. Cylinder/cone/torus: axis. Otherwise zero.
    pub normal: DVec3,
    /// Plane origin, or a point on the axis / the centre.
    pub origin: DVec3,
    /// Cylinder/cone reference radius, sphere radius, torus major radius.
    pub radius: f64,
    pub area: f64,
    /// Number of holes (inner wires).
    pub inner_wires: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    Line,
    /// Full circle or arc.
    Circle,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInfo {
    pub kind: EdgeKind,
    pub start: DVec3,
    pub end: DVec3,
    pub radius: f64,
    pub length: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Props {
    pub volume: f64,
    pub area: f64,
    pub min: DVec3,
    pub max: DVec3,
    pub centroid: DVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Fuse,
    Cut,
    Common,
}

/// Which faces of a result descend from which faces of the operands.
#[derive(Debug, Clone, Default)]
pub struct History {
    /// `(operand, source face, result face)`; operand 0 is the first argument.
    pub links: Vec<(u8, FaceId, FaceId)>,
}

impl History {
    /// Result faces that descend from `source` of `operand`.
    pub fn successors(&self, operand: u8, source: FaceId) -> Vec<FaceId> {
        self.links
            .iter()
            .filter(|l| l.0 == operand && l.1 == source)
            .map(|l| l.2)
            .collect()
    }

    /// Joins two histories of operand 0: faces of the original that became faces of the middle
    /// solid, and those that became faces of the final solid.
    pub fn compose(&self, next: &History) -> History {
        let mut links = Vec::new();
        for l in self.links.iter().filter(|l| l.0 == 0) {
            for r in next.successors(0, l.2) {
                links.push((0, l.1, r));
            }
        }
        History { links }
    }

    /// Faces of a result with `face_count` faces that descend from no face of operand 0, such
    /// as the faces a fillet creates.
    pub fn new_faces(&self, face_count: usize) -> Vec<FaceId> {
        let mut seen = vec![false; face_count];
        for l in self.links.iter().filter(|l| l.0 == 0) {
            if let Some(x) = seen.get_mut(l.2 .0 as usize) {
                *x = true;
            }
        }
        (0..face_count as u32)
            .filter(|i| !seen[*i as usize])
            .map(FaceId)
            .collect()
    }
}

/// A result of a modelling operation: the new solid and where its faces came from.
#[derive(Debug, Clone)]
pub struct OpResult {
    pub solid: Solid,
    pub history: History,
}

/// An immutable B-rep body. Cloning is cheap (shared native handle).
#[derive(Clone)]
pub struct Solid {
    shape: SharedPtr<native::Shape>,
    /// Volume is expensive on large parts and the shape is immutable, so cache it.
    volume: std::sync::Arc<std::sync::OnceLock<f64>>,
    /// The box around the shape (cached for the same reason).
    bbox: std::sync::Arc<std::sync::OnceLock<[f64; 6]>>,
    /// Edge ends and adjacency for smooth chains (built when first needed).
    chain_index: std::sync::Arc<std::sync::OnceLock<blend::ChainIndex>>,
    /// The BRep bytes of the shape for the file (an unchanged body is not written again at every autosave).
    brep: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<Vec<u8>>>>,
    /// Rounds and chamfers recognised from the geometry (built when first needed).
    blend_map: std::sync::Arc<std::sync::OnceLock<rounds::BlendMap>>,
}

impl std::fmt::Debug for Solid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Solid({} faces, {} edges)",
            self.face_count(),
            self.edge_count()
        )
    }
}

fn v3(a: [f64; 3]) -> DVec3 {
    DVec3::new(a[0], a[1], a[2])
}

impl Solid {
    pub(crate) fn from_shape(shape: SharedPtr<native::Shape>) -> Self {
        Solid {
            shape,
            volume: Default::default(),
            bbox: Default::default(),
            chain_index: Default::default(),
            brep: Default::default(),
            blend_map: Default::default(),
        }
    }
    pub(crate) fn raw(&self) -> &native::Shape {
        &self.shape
    }

    /// Axis-aligned box between two corners.
    pub fn cuboid(min: DVec3, max: DVec3) -> Result<Solid> {
        let s = native::make_box(min.x, min.y, min.z, max.x, max.y, max.z)?;
        Solid::from_shape(s).validated()
    }

    /// Cylinder with its base circle centred at `origin`, extending `height` along `axis`.
    pub fn cylinder(radius: f64, height: f64, origin: DVec3, axis: DVec3) -> Result<Solid> {
        if axis.length() < 1e-9 {
            return Err(KernelError::Invalid("cylinder axis must not be zero".into()));
        }
        let s = native::make_cylinder(radius, height, origin.to_array(), axis.normalize().to_array())?;
        Solid::from_shape(s).validated()
    }

    pub fn face_count(&self) -> usize {
        native::face_count(&self.shape) as usize
    }
    pub fn edge_count(&self) -> usize {
        native::edge_count(&self.shape) as usize
    }
    pub fn faces(&self) -> impl Iterator<Item = FaceId> {
        (0..self.face_count() as u32).map(FaceId)
    }
    pub fn edges(&self) -> impl Iterator<Item = EdgeId> {
        (0..self.edge_count() as u32).map(EdgeId)
    }

    pub fn face_info(&self, face: FaceId) -> Result<FaceInfo> {
        let i = native::face_info(&self.shape, face.0)?;
        Ok(FaceInfo {
            kind: match i.kind {
                0 => FaceKind::Plane,
                1 => FaceKind::Cylinder,
                2 => FaceKind::Cone,
                3 => FaceKind::Sphere,
                4 => FaceKind::Torus,
                _ => FaceKind::Other,
            },
            normal: DVec3::new(i.nx, i.ny, i.nz),
            origin: DVec3::new(i.ox, i.oy, i.oz),
            radius: i.radius,
            area: i.area,
            inner_wires: i.inner_wires,
        })
    }

    pub fn edge_info(&self, edge: EdgeId) -> Result<EdgeInfo> {
        let i = native::edge_info(&self.shape, edge.0)?;
        Ok(EdgeInfo {
            kind: match i.kind {
                0 => EdgeKind::Line,
                1 => EdgeKind::Circle,
                _ => EdgeKind::Other,
            },
            start: DVec3::new(i.sx, i.sy, i.sz),
            end: DVec3::new(i.ex, i.ey, i.ez),
            radius: i.radius,
            length: i.length,
        })
    }

    /// Edges bounding a face.
    pub fn face_edges(&self, face: FaceId) -> Result<Vec<EdgeId>> {
        Ok(native::face_edges(&self.shape, face.0)?
            .into_iter()
            .map(EdgeId)
            .collect())
    }

    /// Boundary polylines of a face (for preview drawing), as lists of points.
    pub fn face_outline(&self, face: FaceId, deflection: f64) -> Result<Vec<Vec<Vec3>>> {
        let mut pts = Vec::new();
        let mut offs = Vec::new();
        native::face_outline(&self.shape, face.0, deflection, &mut pts, &mut offs)?;
        let mut out = Vec::new();
        for w in offs.windows(2) {
            let (a, b) = (w[0] as usize, w[1] as usize);
            out.push(
                (a..b)
                    .map(|i| Vec3::new(pts[3 * i], pts[3 * i + 1], pts[3 * i + 2]))
                    .collect(),
            );
        }
        Ok(out)
    }

    pub fn props(&self) -> Result<Props> {
        let p = native::props(&self.shape)?;
        Ok(Props {
            volume: p.volume,
            area: p.area,
            min: v3(p.min),
            max: v3(p.max),
            centroid: v3(p.centroid),
        })
    }
    /// Short technical summary (solids, shells, orientation, largest tolerances) for diagnostics.
    pub fn diagnostics(&self) -> Result<String> {
        Ok(native::shape_diag(&self.shape)?)
    }

    /// The identities of the faces, edges and vertices of the solid: solids that have one in common share data in the kernel
    /// and must not be worked on at the same moment.
    pub fn shared_keys(&self) -> Result<Vec<u64>> {
        Ok(native::shared_keys(&self.shape)?)
    }

    /// An identifier of the native shape: equal for clones of one solid, different for solids made by an operation.
    /// (Valid while a clone of the solid is alive, which is how the display caches use it.)
    pub fn shape_id(&self) -> usize {
        &*self.shape as *const native::Shape as usize
    }

    /// Whether `other` is this very solid (a clone of it), not only one of the same shape: the same one is not made again.
    pub fn is_same_solid(&self, other: &Solid) -> bool {
        std::sync::Arc::ptr_eq(&self.volume, &other.volume)
    }

    /// The box around the solid (cached): the smallest and the largest corner.
    pub fn bounding_box(&self) -> Result<(DVec3, DVec3)> {
        if let Some(b) = self.bbox.get() {
            return Ok((DVec3::new(b[0], b[1], b[2]), DVec3::new(b[3], b[4], b[5])));
        }
        let v = native::bounds_of(&self.shape)?;
        let b = [v[0], v[1], v[2], v[3], v[4], v[5]];
        let _ = self.bbox.set(b);
        Ok((DVec3::new(b[0], b[1], b[2]), DVec3::new(b[3], b[4], b[5])))
    }

    /// Whether the two solids may be closer than `gap` mm: false when their boxes are further apart (a cheap test that saves the exact
    /// distance, which costs much more on parts with many faces).
    pub fn may_be_within(&self, other: &Solid, gap: f64) -> bool {
        match (self.bounding_box(), other.bounding_box()) {
            (Ok((a0, a1)), Ok((b0, b1))) => (0..3).all(|k| a0[k] <= b1[k] + gap && b0[k] <= a1[k] + gap),
            _ => true,
        }
    }

    /// Smallest distance to another solid in mm; 0 when they touch, overlap or one lies inside the other.
    pub fn distance_to(&self, other: &Solid) -> Result<f64> {
        Ok(native::solid_distance(&self.shape, &other.shape)?)
    }

    pub fn volume(&self) -> Result<f64> {
        if let Some(v) = self.volume.get() {
            return Ok(*v);
        }
        // only the volume: the full properties (area, bounding box) cost about as much again
        let v = native::volume_of(&self.shape)?;
        let _ = self.volume.set(v);
        Ok(v)
    }

    /// True if the point lies inside the solid.
    pub fn contains_point(&self, p: DVec3) -> Result<bool> {
        Ok(native::point_in_solid(&self.shape, p.to_array()))
    }

    /// Native validity check plus "exactly one solid" check.
    pub fn is_valid(&self) -> bool {
        native::check_shape(&self.shape) && native::solid_count(&self.shape) == 1
    }

    /// [`Self::validated`] plus: the body must enclose a positive volume (a solid that came out inside out, or empty, is
    /// refused). Used for bodies made by sweeping, lofting and revolving.
    pub(crate) fn validated_body(self) -> Result<Solid> {
        let s = self.validated()?;
        let v = s.volume()?;
        if !(v.is_finite() && v > 1e-9) {
            return Err(KernelError::Invalid(
                "The operation did not produce a closed body with a positive volume, so the model was not changed".into(),
            ));
        }
        Ok(s)
    }

    pub(crate) fn validated(self) -> Result<Solid> {
        // the checks below cannot be interrupted, so look at the cancel flag first
        if native::cancel_requested() {
            return Err(KernelError::Cancelled);
        }
        if native::solid_count(&self.shape) > 1 {
            return Err(KernelError::SplitResult);
        }
        if !native::check_shape(&self.shape) || native::solid_count(&self.shape) != 1 {
            return Err(KernelError::InvalidResult);
        }
        Ok(self)
    }

    /// Triangulate with the given linear (mm) and angular (rad) deflection.
    pub fn mesh(&self, linear: f64, angular: f64) -> Result<Mesh> {
        let mut m = Mesh::from_native(native::mesh_shape(&self.shape, linear, angular)?);
        m.deflection = linear;
        m.face_deflection = vec![linear; self.face_count()];
        Ok(m)
    }

    /// Triangulation tuned for display: the chord error scales with the size of the part, and with the size of the smallest features of a face
    /// (a thread of a thin wire on a big cone is meshed for the wire, not for the cone).
    pub fn display_mesh(&self) -> Result<Mesh> {
        // only the box is needed here; the full properties (volume, area) cost much more
        let b = native::bounds_of(&self.shape)?;
        let diag = (DVec3::new(b[3], b[4], b[5]) - DVec3::new(b[0], b[1], b[2])).length();
        let linear = (diag * FIRST_DISPLAY_FRACTION).clamp(0.005, 0.5);
        let caps = self.face_caps()?;
        if caps.iter().all(|c| *c >= linear) {
            return self.mesh(linear, VIEW_ANGLE);
        }
        // The first mesh is for the whole part in the window: a thin feature (the wire of a thread) is a few pixels wide there, so
        // it is meshed with the deflection of the part and a wide angle, not to a fraction of its own radius (that would be hundreds of thousands of
        // triangles for 40 turns); the finer mesh for what the camera comes close to is made afterwards (`view_mesh_faces`).
        let very_thin = |c: f64| c < linear * BASE_THIN_RATIO;
        let target: Vec<f64> = caps
            .iter()
            .map(|c| if very_thin(*c) { linear } else { c.min(linear) })
            .collect();
        let angles: Vec<f64> = caps
            .iter()
            .map(|c| {
                if very_thin(*c) {
                    BASE_THIN_ANGLE
                } else if *c < linear {
                    THIN_ANGLE
                } else {
                    VIEW_ANGLE
                }
            })
            .collect();
        // the shortest element of the base mesh of a thin face: at least five along its smallest circle, and not shorter than two pixels
        let thin_cap = caps
            .iter()
            .filter(|c| very_thin(**c))
            .fold(f64::MAX, |a, c| a.min(*c));
        let min_size = (2.0 * linear).min(12.5 * thin_cap);
        let (part, used) = self.mesh_in_groups(&target, &angles, min_size)?;
        let mut m = part;
        m.deflection = linear;
        m.face_asked = used.clone();
        m.face_deflection = used;
        Ok(m)
    }

    /// The deflection each face needs to stay close to its smallest feature (see `CAP_FRACTION`); large for a plane.
    fn face_caps(&self) -> Result<Vec<f64>> {
        Ok(native::face_scales(&self.shape)?
            .into_iter()
            .map(|r| (r * CAP_FRACTION).max(MIN_FACE_DEFLECTION))
            .collect())
    }

    /// Meshes the faces with their own deflections (`target`, 0 = not meshed): the faces whose deflections are within a factor of two of
    /// each other go into one call of the mesher, with the finest of them. Returns the mesh of those faces and the deflection each got.
    fn mesh_in_groups(&self, target: &[f64], angles: &[f64], thin_min_size: f64) -> Result<(Mesh, Vec<f64>)> {
        let n = target.len();
        // the key: the power of two the deflection lies under, and the angle (in thousandths)
        let key_of = |i: usize| (target[i].log2().floor() as i32, (angles[i] * 1000.0) as i32);
        let mut groups: std::collections::BTreeMap<(i32, i32), f64> = std::collections::BTreeMap::new();
        for i in (0..n).filter(|i| target[*i] > 0.0) {
            let e = groups.entry(key_of(i)).or_insert(f64::MAX);
            *e = e.min(target[i]);
        }
        let mut used = vec![0.0; n];
        let mut acc = Mesh::default();
        let whole = groups.len() == 1 && target.iter().all(|t| *t > 0.0);
        for (key, linear) in &groups {
            let mask: Vec<u8> = (0..n)
                .map(|i| (target[i] > 0.0 && key_of(i) == *key) as u8)
                .collect();
            let part = Mesh::from_native(native::mesh_shape_regions(
                &self.shape,
                *linear,
                key.1 as f64 / 1000.0,
                // the groups of the wide base angle (thin faces in the base mesh) have a shortest element
                if key.1 == (BASE_THIN_ANGLE * 1000.0) as i32 {
                    thin_min_size
                } else {
                    0.0
                },
                if whole { &[] } else { &mask },
            )?);
            for i in 0..n {
                if mask[i] == 1 {
                    used[i] = *linear;
                }
            }
            acc = if acc.positions.is_empty() {
                part
            } else {
                acc.merged_with(&part, &mask)
            };
        }
        Ok((acc, used))
    }

    /// Triangulation for a view in which one pixel is `pixel_mm` millimetres wide: the triangles and the edge polylines
    /// deviate from the exact curves by at most half a pixel, so that a circle across the whole window looks round. `previous`
    /// is a mesh of this solid made earlier (its deflection and triangle count): the number of triangles is kept under
    /// `max_triangles` by raising the deflection when the estimate from it says the cap would be passed (the number grows about
    /// with one over the deflection on curved faces).
    pub fn view_mesh(
        &self,
        pixel_mm: f64,
        max_triangles: usize,
        previous: Option<(f64, usize)>,
    ) -> Result<Mesh> {
        let (diag, mut linear) = self.view_linear(pixel_mm)?;
        let _ = diag;
        if let Some((d0, n0)) = previous {
            if d0 > 0.0 && n0 > 0 {
                let estimate = n0 as f64 * (d0 / linear).max(1.0);
                if estimate > max_triangles as f64 {
                    linear = (d0 * n0 as f64 / max_triangles as f64).max(linear);
                }
            }
        }
        let mut angle = VIEW_ANGLE;
        let mut m = self.mesh(linear, angle)?;
        // the estimate can be off: coarser again (also in the angle) while the cap is passed
        for _ in 0..3 {
            if m.triangle_count() <= max_triangles {
                break;
            }
            let ratio = (m.triangle_count() as f64 / max_triangles as f64).max(1.5);
            linear *= ratio;
            angle = (angle * ratio.sqrt()).min(0.8);
            m = self.mesh(linear, angle)?;
        }
        Ok(m)
    }

    /// The size of the part and the linear deflection for a pixel of `pixel_mm`.
    /// The deflection a mesh for a pixel of `pixel_mm` is asked for (limited at both ends); what the program compares the meshes with.
    pub fn view_deflection(&self, pixel_mm: f64) -> f64 {
        self.view_linear(pixel_mm)
            .map_or(pixel_mm * VIEW_PIXEL_FRACTION, |v| v.1)
    }

    fn view_linear(&self, pixel_mm: f64) -> Result<(f64, f64)> {
        // the cached box: this is asked for at every frame of the window (is a finer mesh due?), the exact box of a body with
        // hundreds of swept faces takes about a second
        let (lo, hi) = self.bounding_box()?;
        let diag = (hi - lo).length().max(1e-6);
        let linear =
            (pixel_mm * VIEW_PIXEL_FRACTION).clamp(1e-4_f64.max(diag * 1e-7), (diag * 0.01).max(0.005));
        Ok((diag, linear))
    }

    /// The faces marked in `fine` (one flag per face; the faces in view) are triangulated again for the pixel of `pixel_mm`
    /// millimetres; the triangles and edges of the other faces are taken from `base`, a mesh of this solid made earlier (it is made
    /// here when not given). Only the faces in view are meshed, so the time follows what is on the screen. The whole mesh stays under
    /// `max_triangles`: the deflection of the faces in view is raised when the estimate (the triangles of a face grow with one over the
    /// deflection) says that the cap would be passed.
    pub fn view_mesh_faces(
        &self,
        pixel_mm: f64,
        fine: &[bool],
        max_triangles: usize,
        base: Option<&Mesh>,
    ) -> Result<Mesh> {
        let n = self.face_count();
        let (_, mut linear) = self.view_linear(pixel_mm)?;
        let requested = linear;
        let owned;
        let base = match base {
            Some(b) => b,
            None => {
                owned = self.display_mesh()?;
                &owned
            }
        };
        let per_face = base.triangles_per_face(n);
        let defl = |i: usize| {
            base.face_deflection
                .get(i)
                .copied()
                .filter(|d| *d > 0.0)
                .unwrap_or(base.deflection)
        };
        let asked_before = |i: usize| {
            base.face_asked
                .get(i)
                .copied()
                .filter(|d| *d > 0.0)
                .unwrap_or_else(|| defl(i))
        };
        let is_fine = |i: usize| fine.get(i).copied().unwrap_or(false);
        let caps = self.face_caps()?;
        // The deflection a face is meshed with when the display asks for `linear`: not coarser than its cap (the smallest feature of the face),
        // and a pass makes a face at most `MAX_STEP` times finer than its present mesh. The number of triangles of a face grows faster than the
        // estimate below says for long curved faces (a spring), and one mesher call that is far too fine takes minutes and tens of gigabytes. The
        // caller asks again (the deflection asked for is only met when it is reached) until the zoom is met, so a deep zoom is a few quick passes.
        let target = |i: usize, linear: f64| linear.min(caps[i]).max(defl(i) / MAX_STEP);
        let want = |i: usize| requested.min(caps[i]);
        let fixed: f64 = (0..n).filter(|i| !is_fine(*i)).map(|i| per_face[i] as f64).sum();
        // How the triangles of a face in view grow when the deflection gets smaller. The edges of a face are cut into segments, which grow
        // with the square root of 1 / deflection; a plane (with the circles of its holes) and a cylinder (a strip of triangles along the
        // circle) have only that. A sphere, a torus or a free surface also need more rows: they grow with 1 / deflection.
        let faces: Vec<FaceId> = self.faces().collect();
        let exponent: Vec<f64> = (0..n)
            .map(
                |i| match faces.get(i).and_then(|f| self.face_info(*f).ok()).map(|f| f.kind) {
                    Some(FaceKind::Plane) | Some(FaceKind::Cylinder) | Some(FaceKind::Cone) => 0.5,
                    _ => 1.0,
                },
            )
            .collect();
        let estimate = |linear: f64| -> f64 {
            (0..n)
                .filter(|i| is_fine(*i))
                .map(|i| per_face[i] as f64 * (defl(i) / target(i, linear)).max(1.0).powf(exponent[i]))
                .sum()
        };
        // the faces in view get at most the budget, and the whole mesh stays under the cap (aim a little lower: the estimate is rough)
        let room = ((max_triangles as f64 - fixed).max(max_triangles as f64 * 0.2))
            .min(FINE_TRIANGLE_BUDGET as f64)
            * 0.9;
        let first_linear = linear;
        if estimate(linear) > room {
            // the smallest deflection (not finer than asked) for which the estimate fits: the estimate falls as the deflection grows
            let (mut lo, mut hi) = (linear, linear);
            while estimate(hi) > room && hi < 1.0e6 {
                hi *= 2.0;
            }
            for _ in 0..40 {
                let mid = (lo * hi).sqrt();
                if estimate(mid) > room {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            linear = hi;
        }
        let budget_hit = linear > first_linear;
        // the faces that get a finer mesh in this pass (the others are as fine as asked, or finer)
        let mut goal: Vec<f64> = (0..n)
            .map(|i| {
                if is_fine(i) && target(i, linear) < defl(i) * 0.999 {
                    target(i, linear)
                } else {
                    0.0
                }
            })
            .collect();
        if goal.iter().all(|g| *g == 0.0) {
            // nothing to do: the display is told that these faces are as fine as they can be made for this zoom
            let mut kept = base.clone();
            kept.face_asked = (0..n)
                .map(|i| {
                    if is_fine(i) {
                        want(i).max(requested.min(asked_before(i)))
                    } else {
                        asked_before(i)
                    }
                })
                .collect();
            return Ok(kept);
        }
        let mut angle = VIEW_ANGLE;
        let base_area = base.area_per_face(n);
        let mut out = base.clone();
        for attempt in 0..4 {
            let angles: Vec<f64> = (0..n)
                .map(|i| {
                    if caps[i] < requested {
                        angle.min(THIN_ANGLE)
                    } else {
                        angle
                    }
                })
                .collect();
            let (part, used) = self.mesh_in_groups(&goal, &angles, 0.0)?;
            // A finer mesh replaces the coarse one of a face only when it is complete: the face has triangles and covers
            // (nearly) the area the coarse mesh covers. A face the mesher failed on, or only did in part, stays as it was.
            let got = part.triangles_per_face(n);
            let area = part.area_per_face(n);
            let accepted: Vec<u8> = (0..n)
                .map(|i| (goal[i] > 0.0 && got[i] > 0 && area[i] >= base_area[i] * 0.97) as u8)
                .collect();
            out = if accepted.contains(&1) {
                base.merged_with(&part, &accepted)
            } else {
                base.clone()
            };
            out.deflection = used
                .iter()
                .copied()
                .fold(0.0_f64, f64::max)
                .max(out.deflection.min(linear));
            // the deflection the faces really got (the estimate of the next pass starts from it), and the one that was asked for
            out.face_deflection = (0..n)
                .map(|i| if accepted[i] == 1 { used[i] } else { defl(i) })
                .collect();
            out.face_asked = (0..n)
                .map(|i| match accepted[i] {
                    // met the zoom, or the budget does not allow more (do not ask again); else the program asks for the next pass
                    1 if budget_hit || attempt > 0 || goal[i] <= want(i) * 1.001 => requested,
                    1 => goal[i],
                    _ => asked_before(i),
                })
                .collect();
            if out.triangle_count() as f64 <= max_triangles as f64 * 1.25 || attempt == 3 {
                break;
            }
            // the estimate can be off: coarser again (also in the angle) while the cap is passed
            let ratio = (out.triangle_count() as f64 / max_triangles as f64).max(1.5);
            for (i, g) in goal.iter_mut().enumerate().filter(|(_, g)| **g > 0.0) {
                *g *= ratio;
                if *g >= defl(i) * 0.999 {
                    *g = 0.0; // not coarser than the mesh the face has
                }
            }
            if goal.iter().all(|g| *g == 0.0) {
                break;
            }
            angle = (angle * ratio.sqrt()).min(0.8);
        }
        // over the budget in the end: the coarse mesh stays (the faces are never dropped to fit)
        if out.triangle_count() as f64 > max_triangles as f64 * 1.25
            && base.triangle_count() <= out.triangle_count()
        {
            // the faces stay as they are, and the display is told that more is not possible (it does not ask again)
            let mut kept = base.clone();
            kept.face_asked = (0..n)
                .map(|i| if is_fine(i) { requested } else { asked_before(i) })
                .collect();
            return Ok(kept);
        }
        Ok(out)
    }

    pub fn write_step(&self) -> Result<Vec<u8>> {
        Ok(native::write_step(&self.shape)?)
    }
    /// One STEP file holding all the given solids.
    pub fn write_step_many(solids: &[Solid]) -> Result<Vec<u8>> {
        let mut w = native::new_step_writer();
        for s in solids {
            native::step_writer_add(w.pin_mut(), &s.shape)?;
        }
        Ok(native::step_writer_finish(w.pin_mut())?)
    }
    /// Like [`Self::write_step_many`], with the name of every solid written into the file.
    pub fn write_step_named(solids: &[(Solid, String)]) -> Result<Vec<u8>> {
        let mut w = native::new_step_writer();
        for (s, name) in solids {
            native::step_writer_add_named(w.pin_mut(), &s.shape, name)?;
        }
        Ok(native::step_writer_finish(w.pin_mut())?)
    }
    /// The names of the solids of a STEP file in the order of [`Self::read_step_all`] (empty text where there is none).
    pub fn read_step_names(data: &[u8]) -> Result<Vec<String>> {
        Ok(native::read_step_names(data)?)
    }
    pub fn read_step(data: &[u8]) -> Result<Solid> {
        Solid::from_shape(native::read_step(data)?).validated()
    }
    /// Reads every solid of a STEP file. Solids that are not valid are skipped and reported in the
    /// second part of the result; a file without any solid is an error.
    pub fn read_step_all(data: &[u8]) -> Result<(Vec<Solid>, Vec<String>)> {
        let shape = native::read_step(data)?;
        let n = native::solid_count(&shape);
        if n == 0 {
            return Err(KernelError::Invalid(
                "The STEP file contains no solid bodies (only surfaces or curves)".into(),
            ));
        }
        let (mut ok, mut skipped) = (Vec::new(), Vec::new());
        for i in 0..n {
            let one = Solid::from_shape(native::solid_at(&shape, i)?);
            if native::check_shape(&one.shape) {
                ok.push(one);
            } else {
                skipped.push(format!("solid {} of the file is not valid", i + 1));
            }
        }
        Ok((ok, skipped))
    }

    /// Like [`Self::read_step_all`], with the name of each solid (empty where the file has none).
    #[allow(clippy::type_complexity)]
    pub fn read_step_all_named(data: &[u8]) -> Result<(Vec<(Solid, String)>, Vec<String>)> {
        let names = native::read_step_names(data).unwrap_or_default();
        let shape = native::read_step(data)?;
        let n = native::solid_count(&shape);
        if n == 0 {
            return Err(KernelError::Invalid(
                "The STEP file contains no solid bodies (only surfaces or curves)".into(),
            ));
        }
        let (mut ok, mut skipped) = (Vec::new(), Vec::new());
        for i in 0..n {
            let one = Solid::from_shape(native::solid_at(&shape, i)?);
            if native::check_shape(&one.shape) {
                ok.push((one, names.get(i as usize).cloned().unwrap_or_default()));
            } else {
                skipped.push(format!("solid {} of the file is not valid", i + 1));
            }
        }
        Ok((ok, skipped))
    }

    /// Native BREP serialisation (document storage).
    pub fn write_brep(&self) -> Result<std::sync::Arc<Vec<u8>>> {
        if let Some(b) = self.brep.get() {
            return Ok(b.clone());
        }
        let b = std::sync::Arc::new(native::write_brep(&self.shape)?);
        let _ = self.brep.set(b.clone());
        Ok(b)
    }
    pub fn read_brep(data: &[u8]) -> Result<Solid> {
        use std::sync::atomic::Ordering::Relaxed;
        let t = std::time::Instant::now();
        let shape = native::read_brep(data)?;
        READ_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
        let t = std::time::Instant::now();
        let r = Solid::from_shape(shape).validated();
        CHECK_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
        r
    }
}

pub(crate) fn history_from(h: &[u32]) -> History {
    History {
        links: h
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| (c[0] as u8, FaceId(c[1]), FaceId(c[2])))
            .collect(),
    }
}

pub(crate) fn finish(r: native::OpResult) -> Result<OpResult> {
    let solid = Solid::from_shape(r.shape).validated()?;
    Ok(OpResult {
        solid,
        history: history_from(&r.history),
    })
}

/// Like [`finish`] without the check of the shape: for a step in the middle of an operation whose result is checked (the sharp solid after
/// the rounds were taken out; the solid that is made from it is checked). The cancel flag and the number of solids are still looked at.
pub(crate) fn finish_loose(r: native::OpResult) -> Result<OpResult> {
    if native::cancel_requested() {
        return Err(KernelError::Cancelled);
    }
    let solid = Solid::from_shape(r.shape);
    if native::solid_count(&solid.shape) != 1 {
        return Err(KernelError::SplitResult);
    }
    Ok(OpResult {
        solid,
        history: history_from(&r.history),
    })
}

pub(crate) use native as nat;
