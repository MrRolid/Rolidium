// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Simplify a mesh and stretch a mesh or a solid across a plane: one step of Undo, the numbers, the refusals.
use rolidium_doc::meshes::SimplifyTarget;
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_kernel::{DVec3, Solid};
use rolidium_mesh::gen;
use std::sync::Arc;

fn mesh_doc(m: rolidium_mesh::TriMesh) -> (Document, u64) {
    let mut d = Document::new();
    d.apply(Command::AddMeshes { meshes: vec![NewMesh { name: "m".into(), mesh: Arc::new(m), verified: true }] }).unwrap();
    let id = d.meshes()[0].id;
    (d, id)
}

#[test]
fn a_flat_box_of_many_triangles_is_simplified_keeping_its_edges_and_undo_brings_it_back() {
    let m = gen::box_mesh(40.0, 30.0, 20.0, 32);
    let mut m = m;
    let tol = rolidium_mesh::check::merge_tolerance(&m);
    rolidium_mesh::check::merge_vertices(&mut m, tol);
    let before = m.triangles.len();
    let v = m.signed_volume();
    let (mut d, id) = mesh_doc(m);
    let c = d
        .apply(Command::MeshSimplify { body: id, target: SimplifyTarget::Percent(5.0), keep_features: true })
        .unwrap();
    let n = d.meshes()[0].mesh.triangles.len();
    assert!(n <= before / 20 && n >= 12, "{n} of {before}");
    assert!((d.meshes()[0].mesh.signed_volume() - v).abs() < 1e-3);
    assert!(c.notice.unwrap().contains("triangles instead of"));
    d.undo();
    assert_eq!(d.meshes()[0].mesh.triangles.len(), before);
}

#[test]
fn a_mesh_and_a_solid_are_stretched_across_a_plane() {
    let mut m = gen::box_mesh(40.0, 30.0, 20.0, 1);
    let tol = rolidium_mesh::check::merge_tolerance(&m);
    rolidium_mesh::check::merge_vertices(&mut m, tol);
    let (mut d, id) = mesh_doc(m);
    d.apply(Command::StretchInPlane { body: id, point: DVec3::new(20.0, 0.0, 0.0), normal: DVec3::X, distance: 10.0 }).unwrap();
    assert!((d.meshes()[0].mesh.signed_volume() - 30.0 * 20.0 * 50.0).abs() < 1e-3);
    d.undo();
    assert!((d.meshes()[0].mesh.signed_volume() - 24000.0).abs() < 1e-3);
    // a solid
    d.apply(Command::AddSolids { solids: vec![Solid::cuboid(DVec3::new(100.0, 0.0, 0.0), DVec3::new(120.0, 10.0, 10.0)).unwrap()] }).unwrap();
    let b = d.bodies()[0].id;
    d.apply(Command::StretchInPlane { body: b, point: DVec3::new(110.0, 0.0, 0.0), normal: DVec3::X, distance: 5.0 }).unwrap();
    assert!((d.bodies()[0].solid.volume().unwrap() - 25.0 * 100.0).abs() < 1e-3);
    // a plane that misses the mesh is refused with a sentence
    let e = d
        .apply(Command::StretchInPlane { body: id, point: DVec3::new(500.0, 0.0, 0.0), normal: DVec3::X, distance: 5.0 })
        .unwrap_err();
    assert!(e.to_string().contains("does not cut"), "{e}");
}
