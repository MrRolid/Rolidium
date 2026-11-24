// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Tries every STL file of a directory: reading, the check, the search for walls and holes, a document with the mesh saved as .rolidium and read again.
//! `cargo run --release -p rolidium-doc --example stl_dir -- <directory>` prints a table. Nothing is copied or written outside the temporary folder.
use rolidium_doc::file::{load_from_bytes, save_to_bytes};
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_mesh::{check, regions, stl};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("the directory");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("cannot read the directory")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    println!("| file | size | triangles | read | check | walls / holes | regions | .rolidium | ratio | save | open | same |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (i, p) in files.iter().enumerate() {
        let label = format!("file {}", i + 1);
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        if ext != "stl" {
            println!(
                "| {label} (.{ext}) | {:.1} MB | not an STL file: skipped | | | | | | | | | |",
                size as f64 / 1e6
            );
            continue;
        }
        let bytes = std::fs::read(p).unwrap();
        let t = Instant::now();
        let mesh = match stl::parse(&bytes) {
            Ok(m) => m,
            Err(e) => {
                println!(
                    "| {label} | {:.1} MB | {e} | | | | | | | | | |",
                    size as f64 / 1e6
                );
                continue;
            }
        };
        let read = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let rep = check::check(&mesh);
        let chk = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let reg = regions::find(&mesh, 3.0, 4);
        let regs = t.elapsed().as_secs_f64();
        let mut d = Document::new();
        d.apply(Command::AddMeshes {
            meshes: vec![NewMesh {
                name: label.clone(),
                mesh: Arc::new(mesh.clone()),
                verified: rep.is_closed() && rep.inconsistent_edges == 0,
            }],
        })
        .unwrap();
        let t = Instant::now();
        let saved = save_to_bytes(&d, None, "test").unwrap();
        let save = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let back = load_from_bytes(&saved).unwrap();
        let open = t.elapsed().as_secs_f64();
        let same = *back.state.meshes[0].mesh == mesh;
        println!(
            "| {label} | {:.1} MB | {} | {:.2} s | {:.2} s ({}) | {} / {} | {:.2} s | {:.2} MB | {:.1} % | {:.2} s | {:.2} s | {} |",
            size as f64 / 1e6,
            mesh.triangles.len(),
            read,
            chk,
            if rep.is_clean() { "clean".to_string() } else { rep.lines().join("; ") },
            reg.planes.len(),
            reg.cylinders.len(),
            regs,
            saved.len() as f64 / 1e6,
            100.0 * saved.len() as f64 / size.max(1) as f64,
            save,
            open,
            if same { "yes" } else { "NO" },
        );
    }
}
