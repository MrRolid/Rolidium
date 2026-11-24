// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Patterns, mirrors, links and groups through the script language (the same words are tools of the MCP server).
use rolidium_cli::Runner;

fn run(script: &str) -> Runner {
    let mut r = Runner::new();
    r.quiet = true;
    r.run_script(script).unwrap_or_else(|e| panic!("{e}"));
    r
}

fn total(r: &Runner) -> f64 {
    r.doc.bodies().iter().map(|b| b.solid.volume().unwrap()).sum()
}

#[test]
fn a_row_a_grid_and_a_bolt_circle_from_scripts() {
    let r = run("box 10 10 10\nlinear-pattern 5 30 1 0 0\n");
    assert_eq!(r.doc.bodies().len(), 5);
    assert!((total(&r) - 5000.0).abs() < 1e-6);
    let r = run("box 10 10 10\nlinear-pattern 3 15 1 0 0 second 4 20 0 1 0\n");
    assert_eq!(r.doc.bodies().len(), 12);
    // a plate of 100 x 100 x 10 and a tool of 10 x 10 x 30 (x 70..80, y 45..55: 20 mm from the centre), 6 times round the centre: the 6 pieces do not overlap
    let mut r = run("box 100 100 10\nbox 10 10 30\nmove 70 45 -10 body 2\n");
    r.run_line("circular-pattern 6 full 0 0 1 through 50 50 0 from 2 cut 1")
        .unwrap();
    let removed = 100.0 * 100.0 * 10.0 - r.doc.bodies()[0].solid.volume().unwrap();
    assert!(removed > 5000.0 && removed < 6000.0 + 1e-6, "{removed}");
    assert!(!r.doc.bodies()[1].visible, "the tool is hidden");
}

#[test]
fn a_link_is_listed_edited_and_removed() {
    let mut r = run("box 10 10 10\nlinear-pattern 3 20 1 0 0 name \"Row\"\n");
    assert_eq!(r.doc.state().links[0].name, "Row");
    r.run_line("edit-link 1 count 6").unwrap();
    assert_eq!(r.doc.bodies().len(), 6);
    r.run_line("edit-link 1 count 4 spacing 40").unwrap();
    assert_eq!(r.doc.bodies().len(), 4);
    r.run_line("links").unwrap();
    // the copies are derived: a hand edit says what to do
    let e = r.run_line("pull top 5 body 2").unwrap_err();
    assert!(
        e.contains("made by the link 'Row'") && e.contains("unlink"),
        "{e}"
    );
    r.run_line("unlink 1").unwrap();
    assert!(r.doc.state().links.is_empty());
    r.run_line("pull top 5 body 2").unwrap();
    // delete-link removes the copies
    let mut r = run("box 10 10 10\nlinear-pattern 3 20 1 0 0\n");
    r.run_line("delete-link 1").unwrap();
    assert_eq!(r.doc.bodies().len(), 1);
    // the live mirror follows a pull of the original
    let mut r = run("box 10 10 10\nmove 20 0 0\nmirror x live\n");
    assert_eq!(r.doc.bodies().len(), 2);
    r.run_line("pull top 10 body 1").unwrap();
    assert!((r.doc.bodies()[1].solid.volume().unwrap() - 2000.0).abs() < 1e-6);
    // a merged mirror is one body
    let r = run("box 10 10 10\nmove 0 0 0\nmirror x merge\n");
    assert_eq!(r.doc.bodies().len(), 1);
    assert!((total(&r) - 2000.0).abs() < 1e-6);
}

