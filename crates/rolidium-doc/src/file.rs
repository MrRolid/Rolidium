// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The `.rolidium` file: a zip container, see `docs/FILE-FORMAT.md`.
//!
//! Reading keeps everything it does not understand (unknown zip entries, unknown JSON fields) in
//! [`Extras`]; writing puts it back unchanged, so a program without some module never destroys that
//! module's data.

use crate::{BlendKind, Body, BodyId, Document, Feature, Material, SketchDoc, SketchId, State};
use crate::{CurveDef, CurveDoc};
use rolidium_kernel::{DVec3, FaceId, Solid};
use rolidium_sketch::{DVec2, Entity, PlaneDef, Sketch};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The newest file format this program can read and write.
pub const FORMAT_VERSION: u32 = 1;

const MAX_ENTRY: u64 = 2 << 30;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("This is not a valid Rolidium file or it is damaged: {0}. Nothing was changed.")]
    Corrupt(String),
    #[error("This file was saved by a newer Rolidium (file format {file}; this program understands up to {known}). It was not opened and not changed. Please update Rolidium.")]
    Newer { file: u32, known: u32 },
    #[error("{0}")]
    Io(String),
}

/// Content of a file that this program does not interpret.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    /// Unknown top-level fields of `manifest.json`.
    pub manifest: Map<String, Value>,
    /// Zip entries that are not part of the known layout (for example `cam/...`), and the preview.
    pub entries: BTreeMap<String, Vec<u8>>,
    /// Unknown fields of the body and sketch JSON files.
    pub bodies: HashMap<BodyId, Map<String, Value>>,
    pub sketches: HashMap<SketchId, Map<String, Value>>,
    pub curves: HashMap<u64, Map<String, Value>>,
}

/// Everything read from a file.
pub struct Loaded {
    pub state: State,
    pub next_id: u64,
    pub extras: Extras,
    pub program_version: String,
    /// Bodies whose recognised rounds and chamfers could not be restored (reported to the user).
    pub dropped_features: usize,
}

fn corrupt<E: std::fmt::Display>(e: E) -> FileError {
    FileError::Corrupt(e.to_string())
}

fn arr3(v: DVec3) -> Value {
    json!([v.x, v.y, v.z])
}
fn arr2(v: DVec2) -> Value {
    json!([v.x, v.y])
}
fn get_f(v: &Value) -> Result<f64, FileError> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| corrupt("a number is missing or invalid"))
}
fn get3(v: &Value) -> Result<DVec3, FileError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or_else(|| corrupt("a 3D vector is missing"))?;
    Ok(DVec3::new(get_f(&a[0])?, get_f(&a[1])?, get_f(&a[2])?))
}
fn get2(v: &Value) -> Result<DVec2, FileError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| corrupt("a 2D point is missing"))?;
    Ok(DVec2::new(get_f(&a[0])?, get_f(&a[1])?))
}

fn entity_json(e: &Entity) -> Value {
    match *e {
        Entity::Line { a, b } => json!({"type": "line", "a": arr2(a), "b": arr2(b)}),
        Entity::Rect { a, b } => json!({"type": "rect", "a": arr2(a), "b": arr2(b)}),
        Entity::Axis { a, b } => json!({"type": "axis", "a": arr2(a), "b": arr2(b)}),
        Entity::Circle { center, radius } => {
            json!({"type": "circle", "center": arr2(center), "radius": radius})
        }
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            json!({"type": "arc", "center": arr2(center), "radius": radius, "start": start, "sweep": sweep})
        }
    }
}

fn entity_from(v: &Value) -> Result<Option<Entity>, FileError> {
    Ok(match v["type"].as_str() {
        Some("line") => Some(Entity::Line {
            a: get2(&v["a"])?,
            b: get2(&v["b"])?,
        }),
        Some("axis") => Some(Entity::Axis {
            a: get2(&v["a"])?,
            b: get2(&v["b"])?,
        }),
        Some("rect") => Some(Entity::Rect {
            a: get2(&v["a"])?,
            b: get2(&v["b"])?,
        }),
        Some("arc") => Some(Entity::Arc {
            center: get2(&v["center"])?,
            radius: get_f(&v["radius"])?,
            start: get_f(&v["start"])?,
            sweep: get_f(&v["sweep"])?,
        }),
        Some("circle") => Some(Entity::Circle {
            center: get2(&v["center"])?,
            radius: get_f(&v["radius"])?,
        }),
        _ => None, // a shape kind from a newer program: kept as it is
    })
}

/// Serialises a document. `preview` is the PNG of the model, if one is available.
fn curve_json(def: &CurveDef, j: &mut Map<String, Value>) {
    match def {
        CurveDef::Polyline { points, radius } => {
            j.insert("type".into(), json!("polyline"));
            j.insert(
                "points".into(),
                json!(points.iter().map(|p| arr3(*p)).collect::<Vec<_>>()),
            );
            j.insert("radius".into(), json!(radius));
        }
        CurveDef::Helix(h) => {
            j.insert("type".into(), json!("helix"));
            j.insert("origin".into(), json!(arr3(h.origin)));
            j.insert("axis".into(), json!(arr3(h.axis)));
            j.insert("x_dir".into(), json!(arr3(h.x_dir)));
            j.insert("radius".into(), json!(h.radius));
            j.insert("pitch".into(), json!(h.pitch));
            j.insert("turns".into(), json!(h.turns));
            j.insert("right".into(), json!(h.right));
            j.insert("start_deg".into(), json!(h.start_deg));
            j.insert("cone_deg".into(), json!(h.cone_deg));
        }
    }
}

fn curve_from(j: &Map<String, Value>) -> Result<Option<CurveDef>, FileError> {
    let num = |k: &str| {
        j.get(k)
            .and_then(Value::as_f64)
            .ok_or_else(|| corrupt(format!("curve field {k}")))
    };
    Ok(match j.get("type").and_then(Value::as_str) {
        Some("polyline") => {
            let pts = j
                .get("points")
                .and_then(Value::as_array)
                .ok_or_else(|| corrupt("curve points"))?;
            let points = pts.iter().map(get3).collect::<Result<Vec<_>, _>>()?;
            Some(CurveDef::Polyline {
                points,
                radius: j.get("radius").and_then(Value::as_f64).unwrap_or(0.0),
            })
        }
        Some("helix") => Some(CurveDef::Helix(rolidium_kernel::HelixDef {
            origin: get3(&j["origin"])?,
            axis: get3(&j["axis"])?,
            x_dir: get3(&j["x_dir"])?,
            radius: num("radius")?,
            pitch: num("pitch")?,
            turns: num("turns")?,
            right: j.get("right").and_then(Value::as_bool).unwrap_or(true),
            start_deg: j.get("start_deg").and_then(Value::as_f64).unwrap_or(0.0),
            cone_deg: j.get("cone_deg").and_then(Value::as_f64).unwrap_or(0.0),
        })),
        _ => None,
    })
}

fn axis_json(a: &crate::Axis) -> Value {
    json!({"origin": arr3(a.origin), "dir": arr3(a.dir)})
}

fn axis_from(v: &Value) -> Result<crate::Axis, FileError> {
    Ok(crate::Axis {
        origin: get3(&v["origin"])?,
        dir: get3(&v["dir"])?,
    })
}

