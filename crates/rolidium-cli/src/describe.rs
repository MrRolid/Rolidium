// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Describing the model for a language model (bodies, faces, edges, sketches, curves with the numbers and ids other
//! commands use) and finding faces or edges by a query.

use glam::DVec3;
use rolidium_doc::Document;
use rolidium_kernel::{EdgeId, EdgeKind, FaceId, FaceKind, Mesh, Solid};
use serde_json::{json, Value};

/// Bodies with more faces than this are only summarised (no list of faces and edges) unless `full` is asked for.
pub const SMALL_BODY_FACES: usize = 60;

fn r3(v: DVec3) -> Value {
    json!([round(v.x), round(v.y), round(v.z)])
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0 + 0.0
}

fn kind_name(k: FaceKind) -> &'static str {
    match k {
        FaceKind::Plane => "plane",
        FaceKind::Cylinder => "cylinder",
        FaceKind::Cone => "cone",
        FaceKind::Sphere => "sphere",
        FaceKind::Torus => "torus",
        FaceKind::Other => "other",
    }
}

/// What is known about the faces and edges of one body, for describing and querying.
pub struct BodyGeometry {
    pub faces: Vec<FaceRow>,
    pub edges: Vec<EdgeRow>,
}

pub struct FaceRow {
    pub id: u32,
    pub kind: FaceKind,
    pub area: f64,
    pub center: DVec3,
    /// Plane: outward normal; cylinder, cone, torus: axis; else zero.
    pub normal: DVec3,
    pub radius: f64,
    pub holes: u32,
    pub edges: Vec<u32>,
}

pub struct EdgeRow {
    pub id: u32,
    pub kind: EdgeKind,
    pub length: f64,
    pub start: DVec3,
    pub end: DVec3,
    pub mid: DVec3,
    pub radius: f64,
    /// Centre of a circle or an arc.
    pub center: Option<DVec3>,
    pub faces: Vec<u32>,
}

fn circumcentre(a: DVec3, b: DVec3, c: DVec3) -> Option<DVec3> {
    let (ab, ac) = (b - a, c - a);
    let n = ab.cross(ac);
    let d = 2.0 * n.length_squared();
    if d < 1e-18 {
        return None;
    }
    Some(a + (n.cross(ab) * ac.length_squared() + ac.cross(n) * ab.length_squared()) / d)
}

/// Collects the geometry of the faces and edges (the mesh gives centres and midpoints).
pub fn geometry(solid: &Solid) -> Result<BodyGeometry, String> {
    let mesh: Mesh = solid.display_mesh().map_err(|e| e.to_string())?;
    // area-weighted centre of every face from its triangles
    let nf = solid.face_count();
    let mut sum = vec![DVec3::ZERO; nf];
    let mut weight = vec![0.0f64; nf];
    for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let f = mesh.tri_face[t] as usize;
        if f >= nf {
            continue;
        }
        let p = |i: u32| mesh.positions[i as usize].as_dvec3();
        let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
        let area = (b - a).cross(c - a).length() * 0.5;
        sum[f] += (a + b + c) / 3.0 * area;
        weight[f] += area;
    }
    let mut faces = Vec::new();
    let mut face_edges: Vec<Vec<u32>> = vec![Vec::new(); nf];
    for f in solid.faces() {
        let info = solid.face_info(f).map_err(|e| e.to_string())?;
        let es: Vec<u32> = solid
            .face_edges(f)
            .map_err(|e| e.to_string())?
            .iter()
            .map(|e| e.0)
            .collect();
        face_edges[f.0 as usize] = es.clone();
        let i = f.0 as usize;
        let center = if weight[i] > 0.0 {
            sum[i] / weight[i]
        } else {
            info.origin
        };
        faces.push(FaceRow {
            id: f.0,
            kind: info.kind,
            area: info.area,
            center,
            normal: info.normal,
            radius: info.radius,
            holes: info.inner_wires,
            edges: es,
        });
    }
    let mut edges = Vec::new();
    for e in solid.edges() {
        let info = solid.edge_info(e).map_err(|x| x.to_string())?;
        let poly = mesh.edges.iter().find(|p| p.edge == e).map(|p| &p.points);
        let pts: Vec<DVec3> = poly
            .map(|p| p.iter().map(|v| v.as_dvec3()).collect())
            .unwrap_or_default();
        let mid = if pts.is_empty() {
            (info.start + info.end) * 0.5
        } else {
            // the middle of the polyline by length
            let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            let mut run = 0.0;
            let mut at = pts[0];
            for w in pts.windows(2) {
                let l = w[0].distance(w[1]);
                if run + l >= total / 2.0 && l > 0.0 {
                    at = w[0].lerp(w[1], (total / 2.0 - run) / l);
                    break;
                }
                run += l;
            }
            at
        };
        let center = if info.kind == EdgeKind::Circle && pts.len() >= 3 {
            let n = pts.len();
            circumcentre(pts[0], pts[n / 3], pts[2 * n / 3])
        } else {
            None
        };
        let faces_of: Vec<u32> = face_edges
            .iter()
            .enumerate()
            .filter(|(_, es)| es.contains(&e.0))
            .map(|(i, _)| i as u32)
            .collect();
        edges.push(EdgeRow {
            id: e.0,
            kind: info.kind,
            length: info.length,
            start: info.start,
            end: info.end,
            mid,
            radius: info.radius,
            center,
            faces: faces_of,
        });
    }
    Ok(BodyGeometry { faces, edges })
}

