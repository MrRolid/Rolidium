// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use rolidium_cli::{CliError, Runner};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rolidium-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

#[test]
fn box_pull_export_render_scenario() {
    let stl = tmp("a.stl");
    let png = tmp("a.png");
    let step = tmp("a.step");
    let script = format!(
        "box 40 30 20\npull top 10\npull right -8\nundo\nredo\ninfo\nexport-stl {}\nexport-step {}\nrender {} 320 240 iso\n",
        stl.display(),
        step.display(),
        png.display()
    );
    let mut r = Runner::new();
    r.run_script(&script).unwrap();
    // 40x30x20 +10 on top = 40x30x30; right face pulled in by 8 = 32x30x30
    let vol = r.doc.bodies()[0].solid.volume().unwrap();
    assert!((vol - 32.0 * 30.0 * 30.0).abs() < 1e-6, "volume {vol}");

    let stl_bytes = std::fs::read(&stl).unwrap();
    let n = u32::from_le_bytes(stl_bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(stl_bytes.len(), 84 + 50 * n);
    assert!(n >= 12);

    let png_bytes = std::fs::read(&png).unwrap();
    assert_eq!(&png_bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png_bytes[16..20].try_into().unwrap()), 320);
    assert_eq!(u32::from_be_bytes(png_bytes[20..24].try_into().unwrap()), 240);
    assert!(String::from_utf8_lossy(&std::fs::read(&step).unwrap()).contains("ISO-10303-21"));
}

#[test]
fn failing_command_reports_line_and_keeps_document() {
    let mut r = Runner::new();
    let err = r
        .run_script("box 10 10 10\n# comment\npull top -100\n")
        .unwrap_err();
    match err {
        CliError::Script { line, msg } => {
            assert_eq!(line, 3);
            assert!(!msg.is_empty());
        }
        e => panic!("unexpected error {e}"),
    }
    let vol = r.doc.bodies()[0].solid.volume().unwrap();
    assert!((vol - 1000.0).abs() < 1e-6);
}

#[test]
fn bad_input_is_reported_not_panicking() {
    for bad in [
        "box 1 2",
        "box a b c",
        "pull",
        "pull top",
        "pull nowhere 1",
        "frobnicate",
        "undo",
        "export-stl",
    ] {
        let mut r = Runner::new();
        assert!(r.run_script(bad).is_err(), "'{bad}' should fail cleanly");
    }
    let mut r = Runner::new();
    assert!(r.run_script("box 0 5 5").is_err());
    assert!(r.run_script("box 10 10 10\npull top nan").is_err());
}

#[test]
fn stl_roundtrip_volume_matches() {
    let stl = tmp("rt.stl");
    let mut r = Runner::new();
    r.run_script(&format!(
        "box 40 30 20\npull top 10\nexport-stl {}",
        stl.display()
    ))
    .unwrap();
    let b = std::fs::read(&stl).unwrap();
    let n = u32::from_le_bytes(b[80..84].try_into().unwrap()) as usize;
    let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as f64;
    let mut vol = 0.0;
    for t in 0..n {
        let o = 84 + 50 * t + 12;
        let v: Vec<[f64; 3]> = (0..3)
            .map(|k| [f(o + 12 * k), f(o + 12 * k + 4), f(o + 12 * k + 8)])
            .collect();
        let c = [
            v[1][1] * v[2][2] - v[1][2] * v[2][1],
            v[1][2] * v[2][0] - v[1][0] * v[2][2],
            v[1][0] * v[2][1] - v[1][1] * v[2][0],
        ];
        vol += (v[0][0] * c[0] + v[0][1] * c[1] + v[0][2] * c[2]) / 6.0;
    }
    assert!((vol - 36000.0).abs() < 1e-2, "STL volume {vol}");
}

