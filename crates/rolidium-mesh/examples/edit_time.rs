// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The time of the stages of an edit of a plate with a finely divided hole: `edit_time <facets>`.
use rolidium_mesh::{edit, gen, regions};
use std::time::Instant;
fn main() {
    let seg: usize = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(12_000);
    let m0 = gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, seg);
    let mut m = m0.clone();
    println!("{} triangles", m.triangles.len());
    let t = Instant::now();
    let reg = regions::find(&m, 3.0, 64);
    println!("regions {:?}", t.elapsed());
    let top = reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap();
    let t = Instant::now();
    let plan = edit::PullPlan::new(&m, top);
    println!("plan {:?}", t.elapsed());
    let t = Instant::now();
    let r = plan.check(&m, top, 5.0, false);
    println!("check {:?} {:?}", t.elapsed(), r);
    let t = Instant::now();
    let u = edit::pull_plane_with(&mut m, top, 5.0, false).unwrap();
    println!("pull {:?}", t.elapsed());
    u.apply(&mut m);
    if let Some(h) = reg.cylinders.iter().find(|c| c.is_hole) {
        let t = Instant::now();
        let r = edit::set_diameter(&mut m, h, 6.0);
        println!("diameter {:?} {:?}", t.elapsed(), r.map(|u| u.moved.len()));
    }
}
