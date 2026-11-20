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
mod looptool;
mod measure;
mod mesh;
mod occt;
mod ops;
mod sweep;
pub mod timing;

pub use blend::{largest_working_size, CHAIN_ANGLE_DEG};
pub use glam::{DVec3, Vec3};
/// Angular deflection (rad) of the display meshes. It is only a safety limit: the density is set by the linear deflection (a fraction
/// of a pixel), so a shape that is small on the screen is not made denser than its size in pixels asks for.
pub const VIEW_ANGLE: f64 = 0.5;
/// The display mesh deviates from the exact shape by at most this fraction of a pixel.
pub const VIEW_PIXEL_FRACTION: f64 = 0.4;
/// The first display mesh after an operation is rough: its deflection is this fraction of the size of the part (about two pixels of a
/// window of 900 px); the finer mesh for the zoom is made afterwards in the background.
pub const FIRST_DISPLAY_FRACTION: f64 = 0.002;
/// At most this many triangles are made for the faces in view in one go: meshing them costs about 15 microseconds a triangle when few
/// faces are meshed at once (no parallelism), and the background work must be done in about a second and a half.
pub const FINE_TRIANGLE_BUDGET: usize = 60_000;

pub use looptool::{LoopEdge, LoopError, PlanarLoop};
pub use measure::{measure, Angle, Elem, Measurement};
pub use mesh::{EdgePolyline, Mesh};
pub use ops::{
    BlendCombo, BlendGroup, BlendInput, CylinderInfo, LoftInput, Profile, ProfileSegment, SectionCurve,
};
pub use sweep::{
    polyline_segments, HelixDef, Path, PathSeg, PathSource, SweepOptions, SweepResult, SweepWarning,
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

/// Asks the running operation (on any thread) to stop as soon as it can. The flag stays set until [`reset_cancel`].
pub fn request_cancel() {
    native::set_cancel(true);
}

/// Clears the flag; every job calls this before it starts.
pub fn reset_cancel() {
    native::set_cancel(false);
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

    /// Triangulation tuned for display: the chord error scales with the size of the part.
    pub fn display_mesh(&self) -> Result<Mesh> {
        // only the box is needed here; the full properties (volume, area) cost much more
        let b = native::bounds_of(&self.shape)?;
        let diag = (DVec3::new(b[3], b[4], b[5]) - DVec3::new(b[0], b[1], b[2])).length();
        self.mesh((diag * FIRST_DISPLAY_FRACTION).clamp(0.005, 0.5), VIEW_ANGLE)
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
    fn view_linear(&self, pixel_mm: f64) -> Result<(f64, f64)> {
        let b = native::bounds_of(&self.shape)?;
        let diag = (DVec3::new(b[3], b[4], b[5]) - DVec3::new(b[0], b[1], b[2]))
            .length()
            .max(1e-6);
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
        let is_fine = |i: usize| fine.get(i).copied().unwrap_or(false);
        let fixed: f64 = (0..n).filter(|i| !is_fine(*i)).map(|i| per_face[i] as f64).sum();
        let grows: f64 = (0..n)
            .filter(|i| is_fine(*i))
            .map(|i| per_face[i] as f64 * defl(i))
            .sum();
        // the faces in view get at most the budget, and the whole mesh stays under the cap (aim a little lower: the estimate is rough)
        let room = ((max_triangles as f64 - fixed).max(max_triangles as f64 * 0.2))
            .min(FINE_TRIANGLE_BUDGET as f64)
            * 0.9;
        if grows > 0.0 && grows / linear > room {
            linear = (grows / room).max(linear);
        }
        let mask: Vec<u8> = (0..n).map(|i| is_fine(i) as u8).collect();
        if !mask.contains(&1) {
            return Ok(base.clone());
        }
        let mut angle = VIEW_ANGLE;
        let mut out = base.clone();
        for attempt in 0..4 {
            let part = Mesh::from_native(native::mesh_shape_regions(
                &self.shape,
                linear,
                angle,
                linear,
                &mask,
            )?);
            out = base.merged_with(&part, &mask);
            out.deflection = linear;
            out.face_deflection = (0..n)
                .map(|i| if is_fine(i) { requested } else { defl(i) })
                .collect();
            if out.triangle_count() as f64 <= max_triangles as f64 * 1.25 || attempt == 3 {
                break;
            }
            // the estimate can be off: coarser again (also in the angle) while the cap is passed
            let ratio = (out.triangle_count() as f64 / max_triangles as f64).max(1.5);
            linear *= ratio;
            angle = (angle * ratio.sqrt()).min(0.8);
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
    pub fn write_brep(&self) -> Result<Vec<u8>> {
        Ok(native::write_brep(&self.shape)?)
    }
    pub fn read_brep(data: &[u8]) -> Result<Solid> {
        Solid::from_shape(native::read_brep(data)?).validated()
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

pub(crate) use native as nat;
