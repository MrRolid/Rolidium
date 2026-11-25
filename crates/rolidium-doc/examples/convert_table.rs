// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Makes a solid of every STL file of a directory and prints a table: the faces by kind, what is no flat piece, the deviation, the volume and the time.
//! `cargo run --release -p rolidium-doc --example convert_table -- <directory> [accuracy]`. Nothing is copied or written.
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_mesh::{check, stl};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("the directory");
    let tol: f64 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0.02);
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("cannot read the directory").flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    files.sort();
    println!("| file | triangles | faces (plane / cylinder / cone / sphere / torus / other) | single triangles | share of the surface | deviation | volume of the mesh | volume of the solid | difference | time |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (i, p) in files.iter().enumerate() {
        let label = if std::env::var_os("NAMES").is_some() {
            p.file_name().unwrap().to_string_lossy().to_string()
        } else {
            format!("file {}", i + 1)
        };
        if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("stl")) {
            continue;
        }
        let mesh = match stl::parse(&std::fs::read(p).unwrap()) {
            Ok(m) => m,
            Err(e) => {
                println!("| {label} | not read: {e} |");
                continue;
            }
        };
        let rep = check::check(&mesh);
        let tris = mesh.triangles.len();
        let mut d = Document::new();
        d.apply(Command::AddMeshes {
            meshes: vec![NewMesh {
                name: "m".into(),
                mesh: Arc::new(mesh),
                verified: rep.is_watertight(),
            }],
        })
        .unwrap();
        let id = d.meshes()[0].id;
        let t = Instant::now();
        let r = d.apply(Command::ConvertMesh {
            body: id,
            tolerance: tol,
            round: true,
            keep: false,
        });
        let secs = t.elapsed().as_secs_f64();
        match r {
            Ok(_) => {
                let b = &d.bodies()[0];
                let rep = rolidium_doc::meshes::conversion_report(b.id).unwrap();
                let k = rep.kinds;
                let dv = (rep.volume_solid - rep.volume_mesh) / rep.volume_mesh.abs().max(1e-9) * 100.0;
                println!(
                    "| {label} | {tris} | {} / {} / {} / {} / {} / {} | {} | {:.1} % | {:.4} mm | {:.1} | {:.1} | {dv:+.3} % | {secs:.2} s |",
                    k[0], k[1], k[2], k[3], k[4], k[5], rep.single, rep.single_share * 100.0, rep.max_deviation, rep.volume_mesh, rep.volume_solid
                );
            }
            Err(e) => println!("| {label} | {tris} | not made: {e} | | | | | | | {secs:.2} s |"),
        }
    }
}
