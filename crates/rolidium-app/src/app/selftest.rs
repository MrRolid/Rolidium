// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Automatic smoke test of the main tools (`rolidium --selftest <dir>`): drives the same code paths the
//! mouse and keyboard use (select, Pull, sketching, Pull from a region, undo/redo, failing operations, export).

use super::*;
use crate::sketchmode::{Snap, SnapLabel};

pub struct SelfTest {
    dir: PathBuf,
    step: u32,
    wait_frames: u32,
    shot_pending: Option<String>,
    slot: Arc<std::sync::Mutex<Option<Arc<egui::ColorImage>>>>,
    failures: Vec<String>,
    /// Which window size / scale of the left column layout check is running.
    layout_i: usize,
}

impl SelfTest {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        SelfTest {
            dir,
            step: 0,
            wait_frames: 0,
            shot_pending: None,
            slot: Default::default(),
            failures: Vec::new(),
            layout_i: 0,
        }
    }
}

/// Window sizes (in physical pixels) and scales of the left column layout check.
const LAYOUTS: [(f32, f32, f32); 5] = [
    (1280.0, 720.0, 1.0),
    (1920.0, 1080.0, 1.0),
    (1280.0, 600.0, 1.0),
    (1920.0, 1080.0, 1.25),
    (1920.0, 1080.0, 1.5),
];

impl App {
    fn body_volume(&self) -> f64 {
        self.doc
            .bodies()
            .first()
            .and_then(|b| b.solid.volume().ok())
            .unwrap_or(f64::NAN)
    }

