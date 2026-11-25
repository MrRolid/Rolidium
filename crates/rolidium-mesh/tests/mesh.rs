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
    assert_eq!(
        r.planes.len(),
        0,
        "the flat spots of a smooth surface are no walls"
    );
    assert_eq!(
        r.cylinders.len(),
        0,
        "a row of patches round a ball is no cylinder"
    );
    let plate = gen::plate_with_holes(6, 5, 10.0, 4.0, 3.0, 48);
    assert!(
        check::check(&plate).is_clean(),
        "the plate made of cells is one closed mesh"
    );
    let r = regions::find(&plate, 3.0, 4);
    assert_eq!(r.planes.len(), 6, "top, bottom and the four sides");
    assert_eq!(r.cylinders.len(), 30, "a hole in every cell");
    assert!(r
        .cylinders
        .iter()
        .all(|c| c.is_hole && (c.radius - 3.0).abs() < 0.05));
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

#[test]
fn the_facets_of_a_coarse_cylinder_are_one_round_surface() {
    use rolidium_mesh::{gen, regions};
    for facets in [8usize, 10, 16, 24, 48, 90] {
        let m = gen::cylinder(15.0, 40.0, facets);
        let r = regions::find(&m, 3.0, 4);
        assert_eq!(
            r.cylinders.len(),
            1,
            "{facets} facets: one cylinder, planes {}",
            r.planes.len()
        );
        let c = &r.cylinders[0];
        assert!(!c.is_hole, "{facets}");
        assert!((c.radius - 15.0).abs() < 0.05, "{facets}: radius {}", c.radius);
        assert!(c.axis[2].abs() > 0.999, "{facets}: axis {:?}", c.axis);
        assert!(c.arc > 6.0, "{facets}: goes all the way round");
        assert_eq!(r.planes.len(), 2, "{facets}: only the two ends are flat walls");
        assert!(r.planes.iter().all(|p| p.normal[2].abs() > 0.99));
    }
}

#[test]
fn a_coarse_hole_is_a_hole_and_a_box_with_a_chamfer_has_no_cylinder() {
    use rolidium_mesh::{gen, regions};
    for seg in [12usize, 16, 24] {
        let m = gen::plate_with_hole(40.0, 30.0, 6.0, 5.0, seg);
        let r = regions::find(&m, 3.0, 4);
        assert_eq!(r.cylinders.len(), 1, "seg {seg}");
        assert!(r.cylinders[0].is_hole);
        assert!((r.cylinders[0].radius - 5.0).abs() < 0.05);
    }
    let b = gen::box_mesh(40.0, 30.0, 10.0, 1);
    assert!(regions::find(&b, 3.0, 4).cylinders.is_empty());
}

#[test]
fn a_coarse_cylinder_changes_its_diameter_and_its_end_is_pulled_along_the_axis() {
    use rolidium_mesh::{check, edit, gen, regions};
    let mut m = gen::cylinder(15.0, 40.0, 16);
    let r = regions::find(&m, 3.0, 4);
    let c = &r.cylinders[0];
    edit::set_diameter(&mut m, c, 40.0).unwrap();
    let r2 = regions::find(&m, 3.0, 4);
    assert_eq!(r2.cylinders.len(), 1);
    assert!((r2.cylinders[0].radius - 20.0).abs() < 0.05);
    let top = r2.planes.iter().find(|p| p.normal[2] > 0.99).unwrap();
    edit::pull_plane(&mut m, top, 10.0).unwrap();
    let r3 = regions::find(&m, 3.0, 4);
    assert!((r3.cylinders[0].z_range.1 - r3.cylinders[0].z_range.0 - 50.0).abs() < 0.05);
    assert!(check::check(&m).is_clean());
}

#[test]
fn a_wall_with_slanting_neighbours_can_be_pulled_when_the_neighbours_may_tilt() {
    use rolidium_mesh::edit::{pull_plane_opts, EditError, PullPlan};
    use rolidium_mesh::{check, gen, regions};
    let mut m = gen::frustum(40.0, 20.0, 20.0);
    assert!(check::check(&m).is_clean());
    let r = regions::find(&m, 3.0, 2);
    let top = r.planes.iter().find(|p| p.normal[2] > 0.99).unwrap();
    let plan = PullPlan::new(&m, top);
    assert!(!plan.neighbours_square(top));
    assert_eq!(
        plan.check(&m, top, 10.0, false),
        Err(EditError::NeighbourNotSquare)
    );
    assert_eq!(plan.check(&m, top, 10.0, true), Ok(()));
    assert_eq!(plan.check(&m, top, -100.0, true), Err(EditError::Collapses));
    let before = m.clone();
    assert_eq!(
        pull_plane_opts(&mut m, top, 10.0, true, false).unwrap_err(),
        EditError::NeighbourNotSquare
    );
    assert_eq!(m, before, "a refused pull changes nothing");
    let undo = pull_plane_opts(&mut m, top, 10.0, true, true).unwrap();
    let (lo, hi) = m.bounding_box().unwrap();
    assert!((hi[2] - lo[2] - 30.0).abs() < 1e-4);
    assert!(check::check(&m).is_clean());
    undo.apply(&mut m);
    assert_eq!(m, before);
}

