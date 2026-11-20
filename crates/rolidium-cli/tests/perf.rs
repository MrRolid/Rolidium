// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The cost of an operation depends on the bodies that changed, not on the size of the document.

use rolidium_cli::phases::{alone_of, measure_ops, synthetic_machine, visible_inputs};
use rolidium_doc::display::MeshCache;
use rolidium_doc::{Command, PullMode};
use rolidium_kernel::DVec3;

#[test]
fn only_the_changed_bodies_get_new_display_meshes() {
    let (mut doc, body) = synthetic_machine(120);
    let mut cache = MeshCache::default();
    let all = cache.entries(&visible_inputs(&doc)).unwrap();
    assert_eq!(cache.last_made, 121);
    assert_eq!(all.len(), 121);
    // nothing changed: nothing is made
    cache.entries(&visible_inputs(&doc)).unwrap();
    assert_eq!(cache.last_made, 0);
    // a move changes one body
    doc.apply(Command::Move {
        bodies: vec![body],
        translation: DVec3::new(0.0, 0.0, 1.0),
        pivot: DVec3::ZERO,
        axis: DVec3::ZERO,
        angle: 0.0,
        copy: false,
    })
    .unwrap();
    let moved = cache.entries(&visible_inputs(&doc)).unwrap();
    assert_eq!(cache.last_made, 1);
    // the stamp of the others is the same (the graphics card keeps their buffers), the one of the moved body is new
    let stamps: Vec<u64> = all.iter().map(|e| e.stamp).collect();
    let after: Vec<u64> = moved.iter().map(|e| e.stamp).collect();
    assert_eq!(stamps.iter().zip(&after).filter(|(a, b)| a != b).count(), 1);
    // a rename makes nothing, only the name changes
    doc.apply(Command::Rename {
        target: rolidium_doc::ItemId::Body(body),
        name: "New name".into(),
    })
    .unwrap();
    let renamed = cache.entries(&visible_inputs(&doc)).unwrap();
    assert_eq!(cache.last_made, 0);
    assert_eq!(renamed.last().unwrap().name, "New name");
    // Undo goes back to a solid the cache has not got: one body is made again, not all
    doc.undo();
    doc.undo();
    cache.entries(&visible_inputs(&doc)).unwrap();
    assert!(cache.last_made <= 1, "{}", cache.last_made);
    let _ = PullMode::Auto;
}

/// A small operation in a document of 300 bodies is at most twice as slow as the same operation in a document with that body
/// alone (with 30 ms of room for the noise of a busy computer), and never takes a second.
#[test]
fn a_small_operation_in_a_big_document_is_not_much_slower_than_alone() {
    let (mut big, body) = synthetic_machine(300);
    let (mut alone, alone_id) = alone_of(&big, body);
    let in_big = measure_ops(&mut big, body, 5);
    let on_own = measure_ops(&mut alone, alone_id, 5);
    assert!(in_big.len() >= 5, "{in_big:?}");
    for b in &in_big {
        let Some(a) = on_own.iter().find(|a| a.name == b.name) else {
            continue;
        };
        eprintln!(
            "{:<32} in the big document {:>7.1} ms, alone {:>7.1} ms",
            b.name,
            b.after(),
            a.after()
        );
        assert!(
            b.after() <= 2.0 * a.after() + 30.0,
            "{}: {:.1} ms in the document of 301 bodies, {:.1} ms alone",
            b.name,
            b.after(),
            a.after()
        );
        assert!(b.after() < 1000.0, "{}: {:.1} ms", b.name, b.after());
    }
}
