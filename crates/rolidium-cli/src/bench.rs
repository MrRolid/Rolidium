// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Measures the performance targets that do not need a window.

use rolidium_doc::{Command, Document, PullMode};
use rolidium_kernel::{DVec3, FaceKind, Solid};
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time_ms<T>(runs: usize, mut f: impl FnMut() -> T) -> f64 {
    let mut v = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let r = f();
        v.push(t.elapsed().as_secs_f64() * 1e3);
        drop(r);
    }
    median(v)
}

/// Builds a part with about `n` planar faces by pulling bosses out of a base plate.
fn many_face_part(n: usize) -> Solid {
    let mut s = Solid::cuboid(DVec3::ZERO, DVec3::new(400.0, 400.0, 10.0)).unwrap();
    let side = (n as f64 / 5.0).sqrt().ceil() as usize;
    let pitch = 380.0 / side as f64;
    let mut count = 0;
    'o: for i in 0..side {
        for j in 0..side {
            let b = Solid::cuboid(
                DVec3::new(10.0 + i as f64 * pitch, 10.0 + j as f64 * pitch, 5.0),
                DVec3::new(
                    10.0 + i as f64 * pitch + pitch * 0.6,
                    10.0 + j as f64 * pitch + pitch * 0.6,
                    10.0 + (count % 7) as f64,
                ),
            )
            .unwrap();
            s = s.boolean(&b, rolidium_kernel::BooleanOp::Fuse).unwrap().solid;
            count += 1;
            if s.face_count() >= n {
                break 'o;
            }
        }
    }
    s
}