fn ref_json(r: &crate::LoftRef) -> Value {
    match r {
        crate::LoftRef::Region { sketch, region } => {
            json!({"kind": "region", "sketch": sketch, "region": region})
        }
        crate::LoftRef::Face { body, face } => json!({"kind": "face", "body": body, "face": face.0}),
        crate::LoftRef::EdgeLoop { body, edge } => json!({"kind": "edge_loop", "body": body, "edge": edge}),
    }
}

fn ref_from(j: &Value) -> Result<crate::LoftRef, FileError> {
    let n = |k: &str| j[k].as_u64().ok_or_else(|| corrupt("a profile without a number"));
    Ok(match j["kind"].as_str() {
        Some("region") => crate::LoftRef::Region {
            sketch: n("sketch")?,
            region: n("region")? as usize,
        },
        Some("face") => crate::LoftRef::Face {
            body: n("body")?,
            face: rolidium_kernel::FaceId(n("face")? as u32),
        },
        Some("edge_loop") => crate::LoftRef::EdgeLoop {
            body: n("body")?,
            edge: n("edge")? as u32,
        },
        _ => return Err(corrupt("a profile of an unknown kind")),
    })
}

fn path_json(p: &crate::SweepPath) -> Value {
    match p {
        crate::SweepPath::Curve(c) => json!({"kind": "curve", "curve": c}),
        crate::SweepPath::Sketch { sketch, entities } => {
            json!({"kind": "sketch", "sketch": sketch, "entities": entities})
        }
        crate::SweepPath::Edges { body, edges } => json!({"kind": "edges", "body": body, "edges": edges}),
    }
}

fn path_from(j: &Value) -> Result<crate::SweepPath, FileError> {
    let list = |k: &str| -> Vec<u64> {
        j[k].as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default()
    };
    let n = |k: &str| j[k].as_u64().ok_or_else(|| corrupt("a path without a number"));
    Ok(match j["kind"].as_str() {
        Some("curve") => crate::SweepPath::Curve(n("curve")?),
        Some("sketch") => crate::SweepPath::Sketch {
            sketch: n("sketch")?,
            entities: list("entities").into_iter().map(|e| e as _).collect(),
        },
        Some("edges") => crate::SweepPath::Edges {
            body: n("body")?,
            edges: list("edges").into_iter().map(|e| e as u32).collect(),
        },
        _ => return Err(corrupt("a path of an unknown kind")),
    })
}

fn anchor_json(a: &crate::SketchAnchor) -> Value {
    use crate::PlaneKind;
    let (kind, extra) = match &a.plane {
        PlaneKind::AxisPlane => ("axis_plane", None),
        PlaneKind::NormalToTangent => ("normal_to_tangent", None),
        PlaneKind::NormalToAxis => ("normal_to_axis", None),
        PlaneKind::ParallelXY => ("parallel_xy", None),
        PlaneKind::ParallelXZ => ("parallel_xz", None),
        PlaneKind::ParallelYZ => ("parallel_yz", None),
        PlaneKind::Free { normal, x_dir } => ("free", Some((*normal, *x_dir))),
    };
    let mut j = json!({
        "curve": a.curve,
        "end": if a.end == crate::CurveEnd::Start { "start" } else { "end" },
        "plane": kind,
    });
    if let Some((n, x)) = extra {
        j["normal"] = arr3(n);
        j["x_dir"] = arr3(x);
    }
    j
}

/// The anchor of a sketch from its JSON; none when it names a curve that is not in the file or a kind this program does not know (the sketch stays).
fn anchor_from(a: &Value, curves: &[crate::CurveDoc]) -> Option<crate::SketchAnchor> {
    use crate::PlaneKind;
    let curve = a["curve"]
        .as_u64()
        .filter(|c| curves.iter().any(|x| x.id == *c))?;
    let end = match a["end"].as_str()? {
        "start" => crate::CurveEnd::Start,
        "end" => crate::CurveEnd::End,
        _ => return None,
    };
    let plane = match a["plane"].as_str()? {
        "axis_plane" => PlaneKind::AxisPlane,
        "normal_to_tangent" => PlaneKind::NormalToTangent,
        "normal_to_axis" => PlaneKind::NormalToAxis,
        "parallel_xy" => PlaneKind::ParallelXY,
        "parallel_xz" => PlaneKind::ParallelXZ,
        "parallel_yz" => PlaneKind::ParallelYZ,
        "free" => PlaneKind::Free {
            normal: get3(&a["normal"]).ok()?,
            x_dir: get3(&a["x_dir"]).ok()?,
        },
        _ => return None,
    };
    Some(crate::SketchAnchor { curve, end, plane })
}

/// The JSON of a link (without the solid it started from, which is a BREP entry of its own).
fn link_json(l: &crate::Link) -> Value {
    use crate::{LinkKind, PatternMode};
    let mut j = Map::new();
    j.insert("name".into(), json!(l.name));
    j.insert("sources".into(), json!(l.sources));
    j.insert("results".into(), json!(l.results));
    j.insert("error".into(), json!(l.error));
    let mode = |m: &PatternMode, j: &mut Map<String, Value>| {
        let (name, target) = match m {
            PatternMode::New => ("new", None),
            PatternMode::Add(t) => ("add", Some(*t)),
            PatternMode::Cut(t) => ("cut", Some(*t)),
        };
        j.insert("mode".into(), json!(name));
        j.insert("target".into(), json!(target));
    };
    match &l.kind {
        LinkKind::Linear(p, m) => {
            j.insert("type".into(), json!("linear"));
            mode(m, &mut j);
            j.insert("axis".into(), axis_json(&p.axis));
            j.insert("count".into(), json!(p.count));
            j.insert("spacing".into(), json!(p.spacing));
            j.insert("by_total".into(), json!(p.by_total));
            j.insert(
                "second".into(),
                match &p.second {
                    Some(s) => json!({"axis": axis_json(&s.axis), "count": s.count, "spacing": s.spacing, "by_total": s.by_total}),
                    None => Value::Null,
                },
            );
        }
        LinkKind::Circular(p, m) => {
            j.insert("type".into(), json!("circular"));
            mode(m, &mut j);
            j.insert("axis".into(), axis_json(&p.axis));
            j.insert("count".into(), json!(p.count));
            j.insert("angle_deg".into(), json!(p.angle_deg));
            j.insert("full".into(), json!(p.full));
            j.insert("keep_orientation".into(), json!(p.keep_orientation));
        }
        LinkKind::Mirror(m) => {
            j.insert("type".into(), json!("mirror"));
            if !matches!(m.into, PatternMode::New) {
                mode(&m.into, &mut j);
            }
            j.insert("origin".into(), arr3(m.origin));
            j.insert("normal".into(), arr3(m.normal));
        }
        LinkKind::Loft {
            profiles,
            ruled,
            mode: m,
        } => {
            j.insert("type".into(), json!("loft"));
            mode(m, &mut j);
            j.insert(
                "profiles".into(),
                Value::Array(profiles.iter().map(ref_json).collect()),
            );
            j.insert("ruled".into(), json!(ruled));
        }
        LinkKind::Sweep {
            profile,
            path,
            rounded,
            mode: m,
        } => {
            j.insert("type".into(), json!("sweep"));
            mode(m, &mut j);
            j.insert("profile".into(), ref_json(profile));
            j.insert("path".into(), path_json(path));
            j.insert("rounded".into(), json!(rounded));
        }
    }
    if let Some(f) = &l.fill {
        j.insert("fill".into(), fill_json(f));
    }
    Value::Object(j)
}

