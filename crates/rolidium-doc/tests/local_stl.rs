// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The conversion of the private STL files (ignored; nothing of them is in the repository). `ROLIDIUM_STL=<file> cargo test --release -p rolidium-doc --test local_stl -- --ignored --nocapture`
use rolidium_doc::{Command, Document, NewMesh};
use std::sync::Arc;
use std::time::Instant;

#[test]
#[ignore]
fn the_conversion_of_a_private_file() {
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let t = Instant::now();
    let mut m = rolidium_mesh::stl::parse(&bytes).unwrap();
    let tol = rolidium_mesh::check::merge_tolerance(&m);
    rolidium_mesh::check::merge_vertices(&mut m, tol);
    let imp = t.elapsed().as_secs_f64();
    let rep = rolidium_mesh::check::check(&m);
    eprintln!("{path}: {} triangles, import {imp:.2} s, open {} non-manifold {} inconsistent {} pieces {} degenerate {} volume {:.1}", m.triangles.len(), rep.open_edges, rep.non_manifold_edges, rep.inconsistent_edges, rep.components, rep.degenerate, m.signed_volume());
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "m".into(),
            mesh: Arc::new(m),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    let t = Instant::now();
    let r = d.apply(Command::ConvertMesh {
        body: id,
        tolerance: std::env::var("ROLIDIUM_TOL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.02),
        round: false,
        keep: false,
    });
    let secs = t.elapsed().as_secs_f64();
    match r {
        Ok(c) => {
            let b = &d.bodies()[0];
            let rep = rolidium_doc::meshes::conversion_report(b.id).unwrap();
            eprintln!("converted in {secs:.2} s: kinds {:?} single {} ({:.1} %) deviation {:.4} route {} volume {:.1} vs {:.1} {}", rep.kinds, rep.single, rep.single_share * 100.0, rep.max_deviation, rep.route, rep.volume_mesh, rep.volume_solid, c.notice.unwrap_or_default());
        }
        Err(e) => eprintln!("refused after {secs:.2} s: {e}"),
    }
}