#[test]
fn groups_and_sketch_patterns_from_scripts() {
    let mut r = run("box 10 10 10\nbox 10 10 10\nbox 10 10 10\ngroup 1 2 name \"Frame\"\n");
    assert_eq!(r.doc.state().groups[0].name, "Frame");
    r.run_line("move-to-group 1 3").unwrap();
    assert_eq!(r.doc.state().groups[0].bodies.len(), 3);
    r.run_line("move-to-group none 1 2 3").unwrap();
    assert!(r.doc.state().groups[0].bodies.is_empty());
    r.run_line("ungroup 1").unwrap();
    assert!(r.doc.state().groups.is_empty());
    let mut r = run("sketch xy\ncircle 0 0 4\nsketch-pattern linear 4 10 1 0\n");
    assert_eq!(r.doc.sketches()[0].sketch.regions().len(), 4);
    r.run_line("sketch-mirror 0 0 1 0 keep").unwrap();
    assert_eq!(r.doc.sketches()[0].sketch.entities().len(), 8);
    let r = run("sketch xy\ncircle 20 0 4\nsketch-pattern circular 6 full 0 0\n");
    assert_eq!(r.doc.sketches()[0].sketch.regions().len(), 6);
}

/// A plate of 100 x 60 x 4 with a hole of 3 mm diameter 5 mm from two edges (a sketch circle pulled through in cut mode).
const PLATE: &str = "box 100 60 4\nsketch xy -1\ncircle 5 5 3\npull-region 0 6 cut\n";

fn hole_face(r: &Runner) -> String {
    let s = &r.doc.bodies()[0].solid;
    let f = s
        .faces()
        .find(|f| s.cylinder_info(*f).unwrap().is_some_and(|c| c.is_hole))
        .unwrap();
    format!("#{}", f.0)
}

#[test]
fn a_hole_is_repeated_in_the_same_body_from_scripts() {
    let mut r = run(PLATE);
    let v0 = r.doc.bodies()[0].solid.volume().unwrap();
    let hole = std::f64::consts::PI * 1.5 * 1.5 * 4.0;
    // the four corners: three copies of the hole
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} corners top")).unwrap();
    let v1 = r.doc.bodies()[0].solid.volume().unwrap();
    assert!((v0 - v1 - 3.0 * hole).abs() < 1e-3, "{}", v0 - v1);
    assert_eq!(r.doc.state().links.len(), 1);
    // a row whose second piece lies at a point (the hole at (5, 5), the second at (35, 5))
    let mut r = run(PLATE);
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} linear 3 0 1 0 0 to 35 5 4"))
        .unwrap();
    let v1 = r.doc.bodies()[0].solid.volume().unwrap();
    assert!((v0 - v1 - 2.0 * hole).abs() < 1e-3, "{}", v0 - v1);
    assert!(!r.doc.bodies()[0]
        .solid
        .contains_point(rolidium_kernel::DVec3::new(65.0, 5.0, 2.0))
        .unwrap());
    // round an axis, a grid and a mirror
    let mut r = run(PLATE);
    let f = hole_face(&r);
    r.run_line(&format!(
        "repeat-feature {f} circular 4 full 0 0 1 through 30 30 0"
    ))
    .unwrap();
    assert!((v0 - r.doc.bodies()[0].solid.volume().unwrap() - 3.0 * hole).abs() < 1e-3);
    let mut r = run(PLATE);
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} grid 3 2 5 top")).unwrap();
    assert!((v0 - r.doc.bodies()[0].solid.volume().unwrap() - 5.0 * hole).abs() < 1e-3);
    let mut r = run(PLATE);
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} mirror x at 50")).unwrap();
    assert!((v0 - r.doc.bodies()[0].solid.volume().unwrap() - hole).abs() < 1e-3);
    // a face that is not round says what to click
    let e = r.run_line("repeat-feature top linear 2 10 1 0 0").unwrap_err();
    assert!(e.contains("whole round hole"), "{e}");
}

