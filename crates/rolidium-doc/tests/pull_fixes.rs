// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! E1: a wall next to rounds is pulled again and again; when the rounds cannot be made again after a pull, the wall is moved without them and a notice says so
//! (it is never refused with a message about the round).
use rolidium_doc::*;
use rolidium_kernel::{DVec3, FaceKind};

fn stepped() -> (Document, BodyId) {
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "A".into(),
        min: DVec3::ZERO,
        max: DVec3::new(60.0, 40.0, 10.0),
    })
    .unwrap();
    let a = d.bodies()[0].id;
    d.apply(Command::AddBox {
        name: "B".into(),
        min: DVec3::new(10.0, 10.0, 10.0),
        max: DVec3::new(40.0, 30.0, 30.0),
    })
    .unwrap();
    let b = d.bodies()[1].id;
    d.apply(Command::Combine {
        target: a,
        tools: vec![b],
        op: CombineOp::Merge,
        keep_tools: false,
    })
    .unwrap();
    (d, a)
}

fn vertical_edge_at(d: &Document, id: BodyId, x: f64, y: f64) -> u32 {
    let s = &d.body(id).unwrap().solid;
    for e in s.edges() {
        let i = s.edge_info(e).unwrap();
        if (i.start.x - x).abs() < 1e-6
            && (i.start.y - y).abs() < 1e-6
            && (i.end.x - x).abs() < 1e-6
            && (i.end.y - y).abs() < 1e-6
            && (i.start.z - i.end.z).abs() > 1.0
        {
            return e.0;
        }
    }
    panic!("no edge")
}

fn ledge_face(d: &Document, id: BodyId, z: f64) -> rolidium_kernel::FaceId {
    let s = &d.body(id).unwrap().solid;
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99 && (i.origin.z - z).abs() < 1e-6
        })
        .unwrap()
}

#[test]
fn a_wall_next_to_rounds_pulled_again_and_again_is_never_refused_with_a_message_about_the_round() {
    for (label, xs) in [
        ("one", vec![(40.0, 30.0)]),
        ("two", vec![(40.0, 30.0), (10.0, 30.0)]),
        (
            "four",
            vec![(40.0, 30.0), (10.0, 30.0), (10.0, 10.0), (40.0, 10.0)],
        ),
    ] {
        let (d0, id) = stepped();
        let mut d0 = d0;
        for (x, y) in &xs {
            let e = vertical_edge_at(&d0, id, *x, *y);
            d0.apply(Command::FilletEdges {
                body: id,
                edges: vec![e],
                size: 4.0,
                chamfer: false,
            })
            .unwrap();
        }
        let mut notices = 0;
        let planes: Vec<(DVec3, DVec3)> = {
            let s = &d0.body(id).unwrap().solid;
            s.faces()
                .filter_map(|f| {
                    let i = s.face_info(f).unwrap();
                    (i.kind == FaceKind::Plane).then(|| (i.normal.normalize(), i.origin))
                })
                .collect()
        };
        for (n, o) in planes {
            for mode in [PullMode::Auto, PullMode::Add] {
                let mut d = d0.clone();
                let mut off = n.dot(o);
                for (k, dist) in [3.0f64, 3.0, 2.0, -2.0, 5.0].iter().enumerate() {
                    let f = {
                        let s = &d.body(id).unwrap().solid;
                        s.faces().find(|f| {
                            let i = s.face_info(*f).unwrap();
                            i.kind == FaceKind::Plane
                                && i.normal.normalize().dot(n) > 0.9999
                                && (n.dot(i.origin) - off).abs() < 1e-6
                        })
                    };
                    let Some(f) = f else {
                        eprintln!("{label} {n:?} {mode:?} step {k}: face lost");
                        break;
                    };
                    let dd = if mode == PullMode::Add { dist.abs() } else { *dist };
                    let r = d.apply(Command::PullFace {
                        body: id,
                        face: f,
                        distance: dd,
                        mode,
                    });
                    match r {
                        Ok(c) => {
                            off += dd;
                            if c.notice.is_some() {
                                notices += 1;
                            }
                        }
                        Err(e) => {
                            let m = e.to_string();
                            assert!(
                                !m.contains("round could not be made cleanly"),
                                "{label} step {k}: {m}"
                            );
                            break;
                        }
                    }
                }
            }
        }
        eprintln!("{label}: {notices} pulls moved the wall without its round");
    }
}

use glam::DVec2;
use rolidium_sketch::Entity;
use rolidium_sketch::PlaneDef;

fn sketch_on(d: &mut Document, body: Option<BodyId>, z: f64, entities: Vec<Entity>) -> SketchId {
    let c = d
        .apply(Command::AddEntities {
            target: SketchTarget::New {
                plane: PlaneDef::from_face(DVec3::new(0.0, 0.0, z), DVec3::Z),
                body,
            },
            entities,
        })
        .unwrap();
    c.created_sketch.unwrap()
}

