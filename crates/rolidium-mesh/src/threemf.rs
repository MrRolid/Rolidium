// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! 3MF files (geometry only): a zip with the model as XML. Every object of the build becomes one mesh (with the transforms of the build and of the
//! components applied and the unit of the file converted to millimetres). Colours, materials and print settings are ignored.

use crate::TriMesh;
use std::collections::HashMap;
use std::io::Read;

/// A mesh read from a 3MF file: the name of the object and its triangles in millimetres.
pub struct Part {
    pub name: String,
    pub mesh: TriMesh,
}

/// A transform of the 3MF kind: 12 numbers, a 3 x 4 matrix of rows (the points are rows, so the translation is the last row).
type Tr = [f64; 12];
const IDENTITY: Tr = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn parse_transform(text: Option<&str>) -> Result<Tr, String> {
    let Some(t) = text else { return Ok(IDENTITY) };
    let v: Vec<f64> = t
        .split_whitespace()
        .map(|x| {
            x.parse::<f64>()
                .map_err(|_| format!("a transform has a bad number: {x}"))
        })
        .collect::<Result<_, _>>()?;
    if v.len() != 12 || v.iter().any(|x| !x.is_finite()) {
        return Err("a transform of the file does not have 12 numbers".into());
    }
    Ok([
        v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8], v[9], v[10], v[11],
    ])
}

/// `a` first, then `b` (a point is a row: `p * A + ta`, then `* B + tb`).
fn then(a: &Tr, b: &Tr) -> Tr {
    let mut r = [0.0; 12];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    for j in 0..3 {
        r[9 + j] = (0..3).map(|k| a[9 + k] * b[k * 3 + j]).sum::<f64>() + b[9 + j];
    }
    r
}

fn apply(t: &Tr, p: [f64; 3]) -> [f64; 3] {
    [
        p[0] * t[0] + p[1] * t[3] + p[2] * t[6] + t[9],
        p[0] * t[1] + p[1] * t[4] + p[2] * t[7] + t[10],
        p[0] * t[2] + p[1] * t[5] + p[2] * t[8] + t[11],
    ]
}

/// Millimetres per unit of the file (`unit` of the model; millimetre when it is not there).
fn unit_factor(unit: Option<&str>) -> Result<f64, String> {
    Ok(match unit.unwrap_or("millimeter") {
        "micron" => 0.001,
        "millimeter" => 1.0,
        "centimeter" => 10.0,
        "inch" => 25.4,
        "foot" => 304.8,
        "meter" => 1000.0,
        other => return Err(format!("the unit of the file ({other}) is not known")),
    })
}

struct Model {
    factor: f64,
    /// object id -> (name, kind)
    objects: HashMap<String, Object>,
    build: Vec<(String, Tr)>,
}

enum Object {
    Mesh {
        name: String,
        positions: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
    },
    Components {
        name: String,
        /// (model path or empty for this file, object id, transform)
        parts: Vec<(String, String, Tr)>,
    },
}

fn attr<'a>(n: &roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn read_model(xml: &str) -> Result<Model, String> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| format!("the model of the file cannot be read ({e})"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "model" {
        return Err("the model of the file is not a 3MF model".into());
    }
    let factor = unit_factor(attr(&root, "unit"))?;
    let mut objects = HashMap::new();
    let mut build = Vec::new();
    for res in root.children().filter(|n| n.has_tag_name("resources")) {
        for o in res.children().filter(|n| n.has_tag_name("object")) {
            let Some(id) = attr(&o, "id") else { continue };
            let name = attr(&o, "name").unwrap_or("").to_string();
            if let Some(m) = o.children().find(|n| n.has_tag_name("mesh")) {
                let mut positions = Vec::new();
                if let Some(vs) = m.children().find(|n| n.has_tag_name("vertices")) {
                    for v in vs.children().filter(|n| n.has_tag_name("vertex")) {
                        let c = |k: &str| -> Result<f64, String> {
                            attr(&v, k)
                                .and_then(|t| t.parse::<f64>().ok())
                                .filter(|x| x.is_finite())
                                .ok_or_else(|| "a vertex of the file has a bad number".to_string())
                        };
                        positions.push([c("x")?, c("y")?, c("z")?]);
                    }
                }
                let mut triangles = Vec::new();
                if let Some(ts) = m.children().find(|n| n.has_tag_name("triangles")) {
                    for t in ts.children().filter(|n| n.has_tag_name("triangle")) {
                        let c = |k: &str| -> Result<u32, String> {
                            attr(&t, k)
                                .and_then(|x| x.parse::<u32>().ok())
                                .filter(|i| (*i as usize) < positions.len())
                                .ok_or_else(|| {
                                    "a triangle of the file points to a vertex that is not there".to_string()
                                })
                        };
                        triangles.push([c("v1")?, c("v2")?, c("v3")?]);
                    }
                }
                objects.insert(
                    id.to_string(),
                    Object::Mesh {
                        name,
                        positions,
                        triangles,
                    },
                );
            } else if let Some(cs) = o.children().find(|n| n.has_tag_name("components")) {
                let mut parts = Vec::new();
                for c in cs.children().filter(|n| n.has_tag_name("component")) {
                    let Some(oid) = attr(&c, "objectid") else { continue };
                    parts.push((
                        attr(&c, "path").unwrap_or("").trim_start_matches('/').to_string(),
                        oid.to_string(),
                        parse_transform(attr(&c, "transform"))?,
                    ));
                }
                objects.insert(id.to_string(), Object::Components { name, parts });
            }
        }
    }
    for b in root.children().filter(|n| n.has_tag_name("build")) {
        for i in b.children().filter(|n| n.has_tag_name("item")) {
            if let Some(oid) = attr(&i, "objectid") {
                build.push((oid.to_string(), parse_transform(attr(&i, "transform"))?));
            }
        }
    }
    Ok(Model {
        factor,
        objects,
        build,
    })
}

