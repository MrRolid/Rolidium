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

/// E1 (a private file): converted, a vertical edge rounded, then the walls of the body pulled one after another (twice, as "Up to" twice does); every refusal is printed.
/// `ROLIDIUM_STL=<file> cargo test --release -p rolidium-doc --test local_stl -- --ignored --nocapture pull_after_a_round`
#[test]
#[ignore]
fn pull_after_a_round_of_a_private_file() {
    use rolidium_doc::PullMode;
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let mut m = rolidium_mesh::stl::parse(&bytes).unwrap();
    let tol = rolidium_mesh::check::merge_tolerance(&m);
    rolidium_mesh::check::merge_vertices(&mut m, tol);
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
    d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: false,
        keep: false,
    })
    .unwrap();
    let body = d.bodies()[0].id;
    let solid = d.body(body).unwrap().solid.clone();
    println!("converted: {} faces", solid.face_count());
    // vertical straight edges between two flat faces
    let mut verticals = Vec::new();
    for e in solid.edges() {
        let i = solid.edge_info(e).unwrap();
        if i.kind == rolidium_kernel::EdgeKind::Line
            && ((i.end - i.start).normalize().z.abs() > 0.999)
            && i.length > 3.0
        {
            verticals.push((e, i.length));
        }
    }
    println!("vertical edges: {}", verticals.len());
    for (e, len) in verticals.iter().take(12) {
        let e_id = *e;
        let mut dd = d.clone();
        let r = dd.apply(Command::FilletEdges {
            body,
            edges: vec![e.0],
            size: 2.0,
            chamfer: false,
        });
        println!(
            "round of edge {} (length {len:.1}): {:?}",
            e.0,
            r.as_ref().map(|_| "ok").map_err(|e| e.to_string())
        );
        if r.is_err() {
            continue;
        }
        let solid = dd.body(body).unwrap().solid.clone();
        let faces: Vec<_> = solid
            .faces()
            .filter(|f| {
                solid
                    .face_info(*f)
                    .map(|i| i.kind == rolidium_kernel::FaceKind::Plane)
                    .unwrap_or(false)
            })
            .collect();
        for f in faces.iter().take(40) {
            let mut d3 = dd.clone();
            for pass in 0..2 {
                let cur = d3.body(body).unwrap().solid.clone();
                // the same wall again: found by its plane
                let Some(face) = cur.faces().find(|g| {
                    let (a, b) = (cur.face_info(*g), solid.face_info(*f));
                    match (a, b) {
                        (Ok(a), Ok(b)) => {
                            a.kind == b.kind
                                && a.normal.dot(b.normal) > 0.999
                                && ((a.origin - b.origin).dot(b.normal)).abs() < 0.5 + pass as f64 * 3.0
                        }
                        _ => false,
                    }
                }) else {
                    break;
                };
                let dist = [2.0, 6.0, -2.0, 12.0, -6.0][(f.0 as usize + pass) % 5];
                let r = d3.apply(Command::PullFace {
                    body,
                    face,
                    distance: dist,
                    mode: PullMode::Auto,
                });
                if let Err(e) = r {
                    println!(
                        "  round of edge {}: pull of the wall {} by {dist} pass {pass}: {e}",
                        e_id.0, f.0
                    );
                    break;
                }
            }
        }
    }
}
