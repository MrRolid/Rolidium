// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Raw `cxx` bridge to Open CASCADE. The only module in the workspace that touches OCCT.
//! Everything here is wrapped by the safe API in `lib.rs`.

#[allow(clippy::too_many_arguments)]
#[cxx::bridge(namespace = "rol")]
pub mod ffi {
    /// Triangle mesh of a shape plus polylines of its topological edges.
    struct MeshData {
        positions: Vec<f32>,
        normals: Vec<f32>,
        indices: Vec<u32>,
        /// One face index per triangle.
        tri_face: Vec<u32>,
        /// Edge polyline points (xyz triples).
        edge_points: Vec<f32>,
        /// `edge_offsets[i]..edge_offsets[i+1]` are the points (not floats) of polyline `i`.
        edge_offsets: Vec<u32>,
        /// Edge index of each polyline.
        edge_ids: Vec<u32>,
        /// 1 for an edge whose two faces continue each other smoothly (or a seam of one face), else 0.
        edge_smooth: Vec<u8>,
        /// One byte per face: 1 when the face was triangulated with the fine deflection.
        face_fine: Vec<u8>,
    }

    struct FaceInfo {
        /// 0 plane, 1 cylinder, 2 cone, 3 sphere, 4 torus, 5 other
        kind: u32,
        /// Plane: outward unit normal. Cylinder/cone: axis. Otherwise zero.
        nx: f64,
        ny: f64,
        nz: f64,
        /// Plane/cylinder/cone/sphere/torus: a point on the axis or plane origin.
        ox: f64,
        oy: f64,
        oz: f64,
        radius: f64,
        area: f64,
        /// Number of inner wires (holes).
        inner_wires: u32,
    }

    struct EdgeInfo {
        /// 0 line, 1 circle (full or arc), 2 other
        kind: u32,
        sx: f64,
        sy: f64,
        sz: f64,
        ex: f64,
        ey: f64,
        ez: f64,
        radius: f64,
        length: f64,
    }

    struct Props {
        volume: f64,
        area: f64,
        min: [f64; 3],
        max: [f64; 3],
        centroid: [f64; 3],
    }

    struct OpResult {
        shape: SharedPtr<Shape>,
        /// Triples `(source_arg, source_face, result_face)`; `source_arg` 0 is the
        /// first operand, 1 the second.
        history: Vec<u32>,
    }