#[test]
fn reference_part_through_hole_from_sketch() {
    let png = tmp("hole.png");
    let script = format!(
        "box 40 30 20\nsketch top\ncircle 20 15 10\nregions\npull-region 0 -20\ninfo\nrender {} 320 240 iso\nundo\ninfo\n",
        png.display()
    );
    let mut r = Runner::new();
    r.run_script(&script).unwrap();
    // after the final undo the sketch is back and the solid is the plain box
    assert_eq!(r.doc.sketches().len(), 1);
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 24000.0).abs() < 1e-6);
    r.run_script("redo").unwrap();
    let v = r.doc.bodies()[0].solid.volume().unwrap();
    let expect = 24000.0 - std::f64::consts::PI * 25.0 * 20.0;
    assert!((v - expect).abs() < 1e-6 && (v - 22429.2).abs() < 0.1, "{v}");
    assert_eq!(r.doc.bodies()[0].solid.face_count(), 7);
    assert!(std::fs::metadata(&png).unwrap().len() > 500);
}

#[test]
fn new_body_from_base_plane_sketch_and_nested_regions() {
    let mut r = Runner::new();
    r.run_script("sketch xy\nrect 0 0 40 30\ncircle 20 15 10\nregions\npull-region 0 10")
        .unwrap();
    // two regions: the annulus-like rectangle (index depends on order) and the disc; one body was created
    assert_eq!(r.doc.bodies().len(), 1);
    let v = r.doc.bodies()[0].solid.volume().unwrap();
    assert!(v > 0.0 && (v - 10.0 * 1200.0).abs() > 1.0);
}

#[test]
fn sketch_errors_are_reported() {
    for bad in [
        "line 0 0 1 1",
        "sketch xy\nline 1 1 1 1",
        "sketch xy\nrect 0 0 0 5",
        "sketch xy\ncircle 0 0 0",
        "sketch xy\nregions",
        "sketch xy\nrect 0 0 5 5\npull-region 9 1",
        "sketch top",
    ] {
        assert!(
            Runner::new().run_script(bad).is_err(),
            "'{bad}' should fail cleanly"
        );
    }
}

#[test]
fn export_stl_fineness_changes_triangle_count_and_format() {
    let coarse = tmp("c.stl");
    let fine = tmp("f.stl");
    let ascii = tmp("a2.stl");
    let mut r = Runner::new();
    r.run_script(&format!(
        "box 40 30 20\nsketch top\ncircle 20 15 10\npull-region 0 -20\nexport-stl {} --deviation 0.2 --angle 30\nexport-stl {} --deviation 0.01 --angle 5\nexport-stl {} --ascii",
        coarse.display(), fine.display(), ascii.display()
    ))
    .unwrap();
    let tri = |p: &PathBuf| u32::from_le_bytes(std::fs::read(p).unwrap()[80..84].try_into().unwrap());
    assert!(
        tri(&fine) > 3 * tri(&coarse),
        "{} vs {}",
        tri(&fine),
        tri(&coarse)
    );
    assert!(std::fs::read_to_string(&ascii).unwrap().starts_with("solid"));
    for bad in [
        "export-stl x.stl --deviation",
        "export-stl x.stl --angle 200",
        "export-stl x.stl --what",
    ] {
        let mut r = Runner::new();
        assert!(r.run_script(&format!("box 1 1 1\n{bad}")).is_err());
    }
}

#[test]
fn edit_round_and_chamfer_scenarios() {
    // 40 x 30 x 20 box, one vertical edge: R5 -> R8 -> sharp; chamfer 5 -> 3 -> sharp.
    let mut r = Runner::new();
    r.run_script("box 40 30 20\nround 5 near 40 30 10\nfeatures\nedit-feature 0 8\n")
        .unwrap();
    let v = r.doc.bodies()[0].solid.volume().unwrap();
    assert!((v - 23725.31).abs() < 0.01, "{v}");
    r.run_script("edit-feature 0 0\n").unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 24000.0).abs() < 1e-6);
    assert!(r.doc.bodies()[0].features.is_empty());

    let mut r = Runner::new();
    r.run_script("box 40 30 20\nchamfer 5 near 40 30 10\nedit-feature 0 3\n")
        .unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 23910.0).abs() < 1e-6);
    r.run_script("edit-feature 0 0\n").unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 24000.0).abs() < 1e-6);

    // A size that does not fit is an error on the right line and changes nothing.
    let mut r = Runner::new();
    r.run_script("box 40 30 20\nround 5 near 40 30 10\n").unwrap();
    let before = r.doc.bodies()[0].solid.volume().unwrap();
    let err = r.run_script("edit-feature 0 500\n").unwrap_err();
    assert!(matches!(err, CliError::Script { line: 1, .. }));
    assert_eq!(r.doc.bodies()[0].solid.volume().unwrap(), before);
}

