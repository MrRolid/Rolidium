// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The stages of one small operation in a big document: what the window does between the confirmation and the visible
//! result. Used by `rolidium-cli times` and by the test that guards the ratio to the same operation on a body alone.

use rolidium_doc::display::{BodyInput, MeshCache};
use rolidium_doc::{BodyId, Command, Document, ItemId, PullMode};
use rolidium_kernel::{DVec3, FaceId, Solid};
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// The stages of one operation (ms).
#[derive(Debug, Clone, Default)]
pub struct OpPhases {
    pub name: String,
    /// The job in the kernel, with the checks against the other bodies.
    pub kernel: f64,
    /// Storing the step for Undo.
    pub commit: f64,
    /// The file image of the autosave (the window makes it later, in the background).
    pub autosave: f64,
    /// All display meshes made again (how the window worked before the cache), on one thread.
    pub meshes_all: f64,
    /// The display meshes of the changed bodies only.
    pub meshes_changed: f64,
    /// The mass of the selected body for the panel.
    pub panel: f64,
}

impl OpPhases {
    /// Confirmation to result before the change: everything, with all meshes made again and the autosave in line.
    pub fn before(&self) -> f64 {
        self.kernel + self.commit + self.autosave + self.meshes_all + self.panel
    }
    /// After: the changed bodies only, the autosave in the background.
    pub fn after(&self) -> f64 {
        self.kernel + self.commit + self.meshes_changed + self.panel
    }
}

pub fn visible_inputs(doc: &Document) -> Vec<BodyInput> {
    doc.bodies()
        .iter()
        .filter(|b| b.visible)
        .map(|b| (b.id, b.name.clone(), b.solid.clone(), b.color))
        .collect()
}