#[test]
fn an_auxiliary_axis_makes_no_region_and_serves_as_the_axis_of_a_mirror_and_a_pattern() {
    // an axis is no shape: a sketch of one axis and one circle has one region
    let mut r = run("sketch xy\naxis 50 -10 50 70\ncircle 20 20 10\n");
    assert_eq!(r.doc.sketches()[0].sketch.entities().len(), 2);
    assert_eq!(r.doc.sketches()[0].sketch.regions().len(), 1);
    assert!(r.doc.sketches()[0].sketch.entities()[0].1.is_axis());
    // a pattern of "all the shapes" does not repeat the axis
    r.run_line("sketch-mirror 50 0 0 1 keep").unwrap();
    assert_eq!(r.doc.sketches()[0].sketch.entities().len(), 3);
    // a mirror through the axis in 3D: the plane goes through the axis, square to the sketch (x = 50)
    let mut r = run("sketch xy\naxis 50 -10 50 70\nbox 10 10 10\nmove 10 0 0 body 1\n");
    r.run_line("mirror plane-of 1 1 keep body 1").unwrap();
    assert_eq!(r.doc.bodies().len(), 2);
    let (lo, hi) = r.doc.bodies()[1].solid.bounding_box().unwrap();
    assert!(
        (lo.x - 80.0).abs() < 1e-6 && (hi.x - 90.0).abs() < 1e-6,
        "{lo:?} {hi:?}"
    );
    // the hole of the plate mirrored by the axis, in the same body
    let mut r = run(PLATE);
    r.run_script("sketch xy\naxis 50 -10 50 70").unwrap();
    let v0 = r.doc.bodies()[0].solid.volume().unwrap();
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} mirror plane-of 1 1"))
        .unwrap();
    let hole = std::f64::consts::PI * 1.5 * 1.5 * 4.0;
    assert!((v0 - r.doc.bodies()[0].solid.volume().unwrap() - hole).abs() < 1e-3);
    assert!(!r.doc.bodies()[0]
        .solid
        .contains_point(rolidium_kernel::DVec3::new(95.0, 5.0, 2.0))
        .unwrap());
    // a circular pattern round the axis (a line in the plane of the sketch, along y)
    let r = run("sketch xy\naxis 0 -10 0 70\nbox 10 10 10\nmove 30 0 0 body 1\ncircular-pattern 4 full axis-of 1 1 body 1\n");
    assert_eq!(r.doc.bodies().len(), 4);
    // a line that is no line gives a clear message
    let mut r = run("sketch xy\ncircle 0 0 10\nbox 10 10 10\n");
    let e = r.run_line("mirror plane-of 1 1").unwrap_err();
    assert!(e.contains("not a line"), "{e}");
}

#[test]
fn a_hole_is_moved_by_a_shift_or_to_given_distances_from_the_edges() {
    let mut r = run(PLATE);
    let v0 = r.doc.bodies()[0].solid.volume().unwrap();
    let f = hole_face(&r);
    r.run_line(&format!("move-feature {f} 10 20 0")).unwrap();
    let s = &r.doc.bodies()[0].solid;
    assert!(!s
        .contains_point(rolidium_kernel::DVec3::new(15.0, 25.0, 2.0))
        .unwrap());
    assert!(s
        .contains_point(rolidium_kernel::DVec3::new(5.0, 5.0, 2.0))
        .unwrap());
    assert!((s.volume().unwrap() - v0).abs() < 1e-6);
    // 12 mm from the two nearest edges
    let mut r = run(PLATE);
    let f = hole_face(&r);
    r.run_line(&format!("move-feature {f} edges 12 12")).unwrap();
    assert!(!r.doc.bodies()[0]
        .solid
        .contains_point(rolidium_kernel::DVec3::new(12.0, 12.0, 2.0))
        .unwrap());
    // a hole that would leave the plate is refused
    let f = hole_face(&r);
    let e = r.run_line(&format!("move-feature {f} 500 0 0")).unwrap_err();
    assert!(e.contains("outside"), "{e}");
}

