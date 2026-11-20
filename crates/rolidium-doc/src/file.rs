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
        ..Default::default()
    };
    let mut max_id = 0u64;
    let mut dropped = 0usize;
    for b in manifest
        .get("bodies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
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
        let solid = Solid::read_brep(&read_entry(&mut zip, &brep_name)?)
            .map_err(|e| corrupt(format!("body {id}: {e}")))?;
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
        };
        take_keys(
            &mut j,
            &["name", "plane", "target_body", "entities", "visible", "fill"],
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
            ],
        );
        if !j.is_empty() {
            extras.curves.insert(id, j);
        }
        max_id = max_id.max(id);
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
            "bodies",
            "sketches",
            "curves",
        ],
    );
    extras.manifest = manifest;
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
        })
        .unwrap();
        assert!(e.body(id).unwrap().solid.volume().unwrap() < before);
        let face = e.body(id).unwrap().features[0].faces[0];
        e.apply(Command::EditBlend {
            body: id,
            face,
            size: 0.0,
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