fn face_json(f: &FaceRow) -> Value {
    let mut o =
        json!({"id": f.id, "kind": kind_name(f.kind), "area_mm2": round(f.area), "center": r3(f.center)});
    match f.kind {
        FaceKind::Plane => o["normal"] = r3(f.normal),
        FaceKind::Cylinder | FaceKind::Cone | FaceKind::Torus => {
            o["axis"] = r3(f.normal);
            o["radius_mm"] = json!(round(f.radius));
        }
        FaceKind::Sphere => o["radius_mm"] = json!(round(f.radius)),
        FaceKind::Other => {}
    }
    if f.holes > 0 {
        o["holes"] = json!(f.holes);
    }
    o["edges"] = json!(f.edges);
    o
}

fn edge_json(e: &EdgeRow) -> Value {
    let kind = match e.kind {
        EdgeKind::Line => "line",
        EdgeKind::Circle => "circle",
        EdgeKind::Other => "other",
    };
    let mut o = json!({"id": e.id, "kind": kind, "length_mm": round(e.length), "start": r3(e.start), "end": r3(e.end), "mid": r3(e.mid), "faces": e.faces});
    if e.kind == EdgeKind::Circle {
        o["radius_mm"] = json!(round(e.radius));
        if let Some(c) = e.center {
            o["center"] = r3(c);
        }
    }
    o
}

/// The description of the document. `only`: the 1-based number of one body to describe in full.
pub fn describe(
    doc: &Document,
    only: Option<usize>,
    full: bool,
    current_sketch: Option<usize>,
) -> Result<Value, String> {
    let mut bodies = Vec::new();
    for (i, b) in doc.bodies().iter().enumerate() {
        if only.is_some_and(|n| n != i + 1) {
            continue;
        }
        let props = b.solid.props().map_err(|e| e.to_string())?;
        let mat = b.material_or_default();
        let mut o = json!({
            "number": i + 1,
            "name": b.name,
            "visible": b.visible,
            "volume_mm3": round(props.volume),
            "area_mm2": round(props.area),
            "mass_g": b.mass_g().map(round),
            "material": {"name": mat.name, "density_g_cm3": mat.density},
            "bbox": {"min": r3(props.min), "max": r3(props.max)},
            "size": r3(props.max - props.min),
            "centroid": r3(props.centroid),
            "face_count": b.solid.face_count(),
            "edge_count": b.solid.edge_count(),
            "valid": b.solid.is_valid(),
        });
        if full || only.is_some() || b.solid.face_count() <= SMALL_BODY_FACES {
            let g = geometry(&b.solid)?;
            o["faces"] = Value::Array(g.faces.iter().map(face_json).collect());
            o["edges"] = Value::Array(g.edges.iter().map(edge_json).collect());
        } else {
            o["note"] = json!("large body: faces and edges are not listed; ask for this body by number or use select_geometry");
        }
        bodies.push(o);
    }
    let sketches: Vec<Value> = doc
        .sketches()
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let regions: Vec<Value> = s
                .sketch
                .regions()
                .iter()
                .enumerate()
                .map(|(k, r)| json!({"index": k, "area_mm2": round(r.area()), "holes": r.holes.len()}))
                .collect();
            json!({
                "number": i + 1,
                "name": s.name,
                "current": current_sketch == Some(i + 1),
                "plane": {"origin": r3(s.plane.origin), "normal": r3(s.plane.normal), "x_dir": r3(s.plane.x_dir)},
                "shapes": s.sketch.entities().len(),
                "regions": regions,
            })
        })
        .collect();
    let curves: Vec<Value> = doc
        .curves()
        .iter()
        .enumerate()
        .map(|(i, c)| json!({"number": i + 1, "name": c.name, "length_mm": round(c.def.path().map_or(0.0, |p| p.length))}))
        .collect();
    Ok(json!({
        "units": "mm, degrees, g",
        "axes": "Z is up; sketch on xy: x=X y=Y; on xz: x=X y=Z; on yz: x=Y y=Z",
        "bodies": bodies,
        "sketches": sketches,
        "curves": curves,
    }))
}