fn shots_dir() -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/gui-shots");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The number of dark regions of at least `min` pixels inside the light plate of a picture seen from above: the holes of the plate.
fn see_through_regions(png_path: &std::path::Path, min: usize) -> usize {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(png_path).unwrap()));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = info.color_type.samples();
    let light = |x: usize, y: usize| -> bool {
        let i = (y * w + x) * ch;
        buf[i] as i32 + buf[i + 1] as i32 + buf[i + 2] as i32 > 420
    };
    // the box of the light pixels (the plate)
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if light(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return 0;
    }
    let mut seen = vec![false; w * h];
    let mut count = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if seen[y * w + x] || light(x, y) {
                continue;
            }
            let (mut stack, mut size, mut border) = (vec![(x, y)], 0usize, false);
            seen[y * w + x] = true;
            while let Some((cx, cy)) = stack.pop() {
                size += 1;
                if cx == x0 || cy == y0 || cx == x1 || cy == y1 {
                    border = true;
                }
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                    if nx < x0 as i32 || ny < y0 as i32 || nx > x1 as i32 || ny > y1 as i32 {
                        continue;
                    }
                    let (nx, ny) = (nx as usize, ny as usize);
                    if !seen[ny * w + nx] && !light(nx, ny) {
                        seen[ny * w + nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if !border && size >= min {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_part_from_a_step_file_gets_corners_and_a_circle_of_holes_and_the_picture_shows_them() {
    // a plate of 100 x 60 x 10 with a hole of 12 mm diameter; it goes through a STEP file, and the copies are made on the imported body
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp").join(format!("links-step-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let step = d.join("plate.step");
    let src = "box 100 60 10\nsketch xy -1\ncircle 60 30 12\npull-region 0 12 cut\n";
    let r = run(src);
    let mut w = run(src);
    w.run_line(&format!("export-step {}", step.display())).unwrap();
    drop(r);
    // corners: the imported hole is 54 mm from the left edge, 30 from the others; it is not a corner hole, the copies go symmetrically
    let mut r = Runner::new();
    r.quiet = true;
    r.run_line(&format!("import-step {}", step.display())).unwrap();
    assert_eq!(r.doc.bodies().len(), 1);
    let v0 = r.doc.bodies()[0].solid.volume().unwrap();
    let hole = std::f64::consts::PI * 36.0 * 10.0;
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} circular 6 full 0 0 1 through 40 30 0")).unwrap();
    let removed = v0 - r.doc.bodies()[0].solid.volume().unwrap();
    assert!((removed - 5.0 * hole).abs() < 1e-3, "{removed}");
    // the picture from above: six holes (and one before the pattern)
    let before = shots_dir().join("circular-pattern-before.png");
    let after = shots_dir().join("circular-pattern-after.png");
    let mut r0 = Runner::new();
    r0.quiet = true;
    r0.run_line(&format!("import-step {}", step.display())).unwrap();
    r0.run_line(&format!("render {} 800 600 top", before.display())).unwrap();
    r.run_line(&format!("render {} 800 600 top", after.display())).unwrap();
    assert_eq!(see_through_regions(&before, 50), 1);
    assert_eq!(see_through_regions(&after, 50), 6);
    // corners on the imported plate (a hole 8 mm from two edges)
    let step2 = d.join("plate2.step");
    let mut w = run("box 100 60 10\nsketch xy -1\ncircle 8 8 6\npull-region 0 12 cut\n");
    w.run_line(&format!("export-step {}", step2.display())).unwrap();
    let mut r = Runner::new();
    r.quiet = true;
    r.run_line(&format!("import-step {}", step2.display())).unwrap();
    let f = hole_face(&r);
    r.run_line(&format!("repeat-feature {f} corners top")).unwrap();
    let s = &r.doc.bodies()[0].solid;
    for (x, y) in [(8.0, 8.0), (92.0, 8.0), (8.0, 52.0), (92.0, 52.0)] {
        assert!(!s.contains_point(rolidium_kernel::DVec3::new(x, y, 5.0)).unwrap(), "no hole at {x}, {y}");
    }
    let shot = shots_dir().join("step-plate-corner-holes.png");
    r.run_line(&format!("render {} 800 600 top", shot.display())).unwrap();
    assert_eq!(see_through_regions(&shot, 50), 4);
}
