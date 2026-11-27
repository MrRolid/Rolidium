// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Rules of the conversion of a mesh to a solid: a free-form (organic) model is refused at once with a sentence, parts that touch along an edge are separated and a solid is
//! made of each piece, and a part with details as small as the accuracy still becomes a solid (it is made with a finer accuracy and says so).
use rolidium_doc::{Command, Document, NewMesh};
use rolidium_mesh::check::merge_vertices;
use rolidium_mesh::{gen, TriMesh};
use std::sync::Arc;

fn doc_with(m: TriMesh) -> (Document, u64) {
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

fn convert(d: &mut Document, id: u64) -> Result<rolidium_doc::Computed, rolidium_doc::DocError> {
    d.apply(Command::ConvertMesh {
        body: id,
        tolerance: 0.02,
        round: false,
        keep: false,
    })
}

#[test]
fn a_free_shape_is_refused_in_a_moment_with_a_sentence() {
    // a ball with a rough surface: every triangle looks another way
    let mut m = gen::sphere(20.0, 40_000);
    let mut seed = 12345u64;
    let mut rnd = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as f64 / (1u64 << 31) as f64) - 0.5
    };
    for p in &mut m.positions {
        let k = 1.0 + 0.04 * rnd();
        *p = [p[0] * k as f32, p[1] * k as f32, p[2] * k as f32];
    }
    let (mut d, id) = doc_with(m);
    let t = std::time::Instant::now();
    let e = convert(&mut d, id).err().expect("an organic model is refused");
    assert!(e.to_string().contains("Organic model, it stays a mesh"), "{e}");
    assert!(t.elapsed().as_secs_f64() < 15.0, "{:?}", t.elapsed());
    assert_eq!(d.meshes().len(), 1, "the mesh is still there");
}

#[test]
fn two_cubes_that_touch_along_an_edge_become_two_solids() {
    let a = gen::box_mesh(10.0, 10.0, 10.0, 1);
    let mut m = a.clone();
    let base = m.positions.len() as u32;
    m.positions
        .extend(a.positions.iter().map(|p| [p[0] + 10.0, p[1] + 10.0, p[2]]));
    m.triangles.extend(
        a.triangles
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );
    merge_vertices(&mut m, 1e-6);
    let (mut d, id) = doc_with(m);
    let c = convert(&mut d, id).unwrap();
    assert_eq!(d.bodies().len(), 2);
    for b in d.bodies() {
        assert!((b.solid.volume().unwrap() - 1000.0).abs() < 0.5);
    }
    assert!(
        c.notice.unwrap().contains("separated"),
        "the notice says what was done"
    );
}