fn fill_json(f: &crate::FillRule) -> Value {
    use crate::{RectFill, RectRef};
    let rect = match &f.rect {
        RectRef::Face { origin, normal, near } => {
            json!({"face": {"origin": arr3(*origin), "normal": arr3(*normal), "near": arr3(*near)}})
        }
        RectRef::Sketch { sketch, entity } => json!({"sketch": {"id": sketch, "entity": entity}}),
    };
    let how = match f.fill {
        RectFill::Corners => json!({"corners": true}),
        RectFill::Grid {
            columns,
            rows,
            margin,
        } => json!({"columns": columns, "rows": rows, "margin": margin}),
    };
    json!({
        "rect": rect,
        "how": how,
        "sign_a": f.sides.sign_a,
        "near_a": f.sides.near_a,
        "sign_b": f.sides.sign_b,
        "near_b": f.sides.near_b,
        "at": arr3(f.at),
    })
}

fn fill_from(v: &Value) -> Result<Option<crate::FillRule>, FileError> {
    use crate::{EdgeSides, FillRule, RectFill, RectRef};
    if !v.is_object() {
        return Ok(None);
    }
    let r = &v["rect"];
    let rect = if r["face"].is_object() {
        RectRef::Face {
            origin: get3(&r["face"]["origin"])?,
            normal: get3(&r["face"]["normal"])?,
            near: get3(&r["face"]["near"])?,
        }
    } else if r["sketch"].is_object() {
        RectRef::Sketch {
            sketch: r["sketch"]["id"]
                .as_u64()
                .ok_or_else(|| corrupt("a link without a sketch"))?,
            entity: r["sketch"]["entity"]
                .as_u64()
                .ok_or_else(|| corrupt("a link without a shape"))? as u32,
        }
    } else {
        return Ok(None);
    };
    let h = &v["how"];
    let fill = if h["columns"].is_u64() {
        RectFill::Grid {
            columns: h["columns"].as_u64().unwrap_or(1) as u32,
            rows: h["rows"].as_u64().unwrap_or(1) as u32,
            margin: get_f(&h["margin"])?,
        }
    } else {
        RectFill::Corners
    };
    Ok(Some(FillRule {
        rect,
        fill,
        sides: EdgeSides {
            sign_a: get_f(&v["sign_a"])?,
            near_a: get_f(&v["near_a"])?,
            sign_b: get_f(&v["sign_b"])?,
            near_b: get_f(&v["near_b"])?,
        },
        at: get3(&v["at"])?,
    }))
}

/// A link from its JSON; `None` when its kind is not known to this program (the entry is then kept as it is).
fn link_from(
    id: u64,
    j: &Value,
    base: Option<Solid>,
    state: &crate::State,
) -> Result<Option<crate::Link>, FileError> {
    let bodies = &state.bodies[..];
    use crate::{CircularParams, LinearParams, LinkKind, MirrorParams, PatternMode};
    let ids = |k: &str| -> Vec<u64> {
        j[k].as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default()
    };
    let mode = || -> Result<PatternMode, FileError> {
        let t = j["target"].as_u64();
        Ok(match (j["mode"].as_str().unwrap_or("new"), t) {
            ("add", Some(t)) => PatternMode::Add(t),
            ("cut", Some(t)) => PatternMode::Cut(t),
            _ => PatternMode::New,
        })
    };
    let kind = match j["type"].as_str() {
        Some("linear") => LinkKind::Linear(
            LinearParams {
                axis: axis_from(&j["axis"])?,
                count: j["count"]
                    .as_u64()
                    .ok_or_else(|| corrupt("a link without a count"))? as u32,
                spacing: get_f(&j["spacing"])?,
                by_total: j["by_total"].as_bool().unwrap_or(false),
                second: match &j["second"] {
                    Value::Object(_) => Some(crate::Direction2 {
                        axis: axis_from(&j["second"]["axis"])?,
                        count: j["second"]["count"]
                            .as_u64()
                            .ok_or_else(|| corrupt("a link without a count"))?
                            as u32,
                        spacing: get_f(&j["second"]["spacing"])?,
                        by_total: j["second"]["by_total"].as_bool().unwrap_or(false),
                    }),
                    _ => None,
                },
            },
            mode()?,
        ),
        Some("circular") => LinkKind::Circular(
            CircularParams {
                axis: axis_from(&j["axis"])?,
                count: j["count"]
                    .as_u64()
                    .ok_or_else(|| corrupt("a link without a count"))? as u32,
                angle_deg: get_f(&j["angle_deg"])?,
                full: j["full"].as_bool().unwrap_or(true),
                keep_orientation: j["keep_orientation"].as_bool().unwrap_or(false),
            },
            mode()?,
        ),
        Some("mirror") => LinkKind::Mirror(MirrorParams {
            origin: get3(&j["origin"])?,
            normal: get3(&j["normal"])?,
            into: mode()?,
        }),
        Some("loft") => LinkKind::Loft {
            profiles: j["profiles"]
                .as_array()
                .ok_or_else(|| corrupt("a loft without profiles"))?
                .iter()
                .map(ref_from)
                .collect::<Result<_, _>>()?,
            ruled: j["ruled"].as_bool().unwrap_or(false),
            mode: mode()?,
        },
        Some("sweep") => LinkKind::Sweep {
            profile: ref_from(&j["profile"])?,
            path: path_from(&j["path"])?,
            rounded: j["rounded"].as_bool().unwrap_or(false),
            mode: mode()?,
        },
        _ => return Ok(None),
    };
    let sources = ids("sources");
    let results = ids("results");
    // a link that points to bodies that are not in the file is dropped (its bodies stay as ordinary bodies)
    if sources
        .iter()
        .chain(results.iter())
        .any(|b| !bodies.iter().any(|x| x.id == *b))
    {
        return Ok(None);
    }
    if kind.mode().target().is_some() && base.is_none() {
        return Ok(None);
    }
    // a loft or a sweep whose sketches or curves are not in the file is dropped too (its body stays)
    let deps = crate::generate::deps(&kind);
    if deps
        .sketches
        .iter()
        .any(|s| !state.sketches.iter().any(|x| x.id == *s))
        || deps
            .curves
            .iter()
            .any(|c| !state.curves.iter().any(|x| x.id == *c))
        || deps.bodies.iter().any(|b| !bodies.iter().any(|x| x.id == *b))
    {
        return Ok(None);
    }
    let fill = fill_from(&j["fill"])?;
    let seen = sources
        .iter()
        .filter_map(|s| bodies.iter().find(|b| b.id == *s))
        .map(|b| b.solid.clone())
        .collect();
    let mut link = crate::Link {
        id,
        name: j["name"].as_str().unwrap_or("Link").to_string(),
        kind,
        sources,
        results,
        base,
        seen,
        error: j["error"].as_str().map(str::to_string),
        warnings: Vec::new(),
        stamp: 0,
        fill,
    };
    link.stamp = crate::links::stamp_of(state, &link);
    Ok(Some(link))
}

