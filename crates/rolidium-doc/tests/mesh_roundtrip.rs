// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! There and back: a solid made of our own models (CC0) is meshed like an STL file, the mesh is converted to a solid with round surfaces, and the faces by kind, the radii, the
//! volume and the deviation are compared. Then the converted solid is edited like a drawn one (a hole diameter, a radius of a round, a pulled wall).
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_kernel::{DVec3, EdgeId, FaceId, FaceKind, Solid};
use rolidium_mesh::TriMesh;
use std::sync::Arc;

static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

/// The edges of a solid that satisfy a test of their ends.
fn edges_where(s: &Solid, f: impl Fn(DVec3, DVec3) -> bool) -> Vec<EdgeId> {
    s.edges()
        .filter(|e| {
            let Ok(i) = s.edge_info(*e) else { return false };
            f(i.start, i.end)
        })
        .collect()
}

// ---- the models ---------------------------------------------------------------------------------------------------------------------------------------

/// A box 40 x 30 x 20 with all twelve edges rounded (3 mm): cylinders along the edges and spheres in the corners.
pub fn rounded_box() -> Solid {
    let b = Solid::cuboid(v(0.0, 0.0, 0.0), v(40.0, 30.0, 20.0)).unwrap();
    let all: Vec<EdgeId> = b.edges().collect();
    b.fillet(&all, 3.0).unwrap().solid
}

