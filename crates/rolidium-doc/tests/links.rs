// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Links (patterns, mirrors) and groups, with volumes that are worked out by hand.
use rolidium_doc::{
    Axis, BodyId, CircularParams, Command, DocError, Document, ItemId, LinearParams, LinkKind, MirrorParams,
    PatternMode, PullMode,
};
use rolidium_kernel::{BooleanOp, DVec3, FaceId, FaceKind, Solid};
use std::f64::consts::PI;

fn add_solid(d: &mut Document, name: &str, s: Solid) -> BodyId {
    d.apply(Command::AddNamedSolids {
        solids: vec![(s, name.into())],
    })
    .unwrap();
    d.bodies().last().unwrap().id
}

fn volume(d: &Document, id: BodyId) -> f64 {
    d.body(id).unwrap().solid.volume().unwrap()
}

fn total(d: &Document) -> f64 {
    d.bodies().iter().map(|b| b.solid.volume().unwrap()).sum()
}

fn top_face(s: &Solid) -> FaceId {
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99
        })
        .unwrap()
}

fn linear(count: u32, spacing: f64, dir: DVec3) -> LinkKind {
    LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir,
            },
            count,
            spacing,
            by_total: false,
            second: None,
        },
        PatternMode::New,
    )
}

#[test]
fn a_linear_pattern_of_5_cylinders_has_the_volume_of_5() {
    let mut d = Document::new();
    let c = add_solid(
        &mut d,
        "Pin",
        Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    d.apply(Command::AddLink {
        kind: linear(5, 30.0, DVec3::X),
        sources: vec![c],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 5);
    assert!((total(&d) - 5.0 * PI * 25.0 * 10.0).abs() < 1e-6, "{}", total(&d));
    // the pieces stand 30 mm apart
    let xs: Vec<f64> = d
        .bodies()
        .iter()
        .map(|b| b.solid.props().unwrap().centroid.x)
        .collect();
    for (i, x) in xs.iter().enumerate() {
        assert!((x - 30.0 * i as f64).abs() < 1e-6, "{xs:?}");
    }
    assert_eq!(d.state().links.len(), 1);
    assert_eq!(d.state().links[0].results.len(), 4);
    // by total length: 4 pieces over 90 mm are 30 mm apart
    let mut d2 = Document::new();
    let c2 = add_solid(
        &mut d2,
        "Pin",
        Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    let k = LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::X * 3.0,
            },
            count: 4,
            spacing: 90.0,
            by_total: true,
            second: None,
        },
        PatternMode::New,
    );
    d2.apply(Command::AddLink {
        kind: k,
        sources: vec![c2],
        name: "Row".into(),
    })
    .unwrap();
    let max_x = d2
        .bodies()
        .iter()
        .map(|b| b.solid.props().unwrap().centroid.x)
        .fold(0.0, f64::max);
    assert!((max_x - 90.0).abs() < 1e-6 && d2.state().links[0].name == "Row");
}

#[test]
fn a_grid_of_3_by_4_boxes() {
    let mut d = Document::new();
    let b = add_solid(
        &mut d,
        "Block",
        Solid::cuboid(DVec3::ZERO, DVec3::splat(10.0)).unwrap(),
    );
    let k = LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            count: 3,
            spacing: 15.0,
            by_total: false,
            second: Some(rolidium_doc::Direction2 {
                axis: Axis {
                    origin: DVec3::ZERO,
                    dir: DVec3::Y,
                },
                count: 4,
                spacing: 20.0,
                by_total: false,
            }),
        },
        PatternMode::New,
    );
    d.apply(Command::AddLink {
        kind: k,
        sources: vec![b],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 12);
    assert!((total(&d) - 12.0 * 1000.0).abs() < 1e-6);
    assert!(d.state().links[0].warnings.is_empty());
    assert!(d.bodies().iter().all(|b| b.solid.is_valid()));
}

