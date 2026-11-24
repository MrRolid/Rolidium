// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! STL meshes through the script language (the same words are tools of the MCP server).
use rolidium_cli::Runner;
use rolidium_mesh::gen;
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(format!("mesh-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn runner() -> Runner {
    let mut r = Runner::new();
    r.quiet = true;
    r
}

fn size(r: &Runner) -> [f64; 3] {
    let (lo, hi) = r.meshes.last().unwrap().mesh.bounding_box().unwrap();
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
}

#[test]
fn the_cube_of_the_scale_test_is_exported_and_imported_20_mm() {
    let d = dir("cube");
    let mut r = runner();
    r.run_script(rolidium_cli::stl_scale::SCRIPT).unwrap();
    // the script of the scale test writes its files into the working directory of the test: make our own export of the first body
    r.run_line(&format!("export-stl {}", d.join("cube.stl").display())).unwrap();
    let mut m = runner();
    m.run_line(&format!("import-stl {}", d.join("cube.stl").display())).unwrap();
    let s = size(&m);
    assert!(s.iter().all(|v| (v - 20.0).abs() < 1e-3), "{s:?}");
    assert!(m.log.iter().any(|l| l.contains("closed and consistent")), "{:?}", m.log);
}

#[test]
fn inches_and_a_scale_of_your_own() {
    let d = dir("units");
    let f = d.join("one.stl");
    std::fs::write(&f, rolidium_mesh::stl::write_binary(&gen::box_mesh(1.0, 2.0, 3.0, 1))).unwrap();
    let mut r = runner();
    r.run_line(&format!("import-stl {} inch", f.display())).unwrap();
    let s = size(&r);
    assert!((s[0] - 25.4).abs() < 1e-3 && (s[1] - 50.8).abs() < 1e-3 && (s[2] - 76.2).abs() < 1e-3, "{s:?}");
    r.run_line(&format!("import-stl {} scale 10", f.display())).unwrap();
    assert!((size(&r)[2] - 30.0).abs() < 1e-4);
    r.run_line("mesh-scale 2 1 0.5 mesh 2").unwrap();
    let s = size(&r);
    assert!((s[0] - 20.0).abs() < 1e-4 && (s[2] - 15.0).abs() < 1e-4, "{s:?}");
    assert!(r.run_line(&format!("import-stl {} scale -1", f.display())).is_err());
    assert!(r.run_line("import-stl /nonexistent/file.stl").is_err());
}

#[test]
fn a_mesh_box_gets_a_taller_top_wall_and_a_bigger_hole_and_undo_brings_them_back() {
    let d = dir("box");
    let f = d.join("plate.stl");
    std::fs::write(&f, rolidium_mesh::stl::write_binary(&gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, 48))).unwrap();
    let mut r = runner();
    r.run_line(&format!("import-stl {}", f.display())).unwrap();
    r.run_line("mesh-regions").unwrap();
    let top = r
        .log
        .iter()
        .find(|l| l.contains("wall #") && l.contains(", 1.000)"))
        .and_then(|l| l.trim().strip_prefix("wall #"))
        .and_then(|l| l.split(':').next())
        .and_then(|n| n.parse::<usize>().ok())
        .expect("the top wall");
    r.run_line(&format!("mesh-pull {top} 5")).unwrap();
    assert!((size(&r)[2] - 15.0).abs() < 1e-4);
    assert!(rolidium_mesh::check::check(&r.meshes[0].mesh).is_clean(), "the mesh stays closed");
    r.run_line("mesh-regions").unwrap();
    let hole = r
        .log
        .iter()
        .rev()
        .find(|l| l.trim_start().starts_with("hole #"))
        .and_then(|l| l.trim().strip_prefix("hole #"))
        .and_then(|l| l.split(':').next())
        .and_then(|n| n.parse::<usize>().ok())
        .expect("the hole");
    assert!(r.log.iter().any(|l| l.contains("diameter 5.0")), "{:?}", r.log);
    r.run_line(&format!("mesh-hole {hole} 6")).unwrap();
    r.run_line("mesh-regions").unwrap();
    assert!(r.log.iter().rev().take(5).any(|l| l.contains("diameter 6.0")), "{:?}", r.log);
    // one step of undo for each
    r.run_line("mesh-undo").unwrap();
    r.run_line("mesh-regions").unwrap();
    assert!(r.log.iter().rev().take(5).any(|l| l.contains("diameter 5.0")));
    r.run_line("mesh-undo").unwrap();
    assert!((size(&r)[2] - 10.0).abs() < 1e-4);
    r.run_line("mesh-redo").unwrap();
    assert!((size(&r)[2] - 15.0).abs() < 1e-4);
    // a pull through the opposite wall is refused and nothing changes
    let before = r.meshes[0].mesh.clone();
    assert!(r.run_line(&format!("mesh-pull {top} -30")).unwrap_err().contains("turn"));
    assert_eq!(r.meshes[0].mesh, before);
    // export and the compact form
    r.run_line(&format!("export-mesh-stl {}", d.join("out.stl").display())).unwrap();
    r.run_line(&format!("mesh-save {}", d.join("out.rmesh").display())).unwrap();
    r.run_line(&format!("mesh-open {}", d.join("out.rmesh").display())).unwrap();
    assert_eq!(r.meshes[0].mesh, r.meshes[1].mesh);
    assert!(std::fs::metadata(d.join("out.rmesh")).unwrap().len() < std::fs::metadata(d.join("out.stl")).unwrap().len());
}

