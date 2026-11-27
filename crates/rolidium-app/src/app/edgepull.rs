// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Round / chamfer of selected edges: the real result is computed in the background while the
//! value changes and shown as soon as it is ready; the UI never waits.

use super::*;
use crate::i18n::{tr, trf};
use crate::worker::MeshEntry;

#[derive(Default)]
pub struct EdgePrev {
    pub ticket: u64,
    pub inflight: bool,
    pub wanted: Option<f64>,
    pub last_sent: Option<f64>,
}

impl App {
    pub(super) fn begin_edge_pull(
        &mut self,
        body: BodyId,
        edges: Vec<u32>,
        pointer: Pos2,
        rect: Rect,
    ) -> bool {
        let Some(first) = edges.first().and_then(|e| self.edge_polyline(body, *e)) else {
            return false;
        };
        let mid = first[first.len() / 2];
        let (axis_dir, px_per_mm) = self.axis_mapping(mid, Vec3::Z, rect);
        let _ = axis_dir;
        let fallback = rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan());
        self.pull = Some(PullState {
            kind: PullKind::Edges { body },
            normal: Vec3::Z,
            origin: DVec3::ZERO,
            geom: None,
            start: pointer,
            axis_dir: Vec2::X,
            px_per_mm: fallback.max(px_per_mm.min(fallback)),
            d_mouse: 0.0,
            typed: String::new(),
            dragging: false,
            edges,
            cands: Vec::new(),
            snap: None,
            group: Vec::new(),
            round: None,
        });
        self.edge_sel = self
            .pull
            .as_ref()
            .unwrap()
            .edges
            .iter()
            .map(|e| (body, *e))
            .collect();
        self.selection.clear();
        self.region_sel = None;
        self.entity_sel = None;
        self.edge_prev = EdgePrev::default();
        self.layers_changed = true;
        true
    }

    /// Asks for a preview of the current value (coalesced: only the newest value is computed).
    pub(super) fn request_edge_preview(&mut self) {
        let Some(p) = &self.pull else { return };
        if !matches!(p.kind, PullKind::Edges { .. }) {
            return;
        }
        let v = p.distance() as f64;
        if v > 1e-3 {
            self.edge_prev.wanted = Some(v);
        }
    }

    pub(super) fn pump_edge_preview(&mut self) {
        let Some(p) = &self.pull else { return };
        let PullKind::Edges { body } = p.kind else { return };
        if self.edge_prev.inflight {
            return;
        }
        let Some(v) = self.edge_prev.wanted.take() else {
            return;
        };
        if self.edge_prev.last_sent.is_some_and(|l| (l - v).abs() < 1e-6) {
            return;
        }
        let Some(b) = self.doc.body(body) else { return };
        let input = (b.id, b.name.clone(), b.solid.clone(), b.color);
        self.edge_prev.ticket += 1;
        let w = Work::EdgePreview {
            ticket: self.edge_prev.ticket,
            body: input,
            edges: p.edges.clone(),
            size: v,
            chamfer: self.edge_chamfer,
            angle_deg: crate::i18n::parse_num(&self.chamfer_angle_text).unwrap_or(45.0),
            flip: self.chamfer_flip,
        };
        if self.worker.send(w) {
            self.edge_prev.inflight = true;
            self.edge_prev.last_sent = Some(v);
        }
    }

    pub(super) fn edge_preview_done(&mut self, ticket: u64, r: Result<MeshEntry, String>) {
        self.edge_prev.inflight = false;
        let active = self
            .pull
            .as_ref()
            .is_some_and(|p| matches!(p.kind, PullKind::Edges { .. }));
        if !active || ticket != self.edge_prev.ticket {
            return;
        }
        match r {
            Ok(entry) => {
                let list: Vec<MeshEntry> = self
                    .meshes
                    .iter()
                    .map(|m| {
                        if m.id == entry.id {
                            entry.clone()
                        } else {
                            m.clone()
                        }
                    })
                    .collect();
                self.body_override = Some(Arc::new(list));
                self.meshes_changed = true;
                self.set_status("Preview of the result (release to apply)", false);
            }
            Err(msg) => {
                if self.body_override.take().is_some() {
                    self.meshes_changed = true;
                }
                self.set_status_quiet(trf("This value does not fit: {}", &[tr(&msg)]));
            }
        }
    }

    pub(super) fn clear_edge_preview(&mut self) {
        self.edge_prev = EdgePrev::default();
        if self.body_override.take().is_some() {
            self.meshes_changed = true;
        }
    }
}
