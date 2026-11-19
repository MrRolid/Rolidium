// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The volume safety net of Combine.

#[test]
fn the_volume_check_of_combine_accepts_possible_results_only() {
    use rolidium_doc::combine_volume_ok as ok;
    use rolidium_kernel::BooleanOp;
    // merge: at least the larger input, at most the sum
    assert!(ok(BooleanOp::Fuse, 100.0, 40.0, 130.0));
    assert!(
        !ok(BooleanOp::Fuse, 100.0, 40.0, 100.0 - 5.0),
        "the larger body shrank"
    );
    assert!(!ok(BooleanOp::Fuse, 100.0, 40.0, 141.0), "more than the sum");
    // cut: positive, at most the target
    assert!(ok(BooleanOp::Cut, 100.0, 40.0, 70.0));
    assert!(!ok(BooleanOp::Cut, 100.0, 40.0, -40.0), "inside out");
    assert!(!ok(BooleanOp::Cut, 100.0, 40.0, 101.0));
    // intersection: positive, at most the smaller input
    assert!(ok(BooleanOp::Common, 100.0, 40.0, 40.0));
    assert!(!ok(BooleanOp::Common, 100.0, 40.0, 41.0));
    assert!(!ok(BooleanOp::Common, 100.0, 40.0, 0.0));
    assert!(!ok(BooleanOp::Fuse, 1.0, 1.0, f64::NAN));
}