/// E2: a region (a circle, or an arc closed by a line) pulled inwards in Auto makes a pocket of the pulled depth, like Cut; pulled outwards it adds.
#[test]
fn a_region_pulled_inwards_in_auto_makes_a_pocket_of_that_depth_not_a_hole() {
    let arc_and_line = || {
        vec![
            Entity::Arc {
                center: DVec2::new(20.0, 15.0),
                radius: 6.0,
                start: 0.0,
                sweep: std::f64::consts::PI,
            },
            Entity::Line {
                a: DVec2::new(14.0, 15.0),
                b: DVec2::new(26.0, 15.0),
            },
        ]
    };
    let circle = || {
        vec![Entity::Circle {
            center: DVec2::new(20.0, 15.0),
            radius: 5.0,
        }]
    };
    for (name, ents, area) in [
        (
            "circle",
            circle as fn() -> Vec<Entity>,
            std::f64::consts::PI * 25.0,
        ),
        (
            "arc and line",
            arc_and_line as fn() -> Vec<Entity>,
            std::f64::consts::PI * 36.0 / 2.0,
        ),
    ] {
        for body_target in [false, true] {
            for mode in [PullMode::Auto, PullMode::Cut] {
                let mut d = Document::new();
                d.apply(Command::AddBox {
                    name: "B".into(),
                    min: DVec3::ZERO,
                    max: DVec3::new(40.0, 30.0, 20.0),
                })
                .unwrap();
                let id = d.bodies()[0].id;
                let sk = sketch_on(&mut d, body_target.then_some(id), 20.0, ents());
                let r = d.apply(Command::PullRegion {
                    sketch: sk,
                    region: 0,
                    distance: -6.0,
                    mode,
                });
                r.unwrap_or_else(|e| panic!("{name} target {body_target} {mode:?}: {e}"));
                let v = d.body(id).unwrap().solid.volume().unwrap();
                let expect = 24000.0 - area * 6.0;
                assert!(
                    (v - expect).abs() < 0.5,
                    "{name} target {body_target} {mode:?}: volume {v}, expected the pocket {expect}"
                );
            }
        }
    }
}

/// D: a circle sketched on a plane that belongs to no body (the base plane, or one above or below the mesh) cuts through a mesh when the pull crosses it, in Auto and in Cut,
/// whichever way the pull goes; where it crosses nothing the refusal says so.
#[test]
fn a_sketch_on_a_plane_cuts_through_a_mesh_in_auto_and_cut_whichever_way_it_is_pulled() {
    use rolidium_doc::NewMesh;
    use std::sync::Arc;
    let circle = || {
        vec![Entity::Circle {
            center: DVec2::new(20.0, 15.0),
            radius: 5.0,
        }]
    };
    let hole = std::f64::consts::PI * 25.0 * 20.0;
    // (the plane z, the pull, does it cross the mesh that spans z 0..20)
    for (z, dist, crosses) in [
        (25.0f64, -30.0f64, true),
        (-5.0, 30.0, true),
        (0.0, 30.0, true),
        (10.0, -30.0, true),
        (25.0, 30.0, true),
        (-5.0, -30.0, true),
    ] {
        for mode in [PullMode::Auto, PullMode::Cut] {
            // Auto: a pull outwards (positive) adds, so only the inward ones cut; Cut: whichever way the pull goes
            let crosses = crosses && (mode == PullMode::Cut || dist < 0.0);
            if !crosses && mode == PullMode::Auto {
                continue;
            }
            // Auto does not turn the pull round: a pull away from the mesh is a body of its own
            if mode == PullMode::Auto && ((z.min(z + dist)).max(0.0) >= (z.max(z + dist)).min(20.0)) {
                continue;
            }
            let mut d = Document::new();
            d.apply(Command::AddMeshes {
                meshes: vec![NewMesh {
                    name: "m".into(),
                    mesh: Arc::new(rolidium_mesh::gen::box_mesh(40.0, 30.0, 20.0, 1)),
                    verified: true,
                }],
            })
            .unwrap();
            let sk = sketch_on(&mut d, None, z, circle());
            let v0 = d.meshes()[0].mesh.signed_volume();
            let r = d.apply(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: dist,
                mode,
            });
            let v1 = d.meshes().first().map(|m| m.mesh.signed_volume()).unwrap_or(0.0);
            if crosses {
                r.unwrap_or_else(|e| panic!("plane {z} pull {dist} {mode:?}: {e}"));
                let overlap = |dd: f64| {
                    let (a, b) = (z.min(z + dd), z.max(z + dd));
                    (b.min(20.0) - a.max(0.0)).max(0.0)
                };
                // Cut: when the pull goes away from the mesh, the other way is cut
                let len = if overlap(dist) > 0.0 {
                    overlap(dist)
                } else {
                    overlap(-dist)
                };
                let expect = hole / 20.0 * len;
                assert!(
                    (v0 - v1 - expect).abs() < 0.01 * hole,
                    "plane {z} pull {dist} {mode:?}: removed {}, expected {expect}",
                    v0 - v1
                );
            }
        }
    }
}