pub fn save_to_bytes(
    doc: &Document,
    preview: Option<&[u8]>,
    program_version: &str,
) -> Result<Vec<u8>, FileError> {
    let st = doc.state();
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let put = |zip: &mut zip::ZipWriter<&mut std::io::Cursor<Vec<u8>>>,
                   name: &str,
                   data: &[u8]|
         -> Result<(), FileError> {
            zip.start_file(name, opts)
                .map_err(|e| FileError::Io(e.to_string()))?;
            zip.write_all(data).map_err(|e| FileError::Io(e.to_string()))
        };
        let mut manifest = doc.extras.manifest.clone();
        manifest.insert("format_version".into(), json!(FORMAT_VERSION));
        manifest.insert("program_version".into(), json!(program_version));
        manifest.insert("units".into(), json!("mm"));
        manifest.insert("next_id".into(), json!(doc.next_id));
        manifest.insert("body_counter".into(), json!(st.body_counter));
        manifest.insert("sketch_counter".into(), json!(st.sketch_counter));
        manifest.insert("curve_counter".into(), json!(st.curve_counter));
        manifest.insert("link_counter".into(), json!(st.link_counter));
        manifest.insert("group_counter".into(), json!(st.group_counter));
        let mut bodies = Vec::new();
        for b in &st.bodies {
            let brep = b.solid.write_brep().map_err(|e| FileError::Io(e.to_string()))?;
            put(&mut zip, &format!("bodies/{}.brep", b.id), &brep)?;
            let mut meta = doc.extras.bodies.get(&b.id).cloned().unwrap_or_default();
            meta.insert("name".into(), json!(b.name));
            meta.insert("color".into(), json!(b.color));
            meta.insert("visible".into(), json!(b.visible));
            match &b.material {
                Some(m) => {
                    meta.insert("material".into(), json!({"name": m.name, "density": m.density}));
                }
                None => {
                    meta.remove("material");
                }
            }
            meta.insert("face_count".into(), json!(b.solid.face_count()));
            meta.insert(
                "features".into(),
                Value::Array(
                    b.features
                        .iter()
                        .map(|f| {
                            json!({
                                "kind": match f.kind { BlendKind::Round => "round", BlendKind::Chamfer => "chamfer" },
                                "size": f.size,
                                "angle": f.angle,
                                "flip": f.flip,
                                "faces": f.faces.iter().map(|x| x.0).collect::<Vec<_>>(),
                            })
                        })
                        .collect(),
                ),
            );
            put(
                &mut zip,
                &format!("bodies/{}.json", b.id),
                &serde_json::to_vec_pretty(&meta).map_err(corrupt)?,
            )?;
            bodies.push(json!({"id": b.id, "brep": format!("bodies/{}.brep", b.id), "meta": format!("bodies/{}.json", b.id)}));
        }
        let mut sketches = Vec::new();
        for s in &st.sketches {
            let mut j = doc.extras.sketches.get(&s.id).cloned().unwrap_or_default();
            j.insert("name".into(), json!(s.name));
            j.insert(
                "plane".into(),
                json!({"origin": arr3(s.plane.origin), "normal": arr3(s.plane.normal), "x_dir": arr3(s.plane.x_dir)}),
            );
            j.insert("target_body".into(), json!(s.target));
            j.insert("visible".into(), json!(s.visible));
            if s.fill {
                j.insert("fill".into(), json!(true));
            }
            match &s.anchor {
                Some(a) => {
                    j.insert("anchor".into(), anchor_json(a));
                }
                None => {
                    j.remove("anchor");
                }
            }
            j.insert(
                "entities".into(),
                Value::Array(s.sketch.entities().iter().map(|(_, e)| entity_json(e)).collect()),
            );
            // shapes of a newer program that this one cannot show, written back as they were
            if let Some(Value::Array(u)) = j.remove("entities_unknown") {
                if let Some(Value::Array(list)) = j.get_mut("entities") {
                    list.extend(u);
                }
            }
            put(
                &mut zip,
                &format!("sketches/{}.json", s.id),
                &serde_json::to_vec_pretty(&j).map_err(corrupt)?,
            )?;
            sketches.push(json!({"id": s.id, "file": format!("sketches/{}.json", s.id)}));
        }
        manifest.insert("bodies".into(), Value::Array(bodies));
        manifest.insert("sketches".into(), Value::Array(sketches));
        if !st.curves.is_empty() {
            let mut curves = Vec::new();
            for c in &st.curves {
                let mut j = doc.extras.curves.get(&c.id).cloned().unwrap_or_default();
                j.insert("name".into(), json!(c.name));
                j.insert("visible".into(), json!(c.visible));
                curve_json(&c.def, &mut j);
                match c.limit_height {
                    Some(l) => j.insert("limit_height".into(), json!(l)),
                    None => j.remove("limit_height"),
                };
                put(
                    &mut zip,
                    &format!("curves/{}.json", c.id),
                    &serde_json::to_vec_pretty(&j).map_err(corrupt)?,
                )?;
                curves.push(json!({"id": c.id, "file": format!("curves/{}.json", c.id)}));
            }
            manifest.insert("curves".into(), Value::Array(curves));
        } else {
            manifest.remove("curves");
        }
        // links (patterns, mirrors) and groups of bodies
        if !st.links.is_empty() {
            let mut links = Vec::new();
            for l in &st.links {
                put(
                    &mut zip,
                    &format!("links/{}.json", l.id),
                    &serde_json::to_vec_pretty(&link_json(l)).map_err(corrupt)?,
                )?;
                let mut entry = json!({"id": l.id, "file": format!("links/{}.json", l.id)});
                if let Some(b) = &l.base {
                    let brep = b.write_brep().map_err(|e| FileError::Io(e.to_string()))?;
                    put(&mut zip, &format!("links/{}.base.brep", l.id), &brep)?;
                    entry["base"] = json!(format!("links/{}.base.brep", l.id));
                }
                links.push(entry);
            }
            manifest.insert("links".into(), Value::Array(links));
        } else {
            manifest.remove("links");
        }
        if !st.groups.is_empty() {
            manifest.insert(
                "groups".into(),
                Value::Array(
                    st.groups
                        .iter()
                        .map(|g| json!({"id": g.id, "name": g.name, "parent": g.parent, "visible": g.visible, "bodies": g.bodies}))
                        .collect(),
                ),
            );
        } else {
            manifest.remove("groups");
        }
        put(
            &mut zip,
            "manifest.json",
            &serde_json::to_vec_pretty(&manifest).map_err(corrupt)?,
        )?;
        let mut entries = doc.extras.entries.clone();
        if let Some(p) = preview {
            entries.insert("preview.png".into(), p.to_vec());
        }
        for (name, data) in &entries {
            put(&mut zip, name, data)?;
        }
        zip.finish().map_err(|e| FileError::Io(e.to_string()))?;
    }
    Ok(buf.into_inner())
}

fn read_entry(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, FileError> {
    let mut f = zip
        .by_name(name)
        .map_err(|_| corrupt(format!("the entry '{name}' is missing")))?;
    if f.size() > MAX_ENTRY {
        return Err(corrupt(format!("the entry '{name}' is too large")));
    }
    let mut v = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut v)
        .map_err(|e| corrupt(format!("'{name}': {e}")))?;
    Ok(v)
}

fn take_keys(map: &mut Map<String, Value>, keys: &[&str]) {
    for k in keys {
        map.remove(*k);
    }
}

