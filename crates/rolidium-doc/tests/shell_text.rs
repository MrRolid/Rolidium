// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Shell and the Text as links: the numbers stay editable, the text goes back when a shell cannot follow it, the files keep both.
use rolidium_doc::{Command, Document, FaceAnchor, LinkKind, ShellParams, ShellSide, TextAlign, TextParams};
use rolidium_kernel::{DVec3, FaceKind, Solid};

fn doc_with_box(x: f64, y: f64, z: f64) -> (Document, u64) {
    let mut d = Document::new();
    d.apply(Command::AddNamedSolids {
        solids: vec![(Solid::cuboid(DVec3::ZERO, DVec3::new(x, y, z)).unwrap(), "box".into())],
    })
    .unwrap();
    let id = d.bodies()[0].id;
    (d, id)
}

fn face_with_normal(s: &Solid, n: DVec3) -> FaceAnchor {
    let f = s
        .faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.dot(n) > 0.999
        })
        .unwrap();
    FaceAnchor::of(s, f).unwrap()
}

fn text_on_top(body: u64, text: &str, engrave: bool, depth: f64, height: f64, at: DVec3) -> TextParams {
    TextParams {
        target: body,
        text: text.into(),
        font: "sans".into(),
        bold: false,
        italic: false,
        height,
        align: TextAlign::Left,
        rotation: 0.0,
        spacing: 0.0,
        depth,
        engrave,
        origin: at,
        normal: DVec3::Z,
        x_dir: DVec3::X,
    }
}

fn volume(d: &Document, b: u64) -> f64 {
    d.body(b).unwrap().solid.volume().unwrap()
}

