// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The second half of `rolidium --bench file`: small operations on a small body of the opened document, each timed from the
//! confirmation to the frame in which the result is on the screen.

use super::*;

pub(super) struct EditBench {
    step: usize,
    started: Option<Instant>,
    revision: u64,
    waiting: bool,
    results: Vec<(String, f64)>,
    body: BodyId,
    /// Frames to let pass before the next step (the view is quiet, the remesh of the zoom is not running).
    pause: u32,
}

const STEPS: [&str; 7] = [
    "move a small body",
    "pull a face of the small body",
    "round one edge of the small body",
    "rename the small body",
    "select the small body",
    "undo",
    "redo",
];

impl App {
    /// Starts the operations; false when the document has no body.
    pub(super) fn start_edit_bench(&mut self) -> bool {
        let Some(b) = self
            .doc
            .bodies()
            .iter()
            .find(|b| b.name.starts_with("Screw"))
            .or_else(|| self.doc.bodies().last())
        else {
            return false;
        };
        self.edit_bench = Some(EditBench {
            step: 0,
            started: None,
            revision: self.doc.revision(),
            waiting: false,
            results: Vec::new(),
            body: b.id,
            pause: 20,
        });
        true
    }

    /// Once a frame; exits the program after the last step.
    pub(super) fn edit_bench_tick(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.edit_bench.take() else {
            return;
        };
        ctx.request_repaint();
        if e.pause > 0 {
            e.pause -= 1;
            self.edit_bench = Some(e);
            return;
        }
        if e.waiting {
            // the result is on the screen one frame after the commit: the frame that sent the meshes to the card has been drawn
            if self.idle() && self.doc.revision() != e.revision && !self.meshes_changed {
                let ms = e.started.map_or(0.0, |t| t.elapsed().as_secs_f64() * 1e3);
                e.results.push((STEPS[e.step].to_string(), ms));
                e.waiting = false;
                e.step += 1;
                e.pause = 20;
            } else if self.idle()
                && self.doc.revision() == e.revision
                && e.started.is_some_and(|t| t.elapsed().as_millis() > 300)
            {
                e.results.push((
                    format!("{} (refused: {})", STEPS[e.step], self.status.text),
                    f64::NAN,
                ));
                e.waiting = false;
                e.step += 1;
                e.pause = 20;
            } else if e.started.is_some_and(|t| t.elapsed().as_secs() > 60) {
                e.results
                    .push((format!("{} (no result in 60 s)", STEPS[e.step]), 60_000.0));
                e.waiting = false;
                e.step += 1;
            }
            self.edit_bench = Some(e);
            return;
        }
        if e.step >= STEPS.len() {
            println!(
                "\noperation on a small body in this document: confirmation to the result on the screen"
            );
            for (n, ms) in &e.results {
                println!("  {n:<36} {ms:>8.0} ms");
            }
            log(&format!(
                "bench: operations: {}",
                e.results
                    .iter()
                    .map(|(n, ms)| format!("{n} {ms:.0} ms"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            crate::startup::finish();
            std::process::exit(0);
        }
        e.revision = self.doc.revision();
        e.started = Some(Instant::now());
        let body = e.body;
        let sent = match e.step {
            0 => {
                self.submit(Command::Move {
                    bodies: vec![body],
                    translation: DVec3::new(0.0, 0.0, 0.5),
                    pivot: DVec3::ZERO,
                    axis: DVec3::ZERO,
                    angle: 0.0,
                    copy: false,
                });
                true
            }
            1 => rolidium_doc::display::small_edit_targets(&self.doc, body)
                .map(|t| t.0)
                .is_some_and(|face| {
                    self.submit(Command::PullFace {
                        body,
                        face,
                        distance: 0.5,
                        mode: PullMode::Auto,
                    });
                    true
                }),
            2 => rolidium_doc::display::small_edit_targets(&self.doc, body)
                .map(|t| t.1)
                .is_some_and(|edge| {
                    self.submit(Command::FilletEdges {
                        body,
                        edges: vec![edge],
                        size: 0.1,
                        chamfer: false,
                    });
                    true
                }),
            3 => {
                self.submit(Command::Rename {
                    target: ItemId::Body(body),
                    name: "Bench".into(),
                });
                true
            }
            4 => {
                // a selection changes only what is drawn: the next frame shows it
                self.clear_selection();
                self.body_sel = Some(body);
                self.layers_changed = true;
                false
            }
            5 => {
                self.undo();
                true
            }
            _ => {
                self.redo();
                true
            }
        };
        if e.step == 4 {
            // a selection changes only what is drawn: the time of the frame that follows
            ctx.request_repaint();
            e.results.push((STEPS[4].to_string(), self.frame_ms as f64));
            e.step += 1;
        } else if sent && self.busy != Busy::No || sent && self.doc.revision() != e.revision {
            e.waiting = true;
        } else {
            // not possible on this body (no face or edge to work on, or the program refused): leave the step out
            e.results
                .push((format!("{} (not possible here)", STEPS[e.step]), f64::NAN));
            e.step += 1;
        }
        self.edit_bench = Some(e);
    }
}