#[test]
fn a_mesh_with_holes_and_turned_normals_is_checked_and_repaired() {
    let d = dir("bad");
    let mut m = gen::plate_with_hole(60.0, 40.0, 10.0, 2.5, 48);
    for t in [5usize, 60, 61, 150] {
        m.triangles[t].swap(1, 2);
    }
    let f = d.join("bad.stl");
    std::fs::write(&f, rolidium_mesh::stl::write_binary(&m)).unwrap();
    let mut r = runner();
    r.run_line(&format!("import-stl {}", f.display())).unwrap();
    assert!(r.log.iter().any(|l| l.contains("normals do not agree")), "{:?}", r.log);
    r.run_line("mesh-repair").unwrap();
    assert!(r.log.last().unwrap().contains("closed and consistent"), "{:?}", r.log);
    // two triangles missing: a hole in the mesh
    let mut m = gen::box_mesh(10.0, 10.0, 10.0, 3);
    m.triangles.truncate(m.triangles.len() - 2);
    std::fs::write(&f, rolidium_mesh::stl::write_binary(&m)).unwrap();
    r.run_line(&format!("import-stl {}", f.display())).unwrap();
    assert!(r.log.iter().rev().take(6).any(|l| l.contains("open edge")), "{:?}", r.log);
    r.run_line("mesh-regions").unwrap();
    assert!(r.run_line("mesh-pull 1 2").unwrap_err().contains("repair"));
}

#[test]
fn move_rotate_mirror_and_pattern_of_a_mesh() {
    let d = dir("moves");
    let f = d.join("c.stl");
    std::fs::write(&f, rolidium_mesh::stl::write_binary(&gen::box_mesh(10.0, 10.0, 10.0, 1))).unwrap();
    let mut r = runner();
    r.run_line(&format!("import-stl {}", f.display())).unwrap();
    r.run_line("mesh-move 5 0 0").unwrap();
    r.run_line("mesh-mirror x").unwrap();
    assert!(r.meshes[0].mesh.bounding_box().unwrap().1[0] <= 1e-6);
    assert!((r.meshes[0].mesh.signed_volume() - 1000.0).abs() < 1e-3);
    r.run_line("mesh-rotate z 90").unwrap();
    r.run_line("mesh-pattern 3 0 0 20").unwrap();
    assert!((size(&r)[2] - 50.0).abs() < 1e-4);
    assert!(rolidium_mesh::check::check(&r.meshes[0].mesh).components == 3);
}

#[test]
fn the_example_files_import() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/stl");
    for (f, dims) in [("plate-with-hole.stl", [60.0, 40.0, 10.0]), ("plate-with-hole-ascii.stl", [60.0, 40.0, 10.0]), ("cube-20.stl", [20.0, 20.0, 20.0])] {
        let mut r = runner();
        r.run_line(&format!("import-stl {}", root.join(f).display())).unwrap();
        let s = size(&r);
        assert!((0..3).all(|i| (s[i] - dims[i]).abs() < 1e-3), "{f}: {s:?}");
        assert!(rolidium_mesh::check::check(&r.meshes[0].mesh).is_clean(), "{f}");
    }
}