pub fn run() -> i32 {
    println!("Benchmarks (median of several runs, release build recommended)\n");

    let base = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let top = base
        .faces()
        .find(|f| base.face_info(*f).unwrap().normal.z > 0.99)
        .unwrap();

    let t = time_ms(20, || base.pull_face(top, 10.0).unwrap());
    println!("pull +10 on the 6-face box (extrude + fuse + unify + validity): {t:.2} ms   (target < 100 ms for <= 200 faces)");
    let t = time_ms(20, || base.pull_face(top, -10.0).unwrap());
    println!("pull -10 on the 6-face box (extrude + cut + unify + validity):  {t:.2} ms");

    let big = many_face_part(200);
    println!(
        "\ncomplex part: {} faces, valid {}",
        big.face_count(),
        big.is_valid()
    );
    let target = big
        .faces()
        .filter(|f| {
            let i = big.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99
        })
        .last()
        .unwrap();
    let t = time_ms(10, || big.pull_face(target, 5.0).unwrap());
    println!(
        "pull +5 on a {}-face part: {t:.2} ms   (target < 100 ms)",
        big.face_count()
    );
    let t = time_ms(10, || big.pull_face(target, -3.0).unwrap());
    println!("pull -3 on a {}-face part: {t:.2} ms", big.face_count());
    let t = time_ms(10, || big.mesh(0.05, 0.3).unwrap());
    println!("tessellation of the {}-face part: {t:.2} ms", big.face_count());

    let mesh = big.mesh(0.05, 0.3).unwrap();
    let mut buf = Vec::new();
    let t = time_ms(10, || {
        buf.clear();
        mesh.write_stl_binary(&mut buf).unwrap()
    });
    println!(
        "binary STL export ({} triangles): {t:.2} ms",
        mesh.triangle_count()
    );

    // Document round trip as the app does it.
    let mut doc = Document::new();
    doc.apply(Command::AddBox {
        name: "b".into(),
        min: DVec3::ZERO,
        max: DVec3::new(40.0, 30.0, 20.0),
    })
    .unwrap();
    let id = doc.bodies()[0].id;
    let f = doc.bodies()[0]
        .solid
        .faces()
        .find(|f| doc.bodies()[0].solid.face_info(*f).unwrap().normal.z > 0.99)
        .unwrap();
    let t = time_ms(20, || {
        let job = doc.plan(Command::PullFace {
            body: id,
            face: f,
            distance: 5.0,
            mode: PullMode::Auto,
        });
        job.run().unwrap()
    });
    println!("document command (plan + run, no commit): {t:.2} ms");

    // Pull from a sketch region (the reference part: through hole of diameter 10 in the 40x30x20 box).
    {
        use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
        use rolidium_doc::{DocError, SketchTarget};
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: "b".into(),
            min: DVec3::ZERO,
            max: DVec3::new(40.0, 30.0, 20.0),
        })
        .unwrap();
        let body = d.bodies()[0].id;
        let plane = PlaneDef::from_face(DVec3::new(0.0, 0.0, 20.0), DVec3::Z);
        let c = d
            .apply(Command::AddEntity {
                target: SketchTarget::New {
                    plane,
                    body: Some(body),
                },
                entity: Entity::Circle {
                    center: DVec2::new(20.0, 15.0),
                    radius: 5.0,
                },
            })
            .unwrap();
        let sk = c.created_sketch.unwrap();
        let t = time_ms(30, || -> Result<_, DocError> {
            d.plan(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: -20.0,
                mode: PullMode::Auto,
            })
            .run()
        });
        let vol = d
            .plan(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: -20.0,
                mode: PullMode::Auto,
            })
            .run()
            .unwrap()
            .state
            .bodies[0]
            .solid
            .volume()
            .unwrap();
        println!(
            "pull from sketch region, through hole d=10 in the 40x30x20 box: {t:.2} ms (volume {vol:.1} mm3)"
        );
    }

    // Edge round and hole resize
    {
        let e = base
            .edges()
            .find(|e| {
                let i = base.edge_info(*e).unwrap();
                (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9 && i.length > 19.0
            })
            .unwrap();
        let t = time_ms(30, || base.fillet(&[e], 5.0).unwrap());
        println!("round R5 of one edge of the box (real result): {t:.2} ms");
        let hole = Solid::cylinder(5.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap();
        let plate = base
            .boolean(&hole, rolidium_kernel::BooleanOp::Cut)
            .unwrap()
            .solid;
        let cyl = plate
            .faces()
            .find(|f| plate.face_info(*f).unwrap().kind == FaceKind::Cylinder)
            .unwrap();
        let t = time_ms(30, || plate.set_cylinder_diameter(cyl, 12.0).unwrap());
        println!("hole resize d10 -> d12: {t:.2} ms");
    }

    // Interactive preview geometry (what the app rebuilds on every mouse move).
    let mesh = big.mesh(0.05, 0.3).unwrap();
    let t = time_ms(200, || {
        rolidium_render::pull_preview(&mesh, target.0, glam::Vec3::Z, 7.0)
    });
    println!(
        "pull preview geometry for a face of the {}-face part: {t:.3} ms   (target < 16 ms incl. frame)",
        big.face_count()
    );

    viewport_bench();
    0
}

/// A closed-ish synthetic mesh with about one million triangles.
fn million_triangle_mesh() -> rolidium_kernel::Mesh {
    let n = 708usize; // n*n*2 = ~1.0 M triangles
    let mut m = rolidium_kernel::Mesh::default();
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (i as f32 / n as f32 * 200.0, j as f32 / n as f32 * 200.0);
            let z = 10.0 * ((x * 0.07).sin() * (y * 0.05).cos());
            m.positions.push(glam::Vec3::new(x, y, z));
            m.normals.push(
                glam::Vec3::new(
                    -0.7 * (x * 0.07).cos() * (y * 0.05).cos(),
                    0.5 * (x * 0.07).sin() * (y * 0.05).sin(),
                    1.0,
                )
                .normalize(),
            );
        }
    }
    let row = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let a = j * row + i;
            m.indices.extend([a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            m.tri_face.extend([0, 0]);
        }
    }
    m
}

