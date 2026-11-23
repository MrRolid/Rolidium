// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The size of a round can be changed and set back to zero for any number of rounded edges, also when the edges were rounded one
//! after another; a face next to the rounds moves with them (the same commands as the panel sends).
use rolidium_doc::*;
use rolidium_kernel::{DVec3, EdgeId, FaceKind};

fn new_box() -> (Document, BodyId) {
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "B".into(),
        min: DVec3::ZERO,
        max: DVec3::new(40.0, 30.0, 20.0),
    })
    .unwrap();
    let id = d.bodies()[0].id;
    (d, id)
}

/// Lines of the box (point, direction) of the edges to round: the edges of `pick` before anything was rounded.
fn lines(d: &Document, id: BodyId, pick: &dyn Fn(usize, DVec3, DVec3) -> bool) -> Vec<(DVec3, DVec3)> {
    let s = &d.body(id).unwrap().solid;
    s.edges()
        .enumerate()
        .filter_map(|(k, e)| {
            let i = s.edge_info(e).unwrap();
            pick(k, i.start, i.end).then(|| (i.start, (i.end - i.start).normalize()))
        })
        .collect()
}

fn round_one_by_one(d: &mut Document, id: BodyId, edges: &[(DVec3, DVec3)], size: f64, chamfer: bool) {
    for (p0, dir) in edges {
        let e = {
            let s = &d.body(id).unwrap().solid;
            s.edges().find(|e| {
                let i = s.edge_info(*e).unwrap();
                let v = (i.start + i.end) * 0.5 - *p0;
                let off = v - *dir * v.dot(*dir);
                i.start.distance(i.end) > 1e-6
                    && (i.end - i.start).normalize().dot(*dir).abs() > 0.9999
                    && off.length() < 1e-6
            })
        };
        // an edge that a neighbouring round already took along (the kernel rounds tangent chains) is not rounded again
        let Some(e): Option<EdgeId> = e else { continue };
        d.apply(Command::FilletEdges {
            body: id,
            edges: vec![e.0],
            size,
            chamfer,
        })
        .unwrap_or_else(|e| panic!("round of one edge: {e}"));
    }
}

fn any_round_face(d: &Document, id: BodyId) -> rolidium_kernel::FaceId {
    d.body(id).unwrap().features[0].faces[0]
}

#[test]
fn rounding_edges_one_by_one_the_size_can_be_changed_and_set_to_zero_for_two_three_four_and_twelve_edges() {
    type Pick = Box<dyn Fn(usize, DVec3, DVec3) -> bool>;
    let cases: Vec<(&str, Pick)> = vec![
        ("two", Box::new(|k, _, _| k < 2)),
        (
            "three at a corner",
            Box::new(|_, a, b| a.length() < 1e-6 || b.length() < 1e-6),
        ),
        ("four round the top", Box::new(|_, a, b| a.z > 19.9 && b.z > 19.9)),
        ("twelve", Box::new(|_, _, _| true)),
    ];
    for (name, pick) in &cases {
        for chamfer in [false, true] {
            let (mut d, id) = new_box();
            let edges = lines(&d, id, &**pick);
            let size = if *name == "twelve" { 2.0 } else { 3.0 };
            if chamfer || *name == "twelve" {
                // chamfers (made one after another, three chamfers at a corner are not equal any more) and all twelve edges (the kernel
                // does not manage them one after another) in one command
                let ids: Vec<u32> = {
                    let s = &d.body(id).unwrap().solid;
                    edges
                        .iter()
                        .map(|(p0, dir)| {
                            s.edges()
                                .find(|e| {
                                    let i = s.edge_info(*e).unwrap();
                                    let v = (i.start + i.end) * 0.5 - *p0;
                                    (v - *dir * v.dot(*dir)).length() < 1e-6
                                        && (i.end - i.start).normalize().dot(*dir).abs() > 0.9999
                                })
                                .unwrap()
                                .0
                        })
                        .collect()
                };
                d.apply(Command::FilletEdges { body: id, edges: ids, size, chamfer }).unwrap();
            } else {
                round_one_by_one(&mut d, id, &edges, size, chamfer);
            }
            let s = &d.body(id).unwrap().solid;
            assert!(s.is_valid(), "{name}");
            let face = any_round_face(&d, id);
            // all of them follow (whole chain)
            let new = size * 0.5;
            d.apply(Command::EditBlend {
                body: id,
                face,
                size: new,
                chain: true,
            })
            .unwrap_or_else(|e| panic!("{name} chamfer={chamfer}: change: {e}"));
            let s = &d.body(id).unwrap().solid;
            assert!(s.is_valid(), "{name}");
            for f in &d.body(id).unwrap().features {
                assert!((f.size - new).abs() < 1e-6, "{name} chamfer={chamfer}: {} != {new}", f.size);
            }
            // zero: sharp edges and the six faces of the box
            let face = any_round_face(&d, id);
            d.apply(Command::EditBlend {
                body: id,
                face,
                size: 0.0,
                chain: true,
            })
            .unwrap_or_else(|e| panic!("{name} chamfer={chamfer}: zero: {e}"));
            let b = d.body(id).unwrap();
            assert_eq!(b.solid.face_count(), 6, "{name} chamfer={chamfer}");
            assert!((b.solid.volume().unwrap() - 24000.0).abs() < 1e-6);
            assert!(b.features.is_empty());
        }
    }
}

