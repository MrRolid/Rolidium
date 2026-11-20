// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The MCP (Model Context Protocol) server behind `rolidium-mcp`: lets an AI client build and inspect a model without a
//! window. It speaks JSON-RPC 2.0 over standard input and output, one message per line, in both the revision with the
//! `initialize` handshake (2025-11-25 and older) and the one with `server/discover` and per-request `_meta`
//! (2026-07-28). The tools come from the table in [`crate::spec`], the same table that defines the script language, and
//! a tool call is carried out by the very same [`Runner::run_line`] that runs scripts.
//!
//! Safety: files are read and written only under the directory given with `--root`; there is no network access and no
//! command is ever run; every call has a time limit that uses the cancellable calculation of the kernel.

use crate::spec::{self, Cmd, Exec};
use crate::{describe, soft, Runner};
use glam::Vec3;
use rolidium_render::{Camera, StandardView};
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Revisions of the protocol this server answers, newest first.
pub const SUPPORTED_VERSIONS: &[&str] = &[
    "2026-07-28",
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// The longest message line accepted.
pub const MAX_LINE: usize = 8 * 1024 * 1024;
/// The biggest file the server opens or imports.
const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;

const INSTRUCTIONS: &str = "Rolidium is a direct-modelling CAD program. Units are millimetres and degrees; Z is up. \
Typical flow: start_sketch on a plane (xy, xz, yz) or a face, draw closed shapes (sketch_rectangle, sketch_circle, sketch_line, sketch_arc), \
pull_region to give them thickness (mode cut makes holes and pockets), then round_edges / chamfer_edges / combine_bodies, \
and save_document / export_stl. Call describe_model to learn the numbers and ids (bodies from 1, faces and edges from 0) that other tools refer to, \
select_geometry to find faces or edges by a query, and render_view to look at the result (it returns a picture). \
A failing call leaves the model unchanged and explains what to try instead.";

/// What a tool call returns.
pub struct ToolOutput {
    pub text: String,
    /// A PNG picture, if the tool made one.
    pub image: Option<Vec<u8>>,
    pub is_error: bool,
}

pub struct Server {
    pub runner: Runner,
    root: Option<PathBuf>,
    timeout: Duration,
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn rpc_error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({"code": code, "message": message});
    if let Some(d) = data {
        e["data"] = d;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": e})
}

/// What to try instead, for the common failures.
pub fn advice(err: &str) -> Option<&'static str> {
    let e = err.to_lowercase();
    let table: &[(&str, &str)] = &[
        ("choose a plane first", "Call start_sketch first (for example on 'xy'), then draw the shapes."),
        ("no current sketch", "Draw a closed shape first: start_sketch, then sketch_rectangle, sketch_circle, sketch_line or sketch_arc."),
        ("no body yet", "Make a body first: pull_region on a closed sketch region, or add_box."),
        ("could not be made cleanly", "A round or chamfer of this size does not fit that edge. Try a smaller value (for example half of it), or select the whole loop of edges."),
        ("radius too large", "That radius does not fit the edge. Try a smaller value."),
        ("does not fit these edges", "A radius or chamfer of this size does not fit those edges. Try a smaller value (for example half of it)."),
        ("do not touch or overlap", "Check the bounding boxes with describe_model and move one body with move_body so that they overlap, then try again."),
        ("do not overlap", "Check the bounding boxes with describe_model and move one body with move_body so that they overlap, then try again."),
        ("unknown face selector", "Use top, bottom, front, back, left, right or #N with a face id from describe_model or select_geometry."),
        ("no planar face facing", "No flat face looks that way. Use describe_model or select_geometry and pass #N with the id of a face."),
        ("there is no body", "Body numbers start at 1; call describe_model to see which bodies exist."),
        ("there is no curve", "Curve numbers start at 1; add one with add_3d_line or add_helix, and call describe_model to see them."),
        ("there is no sketch", "Sketch numbers start at 1; call describe_model to see them."),
        ("cuts through itself", "Make the profile smaller than the pitch or the bends, or change the path, then try again."),
        ("no such region", "Call list_regions to see the regions of the current sketch; the first one is 0."),
        ("not a number", "Numbers must be plain numbers in mm or degrees."),
        ("unknown command", "Use the tools listed by tools/list."),
        ("not a closed", "The shape must be a closed loop: lines must meet end to end, or use a rectangle or circle."),
    ];
    table.iter().find(|(k, _)| e.contains(k)).map(|(_, v)| *v)
}

impl Server {
    /// `root`: the only directory files are read from and written to (`None`: no file access).
    pub fn new(root: Option<&Path>, timeout: Duration) -> Result<Self, String> {
        let root = match root {
            Some(r) => Some(
                r.canonicalize()
                    .map_err(|e| format!("the root directory {}: {e}", r.display()))?,
            ),
            None => None,
        };
        let mut runner = Runner::new();
        runner.quiet = true;
        Ok(Server {
            runner,
            root,
            timeout,
        })
    }

    /// A path from a tool call, checked to lie under the root. Symbolic links are resolved first.
    pub fn resolve_path(&self, p: &str, exts: &[&str]) -> Result<String, String> {
        let root = self
            .root
            .as_ref()
            .ok_or("File access is disabled: start the server with --root <directory>.")?;
        if p.is_empty() || p.contains('\0') || p.contains('"') {
            return Err("the path is not valid".into());
        }
        let given = Path::new(p);
        let joined = if given.is_absolute() {
            given.to_path_buf()
        } else {
            root.join(given)
        };
        let mut norm = PathBuf::new();
        for c in joined.components() {
            match c {
                Component::ParentDir => {
                    if !norm.pop() {
                        return Err(format!("the path '{p}' is outside the root directory"));
                    }
                }
                Component::CurDir => {}
                c => norm.push(c),
            }
        }
        // resolve links through the deepest part that exists
        let mut existing = norm.clone();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !existing.exists() {
            match existing.file_name() {
                Some(n) => tail.push(n.to_os_string()),
                None => break,
            }
            if !existing.pop() {
                break;
            }
        }
        let mut full = existing
            .canonicalize()
            .map_err(|e| format!("the path '{p}' cannot be used: {e}"))?;
        for t in tail.iter().rev() {
            full.push(t);
        }
        if !full.starts_with(root) {
            return Err(format!(
                "the path '{p}' is outside the root directory ({}); files can only be read and written under it",
                root.display()
            ));
        }
        let ext_ok = full
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)));
        if !exts.is_empty() && !ext_ok {
            return Err(format!(
                "the file name must end with {}",
                exts.iter()
                    .map(|e| format!(".{e}"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
        }
        Ok(full.to_string_lossy().into_owned())
    }

    fn extensions(cmd: &Cmd) -> &'static [&'static str] {
        match cmd.name {
            "open" | "save" => &["rolidium"],
            "export-stl" => &["stl"],
            "export-step" | "import-step" => &["step", "stp"],
            "render" => &["png"],
            _ => &[],
        }
    }

    /// Short state of the document, added to the answer of a call that changes it.
    pub fn summary(&self) -> String {
        let d = &self.runner.doc;
        let mut parts = Vec::new();
        for (i, b) in d.bodies().iter().enumerate() {
            parts.push(format!(
                "body {} '{}' volume {:.2} mm3, {} faces",
                i + 1,
                b.name,
                b.solid.volume().unwrap_or(f64::NAN),
                b.solid.face_count()
            ));
        }
        format!(
            "Model: {} bodies ({}); {} sketches; {} curves.",
            d.bodies().len(),
            parts.join("; "),
            d.sketches().len(),
            d.curves().len()
        )
    }

    /// Runs `f` with a time limit: after it, the calculation is cancelled through the kernel (the model stays unchanged).
    fn with_limit<R>(&mut self, f: impl FnOnce(&mut Runner) -> R) -> (R, bool) {
        let timeout = self.timeout;
        rolidium_kernel::reset_cancel();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fired2 = fired.clone();
        let watchdog = std::thread::spawn(move || {
            if rx.recv_timeout(timeout).is_err() {
                fired2.store(true, std::sync::atomic::Ordering::SeqCst);
                rolidium_kernel::request_cancel();
            }
        });
        let r = f(&mut self.runner);
        let _ = tx.send(());
        let _ = watchdog.join();
        let timed_out = fired.load(std::sync::atomic::Ordering::SeqCst);
        rolidium_kernel::reset_cancel();
        (r, timed_out)
    }

    fn error_text(&self, msg: &str) -> String {
        match advice(msg) {
            Some(a) if !msg.contains(a) => format!("{msg}\nWhat to try: {a}"),
            _ => msg.to_string(),
        }
    }

    /// Carries out one tool call.
    pub fn call_tool(&mut self, tool: &str, args: &Value) -> Result<ToolOutput, String> {
        let cmd = spec::find_by_tool(tool).ok_or_else(|| format!("Unknown tool: {tool}"))?;
        if cmd.exec == Exec::Special {
            return Ok(self.render_tool(cmd, args));
        }
        let mut path_error: Option<String> = None;
        let exts = Self::extensions(cmd);
        let line = {
            let this = &*self;
            let mut resolve = |p: &str| match this.resolve_path(p, exts) {
                Ok(r) => Ok(r),
                Err(e) => {
                    path_error = Some(e.clone());
                    Err(e)
                }
            };
            spec::to_line(cmd, args, &mut resolve)
        };
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                return Ok(ToolOutput {
                    text: format!("{e}\nUsage: {}", spec::usage(cmd)),
                    image: None,
                    is_error: true,
                })
            }
        };
        if cmd.name == "open" || cmd.name == "import-step" {
            if let Some(p) = args.get("path").and_then(Value::as_str) {
                if let Ok(full) = self.resolve_path(p, exts) {
                    match std::fs::metadata(&full) {
                        Ok(m) if m.len() > MAX_FILE_BYTES => {
                            return Ok(ToolOutput {
                                text: format!(
                                    "The file is larger than {} MB and is not opened.",
                                    MAX_FILE_BYTES / 1024 / 1024
                                ),
                                image: None,
                                is_error: true,
                            })
                        }
                        Err(e) => {
                            return Ok(ToolOutput {
                                text: format!("Cannot read the file: {e}"),
                                image: None,
                                is_error: true,
                            })
                        }
                        _ => {}
                    }
                }
            }
        }
        let before = self.runner.log.len();
        let (result, timed_out) = self.with_limit(|r| r.run_line(&line));
        let new_lines: Vec<String> = self.runner.log[before.min(self.runner.log.len())..].to_vec();
        match result {
            Ok(()) => {
                let mut text = new_lines.join("\n");
                if cmd.changes {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&self.summary());
                }
                Ok(ToolOutput {
                    text,
                    image: None,
                    is_error: false,
                })
            }
            Err(e) => {
                let msg = if timed_out || e.to_lowercase().contains("cancelled") {
                    format!(
                        "The operation took longer than the time limit ({:.1} s) and was cancelled; the model is unchanged. \
Try a smaller or simpler operation (fewer turns, a coarser shape) or start the server with a longer --timeout.",
                        self.timeout.as_secs_f64()
                    )
                } else {
                    self.error_text(&e)
                };
                Ok(ToolOutput {
                    text: msg,
                    image: None,
                    is_error: true,
                })
            }
        }
    }

    fn render_tool(&mut self, cmd: &Cmd, args: &Value) -> ToolOutput {
        let fail = |m: String| ToolOutput {
            text: m,
            image: None,
            is_error: true,
        };
        let empty = Map::new();
        let obj = args.as_object().unwrap_or(&empty);
        for k in obj.keys() {
            if !cmd.params.iter().any(|p| p.name == k) {
                return fail(format!("unknown argument '{k}' for render_view"));
            }
        }
        let num = |k: &str| obj.get(k).and_then(Value::as_f64);
        let (w, h) = (num("width").unwrap_or(800.0), num("height").unwrap_or(600.0));
        if !(64.0..=2048.0).contains(&w) || !(64.0..=2048.0).contains(&h) {
            return fail("width and height must be between 64 and 2048 pixels".into());
        }
        let ids = |k: &str| -> Vec<u32> {
            obj.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_u64()).map(|v| v as u32).collect())
                .unwrap_or_default()
        };
        let (hf, he) = (ids("highlight_faces"), ids("highlight_edges"));
        let body_n = num("body").map_or(1, |n| n as usize);
        let view = obj.get("view").and_then(Value::as_str).unwrap_or("iso");
        let doc = &self.runner.doc;
        let mut meshes = Vec::new();
        for b in doc.bodies().iter().filter(|b| b.visible) {
            match b.solid.display_mesh() {
                Ok(m) => meshes.push((m, b.color)),
                Err(e) => return fail(format!("cannot draw body '{}': {e}", b.name)),
            }
        }
        let visible_numbers: Vec<usize> = doc
            .bodies()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.visible)
            .map(|(i, _)| i + 1)
            .collect();
        let mut scene = soft::SoftScene::default();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for (m, _) in &meshes {
            for p in &m.positions {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        for s in doc.sketches().iter().filter(|s| s.visible) {
            let l = rolidium_render::sketch_lines(&s.plane, &s.sketch);
            for pair in l.as_chunks::<2>().0 {
                lo = lo.min(pair[0]).min(pair[1]);
                hi = hi.max(pair[0]).max(pair[1]);
                scene.lines.push((vec![pair[0], pair[1]], [0.95, 0.75, 0.1]));
            }
        }
        for c in doc.curves().iter().filter(|c| c.visible) {
            if let Ok(path) = c.def.path() {
                if let Ok(pts) = path.polyline(0.2) {
                    let pts: Vec<Vec3> = pts.iter().map(|p| p.as_vec3()).collect();
                    for p in &pts {
                        lo = lo.min(*p);
                        hi = hi.max(*p);
                    }
                    scene.lines.push((pts, [0.55, 0.25, 0.85]));
                }
            }
        }
        if lo.x > hi.x {
            lo = Vec3::splat(-50.0);
            hi = Vec3::splat(50.0);
        }
        let mut camera = Camera::default();
        match view {
            "custom" => {
                camera.yaw =
                    -std::f32::consts::FRAC_PI_2 + (num("azimuth").unwrap_or(30.0) as f32).to_radians();
                camera.pitch = (num("elevation").unwrap_or(25.0) as f32).to_radians();
            }
            v => camera.set_view(match v {
                "top" => StandardView::Top,
                "bottom" => StandardView::Bottom,
                "front" => StandardView::Front,
                "back" => StandardView::Back,
                "left" => StandardView::Left,
                "right" => StandardView::Right,
                _ => StandardView::Isometric,
            }),
        }
        camera.fit(lo, hi);
        let mut notes = Vec::new();
        let mut labels_f: Vec<(u32, glam::DVec3)> = Vec::new();
        let mut labels_e: Vec<(u32, glam::DVec3)> = Vec::new();
        if !hf.is_empty() || !he.is_empty() {
            match doc.bodies().get(body_n.wrapping_sub(1)) {
                Some(b) => match describe::label_points(&b.solid, &hf, &he) {
                    Ok((f, e)) => {
                        labels_f = f;
                        labels_e = e;
                    }
                    Err(e) => return fail(e),
                },
                None => return fail(format!("there is no body {body_n} to highlight in")),
            }
        }
        for (id, p) in &labels_f {
            scene.labels.push((p.as_vec3(), format!("F{id}")));
        }
        for (id, p) in &labels_e {
            scene.labels.push((p.as_vec3(), format!("E{id}")));
        }
        let (found_f, found_e) = (labels_f.len(), labels_e.len());
        if found_f < hf.len() || found_e < he.len() {
            notes.push("Some of the ids to highlight do not exist in that body.".to_string());
        }
        let mut meshes_iter = meshes.iter();
        for n in &visible_numbers {
            let (m, color) = meshes_iter.next().unwrap();
            let on = *n == body_n;
            scene.bodies.push(soft::SoftBody {
                mesh: m,
                color: *color,
                highlight_faces: if on {
                    hf.iter().copied().collect()
                } else {
                    Default::default()
                },
                highlight_edges: if on {
                    he.iter().copied().collect()
                } else {
                    Default::default()
                },
            });
        }
        let rgba = soft::render(&scene, &camera, w as usize, h as usize);
        let png = match rolidium_render::encode_png(&rgba, w as u32, h as u32) {
            Ok(p) => p,
            Err(e) => return fail(format!("cannot encode the picture: {e}")),
        };
        if let Some(path) = obj.get("path").and_then(Value::as_str) {
            match self.resolve_path(path, &["png"]) {
                Ok(full) => {
                    if let Err(e) = std::fs::write(&full, &png) {
                        return fail(format!("cannot write the picture: {e}"));
                    }
                    notes.push(format!("Saved to {full}."));
                }
                Err(e) => return fail(e),
            }
        }
        let mut text = format!(
            "{view} view, {w}x{h}, {} bodies, {} sketch shapes drawn. Axes: X red, Y green, Z blue.",
            scene.bodies.len(),
            scene.lines.len()
        );
        for n in notes {
            text.push(' ');
            text.push_str(&n);
        }
        ToolOutput {
            text,
            image: Some(png),
            is_error: false,
        }
    }

    fn tool_list(&self) -> Value {
        let tools: Vec<Value> = spec::COMMANDS
            .iter()
            .map(|c| {
                json!({
                    "name": c.tool,
                    "title": c.tool.replace('_', " "),
                    "description": format!("{} (script command: {})", c.summary, spec::usage(c)),
                    "inputSchema": spec::input_schema(c),
                    "annotations": {
                        "title": c.tool.replace('_', " "),
                        "readOnlyHint": !c.changes && !c.files,
                        "destructiveHint": c.changes,
                        "idempotentHint": false,
                        "openWorldHint": false
                    }
                })
            })
            .collect();
        json!({"tools": tools})
    }

    /// Answers one message line. `None`: nothing to send (a notification).
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(rpc_error(&Value::Null, -32700, &format!("Parse error: {e}"), None).to_string())
            }
        };
        if v.is_array() {
            return Some(
                rpc_error(
                    &Value::Null,
                    -32600,
                    "Batches are not supported; send one message per line",
                    None,
                )
                .to_string(),
            );
        }
        let id = v.get("id").cloned();
        let method = match v.get("method").and_then(Value::as_str) {
            Some(m) => m,
            // a response from the client or garbage: nothing to answer
            None => {
                return id.map(|id| rpc_error(&id, -32600, "Invalid request: no method", None).to_string());
            }
        };
        let params = v.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            // notifications (initialized, cancelled, ...) need no answer
            return None;
        };
        // 2026 revision: the version is carried by every request
        let requested = params
            .get("_meta")
            .and_then(|m| m.get(META_VERSION))
            .and_then(Value::as_str)
            .map(String::from);
        if let Some(r) = &requested {
            if !SUPPORTED_VERSIONS.contains(&r.as_str()) {
                return Some(
                    rpc_error(
                        &id,
                        -32022,
                        "Unsupported protocol version",
                        Some(json!({"supported": SUPPORTED_VERSIONS, "requested": r})),
                    )
                    .to_string(),
                );
            }
        }
        let modern = requested.as_deref() == Some("2026-07-28");
        let result: Result<Value, Value> = match method {
            "initialize" => {
                let asked = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let version = if SUPPORTED_VERSIONS.contains(&asked) {
                    asked
                } else {
                    "2025-11-25"
                };
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "rolidium-mcp", "title": "Rolidium", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS
                }))
            }
            "server/discover" => Ok(json!({
                "supportedVersions": SUPPORTED_VERSIONS,
                "capabilities": {"tools": {}},
                "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "rolidium-mcp", "version": env!("CARGO_PKG_VERSION")}},
                "instructions": INSTRUCTIONS,
                "ttlMs": 3600000,
                "cacheScope": "public"
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.tool_list()),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                match self.call_tool(name, &args) {
                    Err(m) => Err(rpc_error(&id, -32602, &m, None)),
                    Ok(out) => {
                        let mut content = vec![json!({"type": "text", "text": out.text})];
                        if let Some(png) = out.image {
                            content.push(
                                json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}),
                            );
                        }
                        let mut r = json!({"content": content});
                        if out.is_error {
                            r["isError"] = json!(true);
                        }
                        Ok(r)
                    }
                }
            }
            other => Err(rpc_error(
                &id,
                -32601,
                &format!("Method not found: {other}"),
                None,
            )),
        };
        Some(
            match result {
                Ok(mut r) => {
                    if modern {
                        r["resultType"] = json!("complete");
                    }
                    json!({"jsonrpc": "2.0", "id": id, "result": r})
                }
                Err(e) => e,
            }
            .to_string(),
        )
    }

    /// Reads messages line by line until the input ends and writes the answers.
    pub fn serve<R: BufRead, W: Write>(&mut self, mut input: R, mut output: W) -> std::io::Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = std::io::Read::take(&mut input, MAX_LINE as u64 + 1).read_line(&mut line)?;
            if n == 0 {
                return Ok(());
            }
            if line.len() > MAX_LINE {
                writeln!(
                    output,
                    "{}",
                    rpc_error(&Value::Null, -32600, "The message is too large", None)
                )?;
                output.flush()?;
                // skip the rest of the oversized line
                loop {
                    let mut rest = String::new();
                    let k = std::io::Read::take(&mut input, 1 << 20).read_line(&mut rest)?;
                    if k == 0 || rest.ends_with('\n') {
                        break;
                    }
                }
                continue;
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            if let Some(answer) = self.handle(text) {
                writeln!(output, "{answer}")?;
                output.flush()?;
            }
        }
    }
}
