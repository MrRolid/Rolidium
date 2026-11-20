// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Several threads give the same result as one: the same number of triangles, volumes and validity for many independent
//! bodies, and the same again in a stress run. (One test function: the number of threads is a setting of the whole process.)

use rolidium_kernel::{par, BooleanOp, DVec3, Solid};

/// A mixed set of bodies: boxes, cylinders, plates with holes, rounded blocks, a helical pipe.
fn bodies(n: usize) -> Vec<Solid> {
    (0..n)
        .map(|i| {
            let (x, y) = ((i % 8) as f64 * 70.0, (i / 8) as f64 * 70.0);
            match i % 5 {
                0 => Solid::cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 40.0, y + 30.0, 12.0)).unwrap(),
                1 => Solid::cylinder(10.0, 25.0, DVec3::new(x + 20.0, y + 20.0, 0.0), DVec3::Z).unwrap(),
                2 => {
                    let b =
                        Solid::cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 50.0, y + 50.0, 10.0)).unwrap();
                    let h =
                        Solid::cylinder(4.0, 12.0, DVec3::new(x + 25.0, y + 25.0, -1.0), DVec3::Z).unwrap();
                    b.boolean(&h, BooleanOp::Cut).unwrap().solid
                }
                3 => {
                    let b =
                        Solid::cuboid(DVec3::new(x, y, 0.0), DVec3::new(x + 30.0, y + 30.0, 20.0)).unwrap();
                    let e = b.edges().next().unwrap();
                    b.fillet(&[e], 3.0).unwrap().solid
                }
                _ => {
                    let c =
                        Solid::cylinder(12.0, 30.0, DVec3::new(x + 20.0, y + 20.0, 0.0), DVec3::Z).unwrap();
                    let s =
                        Solid::cylinder(5.0, 40.0, DVec3::new(x + 20.0, y + 20.0, -5.0), DVec3::Z).unwrap();
                    c.boolean(&s, BooleanOp::Cut).unwrap().solid
                }
            }
        })
        .collect()
}

#[derive(Debug, PartialEq)]
struct Facts {
    triangles: usize,
    faces: usize,
    volume_um3: i64,
    valid: bool,
}

fn facts(s: &Solid) -> Facts {
    let m = s.display_mesh().unwrap();
    Facts {
        triangles: m.triangle_count(),
        faces: s.face_count(),
        volume_um3: (s.volume().unwrap() * 1000.0).round() as i64,
        valid: s.is_valid(),
    }
}

#[test]
fn threads_give_the_same_results_as_one() {
    let solids = bodies(60);
    // one thread
    par::configure(1);
    assert_eq!(par::threads(), 1);
    let one: Vec<Facts> = solids.iter().map(facts).collect();
    assert!(one.iter().all(|f| f.valid && f.triangles > 0));

    // several threads: the same facts for every body, also in a stress run of fifty repetitions
    for threads in [2usize, 4, 8] {
        par::configure(threads);
        assert_eq!(par::threads(), threads);
        let many = par::map_groups(&solids, |s| s, facts);
        assert_eq!(one, many, "{threads} threads");
    }
    par::configure(4);
    for round in 0..50 {
        let many = par::map_groups(&solids, |s| s, facts);
        assert_eq!(one, many, "stress run {round}");
    }

    // bodies that share data (clones of one solid, a moved copy shares nothing) are put in one group and never meshed together
    let a = solids[0].clone();
    let b = solids[0].clone();
    let items = vec![a, solids[1].clone(), b];
    let groups = par::share_groups(&items, |s| s);
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert!(groups.iter().any(|g| g == &vec![0, 2]));
    let moved = solids[0]
        .moved(DVec3::new(500.0, 0.0, 0.0), DVec3::ZERO, DVec3::Z, 0.0)
        .unwrap()
        .solid;
    let items = vec![solids[0].clone(), moved];
    assert_eq!(
        par::share_groups(&items, |s| s).len(),
        2,
        "a moved copy shares no data"
    );

    // the variable ROLIDIUM_THREADS and the automatic number
    assert!(par::resolve(0) >= 1);
    assert_eq!(par::resolve(3), 3);
    par::configure(0);
}
