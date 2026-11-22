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
    r.run_line("circular-pattern 6 full 0 0 1 through 50 50 0 from 2 cut 1").unwrap();
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
    assert!(e.contains("made by the link 'Row'") && e.contains("unlink"), "{e}");
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