#[test]
fn a_shell_is_a_link_whose_thickness_and_openings_can_be_changed() {
    let (mut d, b) = doc_with_box(40.0, 30.0, 20.0);
    let top = face_with_normal(&d.body(b).unwrap().solid, DVec3::Z);
    let mut p = ShellParams::new(b, 2.0);
    p.open = vec![top];
    d.apply(Command::AddLink {
        kind: LinkKind::Shell(p.clone()),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.state().links.len(), 1);
    assert!((volume(&d, b) - (24000.0 - 36.0 * 26.0 * 18.0)).abs() < 1.0);
    let link = d.state().links[0].id;
    p.thickness = 3.0;
    d.apply(Command::EditLink {
        link,
        kind: Some(LinkKind::Shell(p.clone())),
        name: None,
    })
    .unwrap();
    assert!((volume(&d, b) - (24000.0 - 34.0 * 24.0 * 17.0)).abs() < 1.0, "{}", volume(&d, b));
    // closed and hollow
    p.open.clear();
    p.thickness = 2.0;
    d.apply(Command::EditLink {
        link,
        kind: Some(LinkKind::Shell(p)),
        name: None,
    })
    .unwrap();
    assert!((volume(&d, b) - (24000.0 - 36.0 * 26.0 * 16.0)).abs() < 1.0);
    // one Undo takes the last change, deleting the link gives the solid body back
    d.apply(Command::DeleteLink { link }).unwrap();
    assert!((volume(&d, b) - 24000.0).abs() < 1e-6);
}

#[test]
fn a_thicker_wall_is_one_wall_only() {
    let (mut d, b) = doc_with_box(40.0, 30.0, 20.0);
    let top = face_with_normal(&d.body(b).unwrap().solid, DVec3::Z);
    let left = face_with_normal(&d.body(b).unwrap().solid, -DVec3::X);
    let mut p = ShellParams::new(b, 2.0);
    p.open = vec![top];
    p.heavier = vec![(left, 3.0)];
    d.apply(Command::AddLink {
        kind: LinkKind::Shell(p),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    // the left wall is 5 thick, the rest 2: the cavity is 33 x 26 x 18
    assert!((volume(&d, b) - (24000.0 - 33.0 * 26.0 * 18.0)).abs() < 1.0, "{}", volume(&d, b));
}

#[test]
fn the_text_is_raised_and_engraved_and_its_link_is_edited() {
    let (mut d, b) = doc_with_box(60.0, 30.0, 10.0);
    let v0 = volume(&d, b);
    let mut p = text_on_top(b, "Rolidium \u{11b}\u{161}\u{10d}\u{159} 123", false, 1.0, 5.0, DVec3::new(5.0, 10.0, 10.0));
    d.apply(Command::AddLink {
        kind: LinkKind::Text(p.clone()),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    let raised = volume(&d, b);
    assert!(raised > v0 + 20.0, "raised text adds material: {v0} -> {raised}");
    let (_, hi) = d.body(b).unwrap().solid.bounding_box().unwrap();
    assert!((hi.z - 11.0).abs() < 1e-6, "the letters stand 1 mm high: {hi:?}");
    // the text is changed through the link
    let link = d.state().links[0].id;
    p.text = "Rolidium".into();
    d.apply(Command::EditLink {
        link,
        kind: Some(LinkKind::Text(p.clone())),
        name: None,
    })
    .unwrap();
    assert!(volume(&d, b) < raised);
    // engraved: the other way
    p.engrave = true;
    p.depth = 0.6;
    p.text = "Rolidium \u{11b}\u{161}\u{10d}\u{159} 123".into();
    d.apply(Command::DeleteLink { link }).unwrap();
    assert!((volume(&d, b) - v0).abs() < 1e-6);
    d.apply(Command::AddLink {
        kind: LinkKind::Text(p),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    assert!(volume(&d, b) < v0 - 5.0, "engraved text takes material away");
    let (_, hi) = d.body(b).unwrap().solid.bounding_box().unwrap();
    assert!((hi.z - 10.0).abs() < 1e-6);
    // the file keeps the link
    let bytes = rolidium_doc::file::save_to_bytes(&d, None, "test").unwrap();
    let d2 = Document::from_loaded(rolidium_doc::file::load_from_bytes(&bytes).unwrap());
    let l = d2.state().links.iter().find(|l| matches!(l.kind, LinkKind::Text(_))).expect("the text comes back");
    match &l.kind {
        LinkKind::Text(t) => assert_eq!((t.text.as_str(), t.engrave, t.depth), ("Rolidium \u{11b}\u{161}\u{10d}\u{159} 123", true, 0.6)),
        _ => unreachable!(),
    }
    assert!((volume(&d2, b) - volume(&d, b)).abs() < 1e-6);
}

#[test]
fn a_shell_goes_through_text_sunk_into_the_wall_and_the_text_stays() {
    // a box, text engraved 0.6 mm into the top wall (the strokes of the letters are about 1 mm wide), then a shell of 2 mm that opens the bottom
    let (mut d, b) = doc_with_box(60.0, 30.0, 12.0);
    let p = text_on_top(b, "Rolidium", true, 0.6, 5.0, DVec3::new(5.0, 10.0, 12.0));
    d.apply(Command::AddLink {
        kind: LinkKind::Text(p),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    let engraved = volume(&d, b);
    let bottom = face_with_normal(&d.body(b).unwrap().solid, -DVec3::Z);
    let mut sp = ShellParams::new(b, 2.0);
    sp.open = vec![bottom];
    d.apply(Command::AddLink {
        kind: LinkKind::Shell(sp),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    let hollow = volume(&d, b);
    let shell_link = d.state().links.iter().find(|l| matches!(l.kind, LinkKind::Shell(_))).unwrap();
    println!("warnings: {:?}", shell_link.warnings);
    assert!(
        shell_link.warnings.iter().any(|w| w.contains("text was taken off")),
        "the shell could not follow the letters and put them back: {:?}",
        shell_link.warnings
    );
    assert!(hollow < engraved * 0.5, "the body is hollow: {engraved} -> {hollow}");
    // the text is still in the top wall: the top is 2 mm thick, the letters go 0.6 mm into it, so there is less material than in the plain shell
    let (plain, _) = {
        let (mut d2, b2) = doc_with_box(60.0, 30.0, 12.0);
        let bottom = face_with_normal(&d2.body(b2).unwrap().solid, -DVec3::Z);
        let mut sp = ShellParams::new(b2, 2.0);
        sp.open = vec![bottom];
        d2.apply(Command::AddLink {
            kind: LinkKind::Shell(sp),
            sources: vec![],
            name: String::new(),
        })
        .unwrap();
        (volume(&d2, b2), ())
    };
    assert!(hollow < plain - 3.0, "the text is still there: hollow {hollow}, plain shell {plain}");
    // the thickness is changed afterwards, the text goes back each time
    let link = d.state().links.iter().find(|l| matches!(l.kind, LinkKind::Shell(_))).unwrap().id;
    let mut sp2 = match &d.state().links.iter().find(|l| l.id == link).unwrap().kind {
        LinkKind::Shell(s) => s.clone(),
        _ => unreachable!(),
    };
    sp2.thickness = 2.5;
    d.apply(Command::EditLink {
        link,
        kind: Some(LinkKind::Shell(sp2)),
        name: None,
    })
    .unwrap();
    assert!(volume(&d, b) > hollow, "thicker walls are more material");
    // and the text is changed after the shell: the shell follows
    let tl = d.state().links.iter().find(|l| matches!(l.kind, LinkKind::Text(_))).unwrap().id;
    let mut tp = match &d.state().links.iter().find(|l| l.id == tl).unwrap().kind {
        LinkKind::Text(t) => t.clone(),
        _ => unreachable!(),
    };
    let before = volume(&d, b);
    tp.text = "Roli".into();
    d.apply(Command::EditLink {
        link: tl,
        kind: Some(LinkKind::Text(tp)),
        name: None,
    })
    .unwrap();
    let after = volume(&d, b);
    assert!(after > before, "less text engraved, more material: {before} -> {after}");
    // still hollow: the shell was calculated again over the new text
    assert!(after < 60.0 * 30.0 * 12.0 * 0.5, "{after}");
    let _ = ShellSide::Inwards;
}

#[test]
fn a_hole_through_a_wall_is_put_back_after_the_shell() {
    let (mut d, b) = doc_with_box(40.0, 30.0, 20.0);
    // a hole of 1.2 mm diameter through the side wall: the wall of the shell offset would have to wrap round it
    d.apply(Command::AddNamedSolids {
        solids: vec![(Solid::cylinder(0.6, 10.0, DVec3::new(-5.0, 15.0, 10.0), DVec3::X).unwrap(), "pin".into())],
    })
    .unwrap();
    let tool = d.bodies().iter().find(|x| x.id != b).unwrap().id;
    d.apply(Command::Combine {
        target: b,
        tools: vec![tool],
        op: rolidium_doc::CombineOp::Cut,
        keep_tools: false,
    })
    .unwrap();
    assert!(!d.body(b).unwrap().solid.contains_point(DVec3::new(1.0, 15.0, 10.0)).unwrap());
    let top = face_with_normal(&d.body(b).unwrap().solid, DVec3::Z);
    let mut sp = ShellParams::new(b, 2.0);
    sp.open = vec![top];
    d.apply(Command::AddLink {
        kind: LinkKind::Shell(sp),
        sources: vec![],
        name: String::new(),
    })
    .unwrap();
    let s = &d.body(b).unwrap().solid;
    assert!(volume(&d, b) > 5000.0);
    assert!(!s.contains_point(DVec3::new(1.0, 15.0, 10.0)).unwrap(), "the hole is still in the wall");
    assert!(s.contains_point(DVec3::new(1.0, 5.0, 10.0)).unwrap(), "the rest of the wall is there");
    assert!(!s.contains_point(DVec3::new(20.0, 15.0, 10.0)).unwrap(), "and the body is hollow");
}

#[test]
fn text_on_the_wall_of_a_mesh_is_raised_and_engraved_with_manifold() {
    use rolidium_doc::NewMesh;
    use rolidium_mesh::gen;
    use std::sync::Arc;
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "m".into(),
            mesh: Arc::new(gen::box_mesh(60.0, 30.0, 10.0, 1)),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    let v0 = d.meshes()[0].mesh.signed_volume();
    // the box mesh is centred on the origin? find the top from the bounding box
    let (lo, hi) = {
        let m = &d.meshes()[0].mesh;
        let z = m.positions.iter().map(|p| p[2]).fold((f32::MAX, f32::MIN), |a, v| (a.0.min(v), a.1.max(v)));
        let x = m.positions.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let y = m.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        (DVec3::new(x as f64, y as f64, z.0 as f64), DVec3::new(0.0, 0.0, z.1 as f64))
    };
    let p = text_on_top(id, "Roli", false, 1.0, 5.0, DVec3::new(lo.x + 5.0, lo.y + 10.0, hi.z));
    d.apply(Command::MeshText { body: id, params: p.clone() }).unwrap();
    let raised = d.meshes()[0].mesh.signed_volume();
    assert!(raised > v0 + 5.0, "raised text adds to the mesh: {v0} -> {raised}");
    assert!(d.undo());
    let mut e = p;
    e.engrave = true;
    e.depth = 0.6;
    d.apply(Command::MeshText { body: id, params: e }).unwrap();
    let engraved = d.meshes()[0].mesh.signed_volume();
    assert!(engraved < v0 - 3.0, "engraved text takes material away: {v0} -> {engraved}");
}
