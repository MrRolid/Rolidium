// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Cutting and joining with meshes (Manifold): a mesh and a solid in every combination, one step of Undo, small holes patched and said, a refusal with the reason.
use rolidium_doc::{CombineOp, Command, Document, NewMesh};
use rolidium_kernel::{DVec3, Solid};
use rolidium_mesh::gen;
use std::sync::Arc;

fn mesh_doc(m: rolidium_mesh::TriMesh) -> (Document, u64) {
    let mut d = Document::new();
    d.apply(Command::AddMeshes {
        meshes: vec![NewMesh {
            name: "m".into(),
            mesh: Arc::new(m),
            verified: true,
        }],
    })
    .unwrap();
    let id = d.meshes()[0].id;
    (d, id)
}

fn add_solid(d: &mut Document, s: Solid) -> u64 {
    d.apply(Command::AddSolids { solids: vec![s] }).unwrap();
    d.bodies().last().unwrap().id
}

fn volume(d: &Document, id: u64) -> f64 {
    d.meshes()
        .iter()
        .find(|m| m.id == id)
        .unwrap()
        .mesh
        .signed_volume()
}

#[test]
fn a_cylinder_cut_through_a_mesh_and_one_undo() {
    let (mut d, mesh) = mesh_doc(gen::box_mesh(40.0, 30.0, 20.0, 1));
    let cyl = add_solid(
        &mut d,
        Solid::cylinder(5.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap(),
    );
    d.apply(Command::Combine {
        target: mesh,
        tools: vec![cyl],
        op: CombineOp::Cut,
        keep_tools: false,
    })
    .unwrap();
    let v = volume(&d, mesh);
    let expect = 24000.0 - std::f64::consts::PI * 25.0 * 20.0;
    assert!((v - expect).abs() < 0.5 * expect * 1e-3 + 5.0, "{v} {expect}");
    assert!(d.bodies().is_empty(), "the tool is used up");
    d.undo();
    assert_eq!(d.bodies().len(), 1);
    assert!(
        (volume(&d, mesh) - 24000.0).abs() < 1e-3,
        "one step of Undo brings the mesh back"
    );
}

#[test]
fn join_and_common_part_of_a_mesh_and_a_solid_and_of_two_meshes() {
    let (mut d, mesh) = mesh_doc(gen::box_mesh(10.0, 10.0, 10.0, 1));
    let boxy = add_solid(
        &mut d,
        Solid::cuboid(DVec3::new(5.0, 5.0, 5.0), DVec3::new(15.0, 15.0, 15.0)).unwrap(),
    );
    d.apply(Command::Combine {
        target: mesh,
        tools: vec![boxy],
        op: CombineOp::Merge,
        keep_tools: true,
    })
    .unwrap();
    assert!((volume(&d, mesh) - 1875.0).abs() < 0.5, "{}", volume(&d, mesh));
    d.undo();
    d.apply(Command::Combine {
        target: mesh,
        tools: vec![boxy],
        op: CombineOp::Intersect,
        keep_tools: true,
    })
    .unwrap();
    assert!((volume(&d, mesh) - 125.0).abs() < 0.5);
    // a solid as the target, a mesh as the tool: the result is a mesh
    d.undo();
    d.apply(Command::Combine {
        target: boxy,
        tools: vec![mesh],
        op: CombineOp::Cut,
        keep_tools: true,
    })
    .unwrap();
    assert!(d.bodies().iter().all(|b| b.id != boxy), "the solid became a mesh");
    assert!((volume(&d, boxy) - 875.0).abs() < 0.5);
}

#[test]
fn a_small_flat_hole_is_patched_and_said_and_a_big_one_is_refused_with_the_reason() {
    let mut m = gen::box_mesh(40.0, 30.0, 20.0, 1);
    m.triangles.pop(); // a missing triangle: a small flat hole
    let (mut d, mesh) = mesh_doc(m);
    let cyl = add_solid(
        &mut d,
        Solid::cylinder(5.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap(),
    );
    let c = d
        .apply(Command::Combine {
            target: mesh,
            tools: vec![cyl],
            op: CombineOp::Cut,
            keep_tools: false,
        })
        .unwrap();
    let said = c.notice.unwrap_or_default();
    assert!(said.contains("patched"), "{said:?}");
    // a half sphere: a big curved hole
    let s = gen::sphere(30.0, 2000);
    let mut half = s.clone();
    // the cap above a wavy line is missing: a big hole whose rim is not flat
    half.triangles.retain(|t| {
        t.iter().any(|v| {
            let p = s.positions[*v as usize];
            (p[2] as f64) < 6.0 * (3.0 * (p[1] as f64).atan2(p[0] as f64)).sin()
        })
    });
    let (mut d2, mesh2) = mesh_doc(half);
    let c2 = add_solid(
        &mut d2,
        Solid::cylinder(5.0, 80.0, DVec3::new(0.0, 0.0, -40.0), DVec3::Z).unwrap(),
    );
    let e = d2
        .apply(Command::Combine {
            target: mesh2,
            tools: vec![c2],
            op: CombineOp::Cut,
            keep_tools: false,
        })
        .unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("hole") && msg.contains("curved"), "{msg}");
}
