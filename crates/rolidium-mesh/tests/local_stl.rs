// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Timing on the private STL files (ignored; nothing of them is in the repository): ROLIDIUM_STL=<file> cargo test --release ... -- --ignored --nocapture
use rolidium_mesh::{check, fit, regions, stl};
use std::time::Instant;

fn load(path: &str) -> Option<rolidium_mesh::TriMesh> {
    let bytes = std::fs::read(path).ok()?;
    let t = Instant::now();
    let mut m = stl::parse(&bytes).ok()?;
    let tol = check::merge_tolerance(&m);
    check::merge_vertices(&mut m, tol);
    eprintln!(
        "{path}: import {:.2} s, {} triangles",
        t.elapsed().as_secs_f64(),
        m.triangles.len()
    );
    Some(m)
}

#[test]
#[ignore]
fn timing_of_the_search_after_the_import_and_of_the_full_recognition() {
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let Some(m) = load(&path) else { return };
    let t = Instant::now();
    let r = regions::find_with(&m, 3.0, 4, &mut |_| true).unwrap();
    eprintln!(
        "quick search (walls and holes): {:.2} s, {} planes {} cylinders",
        t.elapsed().as_secs_f64(),
        r.planes.len(),
        r.cylinders.len()
    );
    let t = Instant::now();
    let f = fit::fit(
        &m,
        &fit::FitOptions {
            tolerance: 0.02,
            round: false,
            allow: fit::Allow::ALL,
        },
        &|| false,
    );
    match f {
        Some(f) => eprintln!(
            "full recognition: {:.2} s, organic {} {:?}",
            t.elapsed().as_secs_f64(),
            f.stats.organic,
            f.stats
        ),
        None => eprintln!("full recognition: cancelled"),
    }
}

#[test]
#[ignore]
fn which_edges_of_the_flat_faces_are_not_shared() {
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let Some(m) = load(&path) else { return };
    for merge_from in [2usize, 4] {
        let pf = rolidium_mesh::convert::polyfaces(&m, 0.02, false, merge_from, &|| false).unwrap();
        let mut count: std::collections::HashMap<(u32, u32), Vec<usize>> = std::collections::HashMap::new();
        for (fi, f) in pf.faces.iter().enumerate() {
            for l in f {
                for k in 0..l.len() {
                    let (a, b) = (l[k], l[(k + 1) % l.len()]);
                    count.entry((a.min(b), a.max(b))).or_default().push(fi);
                }
            }
        }
        let bad: Vec<_> = count
            .iter()
            .filter(|(_, v)| v.len() != 2 || v[0] == v[1])
            .collect();
        eprintln!(
            "merge from {merge_from}: {} faces, {} edges not shared by exactly two faces",
            pf.faces.len(),
            bad.len()
        );
        for (e, v) in bad.iter().take(6) {
            eprintln!(
                "  edge {:?} in faces {:?} (face sizes {:?} tris, loops {:?})",
                e,
                v,
                v.iter().map(|f| pf.triangles[*f]).collect::<Vec<_>>(),
                v.iter()
                    .map(|f| pf.faces[*f].iter().map(|l| l.len()).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
#[ignore]
fn the_non_manifold_edges() {
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let Some(m) = load(&path) else { return };
    let rep = check::check(&m);
    eprintln!("{rep:?}");
    let mut edges: std::collections::HashMap<(u32, u32), Vec<usize>> = std::collections::HashMap::new();
    for (t, tri) in m.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(t);
        }
    }
    for ((a, b), ts) in edges.iter().filter(|(_, v)| v.len() > 2) {
        let (pa, pb) = (m.positions[*a as usize], m.positions[*b as usize]);
        let dirs: Vec<i32> = ts
            .iter()
            .map(|t| {
                let tri = m.triangles[*t];
                (0..3)
                    .map(|k| {
                        if tri[k] == *a && tri[(k + 1) % 3] == *b {
                            1
                        } else if tri[k] == *b && tri[(k + 1) % 3] == *a {
                            -1
                        } else {
                            0
                        }
                    })
                    .sum()
            })
            .collect();
        eprintln!(
            "edge {:?}-{:?} triangles {} directions {:?}",
            pa,
            pb,
            ts.len(),
            dirs
        );
    }
}

#[test]
#[ignore]
fn pinches_are_separated() {
    let Ok(path) = std::env::var("ROLIDIUM_STL") else {
        return;
    };
    let Some(mut m) = load(&path) else { return };
    let n = check::separate_pinches(&mut m);
    let rep = check::check(&m);
    eprintln!("separated {n}: {rep:?}");
}