fn small_ops(doc: &Document, body: BodyId) -> Option<(FaceId, u32)> {
    rolidium_doc::display::small_edit_targets(doc, body)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn median_row(runs: Vec<OpPhases>) -> OpPhases {
    let pick = |f: fn(&OpPhases) -> f64| median(runs.iter().map(f).collect());
    OpPhases {
        name: runs[0].name.clone(),
        kernel: pick(|r| r.kernel),
        commit: pick(|r| r.commit),
        autosave: pick(|r| r.autosave),
        meshes_all: pick(|r| r.meshes_all),
        meshes_changed: pick(|r| r.meshes_changed),
        panel: pick(|r| r.panel),
    }
}

/// Times a few operations (then Undo and Redo) on the body in `doc`, stage by stage; the median of `repeat` runs.
pub fn measure_ops(doc: &mut Document, body: BodyId, repeat: usize) -> Vec<OpPhases> {
    if small_ops(doc, body).is_none() {
        return Vec::new();
    }
    let mut cache = MeshCache::default();
    cache.entries(&visible_inputs(doc)).unwrap();
    // the commands are made from the body as it is when the operation comes (an earlier operation changed its faces and edges)
    type Make = Box<dyn Fn(&Document) -> Option<Command>>;
    let ops: Vec<(&str, Make)> = vec![
        (
            "move one body",
            Box::new(move |_| {
                Some(Command::Move {
                    bodies: vec![body],
                    translation: DVec3::new(0.0, 0.0, 0.5),
                    pivot: DVec3::ZERO,
                    axis: DVec3::ZERO,
                    angle: 0.0,
                    copy: false,
                })
            }),
        ),
        (
            "pull a face of the small body",
            Box::new(move |d| {
                small_ops(d, body).map(|(face, _)| Command::PullFace {
                    body,
                    face,
                    distance: 0.5,
                    mode: PullMode::Auto,
                })
            }),
        ),
        (
            "round one edge (radius 0.1)",
            Box::new(move |d| {
                small_ops(d, body).map(|(_, edge)| Command::FilletEdges {
                    body,
                    edges: vec![edge],
                    size: 0.1,
                    chamfer: false,
                })
            }),
        ),
        (
            "rename",
            Box::new(move |_| {
                Some(Command::Rename {
                    target: ItemId::Body(body),
                    name: "Renamed".into(),
                })
            }),
        ),
    ];
    let mut rows = Vec::new();
    for (name, make) in ops {
        let mut runs: Vec<OpPhases> = Vec::new();
        for _ in 0..repeat {
            let mut row = OpPhases {
                name: name.into(),
                ..Default::default()
            };
            let Some(cmd) = make(doc) else { break };
            let t = Instant::now();
            let Ok(c) = doc.plan(cmd).run() else {
                break;
            };
            row.kernel = ms(t);
            let t = Instant::now();
            doc.commit(c).unwrap();
            row.commit = ms(t);
            let t = Instant::now();
            let _ = rolidium_doc::file::save_to_bytes(doc, None, "bench").unwrap();
            row.autosave = ms(t);
            let inputs = visible_inputs(doc);
            let t = Instant::now();
            let mut fresh = MeshCache::serial();
            fresh.entries(&inputs).unwrap();
            row.meshes_all = ms(t);
            let t = Instant::now();
            cache.entries(&inputs).unwrap();
            row.meshes_changed = ms(t);
            let t = Instant::now();
            if let Some(b) = doc.body(body) {
                let _ = b.mass_g();
            }
            row.panel = ms(t);
            runs.push(row);
        }
        if !runs.is_empty() {
            rows.push(median_row(runs));
        }
    }
    // Undo and Redo: the document jumps to another state, then the meshes follow
    for (name, undo) in [("undo", true), ("redo", false)] {
        let mut runs = Vec::new();
        for _ in 0..repeat {
            let mut row = OpPhases {
                name: name.into(),
                ..Default::default()
            };
            if !undo {
                // something to redo: go back one step first (not timed)
                if !doc.undo() {
                    break;
                }
                cache.entries(&visible_inputs(doc)).unwrap();
            }
            let t = Instant::now();
            let ok = if undo { doc.undo() } else { doc.redo() };
            if !ok {
                break;
            }
            row.commit = ms(t);
            let inputs = visible_inputs(doc);
            let t = Instant::now();
            let mut fresh = MeshCache::serial();
            fresh.entries(&inputs).unwrap();
            row.meshes_all = ms(t);
            let t = Instant::now();
            cache.entries(&inputs).unwrap();
            row.meshes_changed = ms(t);
            runs.push(row);
            // back to where it was, so that the next run has something to do
            if undo {
                doc.redo();
            }
            cache.entries(&visible_inputs(doc)).unwrap();
        }
        if !runs.is_empty() {
            rows.push(median_row(runs));
        }
    }
    rows
}

/// A document of `n` small separate bodies (blocks, pins, plates with a hole) and one screw-like body to work on: the size of
/// a machine without needing its file. Returns the document and the id of the small body.
pub fn synthetic_machine(n: usize) -> (Document, BodyId) {
    let mut doc = Document::new();
    let mut solids = Vec::new();
    let side = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (x, y) = ((i % side) as f64 * 60.0, (i / side) as f64 * 60.0);
        let s = match i % 3 {
            0 => Solid::cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 40.0, y + 30.0, 12.0)).unwrap(),
            1 => Solid::cylinder(10.0, 25.0, DVec3::new(x + 20.0, y + 20.0, 0.0), DVec3::Z).unwrap(),
            _ => {
                let b = Solid::cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 40.0, y + 40.0, 10.0)).unwrap();
                let h = Solid::cylinder(4.0, 12.0, DVec3::new(x + 20.0, y + 20.0, -1.0), DVec3::Z).unwrap();
                b.boolean(&h, rolidium_kernel::BooleanOp::Cut).unwrap().solid
            }
        };
        solids.push((s, format!("Part {i}")));
    }
    // the small body: a pin with a head, off to the side
    let pin = Solid::cylinder(3.0, 20.0, DVec3::new(-100.0, -100.0, 0.0), DVec3::Z).unwrap();
    let head = Solid::cylinder(5.0, 4.0, DVec3::new(-100.0, -100.0, 20.0), DVec3::Z).unwrap();
    let pin = pin
        .boolean(&head, rolidium_kernel::BooleanOp::Fuse)
        .unwrap()
        .solid;
    solids.push((pin, "Screw".into()));
    doc.apply(Command::AddNamedSolids { solids }).unwrap();
    let id = doc.bodies().last().unwrap().id;
    (doc, id)
}