/// Reads a file. Older format versions are upgraded here (`migrate`); newer ones are refused.
pub fn load_from_bytes(bytes: &[u8]) -> Result<Loaded, FileError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(corrupt)?;
    let manifest_bytes = read_entry(&mut zip, "manifest.json")?;
    let mut manifest: Map<String, Value> = match serde_json::from_slice(&manifest_bytes).map_err(corrupt)? {
        Value::Object(m) => m,
        _ => return Err(corrupt("manifest.json is not an object")),
    };
    let version = manifest
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| corrupt("the file format version is missing"))? as u32;
    if version > FORMAT_VERSION {
        return Err(FileError::Newer {
            file: version,
            known: FORMAT_VERSION,
        });
    }
    migrate(&mut manifest, version);

    let mut known: Vec<String> = vec!["manifest.json".into()];
    let mut extras = Extras::default();
    let mut state = State {
        body_counter: manifest.get("body_counter").and_then(Value::as_u64).unwrap_or(0) as u32,
        sketch_counter: manifest
            .get("sketch_counter")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        curve_counter: manifest.get("curve_counter").and_then(Value::as_u64).unwrap_or(0) as u32,
        link_counter: manifest.get("link_counter").and_then(Value::as_u64).unwrap_or(0) as u32,
        group_counter: manifest.get("group_counter").and_then(Value::as_u64).unwrap_or(0) as u32,
        ..Default::default()
    };
    let mut max_id = 0u64;
    let mut dropped = 0usize;
    let body_list = manifest
        .get("bodies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // The bodies are independent of each other: their shapes are read on several threads (the zip is read in one go first).
    let t_zip = std::time::Instant::now();
    let brep_data: Vec<Option<Vec<u8>>> = body_list
        .iter()
        .map(|b| b["brep"].as_str().and_then(|n| read_entry(&mut zip, n).ok()))
        .collect();
    rolidium_kernel::timing::record(
        "open: unzip the BREP entries (one thread)",
        t_zip.elapsed().as_secs_f64() * 1e3,
    );
    let t_read = std::time::Instant::now();
    let mut solids_read: Vec<Option<Result<Solid, String>>> = rolidium_kernel::par::map(&brep_data, |d| {
        d.as_ref().map(|d| Solid::read_brep(d).map_err(|e| e.to_string()))
    });
    rolidium_kernel::timing::record(
        "open: read the shapes and check them (threads)",
        t_read.elapsed().as_secs_f64() * 1e3,
    );
    for (bi, b) in body_list.iter().enumerate() {
        let id = b["id"].as_u64().ok_or_else(|| corrupt("a body without an id"))?;
        let (brep_name, meta_name) = (
            b["brep"]
                .as_str()
                .ok_or_else(|| corrupt("a body without geometry"))?
                .to_string(),
            b["meta"]
                .as_str()
                .ok_or_else(|| corrupt("a body without data"))?
                .to_string(),
        );
        let solid = match solids_read[bi].take() {
            Some(Ok(s)) => s,
            Some(Err(e)) => return Err(corrupt(format!("body {id}: {e}"))),
            None => {
                // the entry could not be read in the first pass: the same error as before
                let data = read_entry(&mut zip, &brep_name)?;
                Solid::read_brep(&data).map_err(|e| corrupt(format!("body {id}: {e}")))?
            }
        };
        let mut meta: Map<String, Value> =
            match serde_json::from_slice(&read_entry(&mut zip, &meta_name)?).map_err(corrupt)? {
                Value::Object(m) => m,
                _ => return Err(corrupt("body data is not an object")),
            };
        known.push(brep_name);
        known.push(meta_name);
        let mut features = Vec::new();
        let faces_ok = meta
            .get("face_count")
            .and_then(Value::as_u64)
            .is_none_or(|n| n as usize == solid.face_count());
        if let Some(list) = meta.get("features").and_then(Value::as_array) {
            for f in list {
                let kind = match f["kind"].as_str() {
                    Some("round") => BlendKind::Round,
                    Some("chamfer") => BlendKind::Chamfer,
                    _ => continue,
                };
                let faces: Vec<FaceId> = f["faces"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_u64())
                            .map(|x| FaceId(x as u32))
                            .collect()
                    })
                    .unwrap_or_default();
                if faces_ok && !faces.is_empty() && faces.iter().all(|x| (x.0 as usize) < solid.face_count())
                {
                    features.push(Feature {
                        kind,
                        size: f["size"].as_f64().unwrap_or(0.0),
                        faces,
                        angle: f["angle"].as_f64().unwrap_or(45.0),
                        flip: f["flip"].as_bool().unwrap_or(false),
                    });
                } else {
                    dropped += 1;
                }
            }
        }
        let color = meta
            .get("color")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 3)
            .map(|a| [0, 1, 2].map(|i| a[i].as_f64().unwrap_or(0.7) as f32))
            .unwrap_or([0.72, 0.76, 0.82]);
        let material = meta.get("material").and_then(|m| {
            let d = m.get("density")?.as_f64().filter(|d| d.is_finite() && *d > 0.0)?;
            Some(Material {
                name: m
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("custom")
                    .to_string(),
                density: d,
            })
        });
        let body = Body {
            id,
            name: meta
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Body")
                .to_string(),
            solid,
            color,
            visible: meta.get("visible").and_then(Value::as_bool).unwrap_or(true),
            features,
            material,
        };
        take_keys(
            &mut meta,
            &["name", "color", "visible", "face_count", "features", "material"],
        );
        if !meta.is_empty() {
            extras.bodies.insert(id, meta);
        }
        max_id = max_id.max(id);
        state.bodies.push(body);
    }
    let mut pending_anchors: Vec<(u64, Value)> = Vec::new();
    for s in manifest
        .get("sketches")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let id = s["id"]
            .as_u64()
            .ok_or_else(|| corrupt("a sketch without an id"))?;
        let name = s["file"]
            .as_str()
            .ok_or_else(|| corrupt("a sketch without data"))?
            .to_string();
        let mut j: Map<String, Value> =
            match serde_json::from_slice(&read_entry(&mut zip, &name)?).map_err(corrupt)? {
                Value::Object(m) => m,
                _ => return Err(corrupt("sketch data is not an object")),
            };
        known.push(name);
        let plane = PlaneDef {
            origin: get3(&j["plane"]["origin"])?,
            normal: get3(&j["plane"]["normal"])?,
            x_dir: get3(&j["plane"]["x_dir"])?,
        };
        let mut sketch = Sketch::new();
        let mut unknown = Vec::new();
        for e in j
            .get("entities")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            match entity_from(&e)? {
                Some(ent) => {
                    sketch
                        .add(ent)
                        .map_err(|er| corrupt(format!("sketch {id}: {er}")))?;
                }
                None => unknown.push(e),
            }
        }
        let doc = SketchDoc {
            id,
            name: j
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Sketch")
                .to_string(),
            plane,
            target: j.get("target_body").and_then(Value::as_u64),
            sketch,
            visible: j.get("visible").and_then(Value::as_bool).unwrap_or(true),
            fill: j.get("fill").and_then(Value::as_bool).unwrap_or(false),
            anchor: None,
        };
        // the curves are read after the sketches: the anchor is put on when they are there
        if let Some(a) = j.get("anchor") {
            pending_anchors.push((id, a.clone()));
        }
        take_keys(
            &mut j,
            &[
                "name",
                "plane",
                "target_body",
                "entities",
                "visible",
                "fill",
                "anchor",
            ],
        );
        if !unknown.is_empty() {
            j.insert("entities_unknown".into(), Value::Array(unknown));
        }
        if !j.is_empty() {
            extras.sketches.insert(id, j);
        }
        max_id = max_id.max(id);
        state.sketches.push(doc);
    }
    for c in manifest
        .get("curves")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let id = c["id"].as_u64().ok_or_else(|| corrupt("a curve without an id"))?;
        let name = c["file"]
            .as_str()
            .ok_or_else(|| corrupt("a curve without data"))?
            .to_string();
        let mut j: Map<String, Value> =
            match serde_json::from_slice(&read_entry(&mut zip, &name)?).map_err(corrupt)? {
                Value::Object(m) => m,
                _ => return Err(corrupt("curve data is not an object")),
            };
        // a kind of curve this program does not know stays in the file as it is (not shown, not lost)
        let Some(def) = curve_from(&j)? else { continue };
        known.push(name);
        state.curves.push(CurveDoc {
            id,
            name: j
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Curve")
                .to_string(),
            def,
            visible: j.get("visible").and_then(Value::as_bool).unwrap_or(true),
            limit_height: j.get("limit_height").and_then(Value::as_f64).filter(|l| *l > 0.0),
        });
        take_keys(
            &mut j,
            &[
                "name",
                "visible",
                "type",
                "points",
                "radius",
                "origin",
                "axis",
                "x_dir",
                "pitch",
                "turns",
                "right",
                "start_deg",
                "cone_deg",
                "limit_height",
            ],
        );
        if !j.is_empty() {
            extras.curves.insert(id, j);
        }
        max_id = max_id.max(id);
    }
    // the sketches laid on the end of a curve (an anchor to a curve that is not in the file is dropped, the sketch stays)
    for (id, a) in pending_anchors {
        let anchor = anchor_from(&a, &state.curves);
        if let Some(s) = state.sketches.iter_mut().find(|s| s.id == id) {
            s.anchor = anchor;
        }
    }
    // links and groups (files of an older program have none)
    for l in manifest
        .get("links")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let (Some(id), Some(file)) = (l["id"].as_u64(), l["file"].as_str()) else {
            continue;
        };
        let j: Value = serde_json::from_slice(&read_entry(&mut zip, file)?).map_err(corrupt)?;
        let base = match l["base"].as_str() {
            Some(n) => read_entry(&mut zip, n)
                .ok()
                .and_then(|d| Solid::read_brep(&d).ok()),
            None => None,
        };
        if let Some(link) = link_from(id, &j, base, &state)? {
            known.push(file.to_string());
            if let Some(n) = l["base"].as_str() {
                known.push(n.to_string());
            }
            max_id = max_id.max(id);
            state.links.push(link);
        }
    }
    for g in manifest
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(id) = g["id"].as_u64() else { continue };
        state.groups.push(crate::Group {
            id,
            name: g["name"].as_str().unwrap_or("Group").to_string(),
            parent: g["parent"].as_u64(),
            visible: g["visible"].as_bool().unwrap_or(true),
            bodies: g["bodies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_u64)
                        .filter(|b| state.bodies.iter().any(|x| x.id == *b))
                        .collect()
                })
                .unwrap_or_default(),
        });
        max_id = max_id.max(id);
    }
    // a group whose parent is missing goes to the top
    let ids: Vec<u64> = state.groups.iter().map(|g| g.id).collect();
    for g in state.groups.iter_mut() {
        if g.parent.is_some_and(|p| !ids.contains(&p) || p == g.id) {
            g.parent = None;
        }
    }
    for i in 0..zip.len() {
        let name = zip
            .by_index(i)
            .map_err(corrupt)?
            .name()
            .map_err(corrupt)?
            .to_string();
        if name.ends_with('/') || known.contains(&name) {
            continue;
        }
        extras.entries.insert(name.clone(), read_entry(&mut zip, &name)?);
    }
    let program_version = manifest
        .get("program_version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    take_keys(
        &mut manifest,
        &[
            "format_version",
            "program_version",
            "units",
            "next_id",
            "body_counter",
            "sketch_counter",
            "curve_counter",
            "link_counter",
            "group_counter",
            "bodies",
            "sketches",
            "curves",
            "links",
            "groups",
        ],
    );
    extras.manifest = manifest;
    let mut state = state;
    crate::refresh_features(&mut state);
    Ok(Loaded {
        state,
        next_id: max_id + 1,
        extras,
        program_version,
        dropped_features: dropped,
    })
}

