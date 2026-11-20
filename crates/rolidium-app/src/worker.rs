// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Background thread for all geometry work. The UI thread never waits for the kernel: it sends
//! a [`Work`] item and polls for the [`Done`] answer. Panics inside the kernel are caught here
//! and reported as errors, so the UI always survives and the document is never half-changed.

use crate::logging::log;
use rolidium_doc::file::Loaded;
use rolidium_doc::{BodyId, Computed, DocError, Document, Job, State};
use rolidium_kernel::{DVec3, FaceKind, Mesh, Solid};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// What the UI needs to know about a face without asking the kernel.
#[derive(Debug, Clone, Copy)]
pub struct FaceMeta {
    pub kind: FaceKind,
    pub normal: DVec3,
    pub origin: DVec3,
    pub area: f64,
    pub centroid: glam::Vec3,
    /// Complete hole or pin: its diameter.
    pub cyl_diameter: Option<f64>,
    /// Flat circular face belonging to a hole or pin: (cylindrical face, diameter).
    pub disc: Option<(u32, f64)>,
}

#[derive(Clone)]
pub struct MeshEntry {
    pub id: BodyId,
    #[allow(dead_code)]
    pub name: String,
    pub mesh: Mesh,
    pub faces: Vec<FaceMeta>,
    pub color: [f32; 3],
}

pub type BodyInput = (BodyId, String, Solid, [f32; 3]);

/// A GPU the worker may use to render the preview picture stored in a file.
pub struct GpuHandle {
    pub device: rolidium_render::wgpu::Device,
    pub queue: rolidium_render::wgpu::Queue,
}

