// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Rounds and chamfers recognised from the geometry: a face next to them is moved with them, their size can be changed
//! (also for three and more edges), also for a body that was read from a STEP file.
use rolidium_kernel::*;

const W: f64 = 40.0;
const D: f64 = 30.0;
const H: f64 = 20.0;

fn cuboid(h: f64) -> Solid {
    Solid::cuboid(DVec3::ZERO, DVec3::new(W, D, h)).unwrap()
}
fn bx() -> Solid {
    cuboid(H)
}
fn plane_face(s: &Solid, n: DVec3) -> FaceId {
    s.faces()
        .filter(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.dot(n) > 0.99
        })
        .max_by(|a, b| {
            let (ia, ib) = (s.face_info(*a).unwrap(), s.face_info(*b).unwrap());
            n.dot(ia.origin).total_cmp(&n.dot(ib.origin))
        })
        .unwrap()
}
fn top(s: &Solid) -> FaceId {
    plane_face(s, DVec3::Z)
}
fn top_edges(s: &Solid) -> Vec<EdgeId> {
    let z = s.bounding_box().unwrap().1.z;
    s.edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.start.z > z - 1e-6 && i.end.z > z - 1e-6
        })
        .collect()
}
fn corner_edges(s: &Solid) -> Vec<EdgeId> {
    s.edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.start.length() < 1e-6 || i.end.length() < 1e-6
        })
        .collect()
}
fn dims(s: &Solid) -> (f64, f64, f64) {
    let (a, b) = s.bounding_box().unwrap();
    (b.x - a.x, b.y - a.y, b.z - a.z)
}
/// The size of every face of a round or chamfer (a chamfer made of several strips is one rounding).
fn sizes(s: &Solid, chamfer: bool) -> Vec<f64> {
    let mut v: Vec<f64> = s
        .blend_map()
        .components
        .iter()
        .flat_map(|c| c.roundings.iter())
        .filter(|r| r.chamfer == chamfer)
        .flat_map(|r| r.faces.iter().map(move |_| r.size))
        .collect();
    v.sort_by(f64::total_cmp);
    v
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}
fn round(s: &Solid, edges: &[EdgeId], r: f64, chamfer: bool) -> Solid {
    if chamfer {
        s.chamfer(edges, r).unwrap().solid
    } else {
        s.fillet(edges, r).unwrap().solid
    }
}

#[test]
fn pulling_the_top_face_of_a_box_with_four_rounds_moves_the_rounds_with_it() {
    for chamfer in [false, true] {
        let s = bx();
        let r = round(&s, &top_edges(&s), 3.0, chamfer);
        let faces = r.face_count();
        assert_eq!(sizes(&r, chamfer).len(), 4, "recognised from the geometry");
        let p = r.pull_face(top(&r), 5.0).unwrap().solid;
        assert!(p.is_valid());
        let (x, y, z) = dims(&p);
        close(x, W, 1e-6);
        close(y, D, 1e-6);
        close(z, H + 5.0, 1e-6);
        assert_eq!(p.face_count(), faces, "no prism, no extra faces");
        assert_eq!(sizes(&p, chamfer), vec![3.0; 4], "chamfer={chamfer}");
        // the same as rounding the taller box
        let tall = cuboid(H + 5.0);
        let reference = round(&tall, &top_edges(&tall), 3.0, chamfer);
        close(p.volume().unwrap(), reference.volume().unwrap(), 1e-6);
        // and back (a negative move)
        let back = p.pull_face(top(&p), -5.0).unwrap().solid;
        close(back.volume().unwrap(), r.volume().unwrap(), 1e-6);
        assert_eq!(back.face_count(), faces);
    }
}

#[test]
fn all_twelve_edges_rounded_a_face_can_be_pulled_or_moved_in_every_direction() {
    for chamfer in [false, true] {
        let s = bx();
        let all: Vec<EdgeId> = s.edges().collect();
        let r = round(&s, &all, 2.0, chamfer);
        let faces = r.face_count();
        for (n, grow_axis, what) in [
            (DVec3::Z, 2, "top"),
            (DVec3::X, 0, "side"),
            (DVec3::Y, 1, "front"),
        ] {
            for dist in [5.0, -5.0] {
                let p = r
                    .pull_face(plane_face(&r, n), dist)
                    .unwrap_or_else(|e| panic!("{what} {dist}: {e}"));
                assert!(p.solid.is_valid(), "{what} {dist}");
                let (x, y, z) = dims(&p.solid);
                let mut want = [W, D, H];
                want[grow_axis] += dist;
                close(x, want[0], 1e-6);
                close(y, want[1], 1e-6);
                close(z, want[2], 1e-6);
                assert_eq!(
                    p.solid.face_count(),
                    faces,
                    "{what} {dist} chamfer={chamfer}: same number of faces"
                );
                assert_eq!(sizes(&p.solid, chamfer).len(), 12, "{what} {dist}");
                assert!(sizes(&p.solid, chamfer).iter().all(|v| (v - 2.0).abs() < 1e-6));
                let nb = cuboid(1.0);
                let _ = nb;
                let new_box = Solid::cuboid(DVec3::ZERO, DVec3::new(want[0], want[1], want[2])).unwrap();
                let all2: Vec<EdgeId> = new_box.edges().collect();
                let reference = round(&new_box, &all2, 2.0, chamfer);
                close(p.solid.volume().unwrap(), reference.volume().unwrap(), 1e-5);
            }
        }
    }
}