#[test]
fn pull_modes_in_scripts() {
    let mut r = Runner::new();
    r.run_script("box 40 30 20\npull top 5 cut\n").unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 40.0 * 30.0 * 15.0).abs() < 1e-6);
    r.run_script("pull top -3 add\n").unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 40.0 * 30.0 * 18.0).abs() < 1e-6);
    assert!(r.run_script("pull top 3 sideways\n").is_err());
    // decimal comma is accepted
    r.run_script("pull top 2,5\n").unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 40.0 * 30.0 * 20.5).abs() < 1e-6);
}

fn tdir(name: &str) -> PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(format!("cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn save_open_and_step_scenarios() {
    let d = tdir("files");
    let f = d.join("m.rolidium");
    let step = d.join("m.step");
    let mut r = Runner::new();
    r.run_script(&format!(
        "box 40 30 20\nround 5 near 40 30 10\nsave {}\nexport-step {}\n",
        f.display(),
        step.display()
    ))
    .unwrap();
    let vol = r.doc.bodies()[0].solid.volume().unwrap();
    let mut r2 = Runner::new();
    r2.run_script(&format!("open {}\nedit-feature 0 8\n", f.display()))
        .unwrap();
    assert!(
        r2.doc.bodies()[0].solid.volume().unwrap() < vol,
        "the round is editable after opening"
    );
    let mut r3 = Runner::new();
    r3.run_script(&format!("import-step {}\ninfo\n", step.display()))
        .unwrap();
    assert!((r3.doc.bodies()[0].solid.volume().unwrap() - vol).abs() < 1e-3);
    // a damaged file is an error on its line and the document stays as it was
    let bad = d.join("bad.rolidium");
    std::fs::write(&bad, b"nonsense").unwrap();
    let mut r4 = Runner::new();
    r4.run_script("box 10 10 10").unwrap();
    assert!(r4.run_script(&format!("open {}", bad.display())).is_err());
    assert_eq!(r4.doc.bodies().len(), 1);
}

#[test]
fn move_rotate_revolve_scenarios() {
    let mut r = Runner::new();
    r.run_script("box 40 30 20\nmove 10 0 5\nrotate z 90\nmove 0 0 0 copy\n")
        .unwrap();
    assert_eq!(r.doc.bodies().len(), 2);
    let p = r.doc.bodies()[0].solid.props().unwrap();
    assert!(((p.max - p.min) - rolidium_kernel::DVec3::new(30.0, 40.0, 20.0)).length() < 1e-9);
    let mut r = Runner::new();
    r.run_script("sketch xz\nrect 5 0 15 20\nrevolve 0 z 360\n")
        .unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 12566.37).abs() < 0.01);
    let mut r = Runner::new();
    r.run_script("sketch xz\nrect -5 0 5 20\n").unwrap();
    assert!(r.run_script("revolve 0 z 360").is_err());
}

#[test]
fn combine_and_angled_chamfer_scenarios() {
    for (op, want, n) in [
        ("merge", 12000.0, 1),
        ("cut", 4000.0, 1),
        ("intersect", 4000.0, 1),
    ] {
        let mut r = Runner::new();
        r.run_script(&format!("box 20 20 20\nmove 10 0 0 copy\ncombine {op} 1 2\n"))
            .unwrap();
        assert_eq!(r.doc.bodies().len(), n);
        assert!(
            (r.doc.bodies()[0].solid.volume().unwrap() - want).abs() < 1e-6,
            "{op}"
        );
    }
    let mut r = Runner::new();
    r.run_script("box 20 20 20\nmove 100 0 0 copy\n").unwrap();
    assert!(r.run_script("combine merge 1 2").is_err());
    let mut r = Runner::new();
    r.run_script("box 40 30 20\nchamfer 5 near 40 30 10 angle 30\n")
        .unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 23855.66).abs() < 0.01);
}

