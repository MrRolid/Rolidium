// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The probe for the scripted window tests: with `ROLIDIUM_PROBE=<file>` the program writes a small JSON file several times a second with what a
//! script needs to drive and to check the window: where points of the model are on the screen (`ROLIDIUM_PROBE_POINTS="name=x,y,z;..."`), the status
//! line, the tool, the bodies with their volumes and the holes in them, the links, the hovered face and the live distances. Nothing is written
//! without the variable, and nothing is ever sent anywhere.

use super::*;
use crate::sketchmode::EdgeEdit;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Probe {
    path: std::path::PathBuf,
    points: Vec<(String, DVec3)>,
    last: Option<Instant>,
    frame: u64,
    ppp: f32,
    /// `ROLIDIUM_PROBE_IDLE=1`: the probe does not wake the window and is written at every frame, so that the frame counter shows the frames the program draws by itself.
    idle: bool,
}

impl Probe {
    /// Reads the variables; `None` without `ROLIDIUM_PROBE`.
    pub fn from_env() -> Option<Probe> {
        let path = std::env::var_os("ROLIDIUM_PROBE")?;
        let mut points = Vec::new();
        if let Ok(list) = std::env::var("ROLIDIUM_PROBE_POINTS") {
            for item in list.split(';') {
                let Some((name, xyz)) = item.split_once('=') else {
                    continue;
                };
                let v: Vec<f64> = xyz.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if let [x, y, z] = v[..] {
                    points.push((name.trim().to_string(), DVec3::new(x, y, z)));
                }
            }
        }
        Some(Probe {
            path: path.into(),
            points,
            last: None,
            frame: 0,
            ppp: 1.0,
            idle: std::env::var_os("ROLIDIUM_PROBE_IDLE").is_some(),
        })
    }
}

impl App {
    /// Notes where a widget is (the middle of `r`, in points), so that a script can click it. Nothing happens without the probe.
    pub(super) fn probe_widget(&self, name: &str, r: Rect) {
        if self.probe.is_some() {
            let mut w = self.probe_widgets.borrow_mut();
            w.retain(|(n, _)| n != name);
            w.push((name.to_string(), r));
        }
    }

