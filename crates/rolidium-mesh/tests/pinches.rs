// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Two parts that touch along an edge (four triangles share the edge) are separated by doubling the vertices there, and a mesh of several pieces is cut into them.
use rolidium_mesh::check::{check, merge_vertices, separate_pinches, split_components};
use rolidium_mesh::gen::box_mesh;
use rolidium_mesh::TriMesh;

/// Two cubes of 10 mm that touch along the edge x = 10, y = 10 (welded into one mesh).
fn two_cubes_on_an_edge() -> TriMesh {
    let a = box_mesh(10.0, 10.0, 10.0, 1);
    let b = box_mesh(10.0, 10.0, 10.0, 1);
    let mut m = a.clone();
    let base = m.positions.len() as u32;
    m.positions
        .extend(b.positions.iter().map(|p| [p[0] + 10.0, p[1] + 10.0, p[2]]));
    m.triangles.extend(
        b.triangles
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );
    merge_vertices(&mut m, 1e-6);
    m
}

#[test]
fn two_cubes_on_an_edge_are_two_closed_pieces_after_the_separation() {
    let mut m = two_cubes_on_an_edge();
    let before = check(&m);
    assert_eq!(before.non_manifold_edges, 1, "{before:?}");
    assert_eq!(before.components, 1);
    let v0 = m.signed_volume();
    let n = separate_pinches(&mut m);
    assert_eq!(n, 1);
    let after = check(&m);
    assert_eq!(after.non_manifold_edges, 0, "{after:?}");
    assert_eq!(after.open_edges, 0);
    assert_eq!(after.inconsistent_edges, 0);
    assert_eq!(after.components, 2);
    assert!(
        (m.signed_volume() - v0).abs() < 0.05,
        "the volume stays: {} {v0}",
        m.signed_volume()
    );
    let parts = split_components(&m);
    assert_eq!(parts.len(), 2);
    for p in &parts {
        assert!(check(p).is_clean());
        assert!((p.signed_volume() - 1000.0).abs() < 0.01);
    }
}

#[test]
fn a_mesh_without_pinches_is_left_alone() {
    let mut m = box_mesh(10.0, 10.0, 10.0, 2);
    let before = m.clone();
    assert_eq!(separate_pinches(&mut m), 0);
    assert_eq!(m, before);
}