/// A plate 60 x 40 x 10 with a through hole, a counterbored hole, a countersunk hole and a chamfer round the top.
pub fn plate_with_holes() -> Solid {
    let mut s = Solid::cuboid(v(0.0, 0.0, 0.0), v(60.0, 40.0, 10.0)).unwrap();
    let top = edges_where(&s, |a, b| a.z > 9.9 && b.z > 9.9);
    s = s.chamfer(&top, 1.5).unwrap().solid;
    let z = v(0.0, 0.0, 1.0);
    // through hole
    s = s
        .boolean(
            &Solid::cylinder(3.0, 12.0, v(12.0, 20.0, -1.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Cut,
        )
        .unwrap()
        .solid;
    // counterbore: a hole of 3 and a pocket of 6, 4 deep
    s = s
        .boolean(
            &Solid::cylinder(3.0, 12.0, v(30.0, 20.0, -1.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Cut,
        )
        .unwrap()
        .solid;
    s = s
        .boolean(
            &Solid::cylinder(6.0, 4.0, v(30.0, 20.0, 6.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Cut,
        )
        .unwrap()
        .solid;
    // countersink: a cone from 8 at the top to 3 at 5 below
    s = s
        .boolean(
            &Solid::cylinder(3.0, 12.0, v(48.0, 20.0, -1.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Cut,
        )
        .unwrap()
        .solid;
    s = s
        .boolean(
            &Solid::from_cone(3.0, 8.0, 5.0, v(48.0, 20.0, 5.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Cut,
        )
        .unwrap()
        .solid;
    s
}

/// A plate with a round boss that has a cone on top and a round (fillet) at its foot: a torus.
pub fn boss_with_cone() -> Solid {
    let z = v(0.0, 0.0, 1.0);
    let mut s = Solid::cuboid(v(0.0, 0.0, 0.0), v(50.0, 50.0, 5.0)).unwrap();
    s = s
        .boolean(
            &Solid::cylinder(10.0, 15.0, v(25.0, 25.0, 5.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Fuse,
        )
        .unwrap()
        .solid;
    s = s
        .boolean(
            &Solid::from_cone(10.0, 4.0, 8.0, v(25.0, 25.0, 20.0), z).unwrap(),
            rolidium_kernel::BooleanOp::Fuse,
        )
        .unwrap()
        .solid;
    // the foot: the circle where the boss meets the plate
    let foot = edges_where(&s, |a, b| {
        (a.z - 5.0).abs() < 1e-6 && (b.z - 5.0).abs() < 1e-6 && (a.x - 25.0).hypot(a.y - 25.0) < 10.5
    });
    s.fillet(&foot, 2.0).unwrap().solid
}

/// A container in the style of a Gridfinity bin: 42 x 42 x 25 with rounded vertical corners, a pocket with rounded corners and bottom, and four holes for magnets.
pub fn bin() -> Solid {
    let z = v(0.0, 0.0, 1.0);
    let mut s = Solid::cuboid(v(0.0, 0.0, 0.0), v(42.0, 42.0, 25.0)).unwrap();
    let vertical = edges_where(&s, |a, b| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
    s = s.fillet(&vertical, 4.0).unwrap().solid;
    let mut pocket = Solid::cuboid(v(1.2, 1.2, 5.0), v(40.8, 40.8, 30.0)).unwrap();
    let pv = edges_where(&pocket, |a, b| {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    });
    pocket = pocket.fillet(&pv, 2.8).unwrap().solid;
    s = s.boolean(&pocket, rolidium_kernel::BooleanOp::Cut).unwrap().solid;
    for (x, y) in [(8.0, 8.0), (34.0, 8.0), (8.0, 34.0), (34.0, 34.0)] {
        s = s
            .boolean(
                &Solid::cylinder(3.25, 2.4, v(x, y, -0.1), z).unwrap(),
                rolidium_kernel::BooleanOp::Cut,
            )
            .unwrap()
            .solid;
    }
    s
}

/// The solid as a welded mesh of triangles, like an STL file made by a CAD program.
pub fn mesh_of(s: &Solid, linear: f64, angular: f64) -> TriMesh {
    let m = s.mesh(linear, angular).unwrap();
    let mut t = TriMesh {
        positions: m.positions.iter().map(|p| [p.x, p.y, p.z]).collect(),
        triangles: m.indices.chunks(3).map(|c| [c[0], c[1], c[2]]).collect(),
    };
    let tol = rolidium_mesh::check::merge_tolerance(&t);
    rolidium_mesh::check::merge_vertices(&mut t, tol);
    t
}

#[derive(Debug, Default, Clone)]
pub struct Row {
    pub name: String,
    pub before: [usize; 5],
    pub after: [usize; 5],
    pub radii_before: Vec<f64>,
    pub radii_after: Vec<f64>,
    pub volume_before: f64,
    pub volume_after: f64,
    pub deviation: f64,
    pub unrecognised: f64,
    pub triangles: usize,
    pub seconds: f64,
    pub route: u8,
}

pub fn kinds_of(s: &Solid) -> ([usize; 5], Vec<f64>) {
    let mut k = [0usize; 5];
    let mut radii = Vec::new();
    for f in s.faces() {
        let i = s.face_info(f).unwrap();
        let idx = match i.kind {
            FaceKind::Plane => 0,
            FaceKind::Cylinder => 1,
            FaceKind::Cone => 2,
            FaceKind::Sphere => 3,
            FaceKind::Torus => 4,
            FaceKind::Other => continue,
        };
        k[idx] += 1;
        if idx != 0 && idx != 2 {
            radii.push((i.radius * 1000.0).round() / 1000.0);
        }
    }
    radii.sort_by(|a, b| a.total_cmp(b));
    (k, radii)
}

pub fn roundtrip(name: &str, original: &Solid, linear: f64, angular: f64) -> (Row, Document, u64) {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let m = mesh_of(original, linear, angular);
    let rep = rolidium_mesh::check::check(&m);
    assert!(
        rep.is_watertight(),
        "{name}: the test mesh is not closed: {rep:?}"
    );
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: name.into(),
            mesh: Arc::new(m.clone()),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    let t = std::time::Instant::now();
    d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: false,
        keep: false,
    })
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    let seconds = t.elapsed().as_secs_f64();
    let b = &d.bodies()[0];
    let (before, radii_before) = kinds_of(original);
    let (after, radii_after) = kinds_of(&b.solid);
    let r = rolidium_doc::meshes::conversion_report(b.id).unwrap();
    let row = Row {
        name: name.into(),
        before,
        after,
        radii_before,
        radii_after,
        volume_before: original.volume().unwrap(),
        volume_after: b.solid.volume().unwrap(),
        deviation: r.max_deviation,
        unrecognised: r.single_share,
        triangles: m.triangles.len(),
        seconds,
        route: r.route,
    };
    let bid = b.id;
    (row, d, bid)
}

pub fn print(row: &Row) {
    eprintln!(
        "{:<18} tri {:>6}  faces before {:?} after {:?}  radii {:?} -> {:?}  volume {:.3} -> {:.3}  dev {:.4}  single {:.1}%  {:.2}s route {}",
        row.name, row.triangles, row.before, row.after, row.radii_before, row.radii_after, row.volume_before, row.volume_after, row.deviation, row.unrecognised * 100.0, row.seconds, row.route
    );
}

#[test]
fn the_models_make_the_trip_there_and_back() {
    let models: Vec<(&str, Solid)> = vec![
        ("rounded box", rounded_box()),
        ("plate with holes", plate_with_holes()),
        ("boss with a cone", boss_with_cone()),
        ("bin", bin()),
    ];
    for (name, s) in &models {
        let (row, _d, _id) = roundtrip(name, s, 0.01, 0.2);
        print(&row);
    }
}

#[allow(dead_code)]
fn _unused(_: FaceId) {}

#[test]
#[ignore]
fn debug_one() {
    let which = std::env::var("MODEL").unwrap_or("rounded box".into());
    let s = match which.as_str() {
        "plate" => plate_with_holes(),
        "boss" => boss_with_cone(),
        "bin" => bin(),
        _ => rounded_box(),
    };
    let (row, _d, _id) = roundtrip(&which, &s, 0.01, 0.2);
    print(&row);
}

/// The files of a private test directory (not in the repository): the kinds of faces found, the share that was not recognised, the deviation and the time.
#[test]
#[ignore]
fn private_files() {
    let dir = std::path::PathBuf::from(
        std::env::var("STL_DIR")
            .unwrap_or_else(|_| format!("{}/rolidium-private/stl-test", std::env::var("HOME").unwrap())),
    );
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("no {dir:?}");
        return;
    };
    let mut names: Vec<_> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "stl"))
        .collect();
    names.sort();
    let max_tri: usize = std::env::var("MAX_TRI")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(150_000);
    eprintln!(
        "{:<16} {:>8} {:>5} {:>28} {:>9} {:>8} {:>8} {:>9}",
        "file", "triangles", "route", "planes/cyl/cone/sph/torus", "unrecog%", "dev mm", "vol %", "seconds"
    );
    for p in names {
        let bytes = std::fs::read(&p).unwrap();
        let mut m = rolidium_mesh::stl::parse(&bytes).unwrap();
        let tol = rolidium_mesh::check::merge_tolerance(&m);
        rolidium_mesh::check::merge_vertices(&mut m, tol);
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if m.triangles.len() > max_tri {
            // too big to convert in a test: only the recognition
            let t = std::time::Instant::now();
            let f = rolidium_mesh::fit::fit(
                &m,
                &rolidium_mesh::fit::FitOptions {
                    tolerance: 0.02,
                    round: false,
                    allow: rolidium_mesh::fit::Allow::ALL,
                },
                &|| false,
            )
            .unwrap();
            let s = &f.stats;
            eprintln!(
                "{:<16} {:>8} {:>5} {:>28} {:>9.2} {:>8.4} {:>8} {:>9.2}   (recognition only)",
                name,
                m.triangles.len(),
                "-",
                format!(
                    "{}/{}/{}/{}/{}",
                    s.planes, s.cylinders, s.cones, s.spheres, s.tori
                ),
                s.single_area * 100.0,
                s.max_deviation,
                "-",
                t.elapsed().as_secs_f64()
            );
            continue;
        }
        let mut d = Document::new();
        d.apply(Command::AddMeshes {
            meshes: vec![NewMesh {
                name: name.clone(),
                mesh: Arc::new(m.clone()),
                verified: true,
            }],
        })
        .unwrap();
        let id = d.meshes()[0].id;
        let t = std::time::Instant::now();
        let r = d.apply(Command::ConvertMesh {
            body: id,
            tolerance: 0.02,
            round: false,
            keep: false,
        });
        let secs = t.elapsed().as_secs_f64();
        match r {
            Ok(_) => {
                let b = &d.bodies()[0];
                let rep = rolidium_doc::meshes::conversion_report(b.id).unwrap();
                eprintln!(
                    "{:<16} {:>8} {:>5} {:>28} {:>9.2} {:>8.4} {:>8.3} {:>9.2}",
                    name,
                    m.triangles.len(),
                    rep.route,
                    format!(
                        "{}/{}/{}/{}/{}",
                        rep.kinds[0], rep.kinds[1], rep.kinds[2], rep.kinds[3], rep.kinds[4]
                    ),
                    rep.single_share * 100.0,
                    rep.max_deviation,
                    (rep.volume_solid - rep.volume_mesh) / rep.volume_mesh * 100.0,
                    secs
                );
            }
            Err(e) => eprintln!(
                "{:<16} {:>8}  refused: {e}  ({secs:.1}s)",
                name,
                m.triangles.len()
            ),
        }
    }
}