#[test]
fn loft_scenarios() {
    let mut r = Runner::new();
    r.run_script("sketch xy\ncircle 0 0 40\nsketch xy 30\ncircle 0 0 20\nloft ruled\n")
        .unwrap();
    assert!((r.doc.bodies()[0].solid.volume().unwrap() - 21991.15).abs() < 0.05);
    let mut r = Runner::new();
    r.run_script("sketch xy\ncircle 0 0 40\ncircle 0 0 36\nsketch xy 30\ncircle 0 0 20\ncircle 0 0 16\n")
        .unwrap();
    // the ring is region 1 or 0 depending on the arrangement; the scenario loft takes region 0 of each
    let res = r.run_script("loft ruled");
    if res.is_ok() {
        let v = r.doc.bodies()[0].solid.volume().unwrap();
        assert!(v > 5000.0);
    }
    let mut r = Runner::new();
    r.run_script("sketch xy\ncircle 0 0 40\n").unwrap();
    assert!(r.run_script("loft ruled").is_err());
}

#[test]
fn curve_helix_and_sweep_scenarios() {
    let mut r = Runner::new();
    // a helix with a height: turns follow from the pitch
    r.run_script("helix radius 10 pitch 5 height 25").unwrap();
    r.run_script("curve radius 10 0 0 0 50 0 0 50 50 0 50 50 40")
        .unwrap();
    assert_eq!(r.doc.curves().len(), 2);
    let h = r.doc.curves()[0].def.path().unwrap();
    assert!((h.length - 5.0 * ((2.0 * std::f64::consts::PI * 10.0).powi(2) + 25.0).sqrt()).abs() < 1e-3);
    // a ring swept along the 3D line
    r.run_script("sketch yz\ncircle 0 0 10\ncircle 0 0 6\nsweep 0 curve 2 new")
        .unwrap();
    let v = r.doc.bodies()[0].solid.volume().unwrap();
    let path = r.doc.curves()[1].def.path().unwrap();
    let expected = std::f64::consts::PI * (25.0 - 9.0) * path.length;
    assert!((v - expected).abs() < 2e-3 * expected, "{v} vs {expected}");
    // a too sharp bend is refused and the model stays
    let before = r.doc.bodies().len();
    r.run_script("curve radius 2 0 0 0 30 0 0 30 30 0").unwrap();
    let err = r
        .run_script("sketch yz\ncircle 0 0 20\nsweep 0 curve 3 new")
        .unwrap_err();
    assert!(err.to_string().contains("not changed"), "{err}");
    assert_eq!(r.doc.bodies().len(), before);
    // not a number
    assert!(r.run_script("helix radius x").is_err());
}

#[test]
fn measure_and_mass_scenarios() {
    let mut r = Runner::new();
    r.run_script("box 40 30 20").unwrap();
    r.run_script("measure face left face right").unwrap();
    assert!(
        r.log.last().unwrap().contains("distance 40.000 mm")
            && r.log.last().unwrap().contains("parallel faces 40.000"),
        "{:?}",
        r.log.last()
    );
    r.run_script("measure point 0 0 0 point 3 4 12").unwrap();
    assert!(
        r.log.last().unwrap().contains("distance 13.000"),
        "{:?}",
        r.log.last()
    );
    r.run_script("measure face top face right").unwrap();
    assert!(
        r.log.last().unwrap().contains("angle 90.000"),
        "{:?}",
        r.log.last()
    );
    // mass: 24 cm3 of steel (default), then of aluminium, then of a custom density
    r.run_script("mass").unwrap();
    assert!(
        r.log.last().unwrap().contains("mass 188.400 g"),
        "{:?}",
        r.log.last()
    );
    r.run_script("material aluminium\nmass").unwrap();
    assert!(
        r.log.last().unwrap().contains("mass 64.800 g"),
        "{:?}",
        r.log.last()
    );
    r.run_script("material 1,5\nmass").unwrap();
    assert!(
        r.log.last().unwrap().contains("mass 36.000 g"),
        "{:?}",
        r.log.last()
    );
    assert!(r.run_script("measure face top").is_err());
}