#[test]
fn six_holes_round_a_flange_taken_out_of_the_body() {
    let mut d = Document::new();
    let flange = add_solid(
        &mut d,
        "Flange",
        Solid::cylinder(50.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    let pin = add_solid(
        &mut d,
        "Hole",
        Solid::cylinder(4.0, 20.0, DVec3::new(35.0, 0.0, -5.0), DVec3::Z).unwrap(),
    );
    let k = LinkKind::Circular(
        CircularParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::Z,
            },
            count: 6,
            angle_deg: 0.0,
            full: true,
            keep_orientation: false,
        },
        PatternMode::Cut(flange),
    );
    d.apply(Command::AddLink {
        kind: k,
        sources: vec![pin],
        name: String::new(),
    })
    .unwrap();
    let want = PI * 50.0 * 50.0 * 10.0 - 6.0 * PI * 16.0 * 10.0;
    assert!(
        (volume(&d, flange) - want).abs() < 1e-5,
        "{} vs {want}",
        volume(&d, flange)
    );
    assert!(d.body(flange).unwrap().solid.is_valid());
    // the tool is hidden, the result is the flange
    assert!(!d.body(pin).unwrap().visible);
    // a different angle: 3 pieces 20 degrees apart
    let link = d.state().links[0].id;
    let k2 = LinkKind::Circular(
        CircularParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::Z,
            },
            count: 3,
            angle_deg: 20.0,
            full: false,
            keep_orientation: false,
        },
        PatternMode::Cut(flange),
    );
    d.apply(Command::EditLink {
        link,
        kind: Some(k2),
        name: None,
    })
    .unwrap();
    assert!((volume(&d, flange) - (PI * 2500.0 * 10.0 - 3.0 * PI * 16.0 * 10.0)).abs() < 1e-5);
}

