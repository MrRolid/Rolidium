// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Several threads give the same result as one: the same number of triangles, volumes and validity for many independent bodies, and the
//! same again in a stress run. The number of threads is decided once per process, so every number is run in a process of its own (this test
//! starts the helper test below with `ROLIDIUM_THREADS` set and compares what it prints).

use rolidium_kernel::{par, BooleanOp, DVec3, Solid};
use std::process::Command;

/// A mixed set of bodies: boxes, cylinders, plates with holes, rounded blocks.
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

fn facts(s: &Solid) -> String {
    let m = s.display_mesh().unwrap();
    format!(
        "{}/{}/{}/{}",
        m.triangle_count(),
        s.face_count(),
        (s.volume().unwrap() * 1000.0).round() as i64,
        s.is_valid()
    )
}

/// Run by the test below in a process with `ROLIDIUM_THREADS` set: prints the facts of every body, then repeats the parallel run 50 times.
#[test]
#[ignore = "helper of threads_give_the_same_results_as_one"]
fn helper_print_facts() {
    let solids = bodies(60);
    let first = par::map_groups(&solids, |s| s, facts);
    for round in 0..50 {
        assert_eq!(
            first,
            par::map_groups(&solids, |s| s, facts),
            "stress run {round}"
        );
    }
    println!("THREADS {}", par::threads());
    println!("FACTS {}", first.join(" "));
}

fn run_with(threads: usize) -> (usize, String) {
    let exe = std::env::current_exe().unwrap();
    let out = Command::new(exe)
        .args([
            "--ignored",
            "--exact",
            "helper_print_facts",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("ROLIDIUM_THREADS", threads.to_string())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "{threads} threads: {text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pick = |key: &str| {
        text.lines()
            .find_map(|l| l.split_once(key).map(|(_, v)| v))
            .unwrap()
            .trim()
            .to_string()
    };
    (pick("THREADS ").parse().unwrap(), pick("FACTS "))
}

#[test]
fn threads_give_the_same_results_as_one() {
    let (n1, one) = run_with(1);
    assert_eq!(n1, 1);
    assert!(one.contains("true") && !one.contains("false"));
    for threads in [2usize, 4, 8] {
        let (n, many) = run_with(threads);
        assert_eq!(n, threads);
        assert_eq!(one, many, "{threads} threads");
    }
}

#[test]
fn bodies_that_share_data_are_grouped_and_the_number_is_fixed() {
    let solids = bodies(5);
    let items = vec![solids[0].clone(), solids[1].clone(), solids[0].clone()];
    let groups = par::share_groups(&items, |s| s);
    assert_eq!(groups.len(), 2, "{groups:?}");
    assert!(groups.iter().any(|g| g == &vec![0, 2]));
    let moved = solids[0]
        .moved(DVec3::new(500.0, 0.0, 0.0), DVec3::ZERO, DVec3::Z, 0.0)
        .unwrap()
        .solid;
    assert_eq!(
        par::share_groups(&[solids[0].clone(), moved], |s| s).len(),
        2,
        "a moved copy shares no data"
    );
    // the number is decided once: a later call changes nothing
    let n = par::threads();
    assert_eq!(par::configure(n + 5), n);
    assert_eq!(par::resolve(3), 3);
    assert!(par::resolve(0) >= 1);
}
