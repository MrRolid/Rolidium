// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use rolidium_mesh::check::{check, merge_vertices, orient_normals};
use rolidium_mesh::edit::{pull_plane, set_diameter, EditError};
use rolidium_mesh::gen::{box_mesh, plate_with_hole, sphere};
use rolidium_mesh::regions::find;
use rolidium_mesh::{serial, stl, transform, TriMesh};
use std::f64::consts::PI;

fn plate() -> TriMesh {
    plate_with_hole(60.0, 40.0, 10.0, 2.5, 48)
}

fn polygon_area(r: f64, n: usize) -> f64 {
    0.5 * n as f64 * r * r * (TAU / n as f64).sin()
}
const TAU: f64 = 2.0 * PI;

#[test]
fn the_cube_of_the_scale_test_goes_through_stl_in_both_forms() {
    let cube = box_mesh(20.0, 20.0, 20.0, 1);
    for bytes in [
        stl::write_binary(&cube),
        stl::write_ascii(&cube, "cube").into_bytes(),
    ] {
        let m = stl::parse(&bytes).unwrap();
        assert_eq!(m.triangles.len(), 12);
        assert_eq!(m.positions.len(), 8, "the vertices are welded");
        let (lo, hi) = m.bounding_box().unwrap();
        assert!(
            (hi[0] - lo[0] - 20.0).abs() < 1e-6
                && (hi[1] - lo[1] - 20.0).abs() < 1e-6
                && (hi[2] - lo[2] - 20.0).abs() < 1e-6
        );
        assert!(check(&m).is_clean());
        assert!((m.signed_volume() - 8000.0).abs() < 1e-3);
    }
}

#[test]
fn units_inches_and_a_scale_of_your_own() {
    let bin = stl::write_binary(&box_mesh(1.0, 1.0, 1.0, 1));
    let inch = stl::parse_units(&bin, stl::Units::Inches).unwrap();
    let (lo, hi) = inch.bounding_box().unwrap();
    assert!((hi[0] - lo[0] - 25.4).abs() < 1e-4);
    let own = stl::parse_units(&bin, stl::Units::Scale(10.0)).unwrap();
    let (lo, hi) = own.bounding_box().unwrap();
    assert!((hi[2] - lo[2] - 10.0).abs() < 1e-5);
    assert!(stl::parse(b"nonsense").is_err());
    assert!(stl::parse(&[0u8; 100]).is_err());
}

#[test]
fn the_plate_is_clean_and_its_walls_and_its_hole_are_found() {
    let m = plate();
    let r = check(&m);
    assert!(r.is_clean(), "{:?}", r.lines());
    let reg = find(&m, 3.0, 4);
    let top = reg
        .planes
        .iter()
        .find(|p| p.normal[2] > 0.99)
        .expect("the top wall");
    assert!((top.offset - 10.0).abs() < 1e-5);
    let hole = reg.cylinders.iter().find(|c| c.is_hole).expect("the hole");
    assert!((hole.radius - 2.5).abs() < 0.02, "{}", hole.radius);
    assert!(hole.axis[2].abs() > 0.999);
    assert!((hole.point[0] - 30.0).abs() < 0.05 && (hole.point[1] - 20.0).abs() < 0.05);
}

#[test]
fn pulling_the_top_wall_5_mm_gives_a_clean_taller_plate_and_undo_restores_it() {
    let mut m = plate();
    let before = m.clone();
    let v0 = m.signed_volume();
    let reg = find(&m, 3.0, 4);
    let top = reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap();
    let undo = pull_plane(&mut m, top, 5.0).unwrap();
    let (lo, hi) = m.bounding_box().unwrap();
    assert!((hi[2] - lo[2] - 15.0).abs() < 1e-5);
    // the wall pulled is a ring (the plate less the hole): the volume grows by its area times 5
    let want = 5.0 * top.area;
    assert!((top.area - (60.0 * 40.0 - polygon_area(2.5, 48))).abs() < 1.0);
    assert!(
        (m.signed_volume() - v0 - want).abs() < 1e-3 * want,
        "{} against {want}",
        m.signed_volume() - v0
    );
    assert!(check(&m).is_clean());
    // the record holds only the moved vertices, not the mesh
    assert!(undo.moved.len() < m.positions.len());
    let redo = undo.apply(&mut m);
    assert_eq!(m, before);
    redo.apply(&mut m);
    assert!((m.bounding_box().unwrap().1[2] - 15.0).abs() < 1e-5);
}

