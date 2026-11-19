// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A cancelled calculation leaves the document exactly as it was, with no Undo step. A test binary of its own: the kernel's
//! cancel flag is global.

use rolidium_doc::{Command, DocError, Document};
use rolidium_kernel::{request_cancel, reset_cancel, DVec3, FaceId, KernelError};

#[test]
fn a_cancelled_job_changes_nothing_and_the_same_job_then_works() {
    reset_cancel();
    let mut d = Document::new();
    d.apply(Command::AddBox {
        name: "b".into(),
        min: DVec3::ZERO,
        max: DVec3::new(40.0, 30.0, 20.0),
    })
    .unwrap();
    let id = d.bodies()[0].id;
    let top = d.bodies()[0]
        .solid
        .faces()
        .find(|f| d.bodies()[0].solid.face_info(*f).unwrap().normal.z > 0.9)
        .unwrap();
    let v0 = d.bodies()[0].solid.volume().unwrap();
    let state = d.state().clone();
    let undo_before = d.can_undo();
    let cmd = || Command::PullFace {
        body: id,
        face: FaceId(top.0),
        distance: 5.0,
        mode: Default::default(),
    };

    // cancelled: the job fails with the cancel error, committing is impossible, the state is the very same one
    request_cancel();
    let job = d.plan(cmd());
    let r = job.run();
    reset_cancel();
    assert!(
        matches!(r, Err(DocError::Kernel(KernelError::Cancelled))),
        "{:?}",
        r.map(|_| ())
    );
    assert!(std::sync::Arc::ptr_eq(&state, d.state()));
    assert_eq!(d.can_undo(), undo_before);
    assert!((d.bodies()[0].solid.volume().unwrap() - v0).abs() < 1e-9);

    // not cancelled: the same job gives the result it always gave
    d.apply(cmd()).unwrap();
    assert!((d.bodies()[0].solid.volume().unwrap() - 40.0 * 30.0 * 25.0).abs() < 1e-6);
    assert!(d.can_undo());
}