    /// Called once per frame: writes the file when it is time (and keeps the window repainting so that a script always sees a recent state).
    pub(super) fn probe_tick(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.probe.take() else { return };
        p.frame += 1;
        p.ppp = ctx.pixels_per_point();
        if p.idle || p.last.is_none_or(|t| t.elapsed() >= Duration::from_millis(120)) {
            p.last = Some(Instant::now());
            let v = self.probe_json(&p);
            self.probe_widgets.borrow_mut().clear();
            let tmp = p.path.with_extension("tmp");
            if std::fs::write(&tmp, v.to_string()).is_ok() {
                let _ = std::fs::rename(&tmp, &p.path);
            }
        }
        if !p.idle {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        self.probe = Some(p);
    }

    fn probe_json(&self, p: &Probe) -> Value {
        let ppp = p.ppp;
        let r = self.viewport_rect;
        let points: serde_json::Map<String, Value> = p
            .points
            .iter()
            .map(|(n, v)| {
                let s = self
                    .project(v.as_vec3(), r)
                    .map(|s| json!([(s.x * ppp).round(), (s.y * ppp).round()]));
                (n.clone(), s.unwrap_or(Value::Null))
            })
            .collect();
        let bodies: Vec<Value> = self
            .doc
            .bodies()
            .iter()
            .take(24)
            .map(|b| {
                let holes: Vec<Value> = b
                    .solid
                    .faces()
                    .take(300)
                    .filter_map(|f| b.solid.cylinder_info(f).ok().flatten())
                    .map(|c| json!({"x": round3(c.base.x), "y": round3(c.base.y), "z": round3(c.base.z), "r": round3(c.radius), "hole": c.is_hole}))
                    .collect();
                let bb = b.solid.bounding_box().ok();
                json!({
                    "id": b.id,
                    "name": b.name,
                    "visible": b.visible,
                    "volume": b.solid.volume().map(round3).unwrap_or(0.0),
                    "faces": b.solid.face_count(),
                    "bbox": bb.map(|(lo, hi)| json!([round3(lo.x), round3(lo.y), round3(lo.z), round3(hi.x), round3(hi.y), round3(hi.z)])),
                    "holes": holes,
                })
            })
            .collect();
        let st = self.doc.state();
        let links: Vec<Value> = st
            .links
            .iter()
            .map(|l| json!({"id": l.id, "name": l.name, "kind": l.kind.title(), "error": l.error, "results": l.results, "follows": l.fill.is_some(), "count": link_count(&l.kind)}))
            .collect();
        let sketches: Vec<Value> = self
            .doc
            .sketches()
            .iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "name": s.name,
                    "shapes": s.sketch.entities().len(),
                    "axes": s.sketch.entities().iter().filter(|(_, e)| e.is_axis()).count(),
                    "regions": s.sketch.regions().len(),
                })
            })
            .collect();
        let hover = self.hovered_face().map(|(b, f)| {
            let meta = self.entry(b).and_then(|m| m.faces.get(f as usize));
            let kind = meta.map(|m| format!("{:?}", m.kind));
            let normal = meta.map(|m| json!([round3(m.normal.x), round3(m.normal.y), round3(m.normal.z)]));
            json!({"body": b, "face": f, "kind": kind, "normal": normal})
        });
        let edge_dims = self.probe_edge_dims();
        json!({
            "frame": p.frame,
            "ppp": p.ppp,
            "viewport": [(r.min.x * ppp).round(), (r.min.y * ppp).round(), (r.width() * ppp).round(), (r.height() * ppp).round()],
            "status": self.status.text,
            "status_error": self.status.error,
            "hint": self.hint(),
            "tool": format!("{:?}", self.tool),
            "sketching": self.sk.as_ref().map(|s| format!("{:?}", s.tool)),
            "busy": !self.idle(),
            "bodies": bodies,
            "links": links,
            "sketches": sketches,
            "groups": st.groups.len(),
            "selection": self.selection.iter().map(|(b, f)| json!([b, f])).collect::<Vec<_>>(),
            "entity_sel": self.entity_sel.map(|(s, e)| json!([s, e])),
            "hover": hover,
            "hover_obj": self.hover_obj.map(|o| format!("{o:?}")),
            "link_pick": format!("{:?}", self.link_ui.pick),
            "link_dir": [self.link_ui.p.dir.x, self.link_ui.p.dir.y, self.link_ui.p.dir.z],
            "link_count": self.link_ui.p.count,
            "link_axis": [self.link_ui.p.axis.origin.x, self.link_ui.p.axis.origin.y, self.link_ui.p.axis.origin.z, self.link_ui.p.axis.dir.x, self.link_ui.p.axis.dir.y, self.link_ui.p.axis.dir.z],
            "link_plane": [self.link_ui.p.plane.origin.x, self.link_ui.p.plane.origin.y, self.link_ui.p.plane.origin.z, self.link_ui.p.plane.dir.x, self.link_ui.p.plane.dir.y, self.link_ui.p.plane.dir.z],
            "edge_dims": edge_dims,
            "points": points,
            "widgets": self.probe_widgets.borrow().iter().map(|(n, r)| (n.clone(), json!([(r.center().x * ppp).round(), (r.center().y * ppp).round()]))).collect::<serde_json::Map<String, Value>>(),
            "derived_ask": self.derived_ask.is_some(),
            "circles": self.probe_circles(),
            "shapes": self.probe_shapes(),
            "drawing": self.sk.as_ref().is_some_and(|m| m.drawing.is_some()),
            "click_total": self.click_total,
            "click_log": self.click_log.iter().rev().take(120).collect::<Vec<_>>(),
            "shape_drag": self.shape_drag.is_some(),
            "hole_drag": self.hole_drag.is_some(),
            "meshes": self.probe_meshes(),
            "panel": self.panel_fields(),
            "pick": json!({"ms": round3(self.pick_stats.get().0 as f64), "bvh": self.pick_stats.get().1, "count": self.pick_stats.get().2}),
            "triangles_shown": self.meshes.iter().map(|m| m.mesh.triangle_count()).sum::<usize>(),
            "face": self.probe_face(),
            "mesh_job": self.mesh_ui.job.as_ref().map(|j| round3(j.fraction() as f64)),
            "stl_dialog": self.probe_dialog(),
            "dropped": self.mesh_ui.dropped,
        })
    }

    /// The facts about the one selected face (a wall or a hole of a mesh, a face of a body).
    fn probe_face(&self) -> Value {
        let [(b, f)] = self.selection[..] else {
            return Value::Null;
        };
        let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)) else {
            return Value::Null;
        };
        json!({"body": b, "face": f, "kind": format!("{:?}", meta.kind), "area": round3(meta.area), "diameter": meta.cyl_diameter.map(round3),
               "normal": [round3(meta.normal.x), round3(meta.normal.y), round3(meta.normal.z)], "mesh": self.doc.mesh(b).is_some()})
    }

    /// The dialog of the STL import: the files, the units and the check.
    fn probe_dialog(&self) -> Value {
        let Some(d) = self.mesh_ui.dialog.as_ref() else {
            return Value::Null;
        };
        let items: Vec<Value> = d
            .items
            .iter()
            .map(|i| {
                let f = d.factor().unwrap_or(1.0);
                let size = i
                    .mesh
                    .bounding_box()
                    .map(|(lo, hi)| (0..3).map(|k| round3((hi[k] - lo[k]) * f)).collect::<Vec<f64>>());
                json!({"name": i.name, "triangles": i.mesh.triangles.len(), "size": size, "clean": i.report.is_clean(), "check": super::meshtool::summary_line(&i.report), "note": i.note})
            })
            .collect();
        json!({"units": format!("{:?}", d.units), "factor": d.factor(), "items": items})
    }

    /// The meshes (STL) of the document: their size, what was found in them, and how they are shown.
    fn probe_meshes(&self) -> Vec<Value> {
        self.doc
            .meshes()
            .iter()
            .take(24)
            .map(|m| {
                let r = rolidium_doc::display::regions_of(&m.mesh);
                let bb = m.mesh.bounding_box();
                json!({
                    "id": m.id,
                    "name": m.name,
                    "visible": m.visible,
                    "group": self.doc.state().groups.iter().find(|g| g.bodies.contains(&m.id)).map(|g| g.name.clone()),
                    "triangles": m.mesh.triangles.len(),
                    "bbox": bb.map(|(lo, hi)| json!([round3(lo[0]), round3(lo[1]), round3(lo[2]), round3(hi[0]), round3(hi[1]), round3(hi[2])])),
                    "volume": round3(m.mesh.signed_volume()),
                    "walls": r.as_ref().map(|r| r.planes.len()),
                    "holes": r.as_ref().map(|r| r.cylinders.iter().map(|c| json!({"d": round3(c.radius * 2.0), "hole": c.is_hole, "x": round3(c.point[0]), "y": round3(c.point[1]), "z": round3(c.point[2])})).collect::<Vec<_>>()),
                    "shown": self.entry(m.id).map(|e| e.mesh.triangle_count()),
                })
            })
            .collect()
    }

    /// The live distances that are shown now (a circle being placed, a hole being dragged, the second piece being placed), for the scripts.
    fn probe_edge_dims(&self) -> Value {
        let pack = |d: &rolidium_doc::rolidium_sketch::edgedims::EdgeDims,
                    at: DVec2,
                    typed: &EdgeEdit,
                    snap: Option<rolidium_doc::rolidium_sketch::edgedims::EdgeSnap>| {
            json!({
                "at": [round3(at.x), round3(at.y)],
                "d1": round3(d.first.dist),
                "d2": d.second.map(|e| round3(e.dist)),
                "typed": [typed.text[0], typed.text[1]],
                "snap": snap.map(|s| format!("{s:?}")),
            })
        };
        if let Some(h) = &self.shape_drag {
            if let Some(d) = &h.dims {
                let mut v = pack(d, h.at, &h.edit, h.snap);
                v["what"] = json!("shape");
                return v;
            }
        }
        if let Some(h) = &self.hole_drag {
            if let Some(d) = &h.dims {
                let mut v = pack(d, h.cur, &h.edit, h.snap);
                v["what"] = json!("hole");
                return v;
            }
        }
        if let Some(sm) = &self.sk {
            if let (Some(d), Some(s)) = (&sm.edge_dims, sm.snap) {
                let mut v = pack(d, s.pos, &sm.edge_edit, sm.edge_snap);
                v["what"] = json!("circle");
                return v;
            }
        }
        if let Some(h) = &self.link_ui.second {
            if let Some(d) = &h.dims {
                let mut v = pack(d, h.at, &self.link_ui.second_edit, h.snap);
                v["what"] = json!("second");
                return v;
            }
        }
        Value::Null
    }
}

