// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Scenario runner behind `rolidium-cli`. A scenario is a text file with one command per line
//! (`#` starts a comment):
//!
//! ```text
//! box 40 30 20            # new box from the origin
//! pull top 10 [add|cut]   # pull a face: top|bottom|left|right|front|back|#<index>; add/cut force the direction
//! round 5 near x y z      # round the edge of body 1 that passes closest to the point (chamfer: same, "chamfer")
//! chamfer 5 near x y z angle 30 [flip]   # chamfer with a distance and an angle
//! combine merge|cut|intersect <target> <tool>... [keep]   # bodies are numbered from 1 in the order of `info`
//! fill edges <id>... | fill near x y z   # closed flat loop of edges -> region 0 of a new sketch (pull it with pull-region)
//! join-ends <max_gap> [lines <n>...] | curves   # connect free ends closer than the gap (tiny gaps are welded)
//! features                # list the rounds and chamfers (index, kind, size)
//! edit-feature <i> <size> # change the size of round/chamfer <i>; size 0 brings the sharp edge back
//! undo | redo
//! info                    # print bodies, volume, face count
//! export-stl out.stl [--deviation <mm>] [--angle <deg>] [--ascii]   (defaults: 0.02 mm, 10 deg, binary; all bodies, or `body N`)
//! export-step out.step   (all bodies, or `body N`)
//! move <dx> <dy> <dz> [copy]   # move body 1; rotate <x|y|z> <degrees> [copy] turns it about its centre
//! revolve <region> <x|y|z> <degrees> [add|cut]   # revolve a region of the current sketch about a world axis
//! save [file.rolidium]    # write the document (to the file it was opened from, or the given one)
//! open file.rolidium      # replace the document with a saved one
//! import-step in.step     # add the solids of a STEP file as bodies (invalid ones are reported and skipped)
//! render out.png [width height] [iso|top|front|right|back|left]
//!
//! sketch xy | sketch <face selector>   choose where the next shapes are drawn (base plane or a face of body 1)
//! line x1 y1 x2 y2 | rect x1 y1 x2 y2 | circle cx cy diameter   draw in the current sketch (sketch units: mm)
//! sketch xy|xz|yz [offset]   (the plane can be moved along its normal)
//! curve [radius R] x y z x y z ...   a 3D line through points (a radius rounds the corners)
//! helix radius R pitch P (turns N | height H) [left] [start deg] [cone deg] [at x y z] [axis x y z]   a helix curve
//! curves                   list the curves
//! sweep <region> (curve N | chain S E) [auto|new|add|cut] [rounded]   sweep a region of the current sketch along a path
//! measure <element> <element>   element: point x y z | face <top|left|…|#N> | edge #N; distance, x/y/z parts, angle, axes
//! material <steel|aluminium|brass|copper|PLA|PETG|ABS|g/cm3>   material of the first body
//! mass                     mass of every body from its volume and material
//! loft ruled|smooth [add|cut]   join region 0 of every sketch, in creation order, into a solid
//! arc x1 y1 xm ym x2 y2   (from, a point on the arc, to)
//! regions                  list the closed regions of the current sketch (index, area, holes)
//! pull-region <index> <distance> [add|cut]   pull a region (positive adds / creates, negative cuts)
//! ```