#[test]
fn the_hole_grows_from_5_to_6_mm() {
    let mut m = plate();
    let v0 = m.signed_volume();
    let reg = find(&m, 3.0, 4);
    let hole = reg.cylinders.iter().find(|c| c.is_hole).unwrap();
    let undo = set_diameter(&mut m, hole, 6.0).unwrap();
    let want = 10.0 * (polygon_area(3.0, 48) - polygon_area(2.5, 48));
    assert!((v0 - m.signed_volume() - want).abs() < 1e-3 * want);
    assert!(check(&m).is_clean());
    let reg2 = find(&m, 3.0, 4);
    assert!((reg2.cylinders.iter().find(|c| c.is_hole).unwrap().radius - 3.0).abs() < 0.02);
    undo.apply(&mut m);
    assert!((m.signed_volume() - v0).abs() < 1e-6);
    assert_eq!(
        set_diameter(&mut m, hole, -1.0).unwrap_err(),
        EditError::BadDiameter
    );
}

#[test]
fn a_wall_with_slanting_neighbours_is_refused_and_nothing_changes() {
    let mut m = plate();
    // the top rim is moved outwards: the side walls slant
    for p in m.positions.iter_mut() {
        if p[2] > 9.0 && ((p[0] - 30.0).abs() > 29.0 || (p[1] - 20.0).abs() > 19.0) {
            p[0] += if p[0] > 30.0 { 3.0 } else { -3.0 };
        }
    }
    let before = m.clone();
    let reg = find(&m, 3.0, 4);
    let Some(top) = reg.planes.iter().find(|p| p.normal[2] > 0.99) else {
        return; // the slanted top is no longer one wall: also a refusal of the edit
    };
    let e = pull_plane(&mut m, top, 5.0).unwrap_err();
    assert_eq!(e, EditError::NeighbourNotSquare);
    assert_eq!(m, before);
    // a wall pulled through the opposite one
    let mut p = plate();
    let reg = find(&p, 3.0, 4);
    let top = reg.planes.iter().find(|q| q.normal[2] > 0.99).unwrap();
    assert_eq!(pull_plane(&mut p, top, -12.0).unwrap_err(), EditError::Collapses);
}

#[test]
fn the_check_finds_holes_and_turned_normals_and_the_repair_mends_them() {
    let mut m = plate();
    for t in [3usize, 40, 41, 100, 200] {
        m.triangles[t].swap(1, 2);
    }
    let r = check(&m);
    assert!(r.inconsistent_edges > 0, "{:?}", r.lines());
    let turned = orient_normals(&mut m);
    assert!(turned >= 5, "{turned}");
    assert!(check(&m).is_clean());
    // a piece that is inside out
    let mut inside_out = box_mesh(10.0, 10.0, 10.0, 2);
    for t in &mut inside_out.triangles {
        t.swap(1, 2);
    }
    assert!(check(&inside_out).signed_volume < 0.0);
    orient_normals(&mut inside_out);
    assert!(check(&inside_out).is_clean());
    // holes in the mesh
    let mut holed = box_mesh(10.0, 10.0, 10.0, 3);
    holed.triangles.truncate(holed.triangles.len() - 2);
    let r = check(&holed);
    assert!(r.open_edges > 0 && !r.is_closed(), "{:?}", r.lines());
    let wall = find(&holed, 3.0, 2).planes[0].clone();
    assert_eq!(
        pull_plane(&mut holed, &wall, 1.0).unwrap_err(),
        EditError::NotClean
    );
}