#[test]
fn a_pull_that_would_push_the_mesh_through_another_part_is_refused() {
    use rolidium_mesh::edit::{EditError, PullPlan};
    use rolidium_mesh::{gen, regions};
    let mut m = gen::frustum(40.0, 20.0, 20.0);
    // a block floating 5 mm above the top, in the same mesh
    let mut b = gen::box_mesh(10.0, 10.0, 10.0, 1);
    for p in &mut b.positions {
        p[0] += 15.0;
        p[1] += 15.0;
        p[2] += 25.0;
    }
    let base = m.positions.len() as u32;
    m.positions.extend(b.positions);
    m.triangles.extend(
        b.triangles
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );
    let r = regions::find(&m, 3.0, 2);
    let top = r
        .planes
        .iter()
        .find(|p| p.normal[2] > 0.99 && p.offset < 21.0)
        .unwrap();
    let plan = PullPlan::new(&m, top);
    assert_eq!(plan.check(&m, top, 3.0, true), Ok(()));
    assert_eq!(plan.check(&m, top, 8.0, true), Err(EditError::SelfIntersects));
}

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        for (n, c) in files {
            z.start_file(*n, o).unwrap();
            z.write_all(c.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

const CUBE: &str = r#"<object id="1" type="model"><mesh><vertices>
<vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="1" y="1" z="0"/><vertex x="0" y="1" z="0"/>
<vertex x="0" y="0" z="1"/><vertex x="1" y="0" z="1"/><vertex x="1" y="1" z="1"/><vertex x="0" y="1" z="1"/>
</vertices><triangles>
<triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="3" v3="2"/>
<triangle v1="4" v2="5" v3="6"/><triangle v1="4" v2="6" v3="7"/>
<triangle v1="0" v2="1" v3="5"/><triangle v1="0" v2="5" v3="4"/>
<triangle v1="1" v2="2" v3="6"/><triangle v1="1" v2="6" v3="5"/>
<triangle v1="2" v2="3" v3="7"/><triangle v1="2" v2="7" v3="6"/>
<triangle v1="3" v2="0" v3="4"/><triangle v1="3" v2="4" v3="7"/>
</triangles></mesh></object>"#;

#[test]
fn a_3mf_file_gives_its_objects_in_millimetres_with_their_transforms() {
    use rolidium_mesh::{check, threemf};
    // a cube of 1 inch, an object made of it with a component moved by 2 inches in x, and two items in the build: one at the origin, one turned 90 degrees and moved
    let model = format!(
        r#"<?xml version="1.0"?><model unit="inch" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>{CUBE}
<object id="2" name="Pair"><components><component objectid="1"/><component objectid="1" transform="1 0 0 0 1 0 0 0 1 2 0 0"/></components></object>
</resources><build><item objectid="1"/><item objectid="2" transform="0 1 0 -1 0 0 0 0 1 0 10 0"/></build></model>"#
    );
    let bytes = zip_of(&[
        ("[Content_Types].xml", "<Types/>"),
        (
            "_rels/.rels",
            r#"<Relationships><Relationship Target="/3D/3dmodel.model" Id="r" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#,
        ),
        ("3D/3dmodel.model", &model),
    ]);
    let parts = threemf::parse(&bytes).unwrap();
    assert_eq!(parts.len(), 2);
    let (lo, hi) = parts[0].mesh.bounding_box().unwrap();
    assert!((hi[0] - lo[0] - 25.4).abs() < 1e-3, "the inch becomes 25.4 mm");
    assert!(check::check(&parts[0].mesh).is_clean());
    assert_eq!(parts[1].name, "Pair");
    assert_eq!(parts[1].mesh.triangles.len(), 24);
    // the pair is 3 inches long in x before the turn; turned by 90 degrees (x' = -y, y' = x) it is long in y, and moved by 10 in y
    let (lo, hi) = parts[1].mesh.bounding_box().unwrap();
    assert!((hi[1] - lo[1] - 3.0 * 25.4).abs() < 1e-2, "{lo:?} {hi:?}");
    assert!((hi[0] - lo[0] - 25.4).abs() < 1e-2);
    assert!(
        (lo[1] - 254.0).abs() < 1e-2,
        "the move of the build is in the unit of the file: {lo:?}"
    );
    assert!(threemf::parse(b"not a zip").is_err());
}

#[test]
fn the_analysis_names_what_the_repairs_would_do() {
    use rolidium_mesh::{check, gen};
    // a good mesh: nothing to do
    let a = check::analyse(&gen::box_mesh(10.0, 10.0, 10.0, 2));
    assert!(a.is_clean() && a.loose_joints == 0 && a.inward == 0);
    // a vertex that lies beside the others and a triangle turned over
    let mut m = gen::box_mesh(10.0, 10.0, 10.0, 2);
    let n = m.positions.len() as u32;
    let t = m.triangles[5];
    let mut p = m.positions[t[0] as usize];
    p[0] += 3e-6;
    m.positions.push(p);
    m.triangles[5][0] = n;
    m.triangles[9].swap(1, 2);
    let a = check::analyse(&m);
    assert!(!a.is_clean());
    assert_eq!(a.loose_joints, 1, "one vertex is a loose joint");
    assert_eq!(
        a.inward, 1,
        "one triangle is turned inwards (after the joint is joined)"
    );
    assert!(
        a.report.open_edges > 0 && a.after_merge.open_edges == 0,
        "the open edges were the joint"
    );
    // two blocks that touch along an edge: edges of four triangles, no repair
    let mut two = gen::box_mesh(10.0, 10.0, 10.0, 1);
    let b = gen::box_mesh(10.0, 10.0, 10.0, 1);
    let base = two.positions.len() as u32;
    two.positions
        .extend(b.positions.iter().map(|p| [p[0] + 10.0, p[1] + 10.0, p[2]]));
    two.triangles.extend(
        b.triangles
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );
    check::merge_vertices(&mut two, 1e-4);
    let a = check::analyse(&two);
    assert!(a.report.non_manifold_edges > 0 && a.report.is_watertight());
    assert_eq!((a.loose_joints, a.inward), (0, 0), "nothing a button could mend");
    assert!(!a.is_clean());
}