pub enum Work {
    Command(Job),
    /// Write the document to a file (also used by autosave). The preview needs `gpu` and `meshes`.
    SaveFile {
        doc: Box<Document>,
        path: PathBuf,
        gpu: Option<GpuHandle>,
        meshes: Arc<Vec<MeshEntry>>,
        autosave: bool,
    },
    OpenFile {
        path: PathBuf,
    },
    ReadStep {
        path: PathBuf,
    },
    ExportStep {
        solids: Vec<Solid>,
        path: PathBuf,
    },
    Meshes {
        bodies: Vec<BodyInput>,
    },
    /// New display meshes for the current zoom (in the background, while the person looks at the model).
    Remesh {
        ticket: u64,
        /// The bodies with the deflection and triangle count of their present meshes.
        bodies: Vec<(BodyInput, Option<(f64, usize)>)>,
        pixel_mm: f64,
        max_triangles: usize,
    },
    ExportStl {
        solids: Vec<Solid>,
        path: PathBuf,
        params: StlParams,
    },
    /// Triangle count for the export dialog.
    MeshStats {
        solids: Vec<Solid>,
        params: StlParams,
        ticket: u64,
    },
    /// Real result of a fillet/chamfer for the live preview while dragging.
    EdgePreview {
        ticket: u64,
        body: BodyInput,
        edges: Vec<u32>,
        size: f64,
        chamfer: bool,
        angle_deg: f64,
        flip: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StlParams {
    /// Largest deviation from the exact shape (mm).
    pub deviation: f64,
    /// Largest angle between neighbouring facets (degrees).
    pub angle_deg: f64,
    pub ascii: bool,
}

pub enum Done {
    /// Saved: the path, the state that was written and whether it was an autosave.
    Saved(Result<(PathBuf, Arc<State>, bool), String>),
    Opened(Result<(PathBuf, Box<Loaded>), String>),
    StepRead(Result<(Vec<Solid>, Vec<String>), String>),
    StepExported(Result<PathBuf, String>),
    /// A background job died with an internal error.
    Failed(String),
    Command(Result<(Computed, Vec<MeshEntry>), String>),
    Meshes(Result<Vec<MeshEntry>, String>),
    Export(Result<(PathBuf, usize), String>),
    MeshStats(u64, Result<usize, String>),
    EdgePreview(u64, Result<MeshEntry, String>),
    Remesh(u64, Result<Vec<MeshEntry>, String>),
}

pub struct Worker {
    tx: Sender<Work>,
    rx: Receiver<(Done, f32)>,
}

fn mesh_entry(b: &BodyInput) -> Result<MeshEntry, String> {
    let mesh = b.2.display_mesh().map_err(|e| e.to_string())?;
    entry_of(b, mesh)
}

/// The entry of a body for a mesh made for the current zoom (`pixel_mm` per pixel), with at most `max_triangles` triangles.
fn mesh_entry_view(
    b: &BodyInput,
    pixel_mm: f64,
    max_triangles: usize,
    previous: Option<(f64, usize)>,
) -> Result<MeshEntry, String> {
    let mesh =
        b.2.view_mesh(pixel_mm, max_triangles, previous)
            .map_err(|e| e.to_string())?;
    entry_of(b, mesh)
}

fn entry_of(b: &BodyInput, mesh: Mesh) -> Result<MeshEntry, String> {
    let (id, name, solid, color) = b;
    let mut sums = vec![(glam::Vec3::ZERO, 0u32); solid.face_count()];
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let f = mesh.tri_face[t] as usize;
        for &i in tri {
            sums[f].0 += mesh.positions[i as usize];
            sums[f].1 += 1;
        }
    }
    let mut faces = Vec::with_capacity(sums.len());
    for (i, f) in solid.faces().enumerate() {
        let info = solid.face_info(f).map_err(|e| e.to_string())?;
        let (sum, n) = sums[i];
        faces.push(FaceMeta {
            kind: info.kind,
            normal: info.normal,
            origin: info.origin,
            area: info.area,
            centroid: if n > 0 { sum / n as f32 } else { glam::Vec3::ZERO },
            cyl_diameter: None,
            disc: None,
        });
    }
    // Hole/pin information for the dimension panel (only cheap candidates are examined).
    for (i, f) in solid.faces().enumerate() {
        match faces[i].kind {
            FaceKind::Cylinder => {
                if let Ok(Some(c)) = solid.cylinder_info(f) {
                    faces[i].cyl_diameter = Some(c.radius * 2.0);
                }
            }
            FaceKind::Plane => {
                if let Ok(Some((cyl, r))) = solid.disc_partner(f) {
                    faces[i].disc = Some((cyl.0, r * 2.0));
                }
            }
            _ => {}
        }
    }
    Ok(MeshEntry {
        id: *id,
        name: name.clone(),
        mesh,
        faces,
        color: *color,
    })
}

fn meshes_of(bodies: &[BodyInput]) -> Result<Vec<MeshEntry>, String> {
    bodies
        .iter()
        .map(mesh_entry)
        .filter(|r| r.as_ref().map_or(true, |m| !m.mesh.indices.is_empty()))
        .collect()
}

/// A small isometric picture of the model for the file (None if the GPU cannot be used).
fn preview_png(gpu: &GpuHandle, meshes: &[MeshEntry]) -> Option<Vec<u8>> {
    use rolidium_render::{BodyDraw, Camera, Frame, Renderer, StandardView};
    if meshes.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for m in meshes {
        for p in &m.mesh.positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    let mut camera = Camera::default();
    camera.set_view(StandardView::Isometric);
    camera.fit(lo, hi);
    let mut r = Renderer::new(&gpu.device, rolidium_render::wgpu::TextureFormat::Rgba8Unorm);
    let draws: Vec<BodyDraw> = meshes
        .iter()
        .map(|m| BodyDraw {
            id: m.id,
            mesh: &m.mesh,
            color: m.color,
        })
        .collect();
    r.set_bodies(&gpu.device, &draws);
    let frame = Frame {
        camera,
        hover: rolidium_render::ID_NONE,
        selected: Vec::new(),
        body_marks: Vec::new(),
        size: [320, 240],
        grid: None,
        render_ids: false,
        grid_levels: None,
        scene_radius: 0.0,
    };
    let rgba = r.render_to_rgba(&gpu.device, &gpu.queue, &frame).ok()?;
    rolidium_render::encode_png(&rgba, 320, 240).ok()
}

fn describe_panic(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

fn handle(work: Work) -> Done {
    // a cancel request belongs to the job that was running; every new job starts clean
    rolidium_kernel::reset_cancel();
    match work {
        Work::Command(job) => {
            // for the GUI tests: pretend the operation takes this long (it still reacts to Cancel)
            if let Some(ms) = std::env::var("ROLIDIUM_TEST_DELAY_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                let t = std::time::Instant::now();
                while t.elapsed() < std::time::Duration::from_millis(ms) {
                    if rolidium_kernel::cancel_requested() {
                        return Done::Command(Err("Cancelled".into()));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            let started = std::time::Instant::now();
            let _ = rolidium_kernel::timing::take();
            let mut run_ms = 0.0;
            let r = job.run().map_err(|e: DocError| e.to_string()).and_then(|c| {
                run_ms = started.elapsed().as_secs_f64() * 1e3;
                let inputs: Vec<BodyInput> = c
                    .state
                    .bodies
                    .iter()
                    .filter(|b| b.visible)
                    .map(|b| (b.id, b.name.clone(), b.solid.clone(), b.color))
                    .collect();
                let meshes = meshes_of(&inputs)?;
                Ok((c, meshes))
            });
            // where the time went (the same stages as `rolidium-cli times`), for the log
            crate::logging::log(&format!(
                "operation: job {run_ms:.0} ms, with display mesh {:.0} ms; {}",
                started.elapsed().as_secs_f64() * 1e3,
                rolidium_kernel::timing::describe(&rolidium_kernel::timing::take())
            ));
            Done::Command(r)
        }
        Work::SaveFile {
            doc,
            path,
            gpu,
            meshes,
            autosave,
        } => Done::Saved((|| {
            let preview = gpu.and_then(|g| preview_png(&g, &meshes));
            let bytes =
                rolidium_doc::file::save_to_bytes(&doc, preview.as_deref(), env!("CARGO_PKG_VERSION"))
                    .map_err(|e| e.to_string())?;
            if autosave {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
            }
            rolidium_doc::file::save_atomic(&path, &bytes).map_err(|e| e.to_string())?;
            Ok((path, doc.state().clone(), autosave))
        })()),
        Work::OpenFile { path } => Done::Opened(
            rolidium_doc::file::load_file(&path)
                .map(|l| (path, Box::new(l)))
                .map_err(|e| e.to_string()),
        ),
        Work::ReadStep { path } => Done::StepRead(
            std::fs::read(&path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))
                .and_then(|d| Solid::read_step_all(&d).map_err(|e| e.to_string())),
        ),
        Work::ExportStep { solids, path } => Done::StepExported(
            Solid::write_step_many(&solids)
                .map_err(|e| e.to_string())
                .and_then(|d| {
                    std::fs::write(&path, d).map_err(|e| format!("cannot write {}: {e}", path.display()))
                })
                .map(|_| path),
        ),
        Work::Meshes { bodies } => Done::Meshes(meshes_of(&bodies)),
        Work::MeshStats {
            solids,
            params,
            ticket,
        } => Done::MeshStats(
            ticket,
            (|| {
                let mut n = 0;
                for s in &solids {
                    n += s
                        .mesh(params.deviation, params.angle_deg.to_radians())
                        .map_err(|e| e.to_string())?
                        .triangle_count();
                }
                Ok(n)
            })(),
        ),
        Work::Remesh {
            ticket,
            bodies,
            pixel_mm,
            max_triangles,
        } => Done::Remesh(
            ticket,
            bodies
                .iter()
                .map(|(b, prev)| mesh_entry_view(b, pixel_mm, max_triangles, *prev))
                .filter(|r| r.as_ref().map_or(true, |m| !m.mesh.indices.is_empty()))
                .collect(),
        ),
        Work::EdgePreview {
            ticket,
            body,
            edges,
            size,
            chamfer,
            angle_deg,
            flip,
        } => Done::EdgePreview(
            ticket,
            (|| {
                let ids: Vec<rolidium_kernel::EdgeId> =
                    edges.iter().map(|e| rolidium_kernel::EdgeId(*e)).collect();
                let r = if chamfer && ((angle_deg - 45.0).abs() > 1e-9 || flip) {
                    body.2.chamfer_angled(&ids, size, angle_deg, flip)
                } else if chamfer {
                    body.2.chamfer(&ids, size)
                } else {
                    body.2.fillet(&ids, size)
                }
                .map_err(|e| match e {
                    // the preview must stay quick: no search for the largest size here (the real operation does that); and the
                    // technical text of the kernel is not for the user
                    rolidium_kernel::KernelError::Native(_) => {
                        "too large for the neighbouring faces, or the edges meet in a way that cannot be rounded or chamfered".to_string()
                    }
                    e => e.to_string(),
                })?;
                mesh_entry(&(body.0, body.1.clone(), r.solid, body.3))
            })(),
        ),
        Work::ExportStl { solids, path, params } => Done::Export((|| {
            let mut tris = 0;
            let mut all = Mesh::default();
            for s in &solids {
                let m = s
                    .mesh(params.deviation, params.angle_deg.to_radians())
                    .map_err(|e| e.to_string())?;
                let base = all.positions.len() as u32;
                all.positions.extend(&m.positions);
                all.normals.extend(&m.normals);
                all.indices.extend(m.indices.iter().map(|i| i + base));
                all.tri_face.extend(&m.tri_face);
                tris += m.triangle_count();
            }
            let mut buf = Vec::new();
            if params.ascii {
                all.write_stl_ascii(&mut buf).map_err(|e| e.to_string())?;
            } else {
                all.write_stl_binary(&mut buf).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, buf).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok((path, tris))
        })()),
    }
}

impl Worker {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (tx, wrx) = channel::<Work>();
        let (wtx, rx) = channel::<(Done, f32)>();
        std::thread::Builder::new()
            .name("rolidium-worker".into())
            .spawn(move || {
                while let Ok(work) = wrx.recv() {
                    let t0 = std::time::Instant::now();
                    let (kind, ticket) = match &work {
                        Work::MeshStats { ticket, .. } => (3, *ticket),
                        Work::EdgePreview { ticket, .. } => (4, *ticket),
                        Work::Remesh { ticket, .. } => (6, *ticket),
                        Work::SaveFile { .. }
                        | Work::OpenFile { .. }
                        | Work::ReadStep { .. }
                        | Work::ExportStep { .. } => (5, 0),
                        _ => (0, 0),
                    };
                    let is_cmd = matches!(work, Work::Command(_));
                    let is_mesh = matches!(work, Work::Meshes { .. });
                    let done = match catch_unwind(AssertUnwindSafe(|| handle(work))) {
                        Ok(d) => d,
                        Err(p) => {
                            let msg = format!(
                                "Internal error while computing the operation; the model was not changed ({})",
                                describe_panic(p)
                            );
                            log(&format!("worker panic: {msg}"));
                            if matches!(kind, 5) {
                                Done::Failed(msg)
                            } else if matches!(kind, 3) {
                                Done::MeshStats(ticket, Err(msg))
                            } else if matches!(kind, 4) {
                                Done::EdgePreview(ticket, Err(msg))
                            } else if matches!(kind, 6) {
                                Done::Remesh(ticket, Err(msg))
                            } else if is_cmd {
                                Done::Command(Err(msg))
                            } else if is_mesh {
                                Done::Meshes(Err(msg))
                            } else {
                                Done::Export(Err(msg))
                            }
                        }
                    };
                    if wtx.send((done, t0.elapsed().as_secs_f32() * 1000.0)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("cannot start worker thread");
        Worker { tx, rx }
    }

    pub fn send(&self, w: Work) -> bool {
        self.tx.send(w).is_ok()
    }

    pub fn poll(&self) -> Option<(Done, f32)> {
        self.rx.try_recv().ok()
    }
}
