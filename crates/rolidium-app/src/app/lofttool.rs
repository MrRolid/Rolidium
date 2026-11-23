// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Loft tool: click two or more profiles (sketch regions or flat faces) in the order they should be joined.

use super::*;
use crate::i18n::tr;
use rolidium_doc::{LoftMode, LoftRef};

pub struct LoftUi {
    pub profiles: Vec<LoftRef>,
    pub ruled: bool,
    /// Set once the user chose Ruled or Smooth; until then 3 or more profiles default to Smooth.
    pub chosen: bool,
    pub mode: LoftMode,
}

impl Default for LoftUi {
    fn default() -> Self {
        LoftUi {
            profiles: Vec::new(),
            ruled: true,
            chosen: false,
            mode: LoftMode::NewBody,
        }
    }
}

/// `n` points evenly spaced along the closed polyline `pts` (by length), starting at its first point.
pub fn resample_loop(pts: &[Vec3], n: usize) -> Vec<Vec3> {
    if pts.len() < 2 || n == 0 {
        return pts.to_vec();
    }
    let mut closed: Vec<Vec3> = pts.to_vec();
    if (closed[0] - closed[closed.len() - 1]).length() > 1e-6 {
        closed.push(closed[0]);
    }
    let lens: Vec<f32> = closed.windows(2).map(|w| (w[1] - w[0]).length()).collect();
    let total: f32 = lens.iter().sum();
    if total < 1e-9 {
        return vec![closed[0]; n];
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let target = total * k as f32 / n as f32;
        let mut acc = 0.0;
        for (i, l) in lens.iter().enumerate() {
            if acc + l >= target || i + 1 == lens.len() {
                let t = if *l > 1e-9 {
                    ((target - acc) / l).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                out.push(closed[i].lerp(closed[i + 1], t));
                break;
            }
            acc += l;
        }
    }
    out
}

/// Rotates the start of `cur` and flips its direction so that it follows `prev` without twisting.
pub fn align_loop(prev: &[Vec3], cur: &[Vec3]) -> Vec<Vec3> {
    let n = cur.len();
    if n == 0 || prev.len() != n {
        return cur.to_vec();
    }
    let mut best: Option<(f32, Vec<Vec3>)> = None;
    for rev in [false, true] {
        let base: Vec<Vec3> = if rev {
            cur.iter().rev().copied().collect()
        } else {
            cur.to_vec()
        };
        for shift in 0..n {
            let cand: Vec<Vec3> = (0..n).map(|i| base[(i + shift) % n]).collect();
            // compare shapes after removing the offset between the loops, so that a loft between levels is not
            // dominated by the distance between them
            let (ca, cb) = (
                prev.iter().copied().sum::<Vec3>() / n as f32,
                cand.iter().copied().sum::<Vec3>() / n as f32,
            );
            let d: f32 = (0..n)
                .map(|i| ((prev[i] - ca) - (cand[i] - cb)).length_squared())
                .sum();
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, cand));
            }
        }
    }
    best.map(|b| b.1).unwrap_or_default()
}

impl App {
    /// A closed edge (a circle, for example) as a possible loft profile.
    fn closed_edge(&self, body: BodyId, edge: u32) -> bool {
        self.edge_polyline(body, edge)
            .is_some_and(|p| p.len() >= 8 && (p[0] - p[p.len() - 1]).length() < 1e-3)
    }

    /// The profiles selected in the Select tool, in selection order: sketch regions, flat faces, closed edges.
    pub(super) fn sync_sel_order(&mut self) {
        let mut eligible: Vec<LoftRef> = Vec::new();
        for (b, f) in &self.selection {
            if self
                .entry(*b)
                .and_then(|m| m.faces.get(*f as usize))
                .is_some_and(|m| m.kind == FaceKind::Plane)
            {
                eligible.push(LoftRef::Face {
                    body: *b,
                    face: FaceId(*f),
                });
            }
        }
        let regions: Vec<(SketchId, usize)> = if self.region_multi.is_empty() {
            self.region_sel.into_iter().collect()
        } else {
            self.region_multi.clone()
        };
        eligible.extend(
            regions
                .into_iter()
                .map(|(s, i)| LoftRef::Region { sketch: s, region: i }),
        );
        for (b, e) in &self.edge_sel {
            if self.closed_edge(*b, *e) {
                eligible.push(LoftRef::EdgeLoop { body: *b, edge: *e });
            }
        }
        let mut order: Vec<LoftRef> = self
            .sel_order
            .iter()
            .copied()
            .filter(|p| eligible.contains(p))
            .collect();
        for p in eligible {
            if !order.contains(&p) {
                order.push(p);
            }
        }
        self.sel_order = order;
    }

