// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Commands for triangle meshes (STL): import with units, check and repair, regions, edits of a planar wall and of the diameter of a round hole, moves,
//! scale (also uneven), mirror, pattern, undo and export. A mesh is not a body of the document (it is not converted to B-rep, see D60); the runner keeps the
//! meshes next to the document, and the commands act on the last one (or on `mesh N`).

use crate::Runner;
use rolidium_mesh::{check, edit, regions, serial, stl, transform, TriMesh};

/// A mesh of the runner with its regions and the records that undo its edits.
pub struct MeshBody {
    pub name: String,
    pub mesh: TriMesh,
    pub regions: Option<regions::Regions>,
    pub undo: Vec<edit::Undo>,
    pub redo: Vec<edit::Undo>,
    /// The mesh was checked and is closed and consistent (the edits keep it so, and do not check again).
    pub verified: bool,
}

fn num(w: &[&str], i: usize, what: &str) -> Result<f64, String> {
    w.get(i)
        .ok_or_else(|| format!("{what} is missing"))?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{what} must be a number"))
}

impl Runner {
    fn mesh_index(&self, w: &[&str]) -> Result<usize, String> {
        let n = self.meshes.len();
        if n == 0 {
            return Err("there is no mesh: import one with import-stl".into());
        }
        match w.iter().position(|t| *t == "mesh") {
            Some(i) => {
                let k = num(w, i + 1, "the number of the mesh")? as usize;
                if k == 0 || k > n {
                    return Err(format!("there is no mesh {k}; there are {n}"));
                }
                Ok(k - 1)
            }
            None => Ok(n - 1),
        }
    }

    fn mesh_summary(&self, i: usize) -> String {
        let m = &self.meshes[i];
        format!(
            "mesh {}: {} ({} triangles, {})",
            i + 1,
            m.name,
            m.mesh.triangles.len(),
            check::size_text(&m.mesh)
        )
    }

    fn regions_of(&mut self, i: usize) -> &regions::Regions {
        let m = &mut self.meshes[i];
        if m.regions.is_none() {
            m.regions = Some(regions::find(&m.mesh, 3.0, 4));
        }
        m.regions.as_ref().unwrap()
    }

    fn mesh_changed(&mut self, i: usize, undo: edit::Undo) {
        let m = &mut self.meshes[i];
        m.undo.push(undo);
        m.redo.clear();
        m.regions = None;
    }