/// Upgrades a manifest of an older format version to the current one. Version 1 is the first, so
/// there is nothing to do yet; this is where migrations will go (one step per version).
fn migrate(_manifest: &mut Map<String, Value>, _from: u32) {}

impl Document {
    /// Replaces the content with a loaded file; the document counts as saved. The undo history starts empty.
    pub fn from_loaded(l: Loaded) -> Document {
        let mut d = Document::new();
        d.current = Arc::new(l.state);
        d.next_id = l.next_id.max(1);
        d.extras = l.extras;
        d.mark_saved();
        d
    }
}

// ---- files on disk ----

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes the temporary file next to `path` (first half of an atomic save).
pub fn write_temp(path: &Path, bytes: &[u8]) -> Result<PathBuf, FileError> {
    let tmp = sibling(path, &format!(".tmp{}", std::process::id()));
    let r = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    match r {
        Ok(()) => Ok(tmp),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(FileError::Io(format!("cannot write {}: {e}", path.display())))
        }
    }
}

/// Second half: the previous file becomes `<name>.bak`, the temporary file takes its place.
pub fn commit_temp(tmp: &Path, path: &Path) -> Result<(), FileError> {
    let bak = sibling(path, ".bak");
    let had = path.exists();
    if had {
        std::fs::rename(path, &bak).map_err(|e| FileError::Io(format!("cannot make a backup: {e}")))?;
    }
    if let Err(e) = std::fs::rename(tmp, path) {
        if had {
            let _ = std::fs::rename(&bak, path);
        }
        let _ = std::fs::remove_file(tmp);
        return Err(FileError::Io(format!("cannot save {}: {e}", path.display())));
    }
    Ok(())
}

/// Atomic save: nothing of the previous file is lost if the program dies or the disk is full.
pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    let tmp = write_temp(path, bytes)?;
    commit_temp(&tmp, path)
}

pub fn load_file(path: &Path) -> Result<Loaded, FileError> {
    let bytes =
        std::fs::read(path).map_err(|e| FileError::Io(format!("cannot read {}: {e}", path.display())))?;
    load_from_bytes(&bytes)
}