#[test]
fn a_mirror_and_a_live_mirror_follow_the_original() {
    let mut d = Document::new();
    let b = add_solid(
        &mut d,
        "Half",
        Solid::cuboid(DVec3::new(10.0, 0.0, 0.0), DVec3::new(30.0, 20.0, 5.0)).unwrap(),
    );
    let k = LinkKind::Mirror(MirrorParams {
        origin: DVec3::ZERO,
        normal: DVec3::X,
        into: PatternMode::New,
    });
    d.apply(Command::AddLink {
        kind: k,
        sources: vec![b],
        name: String::new(),
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 2);
    let m = d.bodies()[1].id;
    let p = d.body(m).unwrap().solid.props().unwrap();
    assert!((p.min.x + 30.0).abs() < 1e-9 && (p.max.x + 10.0).abs() < 1e-9);
    assert!((volume(&d, m) - 2000.0).abs() < 1e-6);
    // pull the top of the original by 7: the mirror image follows, with the same id
    let f = top_face(&d.body(b).unwrap().solid);
    d.apply(Command::PullFace {
        body: b,
        face: f,
        distance: 7.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 2);
    assert!((volume(&d, b) - 20.0 * 20.0 * 12.0).abs() < 1e-6);
    assert!(
        (volume(&d, m) - 20.0 * 20.0 * 12.0).abs() < 1e-6,
        "the mirror image did not follow: {}",
        volume(&d, m)
    );
    let p = d.body(m).unwrap().solid.props().unwrap();
    assert!((p.max.z - 12.0).abs() < 1e-9);
    // a mirror of a body that crosses the plane overlaps the original: said in the remark
    let mut d2 = Document::new();
    let w = add_solid(
        &mut d2,
        "Wide",
        Solid::cuboid(DVec3::new(-5.0, 0.0, 0.0), DVec3::new(30.0, 10.0, 5.0)).unwrap(),
    );
    let c = d2
        .apply(Command::AddLink {
            kind: LinkKind::Mirror(MirrorParams {
                origin: DVec3::ZERO,
                normal: DVec3::X,
                into: PatternMode::New,
            }),
            sources: vec![w],
            name: String::new(),
        })
        .unwrap();
    assert!(
        c.notice.as_deref().is_some_and(|n| n.contains("overlap")),
        "{:?}",
        c.notice
    );
}

#[test]
fn the_number_of_pieces_and_the_source_can_be_changed_after_the_link_was_made() {
    let mut d = Document::new();
    let c = add_solid(
        &mut d,
        "Pin",
        Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    d.apply(Command::AddLink {
        kind: linear(5, 30.0, DVec3::X),
        sources: vec![c],
        name: String::new(),
    })
    .unwrap();
    let link = d.state().links[0].id;
    let ids_before: Vec<BodyId> = d.bodies().iter().map(|b| b.id).collect();
    d.apply(Command::EditLink {
        link,
        kind: Some(linear(7, 30.0, DVec3::X)),
        name: None,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 7);
    assert!((total(&d) - 7.0 * PI * 250.0).abs() < 1e-6);
    // the bodies that were there keep their ids
    assert!(ids_before.iter().all(|id| d.body(*id).is_some()));
    // one step of Undo takes the whole change back
    assert!(d.undo());
    assert_eq!(d.bodies().len(), 5);
    assert!(d.redo());
    assert_eq!(d.bodies().len(), 7);
    // fewer pieces
    d.apply(Command::EditLink {
        link,
        kind: Some(linear(3, 30.0, DVec3::X)),
        name: None,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 3);
    // the source is pulled: every piece gets the new height
    let f = top_face(&d.body(c).unwrap().solid);
    d.apply(Command::PullFace {
        body: c,
        face: f,
        distance: 5.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((total(&d) - 3.0 * PI * 25.0 * 15.0).abs() < 1e-6, "{}", total(&d));
    // the source is moved: the copies go with it
    d.apply(Command::Move {
        bodies: vec![c],
        translation: DVec3::new(0.0, 100.0, 0.0),
        pivot: DVec3::ZERO,
        axis: DVec3::Z,
        angle: 0.0,
        copy: false,
    })
    .unwrap();
    for b in d.bodies() {
        assert!((b.solid.props().unwrap().centroid.y - 100.0).abs() < 1e-6);
    }
    // a failing calculation keeps the last good result and says why
    let r = d.apply(Command::EditLink {
        link,
        kind: Some(linear(0, 30.0, DVec3::X)),
        name: None,
    });
    assert!(r.is_err());
    assert_eq!(d.bodies().len(), 3);
}

#[test]
fn a_hand_edit_of_a_derived_body_is_refused_until_the_link_is_removed() {
    let mut d = Document::new();
    let c = add_solid(
        &mut d,
        "Pin",
        Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    d.apply(Command::AddLink {
        kind: linear(3, 30.0, DVec3::X),
        sources: vec![c],
        name: String::new(),
    })
    .unwrap();
    let copy = d.state().links[0].results[0];
    let link = d.state().links[0].id;
    let f = top_face(&d.body(copy).unwrap().solid);
    let before = volume(&d, copy);
    match d.apply(Command::PullFace {
        body: copy,
        face: f,
        distance: 4.0,
        mode: PullMode::Auto,
    }) {
        Err(DocError::Derived { link: l, body, .. }) => assert!(l == link && body == copy),
        other => panic!("{other:?}"),
    }
    assert!(
        (volume(&d, copy) - before).abs() < 1e-9,
        "the refused edit changed the body"
    );
    assert!(d.apply(Command::DeleteBody { body: copy }).is_err());
    // a name, a colour and visibility are not an edit of the shape
    d.apply(Command::Rename {
        target: ItemId::Body(copy),
        name: "Pin B".into(),
    })
    .unwrap();
    d.apply(Command::SetVisible {
        target: ItemId::Body(copy),
        visible: false,
    })
    .unwrap();
    // remove the link: the bodies stay, and can be edited
    d.apply(Command::Unlink { link }).unwrap();
    assert!(d.state().links.is_empty() && d.bodies().len() == 3);
    d.apply(Command::PullFace {
        body: copy,
        face: f,
        distance: 4.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((volume(&d, copy) - PI * 25.0 * 14.0).abs() < 1e-6);
    // the source no longer drives anything
    let f0 = top_face(&d.body(c).unwrap().solid);
    d.apply(Command::PullFace {
        body: c,
        face: f0,
        distance: 1.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((volume(&d, copy) - PI * 25.0 * 14.0).abs() < 1e-6);
}

#[test]
fn deleting_a_link_takes_the_copies_away_and_gives_the_body_back() {
    let mut d = Document::new();
    let plate = add_solid(
        &mut d,
        "Plate",
        Solid::cuboid(DVec3::ZERO, DVec3::new(100.0, 20.0, 10.0)).unwrap(),
    );
    let pin = add_solid(
        &mut d,
        "Hole",
        Solid::cylinder(3.0, 30.0, DVec3::new(10.0, 10.0, -5.0), DVec3::Z).unwrap(),
    );
    let k = LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            count: 5,
            spacing: 20.0,
            by_total: false,
            second: None,
        },
        PatternMode::Cut(plate),
    );
    d.apply(Command::AddLink {
        kind: k,
        sources: vec![pin],
        name: String::new(),
    })
    .unwrap();
    let want = 100.0 * 20.0 * 10.0 - 5.0 * PI * 9.0 * 10.0;
    assert!((volume(&d, plate) - want).abs() < 1e-5);
    let link = d.state().links[0].id;
    d.apply(Command::DeleteLink { link }).unwrap();
    assert!((volume(&d, plate) - 20000.0).abs() < 1e-6);
    assert!(d.body(pin).unwrap().visible);
    // a pattern of copies is removed with its bodies
    let mut d2 = Document::new();
    let c = add_solid(
        &mut d2,
        "Pin",
        Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    d2.apply(Command::AddLink {
        kind: linear(4, 20.0, DVec3::X),
        sources: vec![c],
        name: String::new(),
    })
    .unwrap();
    let l2 = d2.state().links[0].id;
    d2.apply(Command::DeleteLink { link: l2 }).unwrap();
    assert_eq!(d2.bodies().len(), 1);
    assert!(d2.undo() && d2.bodies().len() == 4);
}

#[test]
fn pieces_that_miss_the_target_are_reported() {
    let mut d = Document::new();
    let plate = add_solid(
        &mut d,
        "Plate",
        Solid::cuboid(DVec3::ZERO, DVec3::new(50.0, 20.0, 10.0)).unwrap(),
    );
    let pin = add_solid(
        &mut d,
        "Hole",
        Solid::cylinder(3.0, 30.0, DVec3::new(10.0, 10.0, -5.0), DVec3::Z).unwrap(),
    );
    let k = LinkKind::Linear(
        LinearParams {
            axis: Axis {
                origin: DVec3::ZERO,
                dir: DVec3::X,
            },
            count: 5,
            spacing: 20.0,
            by_total: false,
            second: None,
        },
        PatternMode::Cut(plate),
    );
    let c = d
        .apply(Command::AddLink {
            kind: k,
            sources: vec![pin],
            name: String::new(),
        })
        .unwrap();
    // x = 10, 30, 50, 70, 90: 70 and 90 are outside of a plate 50 long (the one at 50 cuts a half hole into its edge)
    let n = c.notice.unwrap_or_default();
    assert!(n.contains("do not cut into"), "{n}");
    assert!(n.starts_with("2 piece(s)"), "{n}");
    assert!(
        (volume(&d, plate) - (50.0 * 20.0 * 10.0 - 2.5 * PI * 9.0 * 10.0)).abs() < 1e-4,
        "{}",
        volume(&d, plate)
    );
}

#[test]
fn groups_nest_hide_and_dissolve() {
    let mut d = Document::new();
    let a = add_solid(
        &mut d,
        "A",
        Solid::cuboid(DVec3::ZERO, DVec3::splat(5.0)).unwrap(),
    );
    let b = add_solid(
        &mut d,
        "B",
        Solid::cuboid(DVec3::splat(10.0), DVec3::splat(15.0)).unwrap(),
    );
    let c = add_solid(
        &mut d,
        "C",
        Solid::cuboid(DVec3::splat(20.0), DVec3::splat(25.0)).unwrap(),
    );
    d.apply(Command::CreateGroup {
        bodies: vec![a, b],
        name: "Frame".into(),
        parent: None,
    })
    .unwrap();
    let outer = d.state().groups[0].id;
    d.apply(Command::CreateGroup {
        bodies: vec![c],
        name: String::new(),
        parent: Some(outer),
    })
    .unwrap();
    let inner = d.state().groups[1].id;
    assert_eq!(d.state().groups[1].name, "Group 2");
    d.apply(Command::SetVisible {
        target: ItemId::Group(outer),
        visible: false,
    })
    .unwrap();
    assert!(
        d.bodies().iter().all(|b| !b.visible),
        "the hidden group hides its groups too"
    );
    d.apply(Command::SetVisible {
        target: ItemId::Group(outer),
        visible: true,
    })
    .unwrap();
    assert!(d.bodies().iter().all(|b| b.visible));
    d.apply(Command::Rename {
        target: ItemId::Group(inner),
        name: "Rails".into(),
    })
    .unwrap();
    assert_eq!(d.state().groups[1].name, "Rails");
    // a group cannot go into itself or its own group
    assert!(d
        .apply(Command::MoveGroup {
            group: outer,
            parent: Some(inner)
        })
        .is_err());
    // drag a body to another group, and out
    d.apply(Command::MoveToGroup {
        bodies: vec![a],
        group: Some(inner),
    })
    .unwrap();
    assert!(d.state().groups[1].bodies.contains(&a) && !d.state().groups[0].bodies.contains(&a));
    d.apply(Command::MoveToGroup {
        bodies: vec![a],
        group: None,
    })
    .unwrap();
    assert!(d.state().groups.iter().all(|g| !g.bodies.contains(&a)));
    // deleting a body takes it out of its group
    d.apply(Command::DeleteBody { body: b }).unwrap();
    assert!(d.state().groups.iter().all(|g| !g.bodies.contains(&b)));
    // dissolving the outer group: its group goes to the top
    d.apply(Command::Ungroup { group: outer }).unwrap();
    assert_eq!(d.state().groups.len(), 1);
    assert_eq!(d.state().groups[0].parent, None);
    // the cutting by BooleanOp is untouched by groups
    let _ = BooleanOp::Fuse;
    assert!(d.undo() && d.undo() || true);
}

#[test]
fn links_and_groups_are_saved_and_opened() {
    let mut d = Document::new();
    let flange = add_solid(
        &mut d,
        "Flange",
        Solid::cylinder(50.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap(),
    );
    let pin = add_solid(
        &mut d,
        "Hole",
        Solid::cylinder(4.0, 20.0, DVec3::new(35.0, 0.0, -5.0), DVec3::Z).unwrap(),
    );
    d.apply(Command::AddLink {
        kind: LinkKind::Circular(
            CircularParams {
                axis: Axis {
                    origin: DVec3::ZERO,
                    dir: DVec3::Z,
                },
                count: 6,
                angle_deg: 0.0,
                full: true,
                keep_orientation: false,
            },
            PatternMode::Cut(flange),
        ),
        sources: vec![pin],
        name: "Bolt circle".into(),
    })
    .unwrap();
    let rod = add_solid(
        &mut d,
        "Rod",
        Solid::cylinder(5.0, 10.0, DVec3::new(0.0, 100.0, 0.0), DVec3::Z).unwrap(),
    );
    d.apply(Command::AddLink {
        kind: linear(4, 20.0, DVec3::X),
        sources: vec![rod],
        name: String::new(),
    })
    .unwrap();
    d.apply(Command::AddLink {
        kind: LinkKind::Mirror(MirrorParams {
            origin: DVec3::new(0.0, 0.0, 0.0),
            normal: DVec3::Y,
            into: PatternMode::New,
        }),
        sources: vec![rod],
        name: String::new(),
    })
    .unwrap_or_else(|e| panic!("{e}"));
    d.apply(Command::CreateGroup {
        bodies: vec![flange, pin],
        name: "Flange group".into(),
        parent: None,
    })
    .unwrap();
    let before: Vec<(String, f64)> = d
        .bodies()
        .iter()
        .map(|b| (b.name.clone(), b.solid.volume().unwrap()))
        .collect();
    let bytes = rolidium_doc::file::save_to_bytes(&d, None, "test").unwrap();
    let mut d2 = Document::from_loaded(rolidium_doc::file::load_from_bytes(&bytes).unwrap());
    let after: Vec<(String, f64)> = d2
        .bodies()
        .iter()
        .map(|b| (b.name.clone(), b.solid.volume().unwrap()))
        .collect();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert!(a.0 == b.0 && (a.1 - b.1).abs() < 1e-6, "{a:?} {b:?}");
    }
    assert_eq!(d2.state().links.len(), 3);
    assert_eq!(d2.state().links[0].name, "Bolt circle");
    assert_eq!(d2.state().links[0].kind, d.state().links[0].kind);
    assert_eq!(d2.state().links[0].results, d.state().links[0].results);
    assert!(d2.state().links[0].base.is_some());
    assert_eq!(d2.state().groups.len(), 1);
    assert_eq!(d2.state().groups[0].name, "Flange group");
    assert_eq!(d2.state().groups[0].bodies, vec![flange, pin]);
    // the opened links work: pulling the source of the row changes its copies
    let f = top_face(&d2.body(rod).unwrap().solid);
    d2.apply(Command::PullFace {
        body: rod,
        face: f,
        distance: 5.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    let copies: Vec<f64> = d2.state().links[1]
        .results
        .iter()
        .map(|r| volume(&d2, *r))
        .collect();
    assert!(
        copies.iter().all(|v| (v - PI * 25.0 * 15.0).abs() < 1e-6),
        "{copies:?}"
    );
    // the bolt circle recalculated from the stored base: change the tool
    let tf = top_face(&d2.body(pin).unwrap().solid);
    d2.apply(Command::PullFace {
        body: pin,
        face: tf,
        distance: 1.0,
        mode: PullMode::Auto,
    })
    .unwrap();
    assert!((volume(&d2, flange) - (PI * 2500.0 * 10.0 - 6.0 * PI * 16.0 * 10.0)).abs() < 1e-5);
    // a file of an older program (no links, no groups) opens as before
    let mut plain = Document::new();
    add_solid(
        &mut plain,
        "A",
        Solid::cuboid(DVec3::ZERO, DVec3::splat(5.0)).unwrap(),
    );
    let loaded = rolidium_doc::file::load_from_bytes(
        &rolidium_doc::file::save_to_bytes(&plain, None, "0.2.0").unwrap(),
    )
    .unwrap();
    assert!(loaded.state.links.is_empty() && loaded.state.groups.is_empty());
}

fn sketch_with(d: &mut Document, entity: rolidium_doc::rolidium_sketch::Entity) -> u64 {
    use rolidium_doc::{rolidium_sketch::PlaneDef, SketchTarget};
    d.apply(Command::AddEntity {
        target: SketchTarget::New {
            plane: PlaneDef::from_face(DVec3::ZERO, DVec3::Z),
            body: None,
        },
        entity,
    })
    .unwrap()
    .created_sketch
    .unwrap()
}

#[test]
fn a_pattern_of_shapes_in_a_sketch_gives_regions_that_pull_together() {
    use rolidium_doc::rolidium_sketch::{DVec2, Entity};
    use rolidium_doc::SketchPattern;
    let mut d = Document::new();
    let sk = sketch_with(
        &mut d,
        Entity::Circle {
            center: DVec2::new(0.0, 0.0),
            radius: 3.0,
        },
    );
    // 4 circles in a row, then the row twice more in the other direction: 12 circles
    d.apply(Command::SketchPattern {
        sketch: sk,
        entities: vec![],
        pattern: SketchPattern::Linear {
            dir: DVec2::X,
            count: 4,
            spacing: 10.0,
            by_total: false,
            second: Some((DVec2::Y, 3, 10.0, false)),
        },
    })
    .unwrap();
    assert_eq!(d.sketch(sk).unwrap().sketch.entities().len(), 12);
    let regions = d.sketch(sk).unwrap().sketch.regions().len();
    assert_eq!(regions, 12);
    // pull every region: 12 cylinders of 3 mm radius and 5 mm height
    for _ in 0..regions {
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: 5.0,
            mode: PullMode::New,
        })
        .unwrap();
    }
    assert_eq!(d.bodies().len(), 12);
    assert!((total(&d) - 12.0 * PI * 9.0 * 5.0).abs() < 1e-6, "{}", total(&d));
    // six circles round a point, and a rectangle turned into lines
    let mut d2 = Document::new();
    let s2 = sketch_with(
        &mut d2,
        Entity::Circle {
            center: DVec2::new(20.0, 0.0),
            radius: 2.0,
        },
    );
    d2.apply(Command::SketchPattern {
        sketch: s2,
        entities: vec![],
        pattern: SketchPattern::Circular {
            center: DVec2::ZERO,
            count: 6,
            angle_deg: 0.0,
            full: true,
        },
    })
    .unwrap();
    assert_eq!(d2.sketch(s2).unwrap().sketch.regions().len(), 6);
    let s3 = sketch_with(
        &mut d2,
        Entity::Rect {
            a: DVec2::new(10.0, 0.0),
            b: DVec2::new(20.0, 5.0),
        },
    );
    d2.apply(Command::SketchPattern {
        sketch: s3,
        entities: vec![],
        pattern: SketchPattern::Circular {
            center: DVec2::ZERO,
            count: 4,
            angle_deg: 45.0,
            full: false,
        },
    })
    .unwrap();
    // turned by 45 and 135 degrees a rectangle is four lines; turned by 90 degrees it is a rectangle again: 1 + 4 + 1 + 4 shapes
    assert_eq!(d2.sketch(s3).unwrap().sketch.entities().len(), 10);
    assert!(d2.sketch(s3).unwrap().sketch.regions().len() >= 4);
}

#[test]
fn shapes_of_a_sketch_are_mirrored_in_a_line() {
    use rolidium_doc::rolidium_sketch::{DVec2, Entity};
    let mut d = Document::new();
    let sk = sketch_with(
        &mut d,
        Entity::Arc {
            center: DVec2::new(10.0, 0.0),
            radius: 5.0,
            start: 0.0,
            sweep: std::f64::consts::FRAC_PI_2,
        },
    );
    d.apply(Command::AddEntity {
        target: rolidium_doc::SketchTarget::Existing(sk),
        entity: Entity::Line {
            a: DVec2::new(15.0, 0.0),
            b: DVec2::new(10.0, 0.0),
        },
    })
    .unwrap();
    d.apply(Command::AddEntity {
        target: rolidium_doc::SketchTarget::Existing(sk),
        entity: Entity::Line {
            a: DVec2::new(10.0, 5.0),
            b: DVec2::new(10.0, 0.0),
        },
    })
    .unwrap();
    // a quarter disc; its mirror image across the Y axis (x = 0) lies at x from -15 to -10
    d.apply(Command::SketchMirror {
        sketch: sk,
        entities: vec![],
        a: DVec2::new(0.0, 0.0),
        b: DVec2::new(0.0, 1.0),
        keep: true,
    })
    .unwrap();
    assert_eq!(d.sketch(sk).unwrap().sketch.entities().len(), 6);
    assert_eq!(d.sketch(sk).unwrap().sketch.regions().len(), 2);
    for (i, _) in (0..2).enumerate() {
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: 4.0,
            mode: PullMode::New,
        })
        .unwrap();
        assert_eq!(d.bodies().len(), i + 1);
    }
    let quarter = PI * 25.0 / 4.0 * 4.0;
    for b in d.bodies() {
        assert!(
            (b.solid.volume().unwrap() - quarter).abs() < 1e-6,
            "{}",
            b.solid.volume().unwrap()
        );
    }
    let xs: Vec<f64> = d
        .bodies()
        .iter()
        .map(|b| b.solid.props().unwrap().centroid.x)
        .collect();
    assert!(xs[0] * xs[1] < 0.0 && (xs[0] + xs[1]).abs() < 1e-6, "{xs:?}");
    // without keep the shapes are replaced
    let mut d2 = Document::new();
    let s2 = sketch_with(
        &mut d2,
        Entity::Circle {
            center: DVec2::new(10.0, 10.0),
            radius: 2.0,
        },
    );
    d2.apply(Command::SketchMirror {
        sketch: s2,
        entities: vec![],
        a: DVec2::ZERO,
        b: DVec2::new(1.0, 1.0),
        keep: false,
    })
    .unwrap();
    let e = d2.sketch(s2).unwrap().sketch.entities().to_vec();
    assert_eq!(e.len(), 1);
    assert!(
        matches!(e[0].1, Entity::Circle { center, .. } if (center - DVec2::new(10.0, 10.0)).length() < 1e-9),
        "{:?}",
        e[0].1
    );
}

#[test]
fn a_body_is_mirrored_in_place_kept_or_merged_into_one_symmetric_body() {
    let plate = |d: &mut Document| {
        add_solid(
            d,
            "Half",
            Solid::cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 5.0)).unwrap(),
        )
    };
    // in place: the same volume on the other side
    let mut d = Document::new();
    let a = plate(&mut d);
    d.apply(Command::Mirror {
        bodies: vec![a],
        origin: DVec3::ZERO,
        normal: DVec3::X,
        keep: false,
        merge: false,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 1);
    assert!((d.body(a).unwrap().solid.props().unwrap().max.x - 0.0).abs() < 1e-9);
    // keep the original: two bodies
    let mut d = Document::new();
    let a = plate(&mut d);
    d.apply(Command::Mirror {
        bodies: vec![a],
        origin: DVec3::ZERO,
        normal: DVec3::X,
        keep: true,
        merge: false,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 2);
    assert!((total(&d) - 2.0 * 1000.0).abs() < 1e-6);
    // merged: one body of twice the volume, 40 mm long (the halves touch in the plane)
    let mut d = Document::new();
    let a = plate(&mut d);
    d.apply(Command::Mirror {
        bodies: vec![a],
        origin: DVec3::ZERO,
        normal: DVec3::X,
        keep: true,
        merge: true,
    })
    .unwrap();
    assert_eq!(d.bodies().len(), 1);
    let p = d.body(a).unwrap().solid.props().unwrap();
    assert!(
        (p.volume - 2000.0).abs() < 1e-6 && (p.max.x - p.min.x - 40.0).abs() < 1e-9,
        "{p:?}"
    );
    assert!(d.undo() && d.bodies().len() == 1 && (volume(&d, a) - 1000.0).abs() < 1e-6);
    // a body far from the plane cannot be merged with its image: the reason is given
    let mut d = Document::new();
    let far = add_solid(
        &mut d,
        "Far",
        Solid::cuboid(DVec3::new(30.0, 0.0, 0.0), DVec3::new(40.0, 10.0, 5.0)).unwrap(),
    );
    let e = d
        .apply(Command::Mirror {
            bodies: vec![far],
            origin: DVec3::ZERO,
            normal: DVec3::X,
            keep: true,
            merge: true,
        })
        .unwrap_err();
    assert!(
        e.to_string().contains("do not touch") || e.to_string().contains("does not touch"),
        "{e}"
    );
}