#[test]
fn a_move_that_is_too_large_for_the_rounds_changes_nothing_and_says_why() {
    let s = bx();
    let r = s.fillet(&top_edges(&s), 3.0).unwrap().solid;
    let v = r.volume().unwrap();
    let e = r.pull_face(top(&r), -19.0).expect_err("too far");
    eprintln!("{e}");
    assert!(!e.to_string().is_empty());
    close(r.volume().unwrap(), v, 1e-9);
}

#[test]
fn rounds_that_came_from_a_step_file_are_recognised_and_moved_too() {
    let s = bx();
    let r = s.fillet(&top_edges(&s), 3.0).unwrap().solid;
    let back = Solid::read_step(&r.write_step().unwrap()).unwrap();
    assert_eq!(sizes(&back, false), vec![3.0; 4]);
    let p = back.pull_face(top(&back), 5.0).unwrap().solid;
    close(dims(&p).2, H + 5.0, 1e-6);
    assert_eq!(sizes(&p, false), vec![3.0; 4]);
    assert_eq!(p.face_count(), back.face_count());
}

fn volume_of_reference(edges_of: &dyn Fn(&Solid) -> Vec<EdgeId>, r: f64, chamfer: bool) -> f64 {
    let s = bx();
    let e = edges_of(&s);
    if r == 0.0 {
        return s.volume().unwrap();
    }
    round(&s, &e, r, chamfer).volume().unwrap()
}

#[test]
fn the_size_of_rounds_changes_for_any_number_of_edges_and_zero_makes_the_edges_sharp() {
    type Pick = Box<dyn Fn(&Solid) -> Vec<EdgeId>>;
    let picks: Vec<(&str, Pick)> = vec![
        ("one edge", Box::new(|s| s.edges().take(1).collect())),
        ("two edges", Box::new(|s| s.edges().take(2).collect())),
        ("three edges in a corner", Box::new(corner_edges)),
        ("four edges around the top", Box::new(top_edges)),
        ("twelve edges", Box::new(|s| s.edges().collect())),
    ];
    for chamfer in [false, true] {
        for (name, pick) in &picks {
            let s = bx();
            let start = round(&s, &pick(&s), 3.0, chamfer);
            let any = start.blend_map().components[0].roundings[0].faces[0];
            for (new, want_chain) in [(2.0, true), (4.0, true), (2.0, false)] {
                if !want_chain && sizes(&start, chamfer).len() == 1 {
                    continue;
                }
                let e = start
                    .resize_blend(
                        any,
                        new,
                        want_chain,
                        if chamfer { Some((45.0, false)) } else { None },
                    )
                    .unwrap_or_else(|e| panic!("{name} chamfer={chamfer} -> {new}: {e}"));
                assert!(e.solid.is_valid(), "{name}");
                if want_chain {
                    let sz = sizes(&e.solid, chamfer);
                    assert!(
                        sz.iter().all(|v| (v - new).abs() < 1e-6),
                        "{name} chamfer={chamfer}: {sz:?}"
                    );
                    close(
                        e.solid.volume().unwrap(),
                        volume_of_reference(&pick, new, chamfer),
                        1e-6,
                    );
                }
            }
            // back to zero: sharp edges, the original number of faces
            let sharp = start
                .resize_blend(any, 0.0, true, None)
                .unwrap_or_else(|e| panic!("{name} chamfer={chamfer} -> 0: {e}"));
            assert_eq!(sharp.solid.face_count(), 6, "{name} chamfer={chamfer}");
            close(sharp.solid.volume().unwrap(), H * W * D, 1e-6);
        }
    }
}

#[test]
fn a_single_round_among_equal_ones_changes_alone_and_the_chain_option_changes_all() {
    let s = bx();
    let r = s.fillet(&top_edges(&s), 3.0).unwrap().solid;
    let f = r.blend_map().components[0].roundings[0].faces[0];
    let one = r.resize_blend(f, 2.0, false, None);
    match one {
        Ok(e) => {
            let sz = sizes(&e.solid, false);
            assert_eq!(sz.len(), 4);
            assert_eq!(
                sz.iter().filter(|v| (**v - 2.0).abs() < 1e-6).count(),
                1,
                "{sz:?}"
            );
            assert_eq!(
                sz.iter().filter(|v| (**v - 3.0).abs() < 1e-6).count(),
                3,
                "{sz:?}"
            );
        }
        Err(e) => panic!("one round of four: {e}"),
    }
    let all = r.resize_blend(f, 2.0, true, None).unwrap();
    assert_eq!(sizes(&all.solid, false), vec![2.0; 4]);
}