pub fn save_file(
    doc: &Document,
    path: &Path,
    preview: Option<&[u8]>,
    program_version: &str,
) -> Result<(), FileError> {
    save_atomic(path, &save_to_bytes(doc, preview, program_version)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, PullMode, SketchTarget};

    fn tmpdir(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp")
            .join(format!("doc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Box with a through hole, a rounded edge, a loose sketch and a second body.
    fn sample() -> Document {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "Plate".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let id = d.bodies()[0].id;
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
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -20.0,
            mode: PullMode::Auto,
        })
        .unwrap();
        let s = &d.body(id).unwrap().solid;
        let edge = s
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
            edges: vec![edge],
            size: 5.0,
            chamfer: false,
        })
        .unwrap();
        d.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::new(100.0, 0.0, 0.0),
            max: DVec3::new(110.0, 10.0, 10.0),
        })
        .unwrap();
        d.apply(Command::AddEntity {
            target: SketchTarget::New {
                plane: PlaneDef::base(),
                body: None,
            },
            entity: Entity::Rect {
                a: DVec2::new(1.0, 2.0),
                b: DVec2::new(8.0, 9.0),
            },
        })
        .unwrap();
        d
    }

    fn rewrite(
        bytes: &[u8],
        f: impl Fn(&str, Vec<u8>) -> Option<(String, Vec<u8>)>,
        extra: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let mut zin = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            for i in 0..zin.len() {
                let mut e = zin.by_index(i).unwrap();
                let name = e.name().unwrap().to_string();
                let mut data = Vec::new();
                e.read_to_end(&mut data).unwrap();
                if let Some((n, d)) = f(&name, data) {
                    w.start_file(n, o).unwrap();
                    w.write_all(&d).unwrap();
                }
            }
            for (n, d) in extra {
                w.start_file(*n, o).unwrap();
                w.write_all(d).unwrap();
            }
            w.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn save_and_open_give_the_same_document() {
        let d = sample();
        let bytes = save_to_bytes(&d, Some(b"\x89PNG-test"), "9.9.9").unwrap();
        let l = load_from_bytes(&bytes).unwrap();
        assert_eq!(l.program_version, "9.9.9");
        let e = Document::from_loaded(l);
        assert_eq!(e.bodies().len(), 2);
        for (a, b) in d.bodies().iter().zip(e.bodies()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.color, b.color);
            assert_eq!(a.solid.face_count(), b.solid.face_count());
            assert!((a.solid.volume().unwrap() - b.solid.volume().unwrap()).abs() < 1e-6);
            assert_eq!(a.features, b.features);
        }
        assert_eq!(d.sketches().len(), e.sketches().len());
        let (a, b) = (&d.sketches()[0], &e.sketches()[0]);
        assert_eq!(a.name, b.name);
        assert_eq!(a.plane, b.plane);
        assert_eq!(a.target, b.target);
        assert_eq!(
            a.sketch.entities().iter().map(|x| x.1).collect::<Vec<_>>(),
            b.sketch.entities().iter().map(|x| x.1).collect::<Vec<_>>()
        );
        assert!(!e.is_dirty());
        assert_eq!(
            e.extras.entries.get("preview.png").map(|v| v.as_slice()),
            Some(&b"\x89PNG-test"[..])
        );
    }

    #[test]
    fn material_is_saved_and_a_file_without_one_is_unchanged() {
        let mut d = sample();
        let id = d.bodies()[0].id;
        // a body whose material was never chosen: nothing about materials in the file
        let plain = save_to_bytes(&d, None, "t").unwrap();
        let text = String::from_utf8_lossy(&plain).to_string();
        assert!(!text.contains("material"));
        let e = Document::from_loaded(load_from_bytes(&plain).unwrap());
        assert_eq!(e.body(id).unwrap().material, None);
        assert_eq!(e.body(id).unwrap().material_or_default().name, "steel");
        // a chosen material survives saving and opening; the mass follows from the volume
        d.apply(crate::Command::SetMaterial {
            body: id,
            material: Some(Material::preset(4)),
        })
        .unwrap();
        let e = Document::from_loaded(load_from_bytes(&save_to_bytes(&d, None, "t").unwrap()).unwrap());
        let b = e.body(id).unwrap();
        assert_eq!(b.material, Some(Material::preset(4)));
        assert!((b.mass_g().unwrap() - b.solid.volume().unwrap() / 1000.0 * 1.24).abs() < 1e-9);
        // a custom density, and undo back to the default
        d.apply(crate::Command::SetMaterial {
            body: id,
            material: Some(Material {
                name: "custom".into(),
                density: 3.3,
            }),
        })
        .unwrap();
        let e = Document::from_loaded(load_from_bytes(&save_to_bytes(&d, None, "t").unwrap()).unwrap());
        assert_eq!(e.body(id).unwrap().material.as_ref().unwrap().density, 3.3);
        assert!(d
            .apply(crate::Command::SetMaterial {
                body: id,
                material: Some(Material {
                    name: "x".into(),
                    density: -1.0
                })
            })
            .is_err());
    }

    #[test]
    fn the_mass_of_a_cylinder() {
        let mut d = Document::new();
        d.apply(crate::Command::AddBox {
            name: "b".into(),
            min: DVec3::ZERO,
            max: DVec3::ONE,
        })
        .unwrap();
        let id = d.bodies()[0].id;
        let cyl = Solid::cylinder(10.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
        d.apply(crate::Command::AddSolids { solids: vec![cyl] }).unwrap();
        let c = d.bodies().last().unwrap();
        let expected = std::f64::consts::PI * 100.0 * 30.0 / 1000.0 * 7.85; // steel
        assert!(
            (c.mass_g().unwrap() - expected).abs() < 1e-6 * expected,
            "{:?} vs {expected}",
            c.mass_g()
        );
        let _ = id;
    }

    #[test]
    fn rounds_can_be_edited_after_opening() {
        let d = sample();
        let mut e = Document::from_loaded(load_from_bytes(&save_to_bytes(&d, None, "t").unwrap()).unwrap());
        let id = e.bodies()[0].id;
        assert_eq!(e.body(id).unwrap().features.len(), 1);
        let face = e.body(id).unwrap().features[0].faces[0];
        let before = e.body(id).unwrap().solid.volume().unwrap();
        e.apply(Command::EditBlend {
            body: id,
            face,
            size: 8.0,
            chain: true,
        })
        .unwrap();
        assert!(e.body(id).unwrap().solid.volume().unwrap() < before);
        let face = e.body(id).unwrap().features[0].faces[0];
        e.apply(Command::EditBlend {
            body: id,
            face,
            size: 0.0,
            chain: true,
        })
        .unwrap();
        assert!(e.body(id).unwrap().features.is_empty());
        // new items after opening get fresh ids
        e.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::ZERO,
            max: DVec3::ONE,
        })
        .unwrap();
        let ids: Vec<_> = e.bodies().iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids[2] > ids[1] && ids[1] > ids[0]);
    }

    #[test]
    fn unknown_content_survives_open_and_save() {
        let d = sample();
        let bytes = save_to_bytes(&d, None, "t").unwrap();
        let first_body = d.bodies()[0].id;
        let patched = rewrite(
            &bytes,
            |name, data| {
                let mut data = data;
                let edit = |data: &mut Vec<u8>, key: &str, val: Value| {
                    let mut m: Map<String, Value> = serde_json::from_slice(data).unwrap();
                    m.insert(key.into(), val);
                    *data = serde_json::to_vec(&m).unwrap();
                };
                if name == "manifest.json" {
                    edit(&mut data, "cam", json!({"tool": 6}));
                } else if name == format!("bodies/{first_body}.json") {
                    edit(&mut data, "cam_stock", json!("steel"));
                } else if name.starts_with("sketches/") && name.contains(&d.sketches()[0].id.to_string()) {
                    let mut m: Map<String, Value> = serde_json::from_slice(&data).unwrap();
                    m["entities"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"type": "spline", "r": 3}));
                    data = serde_json::to_vec(&m).unwrap();
                }
                Some((name.to_string(), data))
            },
            &[("cam/toolpaths.bin", b"\x01\x02\x03"), ("laser/job.json", b"{}")],
        );
        let l = load_from_bytes(&patched).unwrap();
        let doc = Document::from_loaded(l);
        let again = load_from_bytes(&save_to_bytes(&doc, None, "t2").unwrap()).unwrap();
        assert_eq!(
            again.extras.entries.get("cam/toolpaths.bin").unwrap(),
            b"\x01\x02\x03"
        );
        assert!(again.extras.entries.contains_key("laser/job.json"));
        assert_eq!(again.extras.manifest["cam"], json!({"tool": 6}));
        assert_eq!(again.extras.bodies[&first_body]["cam_stock"], json!("steel"));
        assert_eq!(
            again.extras.sketches[&d.sketches()[0].id]["entities_unknown"][0]["type"],
            json!("spline")
        );
        // and the known content is still all there
        assert_eq!(again.state.bodies.len(), 2);
    }

    #[test]
    fn newer_format_is_refused_with_a_clear_message() {
        let bytes = save_to_bytes(&sample(), None, "t").unwrap();
        let newer = rewrite(
            &bytes,
            |name, data| {
                if name == "manifest.json" {
                    let mut m: Map<String, Value> = serde_json::from_slice(&data).unwrap();
                    m.insert("format_version".into(), json!(FORMAT_VERSION + 1));
                    return Some((name.into(), serde_json::to_vec(&m).unwrap()));
                }
                Some((name.into(), data))
            },
            &[],
        );
        let e = load_from_bytes(&newer).err().unwrap();
        assert!(matches!(e, FileError::Newer { .. }));
        assert!(e.to_string().contains("newer Rolidium"));
        // on disk the file is untouched by a failed open
        let dir = tmpdir("newer");
        let p = dir.join("n.rolidium");
        std::fs::write(&p, &newer).unwrap();
        assert!(load_file(&p).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), newer);
    }

    #[test]
    fn damaged_files_give_errors_not_crashes() {
        let bytes = save_to_bytes(&sample(), None, "t").unwrap();
        assert!(matches!(
            load_from_bytes(b"not a zip"),
            Err(FileError::Corrupt(_))
        ));
        assert!(load_from_bytes(&[]).is_err());
        assert!(load_from_bytes(&bytes[..bytes.len() / 2]).is_err(), "truncated");
        let no_manifest = rewrite(
            &bytes,
            |n, d| (n != "manifest.json").then(|| (n.to_string(), d)),
            &[],
        );
        assert!(load_from_bytes(&no_manifest).is_err());
        let bad_json = rewrite(
            &bytes,
            |n, d| {
                Some((
                    n.to_string(),
                    if n == "manifest.json" {
                        b"{oops".to_vec()
                    } else {
                        d
                    },
                ))
            },
            &[],
        );
        assert!(load_from_bytes(&bad_json).is_err());
        let no_brep = rewrite(
            &bytes,
            |n, d| (!n.ends_with(".brep")).then(|| (n.to_string(), d)),
            &[],
        );
        assert!(load_from_bytes(&no_brep).is_err());
        let bad_brep = rewrite(
            &bytes,
            |n, d| {
                Some((
                    n.to_string(),
                    if n.ends_with(".brep") { vec![0u8; 64] } else { d },
                ))
            },
            &[],
        );
        assert!(load_from_bytes(&bad_brep).is_err());
        let msg = load_from_bytes(b"xx").err().unwrap().to_string();
        assert!(msg.contains("Nothing was changed"));
    }

    #[test]
    fn interrupted_save_keeps_the_original_and_backup_is_made() {
        let dir = tmpdir("atomic");
        let p = dir.join("m.rolidium");
        let d = sample();
        save_file(&d, &p, None, "t").unwrap();
        let first = std::fs::read(&p).unwrap();
        // The program dies after the temporary file was written but before the rename.
        let bytes = save_to_bytes(&Document::new(), None, "t").unwrap();
        let tmp = write_temp(&p, &bytes).unwrap();
        assert!(tmp.exists());
        assert_eq!(std::fs::read(&p).unwrap(), first, "original intact");
        assert!(load_file(&p).is_ok());
        let _ = std::fs::remove_file(tmp);
        // A failing save (missing folder) does not touch anything either.
        assert!(save_file(&d, &dir.join("nope/x.rolidium"), None, "t").is_err());
        // A second save keeps the first version as .bak.
        let mut d2 = sample();
        d2.apply(Command::AddBox {
            name: "Extra".into(),
            min: DVec3::ZERO,
            max: DVec3::ONE,
        })
        .unwrap();
        save_file(&d2, &p, None, "t").unwrap();
        assert_eq!(std::fs::read(dir.join("m.rolidium.bak")).unwrap(), first);
        assert_eq!(load_file(&p).unwrap().state.bodies.len(), 3);
    }

    #[test]
    fn dirty_flag_follows_the_saved_state() {
        let mut d = Document::new();
        assert!(!d.is_dirty());
        d.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::ZERO,
            max: DVec3::ONE,
        })
        .unwrap();
        assert!(d.is_dirty());
        d.mark_saved();
        assert!(!d.is_dirty());
        d.undo();
        assert!(d.is_dirty());
        d.redo();
        assert!(!d.is_dirty(), "back at the saved state");
    }

    #[test]
    fn step_round_trip_keeps_volumes_and_skips_nothing_valid() {
        let d = sample();
        let mut step = Vec::new();
        for b in d.bodies() {
            let (solids, skipped) = Solid::read_step_all(&b.solid.write_step().unwrap()).unwrap();
            assert!(skipped.is_empty());
            assert_eq!(solids.len(), 1);
            assert!((solids[0].volume().unwrap() - b.solid.volume().unwrap()).abs() < 1e-3);
            step.push(solids);
        }
        let mut e = Document::new();
        e.apply(Command::AddSolids {
            solids: step.into_iter().flatten().collect(),
        })
        .unwrap();
        assert_eq!(e.bodies().len(), 2);
        assert_eq!(e.bodies()[0].name, "Body 1");
        let both: Vec<Solid> = d.bodies().iter().map(|b| b.solid.clone()).collect();
        let (back, skipped) = Solid::read_step_all(&Solid::write_step_many(&both).unwrap()).unwrap();
        assert!(skipped.is_empty() && back.len() == 2);
        assert!((back[0].volume().unwrap() - both[0].volume().unwrap()).abs() < 1e-3);
        assert!((back[1].volume().unwrap() - both[1].volume().unwrap()).abs() < 1e-3);
        assert!(Solid::read_step_all(b"garbage").is_err());
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;
    use crate::{Command, Document};
    use rolidium_kernel::DVec3;

    #[test]
    fn names_are_saved_and_come_back_and_an_old_file_opens_as_before() {
        let mut d = Document::new();
        for (i, name) in ["Frame profile left", "Frame profile left", ""]
            .iter()
            .enumerate()
        {
            d.apply(Command::AddBox {
                name: (*name).into(),
                min: DVec3::new(i as f64 * 20.0, 0.0, 0.0),
                max: DVec3::new(i as f64 * 20.0 + 10.0, 10.0, 10.0),
            })
            .unwrap();
        }
        let bytes = save_to_bytes(&d, None, "test").unwrap();
        let back = load_from_bytes(&bytes).unwrap();
        let names: Vec<&str> = back.state.bodies.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Frame profile left", "Frame profile left", "Body 3"]);
        // a file whose bodies have no name entry (written before names were chosen freely) gets the default names
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut out);
            for i in 0..zip.len() {
                let mut f = zip.by_index(i).unwrap();
                let name = f.name().unwrap().to_string();
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut f, &mut data).unwrap();
                if name.starts_with("bodies/") && name.ends_with(".json") {
                    let mut j: serde_json::Value = serde_json::from_slice(&data).unwrap();
                    j.as_object_mut().unwrap().remove("name");
                    data = serde_json::to_vec(&j).unwrap();
                }
                w.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                std::io::Write::write_all(&mut w, &data).unwrap();
            }
            w.finish().unwrap();
        }
        let old = load_from_bytes(&out.into_inner()).unwrap();
        assert_eq!(old.state.bodies.len(), 3);
        assert!(old.state.bodies.iter().all(|b| !b.name.is_empty()));
    }
}