#[test]
fn a_round_of_three_edges_at_a_corner_can_be_changed_alone_without_the_chain() {
    let (mut d, id) = new_box();
    let edges = lines(&d, id, &|_, a, b| a.length() < 1e-6 || b.length() < 1e-6);
    round_one_by_one(&mut d, id, &edges, 3.0, false);
    let face = any_round_face(&d, id);
    d.apply(Command::EditBlend {
        body: id,
        face,
        size: 2.0,
        chain: false,
    })
    .unwrap();
    let sizes: Vec<f64> = d.body(id).unwrap().features.iter().map(|f| f.size).collect();
    assert_eq!(sizes.iter().filter(|s| (**s - 2.0).abs() < 1e-6).count(), 1, "{sizes:?}");
    assert_eq!(sizes.iter().filter(|s| (**s - 3.0).abs() < 1e-6).count(), 2, "{sizes:?}");
}

#[test]
fn the_panel_commands_move_a_face_with_its_rounds_and_a_failure_changes_nothing() {
    let (mut d, id) = new_box();
    let edges = lines(&d, id, &|_, a, b| a.z > 19.9 && b.z > 19.9);
    round_one_by_one(&mut d, id, &edges, 3.0, false);
    let top = |d: &Document| {
        let s = &d.body(id).unwrap().solid;
        s.faces()
            .find(|f| {
                let i = s.face_info(*f).unwrap();
                i.kind == FaceKind::Plane && i.normal.z > 0.99
            })
            .unwrap()
    };
    let faces = d.body(id).unwrap().solid.face_count();
    d.apply(Command::PullFace {
        body: id,
        face: top(&d),
        distance: 5.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    let b = d.body(id).unwrap();
    let (lo, hi) = b.solid.bounding_box().unwrap();
    assert!((hi.z - lo.z - 25.0).abs() < 1e-6);
    assert_eq!(b.solid.face_count(), faces);
    assert!(b.features.len() >= 4, "{} rounds", b.features.len());
    assert!(b.features.iter().all(|f| (f.size - 3.0).abs() < 1e-9));
    // too far inwards: refused, nothing changes
    let rev = d.revision();
    let vol = d.body(id).unwrap().solid.volume().unwrap();
    let e = d
        .apply(Command::PullFace {
            body: id,
            face: top(&d),
            distance: -24.0,
            mode: PullMode::Auto,
        })
        .unwrap_err();
    assert!(!e.to_string().is_empty());
    assert_eq!(d.revision(), rev);
    assert_eq!(d.body(id).unwrap().solid.volume().unwrap(), vol);
}
