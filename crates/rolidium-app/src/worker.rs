// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Background thread for all geometry work. The UI thread never waits for the kernel: it sends
//! a [`Work`] item and polls for the [`Done`] answer. Panics inside the kernel are caught here
//! and reported as errors, so the UI always survives and the document is never half-changed.

use crate::logging::log;
use rolidium_doc::file::Loaded;
use rolidium_doc::{Computed, DocError, Document, Job, State};
use rolidium_kernel::{Mesh, Solid};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// What the UI needs to know about a face without asking the kernel.
/// A body to be meshed again for the zoom: the body, its present mesh, and the faces that are in view (the others stay as they are).
pub struct RemeshBody {
    pub body: BodyInput,
    /// The present mesh of the body: the faces out of view are taken from it.
    pub base: Arc<Mesh>,
    pub fine: Vec<bool>,
    /// The size of a pixel (mm) at this body, and the most triangles it may get.
    pub pixel_mm: f64,
    pub max_triangles: usize,
}

use rolidium_doc::display::{entry_of, mesh_entry, MeshCache};
pub use rolidium_doc::display::{BodyInput, FaceMeta, MeshEntry, MeshInput};

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
        solids: Vec<(Solid, String)>,
        path: PathBuf,
    },
    Meshes {
        bodies: Vec<BodyInput>,
        /// The meshes (STL) of the document with their regions.
        meshes: Vec<MeshInput>,
    },
    /// New display meshes for the current zoom (in the background, while the person looks at the model).
    Remesh {
        ticket: u64,
        /// The bodies with the deflection and triangle count of their present meshes.
        bodies: Vec<RemeshBody>,
    },
    ExportStl {
        solids: Vec<Solid>,
        /// The meshes (STL) of the document that are exported with the solids as they are.
        meshes: Vec<Arc<rolidium_mesh::TriMesh>>,
        path: PathBuf,
        params: StlParams,
    },
    /// Reads STL files (binary or ASCII) and checks the meshes.
    ReadStl {
        paths: Vec<PathBuf>,
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

/// A mesh read from an STL file, as it is in the file (the unit is chosen afterwards), with the result of the check.
pub struct StlLoaded {
    pub name: String,
    pub mesh: rolidium_mesh::TriMesh,
    pub analysis: rolidium_mesh::check::Analysis,
    /// The unit is the one of the file (3MF): the choice of the unit in the dialog does not apply.
    pub fixed_unit: bool,
    /// The share (0 to 1) of the surface that is in flat pieces of four triangles or more; `None` for a big mesh (not looked at).
    pub flat_share: Option<f64>,
}

/// The solids of a STEP file with their names, and the reasons why some were skipped.
pub type StepSolids = (Vec<(Solid, String)>, Vec<String>);

#[allow(clippy::large_enum_variant)]
pub enum Done {
    /// Saved: the path, the state that was written and whether it was an autosave.
    Saved(Result<(PathBuf, Arc<State>, bool), String>),
    Opened(Result<(PathBuf, Box<Loaded>), String>),
    StepRead(Result<StepSolids, String>),
    StlRead(Vec<Result<StlLoaded, String>>),
    StepExported(Result<PathBuf, String>),
    /// A background job died with an internal error.
    Failed(String),
    Command(Result<(Computed, Vec<MeshEntry>), String>),
    /// The command edits a body that a link made: the command, the link, its name and the name of the body (the person decides).
    Derived(Box<(rolidium_doc::Command, rolidium_doc::LinkId, String, String)>),
    Meshes(Result<Vec<MeshEntry>, String>),
    Export(Result<(PathBuf, usize), String>),
    MeshStats(u64, Result<usize, String>),
    EdgePreview(u64, Result<MeshEntry, String>),
    Remesh(u64, Result<Vec<MeshEntry>, String>),
}

pub struct Worker {
    tx: Sender<Work>,
    atx: Sender<Work>,
    rx: Receiver<(Done, f32)>,
}

/// The entry of a body for a mesh made for the current zoom (`pixel_mm` per pixel), with at most `max_triangles` triangles.
fn mesh_entry_view(r: &RemeshBody) -> Result<MeshEntry, String> {
    let b = &r.body;
    let mesh =
        b.2.view_mesh_faces(r.pixel_mm, &r.fine, r.max_triangles, Some(&*r.base))
            .map_err(|e| e.to_string())?;
    entry_of(b, mesh)
}

/// The finer meshes for these bodies (independent bodies on several threads).
pub fn remesh_entries(bodies: &[RemeshBody]) -> Vec<Result<MeshEntry, String>> {
    rolidium_kernel::par::map_groups(bodies, |r| &r.body.2, mesh_entry_view)
}

fn meshes_of(bodies: &[BodyInput], cache: &mut MeshCache) -> Result<Vec<MeshEntry>, String> {
    cache.entries(bodies)
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
            stamp: m.stamp,
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
        xray: false,
        clip: None,
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

fn handle(work: Work, cache: &mut MeshCache) -> Done {
    // a cancel request belongs to the job that was running; every new job starts clean
    // (the autosave runs beside the operations and must not clear their cancel request)
    if !matches!(work, Work::SaveFile { autosave: true, .. }) {
        rolidium_kernel::reset_cancel();
    }
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
            let command = job.command.clone();
            let ran = job.run();
            if let Err(DocError::Derived {
                link,
                link_name,
                body_name,
                ..
            }) = &ran
            {
                return Done::Derived(Box::new((command, *link, link_name.clone(), body_name.clone())));
            }
            let r = ran.map_err(|e: DocError| e.to_string()).and_then(|c| {
                run_ms = started.elapsed().as_secs_f64() * 1e3;
                let inputs: Vec<BodyInput> = c
                    .state
                    .bodies
                    .iter()
                    .filter(|b| b.visible)
                    .map(|b| (b.id, b.name.clone(), b.solid.clone(), b.color))
                    .collect();
                let mut meshes = meshes_of(&inputs, cache)?;
                meshes.extend(cache.mesh_entries(&rolidium_doc::display::mesh_inputs(&c.state)));
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
                .and_then(|d| Solid::read_step_all_named(&d).map_err(|e| e.to_string())),
        ),
        Work::ExportStep { solids, path } => Done::StepExported(
            Solid::write_step_named(&solids)
                .map_err(|e| e.to_string())
                .and_then(|d| {
                    std::fs::write(&path, d).map_err(|e| format!("cannot write {}: {e}", path.display()))
                })
                .map(|_| path),
        ),
        Work::Meshes { bodies, meshes } => Done::Meshes(meshes_of(&bodies, cache).map(|mut v| {
            v.extend(cache.mesh_entries(&meshes));
            v
        })),
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
        Work::Remesh { ticket, bodies } => Done::Remesh(ticket, {
            // independent bodies on several cores; the refined meshes stay in the cache for the next operation
            let made = remesh_entries(&bodies);
            for (r, e) in bodies.iter().zip(&made) {
                if let Ok(e) = e {
                    cache.replace(e, &r.body.2);
                }
            }
            made.into_iter()
                .filter(|r| r.as_ref().map_or(true, |m| !m.mesh.indices.is_empty()))
                .collect()
        }),
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
        Work::ReadStl { paths } => Done::StlRead(std::thread::scope(|sc| {
            let hs: Vec<_> = paths
                .iter()
                .map(|path| {
                    sc.spawn(move || -> Vec<Result<StlLoaded, String>> {
                        let shown = path
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
                        let bytes = match std::fs::read(path) {
                            Ok(b) => b,
                            Err(e) => return vec![Err(format!("{shown}: cannot read the file ({e})"))],
                        };
                        let stem = path
                            .file_stem()
                            .map_or_else(|| "Mesh".to_string(), |n| n.to_string_lossy().to_string());
                        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("3mf")) {
                            // every object of a 3MF file is a mesh of its own; the unit is the one of the file
                            return match rolidium_mesh::threemf::parse(&bytes) {
                                Ok(parts) => {
                                    let many = parts.len() > 1;
                                    parts
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, p)| {
                                            let analysis = rolidium_mesh::check::analyse(&p.mesh);
                                            let name = if many && p.name.starts_with("Part ") {
                                                format!("{stem} {}", i + 1)
                                            } else if p.name.is_empty() || p.name.starts_with("Part ") {
                                                stem.clone()
                                            } else {
                                                p.name
                                            };
                                            let flat_share = (p.mesh.triangles.len() <= 300_000)
                                                .then(|| rolidium_mesh::convert::flat_share(&p.mesh, 0.02));
                                            Ok(StlLoaded {
                                                name,
                                                mesh: p.mesh,
                                                analysis,
                                                fixed_unit: true,
                                                flat_share,
                                            })
                                        })
                                        .collect()
                                }
                                Err(e) => vec![Err(format!("{shown}: {e}"))],
                            };
                        }
                        let mesh = match rolidium_mesh::stl::parse(&bytes) {
                            Ok(m) => m,
                            Err(e) => return vec![Err(format!("{shown}: {e}"))],
                        };
                        let analysis = rolidium_mesh::check::analyse(&mesh);
                        let flat_share = (mesh.triangles.len() <= 300_000)
                            .then(|| rolidium_mesh::convert::flat_share(&mesh, 0.02));
                        vec![Ok(StlLoaded {
                            name: stem,
                            mesh,
                            analysis,
                            fixed_unit: false,
                            flat_share,
                        })]
                    })
                })
                .collect();
            hs.into_iter()
                .flat_map(|h| {
                    h.join()
                        .unwrap_or_else(|_| vec![Err("the file could not be read".to_string())])
                })
                .collect()
        })),
        Work::ExportStl {
            solids,
            meshes,
            path,
            params,
        } => Done::Export((|| {
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
            // the meshes of the document: their triangles as they are
            for m in &meshes {
                let base = all.positions.len() as u32;
                all.positions
                    .extend(m.positions.iter().map(|p| glam::Vec3::new(p[0], p[1], p[2])));
                all.normals
                    .extend(std::iter::repeat_n(glam::Vec3::ZERO, m.positions.len()));
                for t in &m.triangles {
                    all.indices.extend([t[0] + base, t[1] + base, t[2] + base]);
                    all.tri_face.push(0);
                }
                tris += m.triangles.len();
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

fn worker_loop(wrx: Receiver<Work>, wtx: Sender<(Done, f32)>, ctx: eframe::egui::Context) {
    let mut cache = MeshCache::default();
    while let Ok(work) = wrx.recv() {
        let t0 = std::time::Instant::now();
        let (kind, ticket) = match &work {
            Work::MeshStats { ticket, .. } => (3, *ticket),
            Work::EdgePreview { ticket, .. } => (4, *ticket),
            Work::Remesh { ticket, .. } => (6, *ticket),
            Work::SaveFile { .. }
            | Work::OpenFile { .. }
            | Work::ReadStl { .. }
            | Work::ReadStep { .. }
            | Work::ExportStep { .. } => (5, 0),
            _ => (0, 0),
        };
        let is_cmd = matches!(work, Work::Command(_));
        let is_mesh = matches!(work, Work::Meshes { .. });
        let done = match catch_unwind(AssertUnwindSafe(|| handle(work, &mut cache))) {
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
}

impl Worker {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (tx, wrx) = channel::<Work>();
        let (wtx, rx) = channel::<(Done, f32)>();
        // The autosave has a thread of its own: writing the file image of a big document takes a while and must never keep
        // an operation waiting.
        let (atx, arx) = channel::<Work>();
        {
            let (wtx, ctx) = (wtx.clone(), ctx.clone());
            std::thread::Builder::new()
                .name("rolidium-autosave".into())
                .spawn(move || worker_loop(arx, wtx, ctx))
                .expect("cannot start autosave thread");
        }
        std::thread::Builder::new()
            .name("rolidium-worker".into())
            .spawn(move || worker_loop(wrx, wtx, ctx))
            .expect("cannot start worker thread");
        Worker { tx, atx, rx }
    }

    pub fn send(&self, w: Work) -> bool {
        match w {
            Work::SaveFile { autosave: true, .. } => self.atx.send(w).is_ok(),
            w => self.tx.send(w).is_ok(),
        }
    }

    pub fn poll(&self) -> Option<(Done, f32)> {
        self.rx.try_recv().ok()
    }
}