impl App {
    /// Every shape of the sketches with its numbers, for the scripts.
    fn probe_shapes(&self) -> Vec<Value> {
        let mut v = Vec::new();
        for s in self.doc.sketches() {
            for (id, e) in s.sketch.entities() {
                let j = match *e {
                    Entity::Circle { center, radius } => {
                        json!({"kind": "circle", "x": round3(center.x), "y": round3(center.y), "r": round3(radius)})
                    }
                    Entity::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    } => {
                        json!({"kind": "arc", "x": round3(center.x), "y": round3(center.y), "r": round3(radius), "start": round3(start), "sweep": round3(sweep)})
                    }
                    Entity::Line { a, b } => {
                        json!({"kind": "line", "a": [round3(a.x), round3(a.y)], "b": [round3(b.x), round3(b.y)]})
                    }
                    Entity::Axis { a, b } => {
                        json!({"kind": "axis", "a": [round3(a.x), round3(a.y)], "b": [round3(b.x), round3(b.y)]})
                    }
                    Entity::Rect { a, b } => {
                        json!({"kind": "rect", "a": [round3(a.x), round3(a.y)], "b": [round3(b.x), round3(b.y)]})
                    }
                };
                let mut j = j;
                j["sketch"] = json!(s.id);
                j["id"] = json!(id);
                v.push(j);
            }
        }
        v
    }

    /// The circles of the sketches (centre and radius), for the scripts.
    fn probe_circles(&self) -> Vec<Value> {
        let mut v = Vec::new();
        for s in self.doc.sketches() {
            for (_, e) in s.sketch.entities() {
                if let Entity::Circle { center, radius } = e {
                    v.push(json!({"sketch": s.id, "x": round3(center.x), "y": round3(center.y), "r": round3(*radius)}));
                }
            }
        }
        v
    }
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn link_count(k: &rolidium_doc::LinkKind) -> u32 {
    match k {
        rolidium_doc::LinkKind::Linear(p, _) => p.count * p.second.as_ref().map_or(1, |s| s.count),
        rolidium_doc::LinkKind::Circular(p, _) => p.count,
        _ => 0,
    }
}