    pub(crate) fn run_mesh_command(&mut self, w: &[&str]) -> Result<(), String> {
        match w[0] {
            "import-stl" => {
                let path = w.get(1).ok_or("import-stl needs the file")?;
                let units = if w.contains(&"inch") {
                    stl::Units::Inches
                } else if let Some(i) = w.iter().position(|t| *t == "scale") {
                    let k = num(w, i + 1, "the scale")?;
                    if k <= 0.0 {
                        return Err("the scale must be positive".into());
                    }
                    stl::Units::Scale(k)
                } else {
                    stl::Units::Millimetres
                };
                let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
                let mut mesh = stl::parse_units(&bytes, units).map_err(|e| e.to_string())?;
                let merged = if w.contains(&"weld") {
                    check::merge_vertices(&mut mesh, 1e-4)
                } else {
                    0
                };
                let name = std::path::Path::new(path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "mesh".into());
                self.meshes.push(crate::mesh_cmd::MeshBody {
                    name,
                    mesh,
                    regions: None,
                    undo: Vec::new(),
                    redo: Vec::new(),
                    verified: false,
                });
                let i = self.meshes.len() - 1;
                self.say(self.mesh_summary(i));
                if merged > 0 {
                    self.say(format!("{merged} vertices merged"));
                }
                let rep = check::check(&self.meshes[i].mesh);
                self.meshes[i].verified = rep.is_watertight();
                for l in rep.lines() {
                    self.say(format!("  {l}"));
                }
            }
            "mesh-to-doc" => {
                // the mesh becomes a body of the document: it is saved in the .rolidium file and exported with the solids
                let i = self.mesh_index(w)?;
                let m = &self.meshes[i];
                let verified = m.verified || {
                    let r = check::check(&m.mesh);
                    r.is_watertight()
                };
                let c = self
                    .doc
                    .apply(rolidium_doc::Command::AddMeshes {
                        meshes: vec![rolidium_doc::NewMesh {
                            name: m.name.clone(),
                            mesh: std::sync::Arc::new(m.mesh.clone()),
                            verified,
                        }],
                    })
                    .map_err(|e| e.to_string())?;
                self.say(format!(
                    "mesh {} is a body of the document now (mesh {})",
                    i + 1,
                    c.touched_body.unwrap_or(0)
                ));
            }
            "mesh-info" => {
                let i = self.mesh_index(w)?;
                self.say(self.mesh_summary(i));
                let m = &self.meshes[i].mesh;
                self.say(format!("volume {:.3} mm3", m.signed_volume()));
            }
            "mesh-check" => {
                let i = self.mesh_index(w)?;
                self.say(self.mesh_summary(i));
                for l in check::check(&self.meshes[i].mesh).lines() {
                    self.say(format!("  {l}"));
                }
            }
            "mesh-repair" => {
                let i = self.mesh_index(w)?;
                let size = rolidium_mesh::len(match self.meshes[i].mesh.bounding_box() {
                    Some((lo, hi)) => rolidium_mesh::sub(hi, lo),
                    None => [0.0; 3],
                });
                let m = &mut self.meshes[i].mesh;
                let merged = check::merge_vertices(m, (size * 1e-6).max(1e-6));
                let turned = check::orient_normals(m);
                self.meshes[i].regions = None;
                self.meshes[i].verified = false;
                self.say(format!(
                    "repair: {merged} vertices merged, {turned} triangles turned"
                ));
                for l in check::check(&self.meshes[i].mesh).lines() {
                    self.say(format!("  {l}"));
                }
            }
            "mesh-regions" => {
                let i = self.mesh_index(w)?;
                let (planes, cyls) = {
                    let r = self.regions_of(i);
                    (r.planes.clone(), r.cylinders.clone())
                };
                self.say(format!(
                    "{} planar wall(s), {} round hole(s) or pin(s)",
                    planes.len(),
                    cyls.len()
                ));
                for (k, p) in planes.iter().enumerate().take(40) {
                    self.say(format!(
                        "  wall #{}: normal ({:.3}, {:.3}, {:.3}) at {:.3}, area {:.2} mm2, {} triangles",
                        k + 1,
                        p.normal[0],
                        p.normal[1],
                        p.normal[2],
                        p.offset,
                        p.area,
                        p.triangles.len()
                    ));
                }
                for (k, c) in cyls.iter().enumerate().take(40) {
                    self.say(format!(
                        "  {} #{}: diameter {:.3} mm, axis ({:.3}, {:.3}, {:.3}) through ({:.3}, {:.3}, {:.3})",
                        if c.is_hole { "hole" } else { "pin" },
                        k + 1,
                        c.radius * 2.0,
                        c.axis[0],
                        c.axis[1],
                        c.axis[2],
                        c.point[0],
                        c.point[1],
                        c.point[2]
                    ));
                }
            }
            "mesh-pull" => {
                let i = self.mesh_index(w)?;
                let k = num(w, 1, "the number of the wall")? as usize;
                let d = num(w, 2, "the distance")?;
                let wall = self
                    .regions_of(i)
                    .planes
                    .get(k.wrapping_sub(1))
                    .cloned()
                    .ok_or_else(|| format!("there is no wall #{k}: list them with mesh-regions"))?;
                let v = !self.meshes[i].verified;
                let undo = edit::pull_plane_with(&mut self.meshes[i].mesh, &wall, d, v)
                    .map_err(|e| e.to_string())?;
                self.meshes[i].verified = true;
                self.mesh_changed(i, undo);
                self.say(format!("wall #{k} pulled by {d} mm: {}", self.mesh_summary(i)));
            }
            "mesh-hole" => {
                let i = self.mesh_index(w)?;
                let k = num(w, 1, "the number of the hole")? as usize;
                let dia = num(w, 2, "the diameter")?;
                let cyl = self
                    .regions_of(i)
                    .cylinders
                    .get(k.wrapping_sub(1))
                    .cloned()
                    .ok_or_else(|| format!("there is no hole or pin #{k}: list them with mesh-regions"))?;
                let v = !self.meshes[i].verified;
                let undo = edit::set_diameter_with(&mut self.meshes[i].mesh, &cyl, dia, v)
                    .map_err(|e| e.to_string())?;
                self.meshes[i].verified = true;
                self.mesh_changed(i, undo);
                self.say(format!("diameter of #{k} is {dia} mm now"));
            }
            "mesh-undo" | "mesh-redo" => {
                let i = self.mesh_index(w)?;
                let m = &mut self.meshes[i];
                let undoing = w[0] == "mesh-undo";
                let rec = if undoing { m.undo.pop() } else { m.redo.pop() };
                let Some(rec) = rec else {
                    return Err(format!("nothing to {}", if undoing { "undo" } else { "redo" }));
                };
                let back = rec.apply(&mut m.mesh);
                if undoing {
                    m.redo.push(back);
                } else {
                    m.undo.push(back);
                }
                m.regions = None;
                self.say(self.mesh_summary(i));
            }
            "mesh-scale" => {
                let i = self.mesh_index(w)?;
                let f = [
                    num(w, 1, "the factor x")?,
                    num(w, 2, "the factor y")?,
                    num(w, 3, "the factor z")?,
                ];
                if f.contains(&0.0) {
                    return Err("a factor must not be zero".into());
                }
                let c = self.meshes[i]
                    .mesh
                    .bounding_box()
                    .map(|(lo, _)| lo)
                    .unwrap_or([0.0; 3]);
                transform::scale(&mut self.meshes[i].mesh, c, f);
                self.meshes[i].regions = None;
                self.say(self.mesh_summary(i));
            }
            "mesh-move" => {
                let i = self.mesh_index(w)?;
                let d = [num(w, 1, "dx")?, num(w, 2, "dy")?, num(w, 3, "dz")?];
                transform::translate(&mut self.meshes[i].mesh, d);
                self.meshes[i].regions = None;
                self.say(self.mesh_summary(i));
            }
            "mesh-rotate" => {
                let i = self.mesh_index(w)?;
                let axis = match *w.get(1).ok_or("the axis is missing (x, y or z)")? {
                    "x" => [1.0, 0.0, 0.0],
                    "y" => [0.0, 1.0, 0.0],
                    "z" => [0.0, 0.0, 1.0],
                    o => return Err(format!("unknown axis '{o}'")),
                };
                let a = num(w, 2, "the angle")?.to_radians();
                transform::rotate(&mut self.meshes[i].mesh, [0.0; 3], axis, a);
                self.meshes[i].regions = None;
                self.say(self.mesh_summary(i));
            }
            "mesh-mirror" => {
                let i = self.mesh_index(w)?;
                let n = match *w.get(1).ok_or("the plane is missing (x, y or z)")? {
                    "x" => [1.0, 0.0, 0.0],
                    "y" => [0.0, 1.0, 0.0],
                    "z" => [0.0, 0.0, 1.0],
                    o => return Err(format!("unknown plane '{o}'")),
                };
                transform::mirror(&mut self.meshes[i].mesh, [0.0; 3], n);
                self.meshes[i].regions = None;
                self.say(self.mesh_summary(i));
            }
            "mesh-pattern" => {
                let i = self.mesh_index(w)?;
                let count = num(w, 1, "the number of pieces")? as usize;
                if !(1..=1000).contains(&count) {
                    return Err("the number of pieces must be between 1 and 1000".into());
                }
                let step = [num(w, 2, "dx")?, num(w, 3, "dy")?, num(w, 4, "dz")?];
                let m = transform::linear_pattern(&self.meshes[i].mesh, count, step);
                self.meshes[i].mesh = m;
                self.meshes[i].regions = None;
                self.meshes[i].undo.clear();
                self.meshes[i].verified = false;
                self.say(self.mesh_summary(i));
            }
            "export-mesh-stl" => {
                let i = self.mesh_index(w)?;
                let path = w.get(1).ok_or("export-mesh-stl needs the file")?;
                let m = &self.meshes[i];
                if w.contains(&"ascii") {
                    std::fs::write(path, stl::write_ascii(&m.mesh, &m.name))
                } else {
                    std::fs::write(path, stl::write_binary(&m.mesh))
                }
                .map_err(|e| format!("cannot write {path}: {e}"))?;
                self.say(format!("wrote {path}"));
            }
            "mesh-save" => {
                let i = self.mesh_index(w)?;
                let path = w.get(1).ok_or("mesh-save needs the file")?;
                let bytes = serial::encode(&self.meshes[i].mesh);
                std::fs::write(path, &bytes).map_err(|e| format!("cannot write {path}: {e}"))?;
                self.say(format!("saved {path} ({} bytes)", bytes.len()));
            }
            "mesh-open" => {
                let path = w.get(1).ok_or("mesh-open needs the file")?;
                let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
                let mesh = serial::decode(&bytes)?;
                let name = std::path::Path::new(path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "mesh".into());
                self.meshes.push(MeshBody {
                    name,
                    mesh,
                    regions: None,
                    undo: Vec::new(),
                    redo: Vec::new(),
                    verified: false,
                });
                let i = self.meshes.len() - 1;
                self.say(self.mesh_summary(i));
            }
            other => return Err(format!("unknown mesh command '{other}'")),
        }
        Ok(())
    }
}