/// The same small body alone in a document.
pub fn alone_of(doc: &Document, body: BodyId) -> (Document, BodyId) {
    let mut d = Document::new();
    let b = doc.body(body).unwrap();
    d.apply(Command::AddNamedSolids {
        solids: vec![(b.solid.clone(), b.name.clone())],
    })
    .unwrap();
    let id = d.bodies()[0].id;
    (d, id)
}

fn print_phases(title: &str, rows: &[OpPhases]) {
    println!("\n{title}");
    println!(
        "{:<32} {:>8} {:>8} {:>9} {:>11} {:>10} {:>7} {:>10} {:>9}",
        "operation", "kernel", "Undo", "autosave", "meshes all", "meshes new", "panel", "before", "after"
    );
    for r in rows {
        println!(
            "{:<32} {:>8.1} {:>8.1} {:>9.1} {:>11.1} {:>10.1} {:>7.1} {:>10.1} {:>9.1}",
            r.name,
            r.kernel,
            r.commit,
            r.autosave,
            r.meshes_all,
            r.meshes_changed,
            r.panel,
            r.before(),
            r.after()
        );
    }
}

/// The machine of the demo (the file of the person, else the script's base), if it is on disk.
pub fn open_machine() -> Option<(String, Document, f64)> {
    let candidates = [
        std::env::var("ROLIDIUM_TIMES_MACHINE").unwrap_or_default(),
        "examples/demos/10-cnc-router/10-cnc-router.rolidium".into(),
        "target/demos/10-cnc-router/10-cnc-router-base.rolidium".into(),
        "target/demos/10-cnc-router/10-cnc-router.rolidium".into(),
    ];
    for c in candidates.iter().filter(|c| !c.is_empty()) {
        let path = std::path::Path::new(c);
        if path.exists() {
            let mut r = crate::Runner::new();
            r.quiet = true;
            let t = Instant::now();
            if r.open_file(path).is_ok() {
                return Some((c.clone(), r.doc, ms(t)));
            }
        }
    }
    None
}

/// The rows of `rolidium-cli times` about the machine.
pub fn machine_times() {
    println!(
        "\nThe small body in a big document (ms). kernel = the job incl. the checks against the other bodies; Undo = storing the step; \
autosave = the file image (now made later, in the background); meshes all = every body meshed again (the window before the change); \
meshes new = only the changed bodies; panel = mass of the selected body; before = kernel+Undo+autosave+meshes all+panel; \
after = kernel+Undo+meshes new+panel. Threads: {}.",
        rolidium_kernel::par::threads()
    );
    let (mut doc, body) = match open_machine() {
        Some((path, doc, read_ms)) => {
            let inputs = visible_inputs(&doc);
            let groups = rolidium_kernel::par::share_groups(&inputs, |b| &b.2);
            println!(
                "\nbodies that share kernel data are meshed one after the other: {} groups for {} bodies (largest {})",
                groups.len(),
                inputs.len(),
                groups.iter().map(Vec::len).max().unwrap_or(0)
            );
            let t = Instant::now();
            let mut cache = MeshCache::default();
            cache.entries(&inputs).unwrap();
            let mesh_ms = ms(t);
            println!(
                "\nopen the machine ({path}): {} bodies; the file {read_ms:.0} ms, the display meshes {mesh_ms:.0} ms, together {:.0} ms",
                doc.bodies().len(),
                read_ms + mesh_ms
            );
            let id = doc
                .bodies()
                .iter()
                .find(|b| b.name.starts_with("Screw"))
                .map_or(doc.bodies()[0].id, |b| b.id);
            (doc, id)
        }
        None => {
            println!("\n(no machine file found: a synthetic document of 340 bodies)");
            synthetic_machine(340)
        }
    };
    let (mut alone, alone_id) = alone_of(&doc, body);
    let in_machine = measure_ops(&mut doc, body, 3);
    let on_its_own = measure_ops(&mut alone, alone_id, 3);
    print_phases(
        &format!("in the document of {} bodies", doc.bodies().len()),
        &in_machine,
    );
    print_phases("the same body alone in a document", &on_its_own);
}