    fn top_face(&self) -> Option<(BodyId, u32)> {
        let m = self.meshes.first()?;
        let i = m
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.kind == FaceKind::Plane && f.normal.z > 0.99)
            .max_by(|a, b| a.1.origin.z.partial_cmp(&b.1.origin.z).unwrap())?
            .0;
        Some((m.id, i as u32))
    }

    pub(super) fn selftest_tick(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.selftest.take() else {
            return;
        };
        let shots: Vec<_> = st
            .slot
            .lock()
            .ok()
            .and_then(|mut s| s.take())
            .into_iter()
            .collect();
        for img in shots {
            if let Some(name) = st.shot_pending.take() {
                let path = st.dir.join(format!("{name}.png"));
                let [w, h] = img.size;
                let mut rgba = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    rgba.extend_from_slice(&p.to_array());
                }
                match rolidium_render::encode_png(&rgba, w as u32, h as u32) {
                    Ok(png) => {
                        let _ = std::fs::write(&path, png);
                        println!("selftest: wrote {}", path.display());
                    }
                    Err(e) => st.failures.push(format!("png: {e}")),
                }
            }
        }
        ctx.request_repaint();
        // the last steps start from an empty document
        if !self.idle() || st.shot_pending.is_some() || (self.meshes.is_empty() && st.step < 36) {
            self.selftest = Some(st);
            return;
        }
        if st.wait_frames > 0 {
            st.wait_frames -= 1;
            self.selftest = Some(st);
            return;
        }
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let check = |st: &mut SelfTest, name: &str, ok: bool| {
            println!("selftest: {} {name}", if ok { "PASS" } else { "FAIL" });
            if !ok {
                st.failures.push(name.to_string());
            }
        };
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        let shot = |st: &mut SelfTest, ctx: &egui::Context, name: &str| {
            st.shot_pending = Some(name.to_string());
            let slot = st.slot.clone();
            let c2 = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                move |img| {
                    if let Ok(mut s) = slot.lock() {
                        *s = Some(img);
                    }
                    c2.request_repaint();
                },
            )));
            st.wait_frames = 3;
        };
        let pi = std::f64::consts::PI;
        match st.step {
            0 => {
                check(
                    &mut st,
                    "box exists with volume 24000",
                    near(self.body_volume(), 24000.0),
                );
                check(
                    &mut st,
                    "box has 6 faces",
                    self.meshes.first().is_some_and(|m| m.faces.len() == 6),
                );
                // the graphics layer draws every part of the scene: grid, axes, a sketch region and a body
                match rolidium_render::render_test_scene(
                    &self.render_state.device,
                    &self.render_state.queue,
                    [640, 480],
                ) {
                    Ok((rgba, scene)) => {
                        println!("selftest: offscreen scene: {}", scene.describe());
                        check(
                            &mut st,
                            "offscreen scene has grid, axes, sketch and body",
                            scene.ok(),
                        );
                        if let Ok(png) = rolidium_render::encode_png(&rgba, 640, 480) {
                            let _ = std::fs::write(st.dir.join("0-offscreen-scene.png"), png);
                        }
                    }
                    Err(e) => check(&mut st, &format!("offscreen scene renders ({e})"), false),
                }
                // every graphics backend in a child process of its own with a time limit: a crash of a driver is a result, not the end
                for o in crate::diagnose::run_isolated() {
                    println!("selftest: backend {}: {}", o.backend, o.describe_ending());
                    for ad in &o.adapters {
                        match &ad.result {
                            Ok(t) => println!("selftest:   {}: {t}", ad.label),
                            Err(e) => println!("selftest:   {}: FAILED: {e}", ad.label),
                        }
                    }
                }
                self.tool = Tool::Select;
                self.selection = self.top_face().into_iter().collect();
                check(&mut st, "top face selectable", self.selection.len() == 1);
                shot(&mut st, ctx, "1-selected");
            }
            1 => {
                self.tool = Tool::Pull;
                let (body, face) = self.selection[0];
                self.begin_pull(PullKind::Face { body, face }, Pos2::new(400.0, 300.0), rect);
                if let Some(p) = &mut self.pull {
                    p.d_mouse = 10.0;
                }
                self.update_pull_preview();
                check(
                    &mut st,
                    "pull preview built",
                    self.preview.as_ref().is_some_and(|p| !p.triangles.is_empty()),
                );
                check(&mut st, "pull preview under 16 ms", self.preview_ms < 16.0);
                shot(&mut st, ctx, "2-pull-preview");
            }
            2 => {
                self.commit_pull();
                st.wait_frames = 2;
            }
            3 => {
                check(
                    &mut st,
                    "pull +10 gives volume 36000",
                    near(self.body_volume(), 36000.0),
                );
                check(&mut st, "new top face stays selected", self.selection.len() == 1);
                shot(&mut st, ctx, "3-after-pull");
            }
            4 => {
                self.typed_char('-', rect);
                self.typed_char('5', rect);
                check(
                    &mut st,
                    "typed distance is -5",
                    self.pull
                        .as_ref()
                        .is_some_and(|p| (p.distance() + 5.0).abs() < 1e-6),
                );
                shot(&mut st, ctx, "4-numeric-inward");
            }
            5 => {
                self.commit_pull();
                st.wait_frames = 2;
            }
            6 => {
                check(
                    &mut st,
                    "pull -5 gives volume 30000",
                    near(self.body_volume(), 30000.0),
                );
                self.undo();
                st.wait_frames = 2;
            }
            7 => {
                check(
                    &mut st,
                    "undo gives volume 36000",
                    near(self.body_volume(), 36000.0),
                );
                self.undo();
                st.wait_frames = 2;
            }
            8 => {
                check(
                    &mut st,
                    "second undo gives volume 24000",
                    near(self.body_volume(), 24000.0),
                );
                self.redo();
                st.wait_frames = 2;
            }
            9 => {
                check(
                    &mut st,
                    "redo gives volume 36000",
                    near(self.body_volume(), 36000.0),
                );
                if let Some((b, f)) = self.top_face() {
                    self.submit(Command::PullFace {
                        body: b,
                        face: FaceId(f),
                        distance: -500.0,
                        mode: PullMode::Auto,
                    });
                }
                st.wait_frames = 2;
            }
            10 => {
                check(&mut st, "impossible pull reports an error", self.status.error);
                check(
                    &mut st,
                    "impossible pull leaves the model unchanged",
                    near(self.body_volume(), 36000.0),
                );
                // sketch on the top face
                self.tool = Tool::Select;
                self.selection = self.top_face().into_iter().collect();
                self.enter_sketch();
                check(
                    &mut st,
                    "sketch mode on the top face",
                    self.sk
                        .as_ref()
                        .is_some_and(|s| s.body.is_some() && (s.plane.origin.z - 30.0).abs() < 1e-9),
                );
                check(
                    &mut st,
                    "face outline known for snapping",
                    self.sk
                        .as_ref()
                        .is_some_and(|s| s.outline.len() == 4 && s.corners.len() == 4),
                );
                let sm = self.sk.as_mut().unwrap();
                sm.set_tool(SkTool::Circle);
                sm.begin(DVec2::new(20.0, 15.0));
                for c in "10".chars() {
                    sm.type_char(c);
                }
                sm.snap = Some(Snap {
                    pos: DVec2::new(25.0, 15.0),
                    label: SnapLabel::Free,
                    horizontal: false,
                    vertical: false,
                });
                self.finish_drawing();
                check(
                    &mut st,
                    "circle with typed diameter creates a sketch",
                    self.doc.sketches().len() == 1,
                );
                self.regions_rev = u64::MAX;
                self.refresh_regions();
                let a = self
                    .regions
                    .first()
                    .and_then(|r| r.regions.first())
                    .map(|r| r.area())
                    .unwrap_or(0.0);
                check(&mut st, "one region with area pi*25", near(a, pi * 25.0));
                self.view_normal_to_sketch();
                shot(&mut st, ctx, "5-sketch");
            }
            11 => {
                self.leave_sketch();
                self.tool = Tool::Pull;
                let sid = self.doc.sketches()[0].id;
                self.begin_pull(
                    PullKind::Region { sketch: sid, idx: 0 },
                    Pos2::new(400.0, 300.0),
                    rect,
                );
                self.typed_char('-', rect);
                self.typed_char('3', rect);
                self.typed_char('0', rect);
                check(
                    &mut st,
                    "region pull preview built",
                    self.preview.as_ref().is_some_and(|p| !p.triangles.is_empty()),
                );
                shot(&mut st, ctx, "6-region-preview");
            }
            12 => {
                self.commit_pull();
                st.wait_frames = 2;
            }
            13 => {
                check(
                    &mut st,
                    "region pull through the body gives a through hole",
                    near(self.body_volume(), 36000.0 - pi * 25.0 * 30.0),
                );
                check(&mut st, "used-up sketch is gone", self.doc.sketches().is_empty());
                println!(
                    "selftest: pull from region took {:.1} ms (incl. meshing)",
                    self.last_op_ms.unwrap_or(f32::NAN)
                );
                shot(&mut st, ctx, "7-through-hole");
            }
            14 => {
                self.undo();
                st.wait_frames = 2;
            }
            15 => {
                check(
                    &mut st,
                    "undo brings back the sketch",
                    self.doc.sketches().len() == 1,
                );
                check(
                    &mut st,
                    "undo restores the solid",
                    near(self.body_volume(), 36000.0),
                );
                // dimension edit of a sketch shape, then delete it
                let sk = self.doc.sketches()[0].id;
                let (eid, old) = {
                    let (i, e) = &self.doc.sketches()[0].sketch.entities()[0];
                    (*i, *e)
                };
                self.apply_entity(sk, eid, old, &["12".to_string()]);
                let r = self
                    .doc
                    .sketch(sk)
                    .and_then(|s| s.sketch.regions().first().map(|r| r.area()))
                    .unwrap_or(0.0);
                check(&mut st, "circle diameter changed to 12", near(r, pi * 36.0));
                self.entity_sel = Some((sk, eid));
                self.delete_selected();
                check(
                    &mut st,
                    "Delete removes the shape and the empty sketch",
                    self.doc.sketches().is_empty(),
                );
                self.undo();
                st.wait_frames = 3;
            }
            16 => {
                check(
                    &mut st,
                    "undo brings the deleted shape back",
                    self.doc.sketches().len() == 1,
                );
                // through hole of diameter 12, then resize it to 8 like the dimension panel does
                let sid = self.doc.sketches()[0].id;
                self.submit(Command::PullRegion {
                    sketch: sid,
                    region: 0,
                    distance: -30.0,
                    mode: PullMode::Auto,
                });
                st.wait_frames = 2;
            }
            17 => {
                check(
                    &mut st,
                    "through hole d12 volume",
                    near(self.body_volume(), 36000.0 - pi * 36.0 * 30.0),
                );
                let (b, cyl) = {
                    let m = self.meshes.first().unwrap();
                    (
                        m.id,
                        m.faces.iter().position(|f| f.kind == FaceKind::Cylinder).unwrap() as u32,
                    )
                };
                check(
                    &mut st,
                    "hole face reports its diameter",
                    self.meshes[0].faces[cyl as usize]
                        .cyl_diameter
                        .is_some_and(|d| (d - 12.0).abs() < 1e-6),
                );
                self.submit(Command::SetCylinderDiameter {
                    body: b,
                    face: FaceId(cyl),
                    diameter: 8.0,
                });
                st.wait_frames = 2;
            }
            18 => {
                check(
                    &mut st,
                    "hole resized to d8",
                    near(self.body_volume(), 36000.0 - pi * 16.0 * 30.0),
                );
                // fillet preview by dragging an edge: the real result is computed in the background
                let m = &self.meshes[0];
                let edge = m
                    .mesh
                    .edges
                    .iter()
                    .find(|e| {
                        let (a, b) = (e.points[0], *e.points.last().unwrap());
                        (a.x - b.x).abs() < 1e-3
                            && (a.y - b.y).abs() < 1e-3
                            && a.x < 1.0
                            && a.y < 1.0
                            && (a.z - b.z).abs() > 20.0
                    })
                    .map(|e| (m.id, e.edge.0));
                check(&mut st, "a vertical edge exists", edge.is_some());
                if let Some((b, e)) = edge {
                    self.tool = Tool::Pull;
                    self.begin_edge_pull(b, vec![e], Pos2::new(400.0, 300.0), rect);
                    self.typed_char('3', rect);
                }
                st.wait_frames = 25;
            }
            19 => {
                check(
                    &mut st,
                    "edge preview result arrived without blocking",
                    self.body_override.is_some(),
                );
                shot(&mut st, ctx, "8-edge-preview");
            }
            20 => {
                self.commit_pull();
                st.wait_frames = 2;
            }
            21 => {
                let expect = 36000.0 - pi * 16.0 * 30.0 - (9.0 - pi * 9.0 / 4.0) * 30.0;
                check(
                    &mut st,
                    "round R3 applied to the edge",
                    near(self.body_volume(), expect),
                );
                check(&mut st, "preview override is gone", self.body_override.is_none());
                shot(&mut st, ctx, "9-rounded");
            }
            22 => {
                // click the finished round: the panel offers its radius, the value can be changed
                let (b, f) = {
                    let bd = &self.doc.bodies()[0];
                    (bd.id, bd.features.first().map(|f| f.faces[0].0))
                };
                check(&mut st, "the round is recognised as a feature", f.is_some());
                if let Some(f) = f {
                    self.selection = vec![(b, f)];
                    check(
                        &mut st,
                        "panel offers the radius of the round",
                        self.panel_rows().iter().any(|r| r == &tr("Radius")),
                    );
                    self.submit(Command::EditBlend {
                        body: b,
                        face: FaceId(f),
                        size: 4.0,
                    });
                }
                st.wait_frames = 2;
            }
            23 => {
                check(
                    &mut st,
                    "round changed from R3 to R4",
                    near(
                        self.body_volume(),
                        36000.0 - pi * 16.0 * 30.0 - (16.0 - pi * 4.0) * 30.0,
                    ),
                );
                let (b, f) = {
                    let bd = &self.doc.bodies()[0];
                    (bd.id, bd.features[0].faces[0].0)
                };
                self.submit(Command::EditBlend {
                    body: b,
                    face: FaceId(f),
                    size: 0.0,
                });
                st.wait_frames = 2;
            }
            24 => {
                check(
                    &mut st,
                    "size 0 restores the sharp edge",
                    near(self.body_volume(), 36000.0 - pi * 16.0 * 30.0)
                        && self.doc.bodies()[0].features.is_empty(),
                );
                // flat face: panel with Move by and the distance to the opposite face, forced Cut mode
                self.selection = self.top_face().into_iter().collect();
                let rows = self.panel_rows();
                check(
                    &mut st,
                    "panel offers Move by and the opposite face",
                    rows.iter().any(|r| r == &tr("Move by"))
                        && rows.iter().any(|r| r == &tr("Distance to opposite face")),
                );
                let (b, f) = self.selection[0];
                let meta = self.entry(b).unwrap().faces[f as usize];
                self.submit_pull(
                    PullKind::Face { body: b, face: f },
                    meta.origin,
                    meta.normal.as_vec3(),
                    5.0,
                    PullMode::Cut,
                );
                st.wait_frames = 2;
            }
            25 => {
                check(
                    &mut st,
                    "Cut mode removes material although the distance is positive",
                    near(
                        self.body_volume(),
                        36000.0 - pi * 16.0 * 30.0 - (1200.0 - pi * 16.0) * 5.0,
                    ),
                );
                // up to a point 7 mm above the top face
                let (b, f) = self.top_face().unwrap();
                let meta = self.entry(b).unwrap().faces[f as usize];
                let d = snap::up_to_distance(
                    meta.origin,
                    meta.normal,
                    UpTo::Point(meta.origin + DVec3::new(3.0, 3.0, 7.0)),
                );
                check(&mut st, "up-to distance is 7", near(d, 7.0));
                self.submit_pull(
                    PullKind::Face { body: b, face: f },
                    meta.origin,
                    meta.normal.as_vec3(),
                    d,
                    PullMode::Auto,
                );
                st.wait_frames = 2;
            }
            26 => {
                check(
                    &mut st,
                    "pulled up to the target",
                    near(
                        self.body_volume(),
                        36000.0 - pi * 16.0 * 30.0 - (1200.0 - pi * 16.0) * 5.0 + (1200.0 - pi * 16.0) * 7.0,
                    ),
                );
                shot(&mut st, ctx, "10-up-to");
            }
            27 => {
                // a second body: its faces and vertices become snap targets for the first one
                self.submit(Command::AddBox {
                    name: String::new(),
                    min: DVec3::new(50.0, 0.0, 0.0),
                    max: DVec3::new(60.0, 10.0, 40.0),
                });
                st.wait_frames = 2;
            }
            28 => {
                let first = self.doc.bodies()[0].id;
                let (b, f) = self.top_face().unwrap();
                check(&mut st, "top face belongs to the first body", b == first);
                let meta = self.entry(b).unwrap().faces[f as usize];
                let cands = snap::candidates(&self.meshes, meta.origin, meta.normal);
                let want = 40.0 - meta.origin.z;
                check(
                    &mut st,
                    "the top face of the other body is a snap target",
                    cands.iter().any(|c| {
                        (c.dist - want).abs() < 1e-6
                            && matches!(c.target, SnapTarget::Face { body, .. } if body != first)
                    }),
                );
                check(
                    &mut st,
                    "vertices of the other body are snap targets",
                    cands.iter().any(|c| matches!(c.target, SnapTarget::Vertex(_))),
                );
                let (d, t) = snap::snap_distance(want - 0.3, &cands, 10.0, 1.0, true, false);
                check(
                    &mut st,
                    "dragging near it snaps onto its plane",
                    (d - want).abs() < 1e-6 && t.is_some(),
                );
                self.undo();
                st.wait_frames = 3;
            }
            30 => {
                // an impossible radius changes nothing and says why
                let rev = self.doc.revision();
                let m = &self.meshes[0];
                let e = m.mesh.edges[0].edge.0;
                let b = m.id;
                self.submit(Command::FilletEdges {
                    body: b,
                    edges: vec![e],
                    size: 500.0,
                    chamfer: false,
                });
                st.wait_frames = 2;
                st.step += 1;
                self.selftest = Some(st);
                let _ = rev;
                return;
            }
            31 => {
                check(&mut st, "too large radius reports an error", self.status.error);
                self.export_to(
                    PathBuf::from("rolidium-selftest-export.stl"),
                    StlParams {
                        deviation: 0.05,
                        angle_deg: 15.0,
                        ascii: true,
                    },
                );
                st.wait_frames = 2;
            }
            32 => {
                // the layout of the left column in windows of several sizes and scales
                let (w, h, scale) = LAYOUTS[st.layout_i];
                ctx.set_pixels_per_point(scale);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(w, h)));
                st.wait_frames = 12;
            }
            33 => {
                let (_, _, scale) = LAYOUTS[st.layout_i];
                let screen = ctx.content_rect();
                let name = if scale == 1.0 {
                    format!("left-column-{}x{}", screen.width() as i32, screen.height() as i32)
                } else {
                    format!(
                        "left-column-{}x{}-scale-{}",
                        screen.width() as i32,
                        screen.height() as i32,
                        scale
                    )
                };
                // the window manager may not give exactly the size: the check is about what is shown
                let bottom_visible = self.left_bottom_rect.max.y <= self.status_top + 0.5
                    && self.status_top <= screen.max.y
                    && self.left_file_rect.min.y >= 0.0
                    && self.left_file_rect.max.y <= self.left_bottom_rect.min.y + 1.0;
                // from 720 points high nothing has to scroll
                let fits = bottom_visible && (screen.height() < 700.0 || !self.left_scrolls);
                println!(
                    "selftest: left column in {}x{} (scale {}): scrolls {}, File block {:.0}..{:.0}, Settings row bottom {:.0}, status bar top {:.0}",
                    screen.width(),
                    screen.height(),
                    scale,
                    self.left_scrolls,
                    self.left_file_rect.min.y,
                    self.left_file_rect.max.y,
                    self.left_bottom_rect.max.y,
                    self.status_top
                );
                check(
                    &mut st,
                    &format!(
                        "the left column fits or scrolls with the File block and Settings visible ({name})"
                    ),
                    fits,
                );
                shot(&mut st, ctx, &name);
                if st.layout_i + 1 < LAYOUTS.len() {
                    st.layout_i += 1;
                    st.step = 31; // the increment after this step leads back to the size change
                } else {
                    ctx.set_pixels_per_point(1.0);
                }
            }
            36 => {
                // a new document starts in a sketch on XY with the Circle tool, and the view turns square to the plane
                self.settings.new_doc_sketch = true;
                self.perform(Pending::New);
                st.wait_frames = 3;
            }
            37 => {
                let started = self
                    .sk
                    .as_ref()
                    .is_some_and(|s| s.tool == SkTool::Circle && s.plane.same_plane(&PlaneDef::base()));
                check(
                    &mut st,
                    "a new document starts in a sketch on XY with the Circle tool",
                    started,
                );
                check(
                    &mut st,
                    "the guide is running in a new document",
                    self.guide.active,
                );
                if self.view_anim.is_some() {
                    // the animation (with its pause) is still running: look again next frame
                    self.selftest = Some(st);
                    return;
                }
                let mut top = self.camera;
                top.set_view(StandardView::Top);
                check(
                    &mut st,
                    "after the animation the view is square to the plane",
                    (self.camera.pitch - top.pitch).abs() < 1e-3,
                );
                shot(&mut st, ctx, "first-start-sketch");
            }
            38 => {
                // an opened (here: replaced) document does not start a sketch or the guide
                self.replace_document(Document::new(), None);
                check(&mut st, "an opened document starts no sketch", self.sk.is_none());
                check(
                    &mut st,
                    "an opened document does not start the guide",
                    !self.guide.active,
                );
                st.wait_frames = 2;
            }
            39 => {
                // Fill: the inner circle of a pipe becomes a region, the Pull tool starts with it, a pull down gives a full cylinder
                use rolidium_kernel::{BooleanOp, EdgeKind, Solid};
                let outer = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
                let bore = Solid::cylinder(12.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
                let pipe = outer.boolean(&bore, BooleanOp::Cut).unwrap().solid;
                self.replace_document(Document::new(), None);
                self.settings.guide_enabled = false;
                self.guide.enabled = false;
                let _ = self.doc.apply(Command::AddSolids { solids: vec![pipe] });
                let id = self.doc.bodies()[0].id;
                let s = self.doc.bodies()[0].solid.clone();
                let e = s
                    .edges()
                    .find(|e| {
                        let i = s.edge_info(*e).unwrap();
                        i.kind == EdgeKind::Circle && (i.radius - 12.0).abs() < 1e-6 && i.start.z > 29.0
                    })
                    .unwrap();
                self.edge_sel = vec![(id, e.0)];
                self.request_meshes();
                st.wait_frames = 2;
            }
            40 => {
                if self.meshes.is_empty() {
                    self.selftest = Some(st);
                    return;
                }
                self.activate_fill();
                st.wait_frames = 3;
            }
            41 => {
                check(
                    &mut st,
                    "Fill makes a region of the inner circle and the Pull tool starts with it",
                    self.tool == Tool::Pull && self.pull.is_some() && self.region_sel.is_some(),
                );
                if let Some(p) = &mut self.pull {
                    p.d_mouse = -30.0;
                }
                self.commit_pull();
                st.wait_frames = 3;
            }
            42 => {
                let v = self
                    .doc
                    .bodies()
                    .first()
                    .and_then(|b| b.solid.volume().ok())
                    .unwrap_or(0.0);
                check(
                    &mut st,
                    "pulling the filled circle down gives a full cylinder",
                    self.doc.bodies().len() == 1 && (v - std::f64::consts::PI * 400.0 * 30.0).abs() < 1e-3,
                );
                // Join ends: a square with gaps in the corners
                self.replace_document(Document::new(), None);
                let g = 0.3;
                let v2 = rolidium_doc::rolidium_sketch::DVec2::new;
                let mut sk = None;
                for (a, b) in [
                    (v2(g, 0.0), v2(10.0 - g, 0.0)),
                    (v2(10.0, g), v2(10.0, 10.0 - g)),
                    (v2(10.0 - g, 10.0), v2(g, 10.0)),
                    (v2(0.0, 10.0 - g), v2(0.0, g)),
                ] {
                    let c = self
                        .doc
                        .apply(Command::AddEntity {
                            target: match sk {
                                None => SketchTarget::New {
                                    plane: PlaneDef::base(),
                                    body: None,
                                },
                                Some(id) => SketchTarget::Existing(id),
                            },
                            entity: Entity::Line { a, b },
                        })
                        .unwrap();
                    sk = sk.or(c.created_sketch);
                }
                self.tool = Tool::Select;
                self.activate_join();
                let joined = self.join_plan().map_or(0, |(_, p)| p.joined());
                check(
                    &mut st,
                    "Join ends finds the four gaps of the square",
                    joined == 4,
                );
                self.join_apply();
                self.refresh_regions();
                let regions: usize = self.regions.iter().map(|r| r.regions.len()).sum();
                check(&mut st, "after joining the square has a region", regions == 1);
                st.wait_frames = 2;
            }
            43 => {
                // smooth display: a circle across the whole window deviates by less than half a pixel
                use rolidium_kernel::Solid;
                self.replace_document(Document::new(), None);
                let cyl = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
                let _ = self.doc.apply(Command::AddSolids { solids: vec![cyl] });
                self.request_meshes();
                st.wait_frames = 2;
            }
            44 => {
                if self.meshes.is_empty() {
                    self.selftest = Some(st);
                    return;
                }
                let before = self.meshes[0].mesh.triangle_count();
                self.camera.set_view(StandardView::Top);
                self.camera.target = Vec3::new(0.0, 0.0, 30.0);
                // the circle (radius 20) fills the height of the view
                self.camera.distance = 2.1 * 20.0 / (2.0 * (self.camera.fov_y * 0.5).tan());
                self.remesh_now();
                let px = self.pixel_mm();
                let m = &self.meshes[0].mesh;
                let worst = m
                    .edges
                    .iter()
                    .filter(|e| e.points.len() > 8)
                    .flat_map(|e| {
                        e.points.windows(2).map(|w| {
                            let mid = (w[0] + w[1]) * 0.5;
                            (20.0 - (mid.x * mid.x + mid.y * mid.y).sqrt()).abs()
                        })
                    })
                    .fold(0.0_f32, f32::max);
                println!(
                    "selftest: zoomed circle: {px:.4} mm per pixel, worst deviation {worst:.4} mm, {before} -> {} triangles",
                    m.triangle_count()
                );
                check(
                    &mut st,
                    "a circle across the window deviates by less than half a pixel",
                    f64::from(worst) <= 0.5 * px,
                );
                check(
                    &mut st,
                    "the zoomed mesh is finer than the first one",
                    m.triangle_count() > before,
                );
                self.layers_changed = true;
                st.wait_frames = 3;
                shot(&mut st, ctx, "zoomed-circle");
            }
            45 => {
                // the floating panels of the tools start inside the model area, clear of both side columns
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(1280.0, 720.0)));
                st.layout_i = 0;
                st.wait_frames = 10;
            }
            46 => {
                const PANELS: [(Tool, &str); 5] = [
                    (Tool::Sweep, "sweep"),
                    (Tool::Helix, "helix"),
                    (Tool::Measure, "measure"),
                    (Tool::Loft, "loft"),
                    (Tool::Join, "join"),
                ];
                let (tool, kind) = PANELS[st.layout_i];
                if self.tool != tool {
                    self.tool = tool;
                    st.wait_frames = 6;
                    self.selftest = Some(st);
                    return;
                }
                let vr = self.viewport_rect;
                let inside = self.docks.rect(kind).is_some_and(|r| {
                    r.min.x >= vr.min.x && r.max.x <= vr.max.x && r.min.y >= vr.min.y && r.max.y <= vr.max.y
                });
                println!(
                    "selftest: the {kind} panel {:?} model area {:?}",
                    self.docks.rect(kind),
                    vr
                );
                check(
                    &mut st,
                    &format!("the {kind} panel starts inside the model area"),
                    inside,
                );
                if kind == "sweep" {
                    shot(&mut st, ctx, "tool-panel-1280x720");
                }
                if st.layout_i + 1 < PANELS.len() {
                    st.layout_i += 1;
                    st.step = 45;
                } else {
                    self.tool = Tool::Select;
                }
            }
            47 => {
                // a document of many bodies (a machine): a big plate with a small hole, 300 screws; the zoom must make the hole round
                use rolidium_kernel::{BooleanOp, Solid};
                self.replace_document(Document::new(), None);
                let plate =
                    Solid::cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(1200.0, 800.0, 20.0)).unwrap();
                // a hundred holes of 6 mm in a 10 x 10 pattern: the plate has many curved faces, so a small budget of triangles would starve the zoom
                let mut plate = plate;
                for k in 0..100 {
                    let (x, y) = (100.0 + (k % 10) as f64 * 100.0, 100.0 + (k / 10) as f64 * 65.0);
                    let hole = Solid::cylinder(3.0, 30.0, DVec3::new(x, y, -5.0), DVec3::Z).unwrap();
                    plate = plate.boolean(&hole, BooleanOp::Cut).unwrap().solid;
                }
                let mut solids = vec![(plate, "Table plate".to_string())];
                for i in 0..300 {
                    let (x, y) = (150.0 + (i % 20) as f64 * 50.0, 150.0 + (i / 20) as f64 * 40.0);
                    let shank = Solid::cylinder(2.5, 20.0, DVec3::new(x, y, 20.0), DVec3::Z).unwrap();
                    let head = Solid::cylinder(4.0, 4.0, DVec3::new(x, y, 40.0), DVec3::Z).unwrap();
                    let screw = shank.boolean(&head, BooleanOp::Fuse).unwrap().solid;
                    solids.push((screw, format!("Screw {i}")));
                }
                let _ = self.doc.apply(Command::AddNamedSolids { solids });
                self.request_meshes();
                st.wait_frames = 2;
            }
            48 => {
                if self.meshes.len() < 301 {
                    self.selftest = Some(st);
                    return;
                }
                // look down at the hole in the plate (it fills a third of the height of the view), settle, measure the circle
                self.view_anim = None;
                self.sk = None;
                self.tool = Tool::Select;
                self.camera.set_view(StandardView::Top);
                self.camera.target = Vec3::new(100.0, 100.0, 20.0);
                self.camera.distance = 17.0 / (2.0 * (self.camera.fov_y * 0.5).tan());
                let first = self.meshes[0].mesh.triangle_count();
                self.remesh_now();
                let px = self.pixel_mm();
                let m = &self.meshes[0].mesh;
                let (cx, cy, r) = (100.0_f32, 100.0_f32, 3.0_f32);
                let ring = |p: &Vec3| (((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt() - r).abs() < 0.05;
                let segments: usize = m
                    .edges
                    .iter()
                    .filter(|e| e.points.len() > 3 && e.points.iter().all(ring))
                    .map(|e| e.points.len() - 1)
                    .sum();
                let worst = m
                    .edges
                    .iter()
                    .filter(|e| e.points.len() > 3 && e.points.iter().all(ring))
                    .flat_map(|e| {
                        e.points.windows(2).map(|w| {
                            let mid = (w[0] + w[1]) * 0.5;
                            (r - ((mid.x - cx).powi(2) + (mid.y - cy).powi(2)).sqrt()).abs()
                        })
                    })
                    .fold(0.0_f32, f32::max);
                println!(
                    "selftest: hole in the plate of a document of {} bodies: {px:.4} mm per pixel, {segments} segments, worst deviation {worst:.5} mm, {first} -> {} triangles",
                    self.meshes.len(),
                    m.triangle_count()
                );
                check(
                    &mut st,
                    "the hole in the table plate of a big document deviates by less than half a pixel after the zoom",
                    segments >= 24 && f64::from(worst) <= 0.5 * px,
                );
                // the triangles of the face too: the cylinder wall of the hole, seen from above, is a ring of narrow strips
                let tris = self.meshes[0].mesh.triangle_count();
                check(
                    &mut st,
                    "the plate got finer triangles for the hole",
                    tris > first,
                );
                self.layers_changed = true;
                st.wait_frames = 3;
                shot(&mut st, ctx, "zoomed-hole-in-plate");
            }
            49 => {
                // a screw head in the same document: its outline is a circle too
                self.camera.target = Vec3::new(150.0, 150.0, 44.0);
                self.camera.distance = 12.0 / (2.0 * (self.camera.fov_y * 0.5).tan());
                self.remesh_now();
                let px = self.pixel_mm();
                let m = &self.meshes[1].mesh;
                let (cx, cy, r) = (150.0_f32, 150.0_f32, 4.0_f32);
                let ring = |p: &Vec3| (((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt() - r).abs() < 0.05;
                let worst = m
                    .edges
                    .iter()
                    .filter(|e| e.points.len() > 3 && e.points.iter().all(ring))
                    .flat_map(|e| {
                        e.points.windows(2).map(|w| {
                            let mid = (w[0] + w[1]) * 0.5;
                            (r - ((mid.x - cx).powi(2) + (mid.y - cy).powi(2)).sqrt()).abs()
                        })
                    })
                    .fold(0.0_f32, f32::max);
                println!("selftest: screw head: {px:.4} mm per pixel, worst deviation {worst:.5} mm");
                check(
                    &mut st,
                    "the outline of a screw head deviates by less than half a pixel after the zoom",
                    worst > 0.0 && f64::from(worst) <= 0.5 * px,
                );
            }
            50 => {
                // the list of bodies: the first body is picked in the model, then the last one of the long list (301 rows)
                let first = self.doc.bodies()[0].id;
                self.clear_selection();
                self.selection.push((first, 0));
                self.layers_changed = true;
                st.wait_frames = 6;
            }
            51 => {
                let first = self.doc.bodies()[0].id;
                check(
                    &mut st,
                    "a face of the first body picked in the model marks its row in the list, which is not scrolled",
                    self.tree_probe == (Some(first), true, true) && self.tree_scroll_y < 1.0,
                );
                let last = self.doc.bodies().last().unwrap().id;
                self.clear_selection();
                self.selection.push((last, 0));
                self.layers_changed = true;
                st.wait_frames = 8;
            }
            52 => {
                let last = self.doc.bodies().last().unwrap().id;
                println!(
                    "selftest: list of {} bodies, picked the last: probe {:?}, scrolled {:.0} px",
                    self.doc.bodies().len(),
                    self.tree_probe,
                    self.tree_scroll_y
                );
                check(
                    &mut st,
                    "a body from the end of a long list picked in the model: its row is marked, the list scrolled to it and the row is in view",
                    self.tree_probe == (Some(last), true, true) && self.tree_scroll_y > 100.0,
                );
                shot(&mut st, ctx, "list-follows-selection");
            }
            53 => {
                // a face far from the origin: a circle is drawn on it and the Pull tool is chosen; the view must stay at the sketch
                use rolidium_kernel::Solid;
                self.replace_document(Document::new(), None);
                let b =
                    Solid::cuboid(DVec3::new(5000.0, 3000.0, 0.0), DVec3::new(5100.0, 3100.0, 50.0)).unwrap();
                let _ = self.doc.apply(Command::AddSolids { solids: vec![b] });
                self.request_meshes();
                self.fitted = true;
                self.view_anim = None;
                self.camera = rolidium_render::Camera::default();
                // the view looks at the origin from far away, so that the body is in view
                self.camera.distance = 12000.0;
                st.wait_frames = 3;
            }
            54 => {
                if self.meshes.is_empty() {
                    self.selftest = Some(st);
                    return;
                }
                let Some((body, face)) = self.top_face() else {
                    check(&mut st, "the top face of the far body exists", false);
                    self.selftest = Some(st);
                    return;
                };
                self.selection = vec![(body, face)];
                self.key_pressed(Key::K, egui::Modifiers::NONE, rect);
                st.wait_frames = 30;
            }
            55 => {
                self.view_anim = None;
                let has_sketch = self.sk.is_some();
                let ok = self.add_entity(Entity::Circle {
                    center: DVec2::new(0.0, 0.0),
                    radius: 10.0,
                });
                check(
                    &mut st,
                    "a circle was drawn on the face far from the origin",
                    ok && has_sketch,
                );
                // the Pull tool is chosen (P): the sketch is left
                self.key_pressed(Key::P, egui::Modifiers::NONE, rect);
                st.wait_frames = 40;
            }
            56 => {
                self.view_anim = None;
                let sketch_at = Vec3::new(5050.0, 3050.0, 50.0);
                let from_sketch = (self.camera.target - sketch_at).length();
                println!(
                    "selftest: after leaving the sketch the view looks at {:?}, {from_sketch:.0} mm from the sketch, {:.0} mm from the origin",
                    self.camera.target,
                    self.camera.target.length()
                );
                check(
                    &mut st,
                    "after the circle on a face far from the origin and Pull, the centre of the view stays at the sketch",
                    from_sketch < 200.0 && self.sk.is_none() && self.tool == Tool::Pull,
                );
                st.layout_i = 0;
            }
            57 => {
                // the report panel at the bottom of the right column, in two window sizes, folded and open
                const SIZES: [(f32, f32); 2] = [(1280.0, 720.0), (1280.0, 600.0)];
                let (w, h) = SIZES[st.layout_i.min(1)];
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(w, h)));
                self.feedback.expanded = false;
                self.feedback.save_dir = Some(st.dir.clone());
                st.wait_frames = 12;
            }
            58 => {
                let screen = ctx.content_rect();
                let name = format!(
                    "feedback-{}x{}-folded",
                    screen.width() as i32,
                    screen.height() as i32
                );
                let r = self.feedback.rect;
                println!(
                    "selftest: report panel folded in {}x{}: {:.0} points high",
                    screen.width(),
                    screen.height(),
                    r.height()
                );
                check(
                    &mut st,
                    &format!("the folded report panel takes one row and is above the status bar ({name})"),
                    r.height() > 5.0 && r.height() < 40.0 && r.max.y <= self.status_top + 0.5,
                );
                shot(&mut st, ctx, &name);
                self.feedback.expanded = true;
                self.feedback.kind = rolidium_net::Kind::Bug;
                self.feedback.text = "The hole in the plate looks square.".into();
                self.feedback.contact = "me@example.org".into();
                st.wait_frames = 10;
            }
            59 => {
                let screen = ctx.content_rect();
                let name = format!(
                    "feedback-{}x{}-open",
                    screen.width() as i32,
                    screen.height() as i32
                );
                let (r, col) = (self.feedback.rect, self.feedback.column_h);
                println!(
                    "selftest: report panel open in {}x{}: {:.0} of {:.0} points ({:.0} %)",
                    screen.width(),
                    screen.height(),
                    r.height(),
                    col,
                    100.0 * r.height() / col.max(1.0)
                );
                check(
                    &mut st,
                    &format!("the open report panel takes at most about 40 % of the column and stays above the status bar ({name})"),
                    r.height() > 60.0 && r.height() <= col * 0.4 + 1.0 && r.max.y <= self.status_top + 0.5,
                );
                shot(&mut st, ctx, &name);
            }
            60 => {
                // the text survives a change of the tool and folding the panel
                self.tool = Tool::Pull;
                self.feedback.expanded = false;
                st.wait_frames = 3;
            }
            61 => {
                self.feedback.expanded = true;
                check(
                    &mut st,
                    "the text of the report survives a change of the tool and folding the panel",
                    self.feedback.text == "The hole in the plate looks square.",
                );
                if st.layout_i == 0 {
                    st.layout_i = 1;
                    st.step = 56; // the increment leads to the size change of step 57
                } else {
                    // write a text and save it to a file (without the diagnostics, which would start the graphics tests)
                    self.feedback.kind = rolidium_net::Kind::Idea;
                    self.feedback.text = "A ruler tool, please.".into();
                    self.feedback.attach = false;
                    self.press_save(ctx);
                    st.wait_frames = 4;
                }
            }
            62 => {
                let path = self.feedback.saved.clone().unwrap_or_default();
                let names = std::fs::read(&path)
                    .ok()
                    .and_then(|b| feedback::list_zip(&b).ok())
                    .unwrap_or_default();
                let msg = std::fs::File::open(&path)
                    .ok()
                    .and_then(|f| zip::ZipArchive::new(f).ok())
                    .and_then(|mut z| {
                        let mut f = z.by_name("message.txt").ok()?;
                        let mut t = String::new();
                        std::io::Read::read_to_string(&mut f, &mut t).ok()?;
                        Some(t)
                    })
                    .unwrap_or_default();
                check(
                    &mut st,
                    "the report is saved to a file with the text as a file of its own",
                    names.len() == 1 && msg.contains("A ruler tool, please.") && msg.contains("idea"),
                );
                check(
                    &mut st,
                    "the text stays in the panel after saving",
                    self.feedback.text == "A ruler tool, please." && self.feedback.expanded,
                );
                let _ = std::fs::remove_file(&path);
                self.feedback.expanded = false;
                self.feedback.text.clear();
                self.feedback.attach = true;
                self.feedback.save_dir = None;
                self.feedback.saved = None;
            }
            63 => {
                let txt = std::fs::read_to_string("rolidium-selftest-export.stl").unwrap_or_default();
                check(
                    &mut st,
                    "ASCII STL file written",
                    txt.starts_with("solid") && txt.contains("facet normal"),
                );
                let _ = std::fs::remove_file("rolidium-selftest-export.stl");
                println!("selftest: {} failure(s)", st.failures.len());
                crate::startup::finish();
                std::process::exit(if st.failures.is_empty() { 0 } else { 1 });
            }
            _ => {}
        }
        st.step += 1;
        self.selftest = Some(st);
    }
}