    /// The Loft button or key: the profiles selected so far become the profiles of the loft.
    pub(super) fn activate_loft(&mut self) {
        self.sync_sel_order();
        self.tool = Tool::Loft;
        self.leave_sketch();
        let pre = std::mem::take(&mut self.sel_order);
        if !pre.is_empty() {
            self.clear_selection();
            self.loft.profiles = pre;
            self.update_loft_preview();
            self.set_status(
                "Loft: the selected profiles are in the panel; press Enter to apply",
                false,
            );
        }
        self.layers_changed = true;
    }

    pub(super) fn loft_outline(&self, p: &LoftRef) -> Vec<Vec3> {
        match *p {
            LoftRef::EdgeLoop { body, edge } => self
                .edge_polyline(body, edge)
                .map(|p| p.to_vec())
                .unwrap_or_default(),
            LoftRef::Region { sketch, region } => {
                let Some(sd) = self.doc.sketch(sketch) else {
                    return Vec::new();
                };
                self.regions
                    .iter()
                    .find(|r| r.id == sketch)
                    .and_then(|r| r.regions.get(region))
                    .map(|r| {
                        r.outer
                            .polyline(0.1)
                            .iter()
                            .map(|q| sd.plane.to_3d(*q).as_vec3())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            LoftRef::Face { body, face } => self
                .doc
                .body(body)
                .and_then(|b| b.solid.face_outline(face, 0.1).ok())
                .and_then(|l| l.into_iter().next())
                .unwrap_or_default(),
        }
    }

    pub(super) fn loft_input(&mut self, resp: &egui::Response, hovered_face: Option<(BodyId, u32)>) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let pick = if let Some((s, i)) = self.hover_region {
            Some(LoftRef::Region { sketch: s, region: i })
        } else {
            hovered_face.and_then(|(b, f)| {
                let ok = self
                    .entry(b)
                    .and_then(|m| m.faces.get(f as usize))
                    .is_some_and(|m| m.kind == FaceKind::Plane);
                ok.then_some(LoftRef::Face {
                    body: b,
                    face: FaceId(f),
                })
            })
        };
        let Some(p) = pick else { return };
        if let Some(i) = self.loft.profiles.iter().position(|x| *x == p) {
            self.loft.profiles.remove(i);
        } else {
            self.loft.profiles.push(p);
        }
        if !self.loft.chosen {
            self.loft.ruled = self.loft.profiles.len() <= 2;
        }
        self.layers_changed = true;
        self.update_loft_preview();
    }

    /// A cheap translucent preview: the profile outlines joined by straight strips.
    pub(super) fn update_loft_preview(&mut self) {
        let outlines: Vec<Vec<Vec3>> = self
            .loft
            .profiles
            .iter()
            .map(|p| resample_loop(&self.loft_outline(p), 64))
            .filter(|o| o.len() > 2)
            .collect();
        if outlines.len() < 2 {
            self.clear_preview();
            return;
        }
        let mut aligned: Vec<Vec<Vec3>> = vec![outlines[0].clone()];
        for o in &outlines[1..] {
            let a = align_loop(aligned.last().unwrap(), o);
            aligned.push(a);
        }
        let mut tris = Vec::new();
        let mut lines = Vec::new();
        for w in aligned.windows(2) {
            let n = w[0].len();
            for i in 0..n {
                let j = (i + 1) % n;
                tris.extend([w[0][i], w[0][j], w[1][j], w[0][i], w[1][j], w[1][i]]);
                if i % 8 == 0 {
                    lines.extend([w[0][i], w[1][i]]);
                }
            }
        }
        self.preview = Some(PreviewDraw {
            triangles: tris,
            lines,
            color: [0.35, 0.65, 1.0, 0.33],
            line_color: [1.0, 0.85, 0.35, 0.8],
            ..Default::default()
        });
        self.preview_changed = true;
    }

    pub(super) fn apply_loft(&mut self) {
        if self.loft.profiles.len() < 2 {
            self.set_status("Click at least two profiles first", true);
            return;
        }
        if !self.idle() {
            return;
        }
        self.pending = None;
        // a loft is a link: it remembers its profiles and follows them when a sketch changes
        let kind = match self.doc.loft_link(&self.loft.profiles, self.loft.ruled, self.loft.mode) {
            Ok(k) => k,
            Err(e) => {
                self.set_status(e.to_string(), true);
                return;
            }
        };
        self.submit(Command::AddLink {
            kind,
            sources: Vec::new(),
            name: String::new(),
        });
        self.loft.profiles.clear();
        self.clear_preview();
        self.layers_changed = true;
        self.set_status("Computing…", false);
    }

    pub(super) fn clear_loft(&mut self) {
        self.loft.profiles.clear();
        self.clear_preview();
        self.layers_changed = true;
    }

    pub(super) fn loft_layers(&self) -> Option<PreviewDraw> {
        if self.tool != Tool::Loft || self.loft.profiles.is_empty() {
            return None;
        }
        let mut thick = Vec::new();
        for p in &self.loft.profiles {
            for w in self.loft_outline(p).windows(2) {
                thick.extend([w[0], w[1]]);
            }
        }
        Some(PreviewDraw {
            thick,
            thick_color: [1.0, 0.72, 0.25, 1.0],
            ..Default::default()
        })
    }

    pub(super) fn loft_paint(&self, ui: &egui::Ui, rect: Rect) {
        if self.tool != Tool::Loft {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        for (i, p) in self.loft.profiles.iter().enumerate() {
            let o = self.loft_outline(p);
            if o.is_empty() {
                continue;
            }
            let c = o.iter().copied().sum::<Vec3>() / o.len() as f32;
            if let Some(sp) = self.project(c, rect) {
                painter.circle_filled(sp, 10.0, crate::theme::rgb(255, 180, 60));
                painter.text(
                    sp,
                    egui::Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    egui::FontId::proportional(13.0),
                    Color32::BLACK,
                );
            }
        }
    }

    #[allow(deprecated)]
    pub(super) fn loft_panel(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.tool != Tool::Loft {
            return;
        }
        let mut apply = false;
        let mut clear = false;
        let mut changed = false;
        let mut docks = std::mem::take(&mut self.docks);
        let dock = docks.panel(
            ctx,
            "loft",
            Pos2::new(rect.max.x - 250.0, rect.min.y + 10.0),
            if docks.is_moved("loft") {
                self.loft
                    .profiles
                    .first()
                    .and_then(|p| self.loft_outline(p).first().copied())
                    .and_then(|p| self.project(p, rect))
            } else {
                None
            },
            230.0,
            &tr("Loft"),
            false,
            |ui| {
                if self.loft.profiles.is_empty() {
                    ui.label(tr(
                        "Click the profiles in the order they should be joined: sketch regions or flat faces",
                    ));
                }
                for (i, p) in self.loft.profiles.iter().enumerate() {
                    let name = match *p {
                        LoftRef::Region { sketch, .. } => {
                            self.doc.sketch(sketch).map_or("?".into(), |s| s.name.clone())
                        }
                        LoftRef::Face { body, .. } => {
                            self.doc.body(body).map_or("?".into(), |b| b.name.clone())
                        }
                        LoftRef::EdgeLoop { body, .. } => self
                            .doc
                            .body(body)
                            .map_or("?".into(), |b| format!("{} ({})", b.name, tr("edge"))),
                    };
                    ui.label(format!("{}. {name}", i + 1));
                }
                ui.horizontal(|ui| {
                    if ui
                        .selectable_value(&mut self.loft.ruled, true, tr("Ruled"))
                        .clicked()
                    {
                        self.loft.chosen = true;
                        changed = true;
                    }
                    if ui
                        .selectable_value(&mut self.loft.ruled, false, tr("Smooth"))
                        .clicked()
                    {
                        self.loft.chosen = true;
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    for (m, n) in [
                        (LoftMode::NewBody, "New body"),
                        (LoftMode::Add, "Add"),
                        (LoftMode::Cut, "Cut"),
                    ] {
                        ui.selectable_value(&mut self.loft.mode, m, tr(n));
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.loft.profiles.len() >= 2, egui::Button::new(tr("Apply")))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button(tr("Clear")).clicked() {
                        clear = true;
                    }
                });
            },
        );
        self.docks = docks;
        let _ = dock;
        if changed {
            self.layers_changed = true;
        }
        if apply {
            self.apply_loft();
        }
        if clear {
            self.clear_loft();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f32, s: f32) -> Vec<Vec3> {
        vec![
            Vec3::new(-s, -s, z),
            Vec3::new(s, -s, z),
            Vec3::new(s, s, z),
            Vec3::new(-s, s, z),
        ]
    }

    #[test]
    fn resampling_is_even_and_alignment_removes_twist() {
        let a = resample_loop(&square(0.0, 10.0), 40);
        assert_eq!(a.len(), 40);
        // a rotated and reversed copy of a smaller square is brought back in line with the first
        let mut b = resample_loop(&square(30.0, 5.0), 40);
        b.reverse();
        b.rotate_left(13);
        let c = align_loop(&a, &b);
        for i in (0..40).step_by(5) {
            let da = a[i] - Vec3::new(0.0, 0.0, a[i].z);
            let dc = c[i] - Vec3::new(0.0, 0.0, c[i].z);
            assert!(da.normalize().dot(dc.normalize()) > 0.99, "{i}: {da:?} {dc:?}");
        }
    }
}