fn polygon(points: &[[f64; 2]], h: f64) -> Solid {
    let n = points.len();
    let seg: Vec<ProfileSegment> = (0..n)
        .map(|i| ProfileSegment::Line {
            a: points[i],
            b: points[(i + 1) % n],
        })
        .collect();
    Solid::extrude_profile(&Profile { loops: vec![seg] }, DVec3::ZERO, DVec3::Z, DVec3::X, h).unwrap()
}

#[test]
fn faces_that_only_look_like_rounds_or_chamfers_are_not_taken_for_them() {
    // a hexagonal prism: its sides are planar strips between two faces at 60 degrees, but as wide as their neighbours
    let hex: Vec<[f64; 2]> = (0..6)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 6.0;
            [10.0 * a.cos(), 10.0 * a.sin()]
        })
        .collect();
    assert!(
        polygon(&hex, 30.0).blend_map().components.is_empty(),
        "hexagonal prism"
    );
    // a thin plate: its sides are narrow, but they meet the big faces at 90 degrees
    assert!(cuboid(1.0).blend_map().components.is_empty(), "plate");
    // a hole and a pin are cylinders without tangent neighbours
    let pin = Solid::cylinder(5.0, 20.0, DVec3::ZERO, DVec3::Z).unwrap();
    assert!(pin.blend_map().components.is_empty(), "pin");
    // the end of a slot is a half cylinder between two parallel planes: not a round of an edge
    let slot: Vec<[f64; 2]> = vec![[0.0, 0.0], [30.0, 0.0], [30.0, 10.0], [0.0, 10.0]];
    assert!(polygon(&slot, 5.0).blend_map().components.is_empty(), "block");
}

#[test]
fn a_chamfer_is_moved_with_its_face_and_a_chamfer_from_a_step_file_is_found() {
    let s = bx();
    let c = s.chamfer(&top_edges(&s), 3.0).unwrap().solid;
    let back = Solid::read_step(&c.write_step().unwrap()).unwrap();
    assert_eq!(sizes(&back, true).len(), 4);
    let p = back.pull_face(top(&back), 5.0).unwrap().solid;
    assert!(p.is_valid());
    close(dims(&p).2, H + 5.0, 1e-6);
    assert_eq!(p.face_count(), back.face_count());
    let tall = cuboid(H + 5.0);
    let reference = tall.chamfer(&top_edges(&tall), 3.0).unwrap().solid;
    close(p.volume().unwrap(), reference.volume().unwrap(), 1e-6);
    // too large for the chamfer: nothing changes, the message says why
    let e = back.pull_face(top(&back), -19.0).expect_err("too far");
    eprintln!("{e}");
    assert!(!e.to_string().is_empty());
}

#[test]
fn the_time_of_a_pull_with_rounds_is_reported() {
    let s = bx();
    let r = s.fillet(&top_edges(&s), 3.0).unwrap().solid;
    let t = std::time::Instant::now();
    let n = 10;
    for _ in 0..n {
        let _ = r.pull_face(top(&r), 5.0).unwrap();
    }
    let per = t.elapsed().as_secs_f64() * 1e3 / n as f64;
    let plain = {
        let t = std::time::Instant::now();
        for _ in 0..n {
            let _ = s.pull_face(top(&s), 5.0).unwrap();
        }
        t.elapsed().as_secs_f64() * 1e3 / n as f64
    };
    println!("pull of the top face of a box with four rounds: {per:.1} ms (the same box without rounds: {plain:.1} ms)");
    assert!(per < 2000.0);
}

#[test]
fn where_the_time_of_a_pull_with_rounds_goes() {
    use std::time::Instant;
    let s = bx();
    let r = s.fillet(&top_edges(&s), 3.0).unwrap().solid;
    let t = Instant::now();
    let m = r.blend_map();
    let map_ms = t.elapsed().as_secs_f64() * 1e3;
    let ids: Vec<u32> = m.components.iter().flat_map(|c| c.faces.iter().map(|f| f.0)).collect();
    let _ = ids;
    let t = Instant::now();
    let p = r.pull_face(top(&r), 5.0).unwrap();
    let pull_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let _ = p.solid.blend_map();
    let map2_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let _ = s.pull_face(top(&s), 5.0).unwrap();
    let plain_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let _ = s.fillet(&top_edges(&s), 3.0).unwrap();
    let fillet_ms = t.elapsed().as_secs_f64() * 1e3;
    println!("blend map {map_ms:.1} ms; pull with rounds {pull_ms:.1} ms (the map of its result {map2_ms:.1} ms); pull without rounds {plain_ms:.1} ms; four rounds {fillet_ms:.1} ms");
    println!("{}", rolidium_kernel::timing::describe(&rolidium_kernel::timing::take()));
}
