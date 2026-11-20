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
        }
    }
}

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
            32 | 34 => {
                // the layout of the left column in a small and in a large window
                let (w, h) = if st.step == 32 {
                    (1280.0, 720.0)
                } else {
                    (1920.0, 1080.0)
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(w, h)));
                st.wait_frames = 12;
            }
            33 | 35 => {
                let screen = ctx.content_rect();
                let name = format!("left-column-{}x{}", screen.width() as i32, screen.height() as i32);
                // the window manager may not give exactly the size: the check is about what is shown
                let fits = !self.left_scrolls
                    && self.left_bottom_rect.max.y <= self.status_top + 0.5
                    && self.status_top <= screen.max.y;
                println!(
                    "selftest: left column in {}x{}: scrolls {}, Settings row bottom {:.0}, status bar top {:.0}",
                    screen.width(),
                    screen.height(),
                    self.left_scrolls,
                    self.left_bottom_rect.max.y,
                    self.status_top
                );
                check(&mut st, &format!("the whole left column fits ({name})"), fits);
                shot(&mut st, ctx, &name);
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
                check(&mut st, "a new document starts in a sketch on XY with the Circle tool", started);
                check(&mut st, "the guide is running in a new document", self.guide.active);
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
                check(&mut st, "an opened document does not start the guide", !self.guide.active);
                st.wait_frames = 2;
            }
            39 => {
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