fn viewport_bench() {
    use rolidium_render::{BodyDraw, Camera, Frame, Renderer};
    let (device, queue, info) = match rolidium_render::headless_device() {
        Ok(d) => d,
        Err(e) => {
            println!("\nviewport benchmark NOT RUN: {e}");
            return;
        }
    };
    println!(
        "\nviewport benchmark on adapter: {} ({:?})",
        info.name, info.device_type
    );
    if info.device_type == rolidium_render::wgpu::DeviceType::Cpu {
        println!("  NOTE: this is a software rasteriser, the numbers say nothing about a real GPU.");
    }
    let mesh = million_triangle_mesh();
    let mut r = Renderer::new(&device, rolidium_render::wgpu::TextureFormat::Rgba8Unorm);
    r.set_bodies(
        &device,
        &[BodyDraw {
            id: 1,
            mesh: &mesh,
            color: [0.7, 0.7, 0.75],
            stamp: 0,
        }],
    );
    let mut cam = Camera::default();
    cam.fit(
        glam::Vec3::new(0.0, 0.0, -10.0),
        glam::Vec3::new(200.0, 200.0, 10.0),
    );
    let frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size: [1280, 800],
        grid: None,
        render_ids: true,
        grid_levels: None,
        scene_radius: 0.0,
    };
    r.render_and_wait(&device, &queue, &frame).unwrap();
    let t = time_ms(15, || r.render_and_wait(&device, &queue, &frame).unwrap());
    println!("  {} triangles, 1280x800, 4x MSAA + ID pass: {t:.1} ms per frame = {:.1} fps   (target 60 fps on integrated graphics)", mesh.triangle_count(), 1000.0 / t);
}

/// One row of the table: the median of the total time and the average of every stage.
struct Row {
    name: String,
    total: f64,
    stages: Vec<(String, f64)>,
}

fn measure_row(name: &str, runs: usize, mut f: impl FnMut() -> Option<Solid>) -> Row {
    let mut totals = Vec::new();
    let mut acc: Vec<(String, f64)> = Vec::new();
    let mut last: Option<Solid> = None;
    for _ in 0..runs {
        let _ = rolidium_kernel::timing::take();
        let t = Instant::now();
        let r = f();
        totals.push(t.elapsed().as_secs_f64() * 1e3);
        for (n, v) in rolidium_kernel::timing::take() {
            match acc.iter_mut().find(|(m, _)| *m == n) {
                Some(e) => e.1 += v,
                None => acc.push((n, v)),
            }
        }
        last = r.or(last);
    }
    for e in acc.iter_mut() {
        e.1 /= runs as f64;
    }
    // what the window does after the operation: the mesh for the display
    if let Some(s) = last {
        let mesh_ms = time_ms(runs.min(5), || s.display_mesh().unwrap());
        acc.push(("display mesh (window only)".into(), mesh_ms));
    }
    Row {
        name: name.into(),
        total: median(totals),
        stages: acc,
    }
}