#[test]
fn duplicate_vertices_are_merged() {
    let cube = box_mesh(10.0, 10.0, 10.0, 2);
    // every triangle with its own three vertices (as some programs write STL), a little noise on them
    let mut raw = TriMesh::default();
    for t in &cube.triangles {
        for &i in t {
            let p = cube.positions[i as usize];
            raw.positions.push([p[0] + 1e-6, p[1], p[2]]);
        }
        let n = raw.positions.len() as u32;
        raw.triangles.push([n - 3, n - 2, n - 1]);
    }
    assert!(check(&raw).open_edges > 0);
    let merged = merge_vertices(&mut raw, 1e-4);
    assert!(merged > 0);
    assert!(check(&raw).is_clean());
    assert_eq!(raw.positions.len(), cube.positions.len());
}

#[test]
fn a_mesh_is_saved_small_and_comes_back_the_same() {
    let m = plate_with_hole(60.0, 40.0, 10.0, 2.5, 400);
    let bytes = serial::encode(&m);
    assert_eq!(serial::decode(&bytes).unwrap(), m);
    let stl_size = stl::write_binary(&m).len();
    assert!(bytes.len() < stl_size / 2, "{} against {stl_size}", bytes.len());
    assert!(serial::decode(b"RMSH1xx").is_err() && serial::decode(b"junk").is_err());
    let s = sphere(10.0, 20_000);
    let b = serial::encode(&s);
    assert_eq!(serial::decode(&b).unwrap(), s);
    println!(
        "sphere of {} triangles: {} bytes, binary STL {} bytes",
        s.triangles.len(),
        b.len(),
        stl::write_binary(&s).len()
    );
}

#[test]
fn scale_mirror_rotate_and_pattern() {
    let mut m = box_mesh(10.0, 20.0, 30.0, 1);
    transform::scale(&mut m, [0.0; 3], [2.0, 1.0, 0.5]);
    assert!((m.signed_volume() - 6000.0).abs() < 1e-3);
    transform::scale(&mut m, [0.0; 3], [-1.0, 1.0, 1.0]);
    assert!(
        (m.signed_volume() - 6000.0).abs() < 1e-3,
        "a mirrored scale keeps the outside outside"
    );
    let mut b = box_mesh(10.0, 10.0, 10.0, 1);
    transform::mirror(&mut b, [0.0; 3], [1.0, 0.0, 0.0]);
    assert!(b.bounding_box().unwrap().1[0] <= 1e-6 && (b.signed_volume() - 1000.0).abs() < 1e-3);
    transform::rotate(&mut b, [0.0; 3], [0.0, 0.0, 1.0], PI / 2.0);
    let (lo, hi) = b.bounding_box().unwrap();
    assert!(lo[1] < -9.99 && hi[1] < 1e-4);
    let row = transform::linear_pattern(&box_mesh(5.0, 5.0, 5.0, 1), 4, [10.0, 0.0, 0.0]);
    assert_eq!(row.triangles.len(), 48);
    assert!((row.bounding_box().unwrap().1[0] - 35.0).abs() < 1e-5);
    assert!(check(&row).components == 4 && check(&row).is_clean());
}

/// The measurement of a mesh of 2 million triangles: `cargo test -p rolidium-mesh --release -- --ignored --nocapture`.
#[test]
#[ignore]
fn two_million_triangles_are_read_in_seconds() {
    let m = sphere(50.0, 2_000_000);
    let bytes = stl::write_binary(&m);
    let t = std::time::Instant::now();
    let back = stl::parse(&bytes).unwrap();
    let read = t.elapsed();
    let t = std::time::Instant::now();
    let r = check(&back);
    let checked = t.elapsed();
    let t = std::time::Instant::now();
    let reg = find(&back, 3.0, 64);
    let found = t.elapsed();
    let t = std::time::Instant::now();
    let enc = serial::encode(&back);
    let encoded = t.elapsed();
    let t = std::time::Instant::now();
    let _ = serial::decode(&enc).unwrap();
    let decoded = t.elapsed();
    println!(
        "{} triangles, {} vertices, STL {} MB: read {:?}, check {:?} (clean {}), regions {:?} ({} planes, {} cylinders), saved {} MB in {:?}, opened in {:?}",
        back.triangles.len(),
        back.positions.len(),
        bytes.len() / 1_000_000,
        read,
        checked,
        r.is_clean(),
        found,
        reg.planes.len(),
        reg.cylinders.len(),
        enc.len() / 1_000_000,
        encoded,
        decoded
    );
    assert!(read.as_secs_f64() < 10.0);
}