use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::{BodyId, Command, Document, PullMode, SketchId, SketchTarget};
use rolidium_kernel::{DVec3, FaceId, FaceKind, Solid};
use rolidium_render::{BodyDraw, Camera, Frame, Renderer, StandardView};
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod bench;
pub mod describe;
pub mod entry;
mod links_cmd;
pub mod mcp;
pub mod phases;
pub mod soft;
pub mod spec;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("line {line}: {msg}")]
    Script { line: usize, msg: String },
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Runner {
    pub doc: Document,
    gpu: Option<(
        rolidium_render::wgpu::Device,
        rolidium_render::wgpu::Queue,
        Renderer,
        String,
    )>,
    pub log: Vec<String>,
    plane: Option<(PlaneDef, Option<BodyId>)>,
    sketch: Option<SketchId>,
    /// The file the document was opened from or last saved to.
    pub file: Option<PathBuf>,
    /// No printing (the times table runs scripts silently).
    pub quiet: bool,
    /// `body N` of the line being run: the body (1-based) the command works on instead of the first one.
    target_body: Option<usize>,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

/// Finds a planar face by direction word, e.g. `top` (highest face looking up).
pub fn select_face(solid: &Solid, sel: &str) -> Result<FaceId, String> {
    if let Some(n) = sel.strip_prefix('#') {
        let i: u32 = n.parse().map_err(|_| format!("bad face index '{sel}'"))?;
        return if (i as usize) < solid.face_count() {
            Ok(FaceId(i))
        } else {
            Err(format!("no face #{i}"))
        };
    }
    let dir = match sel {
        "top" | "+z" => DVec3::Z,
        "bottom" | "-z" => -DVec3::Z,
        "right" | "+x" => DVec3::X,
        "left" | "-x" => -DVec3::X,
        "back" | "+y" => DVec3::Y,
        "front" | "-y" => -DVec3::Y,
        other => return Err(format!("unknown face selector '{other}'")),
    };
    let mut best: Option<(f64, FaceId)> = None;
    for f in solid.faces() {
        let Ok(i) = solid.face_info(f) else { continue };
        if i.kind != FaceKind::Plane || i.normal.dot(dir) < 0.99 {
            continue;
        }
        let h = i.origin.dot(dir);
        if best.is_none_or(|(bh, _)| h > bh) {
            best = Some((h, f));
        }
    }
    best.map(|b| b.1)
        .ok_or_else(|| format!("no planar face facing '{sel}'"))
}

/// Cuts a comment off a script line: a `#` at the start of a word followed by a space or the end (`#3` is a face id).
fn strip_comment(raw: &str) -> &str {
    let b = raw.as_bytes();
    for (i, c) in raw.char_indices() {
        if c == '#'
            && (i == 0 || b[i - 1].is_ascii_whitespace())
            && b.get(i + 1).is_none_or(|n| n.is_ascii_whitespace() || *n == b'#')
        {
            return &raw[..i];
        }
    }
    raw
}

/// Splits a script line into words; a part in double quotes is one word (so paths may hold spaces).
pub fn split_words(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if quoted {
        return Err("a quote is not closed".into());
    }
    if any {
        out.push(cur);
    }
    Ok(out)
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            doc: Document::new(),
            gpu: None,
            log: Vec::new(),
            plane: None,
            sketch: None,
            file: None,
            quiet: false,
            target_body: None,
        }
    }

    /// Loads a `.rolidium` file into the runner (the current document is replaced only on success).
    pub fn open_file(&mut self, path: &Path) -> Result<(), String> {
        let loaded = rolidium_doc::file::load_file(path).map_err(|e| e.to_string())?;
        self.doc = Document::from_loaded(loaded);
        self.file = Some(path.to_path_buf());
        self.plane = None;
        self.sketch = None;
        let n = self.doc.bodies().len();
        self.say(format!(
            "opened {} ({n} body(ies), {} sketch(es))",
            path.display(),
            self.doc.sketches().len()
        ));
        Ok(())
    }

    fn say(&mut self, s: impl Into<String>) {
        let s = s.into();
        if !self.quiet {
            println!("{s}");
        }
        self.log.push(s);
    }

    fn first_body(&self) -> Result<BodyId, String> {
        if let Some(n) = self.target_body {
            return self
                .doc
                .bodies()
                .get(n.wrapping_sub(1))
                .map(|b| b.id)
                .ok_or_else(|| {
                    format!(
                        "there is no body {n}; the document has {} body(ies)",
                        self.doc.bodies().len()
                    )
                });
        }
        self.doc
            .bodies()
            .first()
            .map(|b| b.id)
            .ok_or_else(|| "the document has no body yet".to_string())
    }

    pub fn run_script(&mut self, text: &str) -> Result<(), CliError> {
        for (n, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            self.run_line(line)
                .map_err(|msg| CliError::Script { line: n + 1, msg })?;
        }
        Ok(())
    }

    pub fn run_line(&mut self, line: &str) -> Result<(), String> {
        let mut tokens = split_words(line)?;
        if tokens.is_empty() {
            return Ok(());
        }
        // `name "Text"` anywhere (except where a command has a name of its own) names the bodies the command creates
        let mut new_name = None;
        if !matches!(
            tokens[0].as_str(),
            "rename"
                | "material"
                | "delete"
                | "describe"
                | "select"
                | "linear-pattern"
                | "circular-pattern"
                | "mirror"
                | "group"
                | "edit-link"
        ) {
            if let Some(i) = tokens.iter().position(|t| t == "name") {
                if let Some(n) = tokens.get(i + 1).cloned() {
                    if n.trim().is_empty() || n.chars().count() > 100 {
                        return Err("a name must have 1 to 100 characters".into());
                    }
                    new_name = Some(n);
                    tokens.drain(i..=i + 1);
                }
            }
        }
        let before: Vec<BodyId> = self.doc.bodies().iter().map(|b| b.id).collect();
        let r = self.run_line_tokens(tokens);
        if let (Ok(()), Some(name)) = (&r, new_name) {
            let created: Vec<BodyId> = self
                .doc
                .bodies()
                .iter()
                .map(|b| b.id)
                .filter(|id| !before.contains(id))
                .collect();
            for id in created {
                self.doc
                    .apply(Command::Rename {
                        target: rolidium_doc::ItemId::Body(id),
                        name: name.clone(),
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
        r
    }

    fn run_line_tokens(&mut self, mut tokens: Vec<String>) -> Result<(), String> {
        // `body N` anywhere in the line chooses the body the command works on
        self.target_body = None;
        if tokens[0] != "delete" {
            if let Some(i) = tokens.iter().position(|t| t == "body") {
                if let Some(n) = tokens.get(i + 1).and_then(|t| t.parse::<usize>().ok()) {
                    self.target_body = Some(n);
                    tokens.drain(i..=i + 1);
                }
            }
        }
        if tokens.is_empty() {
            return Err("the line has only a body number".into());
        }
        let w: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let num = |i: usize| -> Result<f64, String> {
            w.get(i)
                .ok_or_else(|| format!("'{}' is missing argument {i}", w[0]))?
                .replace(',', ".")
                .parse::<f64>()
                .map_err(|_| format!("'{}' is not a number", w[i]))
        };
        let mode = |i: usize| -> Result<PullMode, String> {
            match w.get(i).copied() {
                None | Some("auto") => Ok(PullMode::Auto),
                Some("add") => Ok(PullMode::Add),
                Some("cut") => Ok(PullMode::Cut),
                Some("new") => Ok(PullMode::New),
                Some(o) => Err(format!("unknown mode '{o}' (use auto, new, add or cut)")),
            }
        };
        match w[0] {
            "rename" => {
                let name = w.get(1).ok_or("rename needs a name")?;
                if name.trim().is_empty() || name.chars().count() > 100 {
                    return Err("a name must have 1 to 100 characters".into());
                }
                let id = self.first_body()?;
                self.doc
                    .apply(Command::Rename {
                        target: rolidium_doc::ItemId::Body(id),
                        name: (*name).to_string(),
                    })
                    .map_err(|e| e.to_string())?;
                self.say(format!("renamed to \"{name}\""));
            }
            "box" => {
                let (x, y, z) = (num(1)?, num(2)?, num(3)?);
                self.doc
                    .apply(Command::AddBox {
                        name: "Box".into(),
                        min: DVec3::ZERO,
                        max: DVec3::new(x, y, z),
                    })
                    .map_err(|e| e.to_string())?;
                self.say(format!("box {x} x {y} x {z}"));
            }
            "pull" => {
                let sel = w.get(1).ok_or("pull needs a face selector and a distance")?;
                let d = num(2)?;
                let id = self.first_body()?;
                let face = select_face(&self.doc.body(id).unwrap().solid, sel)?;
                self.doc
                    .apply(Command::PullFace {
                        body: id,
                        face,
                        distance: d,
                        mode: mode(3)?,
                    })
                    .map_err(|e| e.to_string())?;
                let s = &self.doc.body(id).unwrap().solid;
                self.say(format!(
                    "pull {sel} {d}: volume {:.3} mm3, {} faces",
                    s.volume().map_err(|e| e.to_string())?,
                    s.face_count()
                ));
            }
            "fill" => {
                // fill (edges <id>... | near <x> <y> <z>)
                let id = self.first_body()?;
                let solid = self.doc.body(id).unwrap().solid.clone();
                let mut edges: Vec<u32> = Vec::new();
                let mut k = 1;
                while k < w.len() {
                    match w[k] {
                        "edges" => {
                            k += 1;
                            while k < w.len() && w[k] != "near" {
                                let e: u32 = w[k]
                                    .trim_start_matches('#')
                                    .parse()
                                    .map_err(|_| format!("'{}' is not an edge id", w[k]))?;
                                if e as usize >= solid.edge_count() {
                                    return Err(format!(
                                        "the body has no edge {e} (it has {} edges, numbered from 0)",
                                        solid.edge_count()
                                    ));
                                }
                                edges.push(e);
                                k += 1;
                            }
                        }
                        "near" => {
                            let target = DVec3::new(num(k + 1)?, num(k + 2)?, num(k + 3)?);
                            let mut best: Option<(f64, u32)> = None;
                            for e in solid.edges() {
                                let Ok(i) = solid.edge_info(e) else { continue };
                                let d = ((i.start + i.end) * 0.5).distance(target);
                                if best.is_none_or(|(bd, _)| d < bd) {
                                    best = Some((d, e.0));
                                }
                            }
                            edges.push(best.ok_or("the body has no edges")?.1);
                            k += 4;
                        }
                        o => {
                            return Err(format!(
                                "usage: fill (edges <id>... | near <x> <y> <z>); unexpected '{o}'"
                            ))
                        }
                    }
                }
                if edges.is_empty() {
                    return Err("usage: fill (edges <id>... | near <x> <y> <z>)".into());
                }
                let c = self
                    .doc
                    .apply(Command::FillLoop { body: id, edges })
                    .map_err(|e| e.to_string())?;
                let sk = c.created_sketch.ok_or("the fill made no sketch")?;
                self.sketch = Some(sk);
                let area = self
                    .doc
                    .sketch(sk)
                    .and_then(|s| s.sketch.regions().first().map(|r| r.area()))
                    .unwrap_or(0.0);
                self.say(format!("fill: region 0 of the new sketch, area {area:.3} mm2; pull it with pull-region 0 <distance>"));
            }
            "join-ends" => {
                // join-ends <max_gap> [lines <n>...] | [curves]
                let gap = num(1)?;
                if w.get(2) == Some(&"curves") {
                    self.doc
                        .apply(Command::JoinCurves {
                            curves: vec![],
                            max_gap: gap,
                        })
                        .map_err(|e| e.to_string())?;
                    self.say(format!(
                        "join-ends: 3D lines joined, {} curve(s) now",
                        self.doc.curves().len()
                    ));
                    return Ok(());
                }
                let id = self.sketch.ok_or("there is no current sketch")?;
                let all: Vec<_> = self
                    .doc
                    .sketch(id)
                    .map(|s| s.sketch.entities().to_vec())
                    .ok_or("the current sketch no longer exists")?;
                let mut chosen = Vec::new();
                if w.get(2) == Some(&"lines") {
                    for t in &w[3..] {
                        let n: usize = t.parse().map_err(|_| format!("'{t}' is not a line number"))?;
                        chosen.push(
                            all.get(n.wrapping_sub(1))
                                .ok_or(format!("the sketch has no line {n}"))?
                                .0,
                        );
                    }
                }
                let used: Vec<_> = all
                    .iter()
                    .filter(|(i, _)| chosen.is_empty() || chosen.contains(i))
                    .copied()
                    .collect();
                let plan = rolidium_doc::rolidium_sketch::join::plan_join(&used, gap);
                self.doc
                    .apply(Command::JoinEnds {
                        sketch: id,
                        entities: chosen,
                        max_gap: gap,
                    })
                    .map_err(|e| e.to_string())?;
                let regions = self.doc.sketch(id).map_or(0, |s| s.sketch.regions().len());
                self.say(format!(
                    "join-ends: joined {} gap(s), {} end(s) stayed free; the sketch has {regions} region(s)",
                    plan.joined(),
                    plan.free.len()
                ));
            }
            "round" | "chamfer" => {
                // round|chamfer <size> (edges <id>... | near <x> <y> <z>) [angle <deg>] [flip]
                let size = num(1)?;
                let keywords = ["edges", "near", "angle", "flip"];
                let id = self.first_body()?;
                let solid = self.doc.body(id).unwrap().solid.clone();
                let mut edges: Vec<u32> = Vec::new();
                let mut angle: Option<f64> = None;
                let mut flip = false;
                let mut k = 2;
                while k < w.len() {
                    match w[k] {
                        "edges" => {
                            k += 1;
                            while k < w.len() && !keywords.contains(&w[k]) {
                                let e: u32 = w[k]
                                    .trim_start_matches('#')
                                    .parse()
                                    .map_err(|_| format!("'{}' is not an edge id", w[k]))?;
                                if e as usize >= solid.edge_count() {
                                    return Err(format!(
                                        "the body has no edge {e} (it has {} edges, numbered from 0)",
                                        solid.edge_count()
                                    ));
                                }
                                edges.push(e);
                                k += 1;
                            }
                        }
                        "near" => {
                            let target = DVec3::new(num(k + 1)?, num(k + 2)?, num(k + 3)?);
                            let mut best: Option<(f64, u32)> = None;
                            for e in solid.edges() {
                                let Ok(i) = solid.edge_info(e) else { continue };
                                let d = ((i.start + i.end) * 0.5).distance(target);
                                if best.is_none_or(|(bd, _)| d < bd) {
                                    best = Some((d, e.0));
                                }
                            }
                            edges.push(best.ok_or("the body has no edges")?.1);
                            k += 4;
                        }
                        "angle" => {
                            angle = Some(num(k + 1)?);
                            k += 2;
                        }
                        "flip" => {
                            flip = true;
                            k += 1;
                        }
                        o => {
                            return Err(format!(
                                "usage: {} <size> (edges <id>... | near <x> <y> <z>); unexpected '{o}'",
                                w[0]
                            ))
                        }
                    }
                }
                if edges.is_empty() {
                    return Err(format!(
                        "usage: {} <size> (edges <id>... | near <x> <y> <z>)",
                        w[0]
                    ));
                }
                if let (true, Some(angle)) = (w[0] == "chamfer", angle) {
                    self.doc
                        .apply(Command::ChamferAngled {
                            body: id,
                            edges,
                            distance: size,
                            angle_deg: angle,
                            flip,
                        })
                        .map_err(|e| e.to_string())?;
                    let s = &self.doc.body(id).unwrap().solid;
                    self.say(format!(
                        "chamfer {size} at {angle}: volume {:.3} mm3",
                        s.volume().map_err(|e| e.to_string())?
                    ));
                    return Ok(());
                }
                self.doc
                    .apply(Command::FilletEdges {
                        body: id,
                        edges,
                        size,
                        chamfer: w[0] == "chamfer",
                    })
                    .map_err(|e| e.to_string())?;
                let s = &self.doc.body(id).unwrap().solid;
                self.say(format!(
                    "{} {size}: volume {:.3} mm3, {} faces",
                    w[0],
                    s.volume().map_err(|e| e.to_string())?,
                    s.face_count()
                ));
            }
            "new" => {
                self.doc = Document::new();
                self.plane = None;
                self.sketch = None;
                self.file = None;
                self.say("new document");
            }
            "delete" => {
                let what = *w
                    .get(1)
                    .ok_or("delete needs body, sketch or curve and a number")?;
                let n = num(2)? as usize;
                let cmd = match what {
                    "body" => Command::DeleteBody {
                        body: self
                            .doc
                            .bodies()
                            .get(n.wrapping_sub(1))
                            .map(|b| b.id)
                            .ok_or(format!("there is no body {n}"))?,
                    },
                    "sketch" => {
                        let id = self
                            .doc
                            .sketches()
                            .get(n.wrapping_sub(1))
                            .map(|s| s.id)
                            .ok_or(format!("there is no sketch {n}"))?;
                        if self.sketch == Some(id) {
                            self.sketch = None;
                        }
                        Command::DeleteSketch { sketch: id }
                    }
                    "curve" => Command::DeleteCurve {
                        curve: self
                            .doc
                            .curves()
                            .get(n.wrapping_sub(1))
                            .map(|c| c.id)
                            .ok_or(format!("there is no curve {n}"))?,
                    },
                    o => return Err(format!("cannot delete '{o}' (body, sketch or curve)")),
                };
                self.doc.apply(cmd).map_err(|e| e.to_string())?;
                self.say(format!("deleted {what} {n}"));
            }
            "describe" => {
                let full = w.contains(&"full");
                let current = self
                    .sketch
                    .and_then(|id| self.doc.sketches().iter().position(|s| s.id == id))
                    .map(|i| i + 1);
                let v = describe::describe(&self.doc, self.target_body, full, current)?;
                self.say(v.to_string());
            }
            "select" => {
                // select face|edge [normal +z] [surface cylinder] [extreme +z] [of_face N] [edge_kind circle] [min_area A] [limit N]
                let id = self.first_body()?;
                let solid = self.doc.body(id).unwrap().solid.clone();
                let mut q = describe::Query {
                    edges: match *w.get(1).ok_or("select needs face or edge")? {
                        "face" => false,
                        "edge" => true,
                        o => return Err(format!("select face or edge, not '{o}'")),
                    },
                    ..Default::default()
                };
                let mut k = 2;
                while k < w.len() {
                    let val = |i: usize| {
                        w.get(i)
                            .copied()
                            .ok_or_else(|| format!("'{}' needs a value", w[k]))
                    };
                    match w[k] {
                        "normal" => q.normal = Some(describe::direction(val(k + 1)?)?),
                        "extreme" => q.extreme = Some(describe::direction(val(k + 1)?)?),
                        "surface" => q.surface = Some(val(k + 1)?.to_string()),
                        "edge_kind" => q.edge_kind = Some(val(k + 1)?.to_string()),
                        "of_face" => q.of_face = Some(num(k + 1)? as u32),
                        "min_area" => q.min_area = num(k + 1)?,
                        "limit" => q.limit = num(k + 1)? as usize,
                        o => return Err(format!("unknown select option '{o}'")),
                    }
                    k += 2;
                }
                let v = describe::select(&solid, &q)?;
                self.say(v.to_string());
            }
            "loft" => {
                let ruled = match *w.get(1).ok_or("loft needs ruled or smooth")? {
                    "ruled" => true,
                    "smooth" => false,
                    o => return Err(format!("unknown loft type '{o}'")),
                };
                let mode = match w.get(2).copied() {
                    None | Some("new") => rolidium_doc::LoftMode::NewBody,
                    Some("add") => rolidium_doc::LoftMode::Add,
                    Some("cut") => rolidium_doc::LoftMode::Cut,
                    Some(o) => return Err(format!("unknown mode '{o}'")),
                };
                let profiles: Vec<rolidium_doc::LoftRef> = self
                    .doc
                    .sketches()
                    .iter()
                    .map(|s| rolidium_doc::LoftRef::Region {
                        sketch: s.id,
                        region: 0,
                    })
                    .collect();
                self.doc
                    .apply(Command::Loft {
                        profiles,
                        ruled,
                        mode,
                    })
                    .map_err(|e| e.to_string())?;
                let vol: f64 = self
                    .doc
                    .bodies()
                    .iter()
                    .filter_map(|b| b.solid.volume().ok())
                    .sum();
                self.say(format!(
                    "loft: total volume {vol:.3} mm3, {} body(ies)",
                    self.doc.bodies().len()
                ));
            }
            "curve" => {
                // curve [radius R] x y z x y z ...   a 3D line; with a radius the corners are rounded
                let mut i = 1;
                let mut radius = 0.0;
                if w.get(1) == Some(&"radius") {
                    radius = num(2)?;
                    i = 3;
                }
                let rest = &w[i.min(w.len())..];
                if rest.len() < 6 || !rest.len().is_multiple_of(3) {
                    return Err("curve needs at least two points (x y z each)".into());
                }
                let mut points = Vec::new();
                for k in (0..rest.len()).step_by(3) {
                    let n = |j: usize| {
                        rest[k + j]
                            .replace(',', ".")
                            .parse::<f64>()
                            .map_err(|_| format!("'{}' is not a number", rest[k + j]))
                    };
                    points.push(DVec3::new(n(0)?, n(1)?, n(2)?));
                }
                let c = self
                    .doc
                    .apply(Command::AddCurve {
                        def: rolidium_doc::CurveDef::Polyline { points, radius },
                    })
                    .map_err(|e| e.to_string())?;
                let id = c.created_curve.ok_or("the curve was not created")?;
                let len = self
                    .doc
                    .curve(id)
                    .and_then(|c| c.def.path().ok())
                    .map_or(0.0, |p| p.length);
                self.say(format!("curve {}: length {len:.3} mm", self.doc.curves().len()));
            }
            "helix" => {
                // helix radius R pitch P (turns N | height H) [left] [start deg] [cone deg] [at x y z] [axis x y z]
                let mut h = rolidium_kernel::HelixDef {
                    origin: DVec3::ZERO,
                    axis: DVec3::Z,
                    x_dir: DVec3::X,
                    radius: 0.0,
                    pitch: 0.0,
                    turns: 0.0,
                    right: true,
                    start_deg: 0.0,
                    cone_deg: 0.0,
                };
                let mut height = None;
                let mut i = 1;
                while i < w.len() {
                    match w[i] {
                        "radius" => {
                            h.radius = num(i + 1)?;
                            i += 2;
                        }
                        "diameter" => {
                            h.radius = num(i + 1)? / 2.0;
                            i += 2;
                        }
                        "pitch" => {
                            h.pitch = num(i + 1)?;
                            i += 2;
                        }
                        "turns" => {
                            h.turns = num(i + 1)?;
                            i += 2;
                        }
                        "height" => {
                            height = Some(num(i + 1)?);
                            i += 2;
                        }
                        "start" => {
                            h.start_deg = num(i + 1)?;
                            i += 2;
                        }
                        "cone" => {
                            h.cone_deg = num(i + 1)?;
                            i += 2;
                        }
                        "left" => {
                            h.right = false;
                            i += 1;
                        }
                        "right" => {
                            h.right = true;
                            i += 1;
                        }
                        "at" => {
                            h.origin = DVec3::new(num(i + 1)?, num(i + 2)?, num(i + 3)?);
                            i += 4;
                        }
                        "axis" => {
                            h.axis = DVec3::new(num(i + 1)?, num(i + 2)?, num(i + 3)?);
                            i += 4;
                        }
                        o => return Err(format!("unknown helix option '{o}'")),
                    }
                }
                if let Some(ht) = height {
                    if h.pitch.abs() < 1e-9 {
                        return Err("a helix with a height needs a pitch".into());
                    }
                    h.turns = ht / h.pitch.abs();
                }
                let c = self
                    .doc
                    .apply(Command::AddCurve {
                        def: rolidium_doc::CurveDef::Helix(h),
                    })
                    .map_err(|e| e.to_string())?;
                let id = c.created_curve.ok_or("the helix was not created")?;
                let len = self
                    .doc
                    .curve(id)
                    .and_then(|c| c.def.path().ok())
                    .map_or(0.0, |p| p.length);
                self.say(format!(
                    "curve {}: helix, length {len:.3} mm, {:.3} turns",
                    self.doc.curves().len(),
                    h.turns
                ));
            }
            "curves" => {
                let list: Vec<String> = self
                    .doc
                    .curves()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        format!(
                            "curve {} '{}': {:.3} mm",
                            i + 1,
                            c.name,
                            c.def.path().map_or(0.0, |p| p.length)
                        )
                    })
                    .collect();
                for l in list {
                    self.say(l);
                }
            }
            "sweep" => {
                // sweep <region> (curve N | chain <sketch N> <shape N>) [auto|new|add|cut] [rounded]
                let region = num(1)? as usize;
                let sk = self.sketch.ok_or("draw the profile in a sketch first")?;
                let path = match *w.get(2).ok_or("sweep needs 'curve N' or 'chain S E'")? {
                    "curve" => {
                        let n = num(3)? as usize;
                        let c = self
                            .doc
                            .curves()
                            .get(n.wrapping_sub(1))
                            .ok_or(format!("there is no curve {n}"))?;
                        rolidium_doc::SweepPath::Curve(c.id)
                    }
                    "chain" => {
                        let (sn, en) = (num(3)? as usize, num(4)? as usize);
                        let sd = self
                            .doc
                            .sketches()
                            .get(sn.wrapping_sub(1))
                            .ok_or(format!("there is no sketch {sn}"))?;
                        let seed = sd
                            .sketch
                            .entities()
                            .get(en.wrapping_sub(1))
                            .map(|e| e.0)
                            .ok_or(format!("there is no shape {en}"))?;
                        rolidium_doc::SweepPath::Sketch {
                            sketch: sd.id,
                            entities: rolidium_doc::sketch_chain(&sd.sketch, seed),
                        }
                    }
                    o => return Err(format!("unknown path '{o}'")),
                };
                let mut mode = rolidium_doc::SweepMode::Auto;
                let mut rounded = false;
                for o in &w[3..] {
                    match *o {
                        "auto" => mode = rolidium_doc::SweepMode::Auto,
                        "new" => mode = rolidium_doc::SweepMode::NewBody,
                        "add" => mode = rolidium_doc::SweepMode::Add,
                        "cut" => mode = rolidium_doc::SweepMode::Cut,
                        "rounded" => rounded = true,
                        _ => {}
                    }
                }
                let t = std::time::Instant::now();
                let c = self
                    .doc
                    .apply(Command::Sweep {
                        profile: rolidium_doc::LoftRef::Region { sketch: sk, region },
                        path,
                        mode,
                        rounded,
                    })
                    .map_err(|e| e.to_string())?;
                if let Some(n) = &c.notice {
                    self.say(format!("note: {n}"));
                }
                let vol: f64 = self
                    .doc
                    .bodies()
                    .iter()
                    .filter_map(|b| b.solid.volume().ok())
                    .sum();
                self.say(format!(
                    "sweep: {:.1} ms, total volume {vol:.3} mm3, {} body(ies)",
                    t.elapsed().as_secs_f64() * 1000.0,
                    self.doc.bodies().len()
                ));
            }
            "combine" => {
                let op = match *w.get(1).ok_or("combine needs merge, cut or intersect")? {
                    "merge" => rolidium_doc::CombineOp::Merge,
                    "cut" => rolidium_doc::CombineOp::Cut,
                    "intersect" => rolidium_doc::CombineOp::Intersect,
                    o => return Err(format!("unknown operation '{o}'")),
                };
                let keep = w.last() == Some(&"keep");
                let nums: Vec<usize> = w[2..].iter().filter_map(|x| x.parse().ok()).collect();
                if nums.len() < 2 {
                    return Err("combine needs a target and at least one tool (body numbers from 1)".into());
                }
                let id_of = |n: usize| {
                    self.doc
                        .bodies()
                        .get(n.wrapping_sub(1))
                        .map(|b| b.id)
                        .ok_or(format!("there is no body {n}"))
                };
                let target = id_of(nums[0])?;
                let tools: Vec<_> = nums[1..].iter().map(|n| id_of(*n)).collect::<Result<_, _>>()?;
                self.doc
                    .apply(Command::Combine {
                        target,
                        tools,
                        op,
                        keep_tools: keep,
                    })
                    .map_err(|e| e.to_string())?;
                let vol: f64 = self
                    .doc
                    .bodies()
                    .iter()
                    .filter_map(|b| b.solid.volume().ok())
                    .sum();
                self.say(format!(
                    "combine: {} body(ies), total volume {vol:.3} mm3",
                    self.doc.bodies().len()
                ));
            }
            "features" => {
                let id = self.first_body()?;
                let rows: Vec<String> = self
                    .doc
                    .body(id)
                    .unwrap()
                    .features
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        format!(
                            "feature {i}: {:?} {} mm, {} face(s)",
                            f.kind,
                            f.size,
                            f.faces.len()
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    self.say("no rounds or chamfers");
                }
                for r in rows {
                    self.say(r);
                }
            }
            "edit-feature" => {
                let idx = num(1)? as usize;
                let size = num(2)?;
                let id = self.first_body()?;
                let face = self
                    .doc
                    .body(id)
                    .unwrap()
                    .features
                    .get(idx)
                    .and_then(|f| f.faces.first().copied())
                    .ok_or_else(|| format!("there is no round or chamfer number {idx}"))?;
                let t = std::time::Instant::now();
                self.doc
                    .apply(Command::EditBlend { body: id, face, size })
                    .map_err(|e| e.to_string())?;
                let ms = t.elapsed().as_secs_f64() * 1e3;
                let s = &self.doc.body(id).unwrap().solid;
                self.say(format!(
                    "edit-feature {idx} {size}: {ms:.1} ms, volume {:.3} mm3, {} faces",
                    s.volume().map_err(|e| e.to_string())?,
                    s.face_count()
                ));
            }
            "sketch" => {
                let sel = w.get(1).ok_or("sketch needs 'xy' or a face selector")?;
                if *sel == "xy" || *sel == "xz" || *sel == "yz" {
                    let pl = match *sel {
                        "xz" => PlaneDef::xz(),
                        "yz" => PlaneDef::yz(),
                        _ => PlaneDef::base(),
                    };
                    let off = w.get(2).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    let pl = PlaneDef {
                        origin: pl.origin + pl.normal * off,
                        ..pl
                    };
                    self.plane = Some((pl, None));
                } else {
                    let id = self.first_body()?;
                    let solid = &self.doc.body(id).unwrap().solid;
                    let face = select_face(solid, sel)?;
                    let info = solid.face_info(face).map_err(|e| e.to_string())?;
                    if info.kind != FaceKind::Plane {
                        return Err("sketches can only be drawn on flat faces".into());
                    }
                    self.plane = Some((PlaneDef::from_face(info.origin, info.normal), Some(id)));
                }
                self.sketch = None;
                let pl = self.plane.map(|p| p.0).unwrap_or_else(PlaneDef::base);
                let f = |v: DVec3| format!("({:.3}, {:.3}, {:.3})", v.x + 0.0, v.y + 0.0, v.z + 0.0);
                self.say(format!(
                    "sketch on {sel}: sketch point (0, 0) is at {}, sketch x axis = {}, sketch y axis = {}, normal = {}",
                    f(pl.origin),
                    f(pl.x_dir),
                    f(pl.y_dir()),
                    f(pl.normal)
                ));
            }
            "line" | "rect" | "circle" | "arc" => {
                let entity = match w[0] {
                    "arc" => Entity::arc_through(
                        DVec2::new(num(1)?, num(2)?),
                        DVec2::new(num(3)?, num(4)?),
                        DVec2::new(num(5)?, num(6)?),
                    )
                    .ok_or("the three points of an arc must not lie in a line")?,
                    "line" => Entity::Line {
                        a: DVec2::new(num(1)?, num(2)?),
                        b: DVec2::new(num(3)?, num(4)?),
                    },
                    "rect" => Entity::Rect {
                        a: DVec2::new(num(1)?, num(2)?),
                        b: DVec2::new(num(3)?, num(4)?),
                    },
                    _ => Entity::Circle {
                        center: DVec2::new(num(1)?, num(2)?),
                        radius: num(3)? / 2.0,
                    },
                };
                let target = match (self.sketch, self.plane) {
                    (Some(id), _) if self.doc.sketch(id).is_some() => SketchTarget::Existing(id),
                    (_, Some((plane, body))) => SketchTarget::New { plane, body },
                    _ => return Err("choose a plane first with 'sketch xy' or 'sketch top'".into()),
                };
                let c = self
                    .doc
                    .apply(Command::AddEntity { target, entity })
                    .map_err(|e| e.to_string())?;
                if let Some(id) = c.created_sketch {
                    self.sketch = Some(id);
                }
                self.say(format!("{} added", w[0]));
            }
            "regions" => {
                let id = self.sketch.ok_or("there is no current sketch")?;
                let sk = self.doc.sketch(id).ok_or("the current sketch no longer exists")?;
                let rows: Vec<String> = sk
                    .sketch
                    .regions()
                    .iter()
                    .enumerate()
                    .map(|(i, r)| format!("region {i}: area {:.3} mm2, {} hole(s)", r.area(), r.holes.len()))
                    .collect();
                if rows.is_empty() {
                    self.say("no closed regions");
                }
                for r in rows {
                    self.say(r);
                }
            }
            "pull-region" => {
                let id = self.sketch.ok_or("there is no current sketch")?;
                let region = num(1)? as usize;
                let d = num(2)?;
                let t = std::time::Instant::now();
                self.doc
                    .apply(Command::PullRegion {
                        sketch: id,
                        region,
                        distance: d,
                        mode: mode(3)?,
                    })
                    .map_err(|e| e.to_string())?;
                let ms = t.elapsed().as_secs_f64() * 1e3;
                let vol: f64 = self
                    .doc
                    .bodies()
                    .iter()
                    .filter_map(|b| b.solid.volume().ok())
                    .sum();
                self.say(format!(
                    "pull-region {region} {d}: {ms:.1} ms, total volume {vol:.3} mm3, {} body(ies)",
                    self.doc.bodies().len()
                ));
            }
            "move" | "rotate" => {
                let id = self.first_body()?;
                let copy = w.last() == Some(&"copy");
                let (translation, axis, angle) = if w[0] == "move" {
                    (DVec3::new(num(1)?, num(2)?, num(3)?), DVec3::Z, 0.0)
                } else {
                    let axis = match *w.get(1).ok_or("rotate needs an axis (x, y or z) and an angle")? {
                        "x" => DVec3::X,
                        "y" => DVec3::Y,
                        "z" => DVec3::Z,
                        o => return Err(format!("unknown axis '{o}'")),
                    };
                    (DVec3::ZERO, axis, num(2)?.to_radians())
                };
                let pivot = self
                    .doc
                    .body(id)
                    .unwrap()
                    .solid
                    .props()
                    .map_err(|e| e.to_string())?;
                let pivot = (pivot.min + pivot.max) * 0.5;
                self.doc
                    .apply(Command::Move {
                        bodies: vec![id],
                        translation,
                        pivot,
                        axis,
                        angle,
                        copy,
                    })
                    .map_err(|e| e.to_string())?;
                let p = self
                    .doc
                    .bodies()
                    .last()
                    .unwrap()
                    .solid
                    .props()
                    .map_err(|e| e.to_string())?;
                self.say(format!(
                    "{}: {} body(ies), box {:.3} {:.3} {:.3} .. {:.3} {:.3} {:.3}",
                    w[0],
                    self.doc.bodies().len(),
                    p.min.x,
                    p.min.y,
                    p.min.z,
                    p.max.x,
                    p.max.y,
                    p.max.z
                ));
            }
            "revolve" => {
                let id = self.sketch.ok_or("there is no current sketch")?;
                let region = num(1)? as usize;
                let axis = match *w.get(2).ok_or("revolve needs an axis (x, y or z)")? {
                    "x" => DVec3::X,
                    "y" => DVec3::Y,
                    "z" => DVec3::Z,
                    o => return Err(format!("unknown axis '{o}'")),
                };
                let angle = num(3)?.to_radians();
                let mode = match w.get(4).copied() {
                    None | Some("auto") => PullMode::Auto,
                    Some("add") => PullMode::Add,
                    Some("cut") => PullMode::Cut,
                    Some("new") => PullMode::New,
                    Some(o) => return Err(format!("unknown mode '{o}'")),
                };
                self.doc
                    .apply(Command::Revolve {
                        sketch: id,
                        region,
                        axis_point: DVec3::ZERO,
                        axis_dir: axis,
                        angle,
                        mode,
                    })
                    .map_err(|e| e.to_string())?;
                let vol: f64 = self
                    .doc
                    .bodies()
                    .iter()
                    .filter_map(|b| b.solid.volume().ok())
                    .sum();
                self.say(format!(
                    "revolve: total volume {vol:.3} mm3, {} body(ies)",
                    self.doc.bodies().len()
                ));
            }
            "save" => {
                let path = w
                    .get(1)
                    .map(PathBuf::from)
                    .or_else(|| self.file.clone())
                    .ok_or("save needs a file name")?;
                rolidium_doc::file::save_file(&self.doc, &path, None, env!("CARGO_PKG_VERSION"))
                    .map_err(|e| e.to_string())?;
                self.doc.mark_saved();
                self.say(format!("saved {}", path.display()));
                self.file = Some(path);
            }
            "open" => {
                let path = PathBuf::from(w.get(1).ok_or("open needs a file name")?);
                self.open_file(&path)?;
            }
            "import-step" => {
                let path = w.get(1).ok_or("import-step needs a file name")?;
                let data = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
                let (solids, skipped) = Solid::read_step_all_named(&data).map_err(|e| e.to_string())?;
                for s in &skipped {
                    self.say(format!("skipped: {s}"));
                }
                let n = solids.len();
                if n > 0 {
                    self.doc
                        .apply(Command::AddNamedSolids { solids })
                        .map_err(|e| e.to_string())?;
                }
                self.say(format!(
                    "import-step: {n} body(ies) added, {} skipped",
                    skipped.len()
                ));
            }
            "undo" => {
                if !self.doc.undo() {
                    return Err("nothing to undo".into());
                }
                self.say("undo");
            }
            "redo" => {
                if !self.doc.redo() {
                    return Err("nothing to redo".into());
                }
                self.say("redo");
            }
            "measure" => {
                // measure <element> <element>      element: point x y z | face <sel> | edge #N
                let id = self.first_body()?;
                let solid = self.doc.body(id).unwrap().solid.clone();
                let mut k = 1;
                let mut elems: Vec<rolidium_kernel::Elem> = Vec::new();
                let mut texts = Vec::new();
                while k < w.len() {
                    match w[k] {
                        "point" => {
                            elems.push(rolidium_kernel::Elem::Point(DVec3::new(
                                num(k + 1)?,
                                num(k + 2)?,
                                num(k + 3)?,
                            )));
                            texts.push(format!("point {} {} {}", w[k + 1], w[k + 2], w[k + 3]));
                            k += 4;
                        }
                        "face" => {
                            let sel = w.get(k + 1).ok_or("face needs a selector")?;
                            elems.push(rolidium_kernel::Elem::Face(&solid, select_face(&solid, sel)?));
                            texts.push(format!("face {sel}"));
                            k += 2;
                        }
                        "edge" => {
                            let sel = w
                                .get(k + 1)
                                .and_then(|s| s.strip_prefix('#'))
                                .ok_or("edge needs #index")?;
                            let i: u32 = sel.parse().map_err(|_| "bad edge index".to_string())?;
                            if i as usize >= solid.edge_count() {
                                return Err(format!("no edge #{i}"));
                            }
                            elems.push(rolidium_kernel::Elem::Edge(&solid, rolidium_kernel::EdgeId(i)));
                            texts.push(format!("edge #{i}"));
                            k += 2;
                        }
                        other => return Err(format!("unknown element '{other}' (point, face, edge)")),
                    }
                }
                if elems.len() != 2 {
                    return Err("measure needs two elements".into());
                }
                let m = rolidium_kernel::measure(&elems[0], &elems[1]).map_err(|e| e.to_string())?;
                let mut line = format!(
                    "measure {} / {}: distance {:.3} mm (dx {:.3}, dy {:.3}, dz {:.3})",
                    texts[0], texts[1], m.distance, m.delta.x, m.delta.y, m.delta.z
                );
                if let Some(d) = m.parallel_distance {
                    line += &format!(", parallel faces {d:.3} mm apart");
                }
                if let Some(a) = m.angle {
                    line += &format!(", angle {:.3} deg", a.degrees);
                }
                if let Some((d, _, _)) = m.axes_distance {
                    line += &format!(", axes {d:.3} mm apart");
                }
                self.say(line);
            }
            "material" => {
                // material <steel|aluminium|brass|copper|PLA|PETG|ABS|density g/cm3>
                let id = self.first_body()?;
                let what = w.get(1).ok_or("material needs a name or a density (g/cm3)")?;
                let material = match rolidium_doc::MATERIALS
                    .iter()
                    .position(|(n, _)| n.eq_ignore_ascii_case(what))
                {
                    Some(i) => rolidium_doc::Material::preset(i),
                    None => {
                        let d: f64 = what
                            .replace(',', ".")
                            .parse()
                            .map_err(|_| format!("unknown material '{what}'"))?;
                        rolidium_doc::Material {
                            name: "custom".into(),
                            density: d,
                        }
                    }
                };
                self.doc
                    .apply(Command::SetMaterial {
                        body: id,
                        material: Some(material),
                    })
                    .map_err(|e| e.to_string())?;
                self.say(format!("material {what}"));
            }
            "mass" => {
                let rows: Vec<String> = self
                    .doc
                    .bodies()
                    .iter()
                    .map(|b| {
                        let m = b.material_or_default();
                        format!(
                            "body {} '{}': mass {:.3} g ({} {} g/cm3)",
                            b.id,
                            b.name,
                            b.mass_g().unwrap_or(f64::NAN),
                            m.name,
                            m.density
                        )
                    })
                    .collect();
                for r in rows {
                    self.say(r);
                }
            }
            "info" => {
                let rows: Vec<String> = self
                    .doc
                    .bodies()
                    .iter()
                    .map(|b| {
                        let p = b.solid.props().ok();
                        format!(
                            "body {} '{}': {} faces, volume {:.3}, valid {}",
                            b.id,
                            b.name,
                            b.solid.face_count(),
                            p.map(|p| p.volume).unwrap_or(f64::NAN),
                            b.solid.is_valid()
                        )
                    })
                    .collect();
                for r in rows {
                    self.say(r);
                }
            }
            "export-stl" => {
                let path = w.get(1).ok_or("export-stl needs a file name")?;
                // `body N` exports that body, else all bodies go into one file
                let ids: Vec<BodyId> = if self.target_body.is_some() {
                    vec![self.first_body()?]
                } else {
                    self.doc.bodies().iter().map(|b| b.id).collect()
                };
                if ids.is_empty() {
                    self.first_body()?;
                }
                let (mut dev, mut ang, mut ascii) = (0.02_f64, 10.0_f64, false);
                let mut k = 2;
                while k < w.len() {
                    match w[k] {
                        "--deviation" => {
                            dev = w
                                .get(k + 1)
                                .and_then(|v| v.parse().ok())
                                .ok_or("--deviation needs a number (mm)")?;
                            k += 1;
                        }
                        "--angle" => {
                            ang = w
                                .get(k + 1)
                                .and_then(|v| v.parse().ok())
                                .ok_or("--angle needs a number (degrees)")?;
                            k += 1;
                        }
                        "--ascii" => ascii = true,
                        "--binary" => ascii = false,
                        other => return Err(format!("unknown option '{other}'")),
                    }
                    k += 1;
                }
                if !(0.001..=10.0).contains(&dev) || !(1.0..=90.0).contains(&ang) {
                    return Err("deviation must be 0.001..10 mm and angle 1..90 degrees".into());
                }
                let mut mesh = rolidium_kernel::Mesh::default();
                for id in &ids {
                    let m = self
                        .doc
                        .body(*id)
                        .unwrap()
                        .solid
                        .mesh(dev, ang.to_radians())
                        .map_err(|e| e.to_string())?;
                    mesh.append(&m);
                }
                let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
                if ascii {
                    mesh.write_stl_ascii(&mut f).map_err(|e| e.to_string())?;
                } else {
                    mesh.write_stl_binary(&mut f).map_err(|e| e.to_string())?;
                }
                f.flush().map_err(|e| e.to_string())?;
                self.say(format!("wrote {path} ({} triangles)", mesh.triangle_count()));
            }
            "export-step" => {
                let path = w.get(1).ok_or("export-step needs a file name")?;
                // `body N` exports that body, else all bodies go into one file
                let solids: Vec<(Solid, String)> = if self.target_body.is_some() {
                    let b = self.doc.body(self.first_body()?).unwrap();
                    vec![(b.solid.clone(), b.name.clone())]
                } else {
                    self.doc
                        .bodies()
                        .iter()
                        .map(|b| (b.solid.clone(), b.name.clone()))
                        .collect()
                };
                if solids.is_empty() {
                    self.first_body()?;
                }
                let data = Solid::write_step_named(&solids).map_err(|e| e.to_string())?;
                std::fs::write(path, data).map_err(|e| e.to_string())?;
                self.say(format!("wrote {path}"));
            }
            "render" => {
                let path = w.get(1).ok_or("render needs a file name")?;
                let (mut width, mut height) = (800u32, 600u32);
                let mut view = StandardView::Isometric;
                let mut rest = &w[2..];
                if rest.len() >= 2 {
                    if let (Ok(a), Ok(b)) = (rest[0].parse(), rest[1].parse()) {
                        width = a;
                        height = b;
                        rest = &rest[2..];
                    }
                }
                if let Some(v) = rest.first() {
                    view = match *v {
                        "iso" => StandardView::Isometric,
                        "top" => StandardView::Top,
                        "bottom" => StandardView::Bottom,
                        "front" => StandardView::Front,
                        "back" => StandardView::Back,
                        "left" => StandardView::Left,
                        "right" => StandardView::Right,
                        other => return Err(format!("unknown view '{other}'")),
                    };
                }
                self.render_png(Path::new(path), width, height, view)?;
            }
            "linear-pattern" | "circular-pattern" | "mirror" | "links" => {
                self.run_links_command(&w)?;
            }
            "edit-link" | "unlink" | "delete-link" | "group" | "groups" => {
                self.run_links_command(&w)?;
            }
            "move-to-group" | "ungroup" | "sketch-pattern" | "sketch-mirror" => {
                self.run_links_command(&w)?;
            }
            other => return Err(format!("unknown command '{other}'")),
        }
        Ok(())
    }

    fn ensure_gpu(&mut self) -> Result<(), String> {
        if self.gpu.is_none() {
            let (device, queue, info) = rolidium_render::headless_device().map_err(|e| e.to_string())?;
            let renderer = Renderer::new(&device, rolidium_render::wgpu::TextureFormat::Rgba8Unorm);
            let name = format!("{} ({:?})", info.name, info.device_type);
            self.gpu = Some((device, queue, renderer, name));
        }
        Ok(())
    }

    pub fn adapter_name(&self) -> Option<&str> {
        self.gpu.as_ref().map(|g| g.3.as_str())
    }

    pub fn render_png(&mut self, path: &Path, w: u32, h: u32, view: StandardView) -> Result<(), String> {
        self.ensure_gpu()?;
        let mut lo = glam::Vec3::splat(f32::MAX);
        let mut hi = glam::Vec3::splat(f32::MIN);
        for b in self.doc.bodies() {
            let p = b.solid.props().map_err(|e| e.to_string())?;
            lo = lo.min(p.min.as_vec3());
            hi = hi.max(p.max.as_vec3());
        }
        if lo.x > hi.x {
            lo = glam::Vec3::splat(-50.0);
            hi = glam::Vec3::splat(50.0);
        }
        let mut camera = Camera::default();
        camera.set_view(view);
        camera.fit(lo, hi);
        // the meshes are made for this picture, like the window does for its zoom: under half a pixel from the exact shape
        let pixel_mm =
            2.0 * f64::from(camera.distance) * (f64::from(camera.fov_y) * 0.5).tan() / f64::from(h.max(1));
        let cap = (1_500_000 / self.doc.bodies().len().max(1)).clamp(20_000, 300_000);
        let meshes: Vec<_> = self
            .doc
            .bodies()
            .iter()
            .map(|b| {
                b.solid
                    .view_mesh(pixel_mm, cap, None)
                    .map(|m| (b.id, m, b.color))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<_, _>>()?;
        let (device, queue, renderer, _) = self.gpu.as_mut().unwrap();
        let draws: Vec<BodyDraw> = meshes
            .iter()
            .map(|(id, m, c)| BodyDraw {
                id: *id,
                mesh: m,
                stamp: 0,
                color: *c,
            })
            .collect();
        renderer.set_bodies(device, &draws);
        let mut layers = Vec::new();
        let mut fills = Vec::new();
        let mut lines = Vec::new();
        for sk in self.doc.sketches() {
            for r in sk.sketch.regions() {
                fills.push(rolidium_render::region_geometry(&sk.plane, &r));
            }
            lines.extend(rolidium_render::sketch_lines(&sk.plane, &sk.sketch));
        }
        let refs: Vec<&rolidium_render::RegionGeom> = fills.iter().collect();
        layers.push(rolidium_render::fill_layer(&refs, [0.35, 0.65, 1.0, 0.22]));
        layers.push(rolidium_render::PreviewDraw {
            triangles: vec![],
            lines,
            color: [0.0; 4],
            line_color: [1.0, 0.85, 0.35, 1.0],
            ..Default::default()
        });
        renderer.set_layers(device, &layers);
        let grid = self.plane.map(|(p, _)| rolidium_render::GridPlane {
            origin: p.origin.as_vec3(),
            u: p.x_dir.as_vec3(),
            v: p.y_dir().as_vec3(),
        });
        let frame = Frame {
            camera,
            hover: 0,
            selected: vec![],
            body_marks: Vec::new(),
            size: [w, h],
            grid,
            render_ids: true,
            grid_levels: None,
            scene_radius: 0.0,
        };
        let rgba = renderer
            .render_to_rgba(device, queue, &frame)
            .map_err(|e| e.to_string())?;
        let png = rolidium_render::encode_png(&rgba, w, h).map_err(|e| e.to_string())?;
        std::fs::write(path, png).map_err(|e| e.to_string())?;
        let name = self.gpu.as_ref().unwrap().3.clone();
        self.say(format!("wrote {} ({w}x{h}, adapter: {name})", path.display()));
        Ok(())
    }
}