fn read_part(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let mut f = zip
        .by_name(name)
        .map_err(|_| format!("the file has no part named {name}"))?;
    let mut s = String::new();
    f.read_to_string(&mut s)
        .map_err(|e| format!("the part {name} of the file cannot be read ({e})"))?;
    Ok(s)
}

/// The parts of a 3MF file.
pub fn parse(bytes: &[u8]) -> Result<Vec<Part>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "The file is not a 3MF file (it is not a zip)".to_string())?;
    // the model: named by the relationships of the package, else the usual place
    let mut model_path = "3D/3dmodel.model".to_string();
    if let Ok(rels) = read_part(&mut zip, "_rels/.rels") {
        if let Ok(doc) = roxmltree::Document::parse(&rels) {
            for r in doc.descendants().filter(|n| n.has_tag_name("Relationship")) {
                if attr(&r, "Type").is_some_and(|t| t.ends_with("/3dmodel")) {
                    if let Some(t) = attr(&r, "Target") {
                        model_path = t.trim_start_matches('/').to_string();
                        break;
                    }
                }
            }
        }
    }
    let main_xml = read_part(&mut zip, &model_path)
        .map_err(|_| "The file is not a 3MF file (there is no 3D model in it)".to_string())?;
    let main = read_model(&main_xml)?;
    let mut others: HashMap<String, Model> = HashMap::new();
    let mut parts = Vec::new();
    let items: Vec<(String, Tr)> = if main.build.is_empty() {
        // no build section: every object that is not a part of another is taken
        main.objects.keys().map(|k| (k.clone(), IDENTITY)).collect()
    } else {
        main.build.clone()
    };
    for (n, (oid, t0)) in items.iter().enumerate() {
        let mut positions: Vec<[f64; 3]> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let mut name = String::new();
        // an explicit stack: (model path, object id, transform so far)
        let mut stack: Vec<(String, String, Tr, u32)> = vec![(String::new(), oid.clone(), *t0, 0)];
        while let Some((path, id, t, depth)) = stack.pop() {
            if depth > 16 {
                return Err("the components of the file are nested too deeply".into());
            }
            if !path.is_empty() && !others.contains_key(&path) {
                let xml = read_part(&mut zip, &path)?;
                others.insert(path.clone(), read_model(&xml)?);
            }
            let model = if path.is_empty() { &main } else { &others[&path] };
            let Some(obj) = model.objects.get(&id) else {
                return Err(format!("the file refers to an object ({id}) that is not in it"));
            };
            match obj {
                Object::Mesh {
                    name: nm,
                    positions: ps,
                    triangles: ts,
                } => {
                    if name.is_empty() {
                        name = nm.clone();
                    }
                    let base = positions.len() as u32;
                    positions.extend(ps.iter().map(|p| {
                        let q = apply(&t, *p);
                        [q[0] * model.factor, q[1] * model.factor, q[2] * model.factor]
                    }));
                    triangles.extend(ts.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
                }
                Object::Components { name: nm, parts: cs } => {
                    if name.is_empty() {
                        name = nm.clone();
                    }
                    for (p, cid, ct) in cs {
                        // the component's own transform first, then the one of what holds it
                        let total = then(ct, &t);
                        stack.push((
                            if p.is_empty() { path.clone() } else { p.clone() },
                            cid.clone(),
                            total,
                            depth + 1,
                        ));
                    }
                }
            }
        }
        if triangles.is_empty() {
            continue;
        }
        let mut mesh = TriMesh::default();
        mesh.positions = positions
            .iter()
            .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
            .collect();
        mesh.triangles = triangles;
        // a unit that turns the part inside out (a mirroring transform) is mended by turning the triangles
        if mesh.signed_volume() < 0.0 {
            for t in &mut mesh.triangles {
                t.swap(1, 2);
            }
        }
        parts.push(Part {
            name: if name.is_empty() {
                format!("Part {}", n + 1)
            } else {
                name
            },
            mesh,
        });
    }
    if parts.is_empty() {
        return Err("The file has no triangles".into());
    }
    Ok(parts)
}