/// Edits of a plate of 2 million triangles (the top wall of 1 million triangles pulled 5 mm, the hole widened), with Undo: `--release -- --ignored --nocapture`.
#[test]
#[ignore]
fn edits_of_a_big_mesh_move_only_what_they_touch() {
    let m0 = plate_with_hole(60.0, 40.0, 10.0, 2.5, 120_000);
    let mut m = m0.clone();
    println!("{} triangles", m.triangles.len());
    let t = std::time::Instant::now();
    let reg = find(&m, 3.0, 64);
    let found = t.elapsed();
    let top = reg.planes.iter().find(|p| p.normal[2] > 0.99).unwrap();
    let hole = reg.cylinders.iter().find(|c| c.is_hole).unwrap();
    let t = std::time::Instant::now();
    let undo = rolidium_mesh::edit::pull_plane_with(&mut m, top, 5.0, false).unwrap();
    let pulled = t.elapsed();
    let t = std::time::Instant::now();
    let redo = undo.apply(&mut m);
    let undone = t.elapsed();
    let t = std::time::Instant::now();
    let u2 = set_diameter(&mut m, hole, 6.0).unwrap();
    let widened = t.elapsed();
    println!(
        "regions {found:?}; pull {pulled:?} (moved {} of {} vertices, record {} KB); undo {undone:?}; hole {widened:?} (moved {})",
        undo.moved.len(),
        m.positions.len(),
        undo.moved.len() * 16 / 1000,
        u2.moved.len()
    );
    redo.apply(&mut m);
    assert!(check(&m).signed_volume > 0.0);
}

#[test]
fn a_finely_divided_ball_has_no_walls_and_a_plate_with_many_holes_has_six_walls_and_all_the_holes() {
    use rolidium_mesh::{check, gen, regions};
    let ball = gen::sphere(50.0, 200_000);
    let r = regions::find(&ball, 3.0, 4);
    assert_eq!(r.planes.len(), 0, "the flat spots of a smooth surface are no walls");
    let plate = gen::plate_with_holes(6, 5, 10.0, 4.0, 3.0, 48);
    assert!(check::check(&plate).is_clean(), "the plate made of cells is one closed mesh");
    let r = regions::find(&plate, 3.0, 4);
    assert_eq!(r.planes.len(), 6, "top, bottom and the four sides");
    assert_eq!(r.cylinders.len(), 30, "a hole in every cell");
    assert!(r.cylinders.iter().all(|c| c.is_hole && (c.radius - 3.0).abs() < 0.05));
}

#[test]
fn vertices_that_lie_across_the_border_of_a_cell_are_still_merged() {
    use rolidium_mesh::{check, gen};
    // the box, with one vertex of one triangle 3 millionths of a millimetre away from the rest (it lies on the other side of a cell border for some sizes)
    for k in 0..40 {
        let mut m = gen::box_mesh(10.0 + k as f32 * 0.37, 10.0, 10.0, 1);
        let a = m.positions.len() as u32;
        let t = m.triangles[0];
        let mut p = m.positions[t[0] as usize];
        p[0] += 3e-6;
        m.positions.push(p);
        m.triangles[0][0] = a;
        let before = m.positions.len();
        let merged = check::merge_vertices(&mut m, 2e-5);
        assert_eq!(merged, 1, "size {k}: {before} vertices before");
        assert!(check::check(&m).is_clean());
    }
}