/// A direction word: +x -x +y -y +z -z.
pub fn direction(word: &str) -> Result<DVec3, String> {
    Ok(match word {
        "+x" | "x" => DVec3::X,
        "-x" => -DVec3::X,
        "+y" | "y" => DVec3::Y,
        "-y" => -DVec3::Y,
        "+z" | "z" => DVec3::Z,
        "-z" => -DVec3::Z,
        o => return Err(format!("unknown direction '{o}' (use +x -x +y -y +z -z)")),
    })
}

/// A query for faces or edges.
#[derive(Default)]
pub struct Query {
    pub edges: bool,
    pub normal: Option<DVec3>,
    pub surface: Option<String>,
    pub extreme: Option<DVec3>,
    pub of_face: Option<u32>,
    pub edge_kind: Option<String>,
    pub min_area: f64,
    pub limit: usize,
}

pub fn select(solid: &Solid, q: &Query) -> Result<Value, String> {
    let g = geometry(solid)?;
    let limit = if q.limit == 0 { 50 } else { q.limit };
    if !q.edges {
        let mut rows: Vec<&FaceRow> = g
            .faces
            .iter()
            .filter(|f| {
                q.normal
                    .is_none_or(|n| f.kind == FaceKind::Plane && f.normal.dot(n) > 0.99)
            })
            .filter(|f| q.surface.as_deref().is_none_or(|s| kind_name(f.kind) == s))
            .filter(|f| f.area >= q.min_area)
            .collect();
        if let Some(d) = q.extreme {
            let best = rows
                .iter()
                .map(|f| f.center.dot(d))
                .fold(f64::NEG_INFINITY, f64::max);
            rows.retain(|f| (f.center.dot(d) - best).abs() < 1e-6);
        }
        let total = rows.len();
        let list: Vec<Value> = rows.iter().take(limit).map(|f| face_json(f)).collect();
        Ok(json!({"kind": "face", "count": total, "faces": list}))
    } else {
        let mut rows: Vec<&EdgeRow> = g
            .edges
            .iter()
            .filter(|e| q.of_face.is_none_or(|f| e.faces.contains(&f)))
            .filter(|e| {
                q.edge_kind.as_deref().is_none_or(|k| {
                    matches!(
                        (k, e.kind),
                        ("line", EdgeKind::Line) | ("circle", EdgeKind::Circle) | ("other", EdgeKind::Other)
                    )
                })
            })
            .collect();
        if let Some(d) = q.extreme {
            let best = rows
                .iter()
                .map(|e| e.mid.dot(d))
                .fold(f64::NEG_INFINITY, f64::max);
            rows.retain(|e| (e.mid.dot(d) - best).abs() < 1e-6);
        }
        let total = rows.len();
        let list: Vec<Value> = rows.iter().take(limit).map(|e| edge_json(e)).collect();
        Ok(json!({"kind": "edge", "count": total, "edges": list}))
    }
}

/// Used by the picture: the centre of a face and the midpoint of an edge, for the labels.
pub type Labels = Vec<(u32, DVec3)>;

pub fn label_points(solid: &Solid, faces: &[u32], edges: &[u32]) -> Result<(Labels, Labels), String> {
    let g = geometry(solid)?;
    let f = faces
        .iter()
        .filter_map(|id| g.faces.iter().find(|r| r.id == *id).map(|r| (*id, r.center)))
        .collect();
    let e = edges
        .iter()
        .filter_map(|id| g.edges.iter().find(|r| r.id == *id).map(|r| (*id, r.mid)))
        .collect();
    Ok((f, e))
}

pub fn face_id(n: u32) -> FaceId {
    FaceId(n)
}

pub fn edge_id(n: u32) -> EdgeId {
    EdgeId(n)
}