    unsafe extern "C++" {
        include!("bridge.h");

        type Shape;
        fn solid_count(shape: &Shape) -> u32;

        fn make_box(x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> Result<SharedPtr<Shape>>;
        fn make_cylinder(
            radius: f64,
            height: f64,
            origin: [f64; 3],
            axis: [f64; 3],
        ) -> Result<SharedPtr<Shape>>;
        fn make_cone(
            r1: f64,
            r2: f64,
            height: f64,
            origin: [f64; 3],
            axis: [f64; 3],
        ) -> Result<SharedPtr<Shape>>;
        /// `plane` = origin, normal, x direction (9 numbers). See `Profile` encoding in lib.rs.
        fn make_planar_face(plane: [f64; 9], profile: &[f64]) -> Result<SharedPtr<Shape>>;
        fn revolve(shape: &Shape, point: [f64; 3], dir: [f64; 3], angle: f64) -> Result<SharedPtr<Shape>>;
        fn transform_shape(
            shape: &Shape,
            pivot: [f64; 3],
            axis: [f64; 3],
            angle: f64,
            t: [f64; 3],
        ) -> Result<OpResult>;
        /// A compound of the solids of `acc` (none when `first`) and the solids of `s` (used as one tool of a boolean).
        fn compound_append(acc: &Shape, s: &Shape, first: bool) -> Result<SharedPtr<Shape>>;
        fn mirror_shape(shape: &Shape, point: [f64; 3], normal: [f64; 3]) -> Result<OpResult>;
        fn scale_shape(shape: &Shape, centre: [f64; 3], f: [f64; 3]) -> Result<OpResult>;
        fn solid_from_surfaces(
            pts: &[f64],
            point_tol: &[f64],
            surf_kind: &[u32],
            surf_params: &[f64],
            edge_kind: &[u32],
            edge_v: &[u32],
            edge_params: &[f64],
            edge_tol: &[f64],
            loop_edges: &[u32],
            loop_offsets: &[u32],
            face_offsets: &[u32],
            face_surf: &[u32],
            face_area: &[f64],
            tol: f64,
        ) -> Result<SharedPtr<Shape>>;
        fn solid_from_polygons(
            pts: &[f64],
            loop_vertices: &[u32],
            loop_offsets: &[u32],
            face_offsets: &[u32],
            point_tol: &[f64],
            fallback_vertices: &[u32],
            fallback_offsets: &[u32],
            tol: f64,
        ) -> Result<SharedPtr<Shape>>;
        fn prism(shape: &Shape, v: [f64; 3]) -> Result<SharedPtr<Shape>>;

        fn point_in_solid(shape: &Shape, p: [f64; 3]) -> bool;
        fn point_outside(shape: &Shape, p: [f64; 3]) -> bool;
        fn solid_at(shape: &Shape, index: u32) -> Result<SharedPtr<Shape>>;
        fn face_count(shape: &Shape) -> u32;
        fn edge_count(shape: &Shape) -> u32;
        fn face_info(shape: &Shape, index: u32) -> Result<FaceInfo>;
        fn edge_info(shape: &Shape, index: u32) -> Result<EdgeInfo>;
        fn face_shape(shape: &Shape, index: u32) -> Result<SharedPtr<Shape>>;
        /// Boundary of a face: points (xyz floats) and polyline start offsets (in points).
        fn face_outline(
            shape: &Shape,
            index: u32,
            deflection: f64,
            points: &mut Vec<f32>,
            offsets: &mut Vec<u32>,
        ) -> Result<()>;
        /// Indices of the edges bounding a face.
        fn face_edges(shape: &Shape, index: u32) -> Result<Vec<u32>>;

        /// `op`: 0 fuse, 1 cut (a - b), 2 common.
        fn boolean_op(a: &Shape, b: &Shape, op: u32, unify: bool) -> Result<OpResult>;
        fn unify_same_domain(shape: &Shape) -> Result<OpResult>;
        fn fillet_edges(shape: &Shape, edges: &[u32], radius: f64) -> Result<OpResult>;
        fn defeature_faces(shape: &Shape, faces: &[u32]) -> Result<OpResult>;
        fn blend_scan(shape: &Shape) -> Result<Vec<f64>>;
        fn blend_chain(shape: &Shape, edges: &[u32], chamfer: bool) -> Result<Vec<u32>>;
        fn fillet_edges_radii(shape: &Shape, edges: &[u32], radii: &[f64]) -> Result<OpResult>;
        fn chamfer_edges_asym(
            shape: &Shape,
            edges: &[u32],
            faces: &[u32],
            d1: &[f64],
            d2: &[f64],
        ) -> Result<OpResult>;
        fn chamfer_edges(shape: &Shape, edges: &[u32], distance: f64) -> Result<OpResult>;
        type Sewer;
        fn new_sewer(tolerance: f64) -> UniquePtr<Sewer>;
        fn sewer_add(sewer: Pin<&mut Sewer>, shape: &Shape) -> Result<()>;
        fn sewer_finish(sewer: Pin<&mut Sewer>) -> Result<SharedPtr<Shape>>;

        fn mesh_shape(shape: &Shape, linear: f64, angular: f64) -> Result<MeshData>;
        fn mesh_shape_regions(
            shape: &Shape,
            linear: f64,
            angular: f64,
            min_size: f64,
            fine: &[u8],
        ) -> Result<MeshData>;
        fn check_shape(shape: &Shape) -> bool;
        fn props(shape: &Shape) -> Result<Props>;
        /// Section with a plane: flat encoding, see `section` in lib.rs.
        fn section_plane(
            shape: &Shape,
            origin: [f64; 3],
            normal: [f64; 3],
            tolerance: f64,
        ) -> Result<Vec<f64>>;

        fn edge_face(shape: &Shape, index: u32) -> Result<SharedPtr<Shape>>;
        fn edge_ends(shape: &Shape, index: u32) -> Result<Vec<f64>>;
        fn edge_geom(shape: &Shape, index: u32) -> Result<Vec<f64>>;
        fn has_self_intersections(shape: &Shape) -> bool;
        fn shape_distance(
            a: &Shape,
            ka: [u32; 2],
            pa: [f64; 3],
            b: &Shape,
            kb: [u32; 2],
            pb: [f64; 3],
        ) -> Result<Vec<f64>>;
        fn set_cancel(on: bool);
        fn volume_of(shape: &Shape) -> Result<f64>;
        fn shape_diag(shape: &Shape) -> Result<String>;
        fn solid_distance(a: &Shape, b: &Shape) -> Result<f64>;
        fn bounds_of(shape: &Shape) -> Result<Vec<f64>>;
        fn face_scales(shape: &Shape) -> Result<Vec<f64>>;
        fn take_stages() -> String;
        fn set_threads(n: i32);
        fn shared_keys(shape: &Shape) -> Result<Vec<u64>>;
        fn cancel_requested() -> bool;
        fn take_thread_stats() -> Vec<f64>;
        fn take_thread_worst() -> String;
        fn make_path(data: &[f64]) -> Result<SharedPtr<Shape>>;
        fn edges_wire(shape: &Shape, edges: &[u32]) -> Result<SharedPtr<Shape>>;
        fn path_info(path: &Shape) -> Result<Vec<f64>>;
        fn path_points(path: &Shape, deflection: f64) -> Result<Vec<f64>>;
        fn sweep_face(
            face: &Shape,
            path: &Shape,
            fixed: bool,
            ax: f64,
            ay: f64,
            az: f64,
            rounded: bool,
        ) -> Result<SharedPtr<Shape>>;
        type Loft;
        fn new_loft(ruled: bool) -> UniquePtr<Loft>;
        fn loft_add(l: Pin<&mut Loft>, face: &Shape) -> Result<()>;
        fn loft_finish(l: Pin<&mut Loft>) -> Result<SharedPtr<Shape>>;
        type StepWriter;
        fn new_step_writer() -> UniquePtr<StepWriter>;
        fn step_writer_add(w: Pin<&mut StepWriter>, shape: &Shape) -> Result<()>;
        fn step_writer_add_named(w: Pin<&mut StepWriter>, shape: &Shape, name: &str) -> Result<()>;
        fn read_step_names(data: &[u8]) -> Result<Vec<String>>;
        fn step_writer_finish(w: Pin<&mut StepWriter>) -> Result<Vec<u8>>;
        fn write_step(shape: &Shape) -> Result<Vec<u8>>;
        fn read_step(data: &[u8]) -> Result<SharedPtr<Shape>>;
        fn write_brep(shape: &Shape) -> Result<Vec<u8>>;
        fn read_brep(data: &[u8]) -> Result<SharedPtr<Shape>>;
    }
}

// Shapes are immutable once built and every native call is serialised by a global lock.
unsafe impl Send for ffi::Shape {}
unsafe impl Sync for ffi::Shape {}