fn script_without_files(text: &str) -> String {
    text.lines()
        .filter(|l| !l.starts_with("save") && !l.starts_with("render"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `rolidium-cli times`: a table of the times of the main operations, to compare after every phase.
pub fn times(what: Option<&str>) -> i32 {
    match what {
        Some("machine") => {
            crate::phases::machine_times();
            return 0;
        }
        Some("open") => {
            crate::phases::open_times();
            return 0;
        }
        _ => {}
    }
    println!("Times of the main operations (median of the runs; the stages are averages; release build recommended)\n");
    let mut rows = Vec::new();

    let big = many_face_part(200);
    let target = big
        .faces()
        .filter(|f| {
            let i = big.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99
        })
        .last()
        .unwrap();
    rows.push(measure_row(
        &format!("pull a face on a {}-face part", big.face_count()),
        15,
        || Some(big.pull_face(target, 5.0).unwrap().solid),
    ));

    let base = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let edge = base.edges().next().unwrap();
    rows.push(measure_row("round one edge of a box (radius 3)", 15, || {
        Some(base.fillet(&[edge], 3.0).unwrap().solid)
    }));

    let cyl = Solid::cylinder(10.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap();
    rows.push(measure_row("combine: box + cylinder (merge)", 15, || {
        Some(
            base.boolean(&cyl, rolidium_kernel::BooleanOp::Fuse)
                .unwrap()
                .solid,
        )
    }));

    for (name, text, runs) in [
        (
            "bent tube (example 07)",
            include_str!("../../../examples/07-bent-tube.txt"),
            5,
        ),
        (
            "spring (example 08)",
            include_str!("../../../examples/08-spring.txt"),
            3,
        ),
        (
            "thread M10 (example 09)",
            include_str!("../../../examples/09-thread-m10.txt"),
            3,
        ),
    ] {
        let script = script_without_files(text);
        rows.push(measure_row(name, runs, || {
            let mut r = crate::Runner::new();
            r.quiet = true;
            r.run_script(&script).unwrap();
            r.doc.bodies().last().map(|b| b.solid.clone())
        }));
    }

    println!("{:<40} {:>10}   stages (ms)", "operation", "total ms");
    for r in &rows {
        println!(
            "{:<40} {:>10.1}   {}",
            r.name,
            r.total,
            rolidium_kernel::timing::describe(&r.stages)
        );
    }
    println!("\nThe 'total' of a script includes building the part from scratch (sketches, curves, the operations and their checks).");

    // The display mesh: the first display after an operation (rough), then the faces in view for the zoom, made in the background.
    println!("\nDisplay mesh: the first display after an operation, then the faces in view for the zoom (deviation under 0.4 of a pixel of a 900 px high view; at most {MAX_VIEW_TRIANGLES} triangles per body)\n");
    println!(
        "{:<28} {:>9} {:>9} {:>11} {:>11} {:>12}",
        "part", "zoom", "ms", "triangles", "faces", "deviation mm"
    );
    let spring = {
        let mut r = crate::Runner::new();
        r.quiet = true;
        r.run_script(&script_without_files(include_str!(
            "../../../examples/08-spring.txt"
        )))
        .unwrap();
        r.doc.bodies().last().unwrap().solid.clone()
    };
    let cylinder = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    for (name, solid) in [
        ("cylinder (3 faces)", &cylinder),
        (&format!("part with {} faces", big.face_count()) as &str, &big),
        ("spring (example 08)", &spring),
    ] {
        let props = solid.props().unwrap();
        let diag = (props.max - props.min).length();
        let centre = ((props.max + props.min) * 0.5).as_vec3();
        let nfaces = solid.face_count();
        let first = solid.display_mesh().unwrap();
        let ms = time_ms(5, || solid.display_mesh().unwrap());
        println!(
            "{:<28} {:>9} {:>9.1} {:>11} {:>8}/{:<3} {:>12.4}",
            name,
            "first",
            ms,
            first.triangle_count(),
            nfaces,
            nfaces,
            first.deflection
        );
        for zoom in [1.0f64, 10.0, 100.0] {
            let pixel = diag / 900.0 / zoom;
            // the faces whose box meets the box of the view (a cube of the size the window shows around the middle of the part)
            let half = (diag as f32 / zoom as f32 * 0.5 * 0.75).max(1e-6);
            let mut lo = vec![glam::Vec3::splat(f32::MAX); nfaces];
            let mut hi = vec![glam::Vec3::splat(f32::MIN); nfaces];
            for (t, tri) in first.indices.as_chunks::<3>().0.iter().enumerate() {
                let f = first.tri_face[t] as usize;
                for &i in tri {
                    lo[f] = lo[f].min(first.positions[i as usize]);
                    hi[f] = hi[f].max(first.positions[i as usize]);
                }
            }
            let fine: Vec<bool> = (0..nfaces)
                .map(|f| {
                    zoom <= 1.0
                        || (0..3).all(|k| lo[f][k] <= centre[k] + half && hi[f][k] >= centre[k] - half)
                })
                .collect();
            let m = solid
                .view_mesh_faces(pixel, &fine, MAX_VIEW_TRIANGLES, Some(&first))
                .unwrap();
            let ms = time_ms(3, || {
                solid
                    .view_mesh_faces(pixel, &fine, MAX_VIEW_TRIANGLES, Some(&first))
                    .unwrap()
            });
            println!(
                "{:<28} {:>8}x {:>9.1} {:>11} {:>8}/{:<3} {:>12.4}",
                name,
                zoom,
                ms,
                m.triangle_count(),
                fine.iter().filter(|f| **f).count(),
                nfaces,
                m.deflection
            );
        }
    }
    crate::phases::machine_times();
    0
}

/// The cap of the display mesh of one body (the same as in the program).
const MAX_VIEW_TRIANGLES: usize = 150_000;
