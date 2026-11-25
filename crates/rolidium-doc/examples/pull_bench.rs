// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The cost of an edit of a big mesh in the document and in its display: a pull of the top wall and its Undo on a plate with many holes (about 2 million
//! triangles). Prints the times of the command, of the display made again from nothing (what every edit used to do) and of the display as it is made now
//! (patched after the pull, found again after Undo). `cargo run --release -p rolidium-doc --example pull_bench [cells_per_side [facets]]`.
use rolidium_doc::display::{keep_regions, mesh_body_entry, mesh_inputs, MeshCache};
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_mesh::{check, gen, regions};
use std::sync::Arc;
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(32);
    let seg: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(240);
    let mesh = gen::plate_with_holes(n, n, 10.0, 4.0, 3.0, seg);
    println!("{} triangles, {} holes", mesh.triangles.len(), n * n);
    let mesh = Arc::new(mesh);
    let t = Instant::now();
    let reg = regions::find(&mesh, 3.0, 4);
    println!(
        "regions: {:.0} ms ({} walls, {} round)",
        ms(t),
        reg.planes.len(),
        reg.cylinders.len()
    );
    let top = reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap().clone();
    keep_regions(&mesh, Arc::new(reg));
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "plate".into(),
            mesh: mesh.clone(),
            verified: check::check(&mesh).is_watertight(),
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    {
        let t = Instant::now();
        let mut m2 = (*mesh).clone();
        println!("  clone of the mesh {:.0} ms", ms(t));
        let t = Instant::now();
        let plan = rolidium_mesh::edit::PullPlan::new(&m2, &top);
        println!(
            "  plan {:.0} ms ({} vertices, {} neighbours)",
            ms(t),
            plan.verts.len(),
            plan.outside.len()
        );
        let t = Instant::now();
        let r = plan.check(&m2, &top, 1.0, false);
        println!("  check {:.0} ms {:?}", ms(t), r);
        let t = Instant::now();
        let _ = rolidium_mesh::edit::pull_plane_opts(&mut m2, &top, 1.0, false, false).unwrap();
        println!("  the whole edit {:.0} ms", ms(t));
        let t = Instant::now();
        let rc = (*rolidium_doc::display::regions_of(&mesh).unwrap()).clone();
        println!(
            "  clone of the regions {:.0} ms ({} in of_triangle)",
            ms(t),
            rc.of_triangle.len()
        );
    }
    let mut cache = MeshCache::default();
    let t = Instant::now();
    let e0 = cache.mesh_entries(&mesh_inputs(d.state()));
    println!(
        "the display made from nothing: {:.0} ms ({} vertices)",
        ms(t),
        e0[0].mesh.positions.len()
    );
    for round in 0..3 {
        let t = Instant::now();
        d.apply(Command::MeshPull {
            body: id,
            wall: Arc::new(top.clone()),
            distance: 1.0,
            tilt: false,
        })
        .unwrap();
        let cmd = ms(t);
        let inputs = mesh_inputs(d.state());
        let t = Instant::now();
        let full = mesh_body_entry(&inputs[0]);
        let full_ms = ms(t);
        let t = Instant::now();
        let e1 = cache.mesh_entries(&inputs);
        let patched = ms(t);
        assert_eq!(e1[0].mesh.positions.len(), full.mesh.positions.len());
        assert_eq!(
            e1[0].mesh.positions, full.mesh.positions,
            "the patched display is the one made from nothing"
        );
        assert_eq!(e1[0].faces.len(), full.faces.len());
        let t = Instant::now();
        d.undo();
        let undo_cmd = ms(t);
        let t = Instant::now();
        let e2 = cache.mesh_entries(&mesh_inputs(d.state()));
        let undone = ms(t);
        assert_eq!(e2[0].stamp, e0[0].stamp, "Undo finds the display it had");
        println!(
            "round {round}: pull command {cmd:.0} ms; display from nothing {full_ms:.0} ms, patched {patched:.0} ms; Undo {undo_cmd:.1} ms, its display {undone:.1} ms"
        );
        let _ = e1;
        d.redo();
        let _ = cache.mesh_entries(&mesh_inputs(d.state()));
        d.undo();
        let _ = cache.mesh_entries(&mesh_inputs(d.state()));
    }
}