/// How well this computer gives several threads the same work: `work` small independent tasks of pure arithmetic, wall time for 1, 2, 4 and 8
/// threads (no kernel, no lock). It tells how much of a speed-up is possible at all on the computer as it is at that moment.
fn pure_scaling() -> Vec<(usize, f64)> {
    fn task(seed: u64) -> u64 {
        let mut x = seed | 1;
        for _ in 0..4_000_000u64 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
        }
        x
    }
    let jobs = 64u64;
    [1usize, 2, 4, 8]
        .into_iter()
        .map(|n| {
            let next = std::sync::atomic::AtomicU64::new(0);
            let t = Instant::now();
            std::thread::scope(|s| {
                for _ in 0..n {
                    s.spawn(|| {
                        let mut acc = 0u64;
                        loop {
                            let j = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if j >= jobs {
                                break;
                            }
                            acc ^= task(j);
                        }
                        std::hint::black_box(acc);
                    });
                }
            });
            (n, ms(t))
        })
        .collect()
}

/// The open of the machine, nothing else: the file, then all display meshes (for the table by the number of threads), with the parts that
/// run one at a time and the scaling of plain arithmetic on this computer (`pure`: also printed).
pub fn open_times() {
    use rolidium_kernel::{CHECK_NS, READ_NS};
    use std::sync::atomic::Ordering::Relaxed;
    let _ = rolidium_kernel::timing::take();
    let Some((path, doc, read_ms)) = open_machine() else {
        println!("no machine file found (ROLIDIUM_TIMES_MACHINE=file)");
        return;
    };
    let stages = rolidium_kernel::timing::take();
    let inputs = visible_inputs(&doc);
    rolidium_doc::display::MESH_NS.store(0, Relaxed);
    rolidium_doc::display::FACTS_NS.store(0, Relaxed);
    let t = Instant::now();
    let mut cache = MeshCache::default();
    cache.entries(&inputs).unwrap();
    let mesh_ms = ms(t);
    let thread_ms = |a: &std::sync::atomic::AtomicU64| a.load(Relaxed) as f64 / 1e6;
    println!(
        "threads {}: open {path}: {} bodies; the file {read_ms:.0} ms, all display meshes {mesh_ms:.0} ms, together {:.0} ms",
        rolidium_kernel::par::threads(),
        doc.bodies().len(),
        read_ms + mesh_ms
    );
    println!("  file: {}", rolidium_kernel::timing::describe(&stages));
    println!(
        "  summed over the threads: BREP reading {:.0} ms (no lock), checking the shapes {:.0} ms (behind the lock), meshing {:.0} ms (no lock), \
facts about the faces {:.0} ms (behind the lock)",
        thread_ms(&READ_NS),
        thread_ms(&CHECK_NS),
        thread_ms(&rolidium_doc::display::MESH_NS),
        thread_ms(&rolidium_doc::display::FACTS_NS)
    );
    if std::env::var_os("ROLIDIUM_TIMES_PURE").is_some() {
        let base = pure_scaling();
        let t1 = base[0].1;
        println!(
            "  plain arithmetic, 64 equal tasks, wall time by threads: {}",
            base.iter()
                .map(|(n, t)| format!("{n}: {t:.0} ms (x{:.2})", t1 / t))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
