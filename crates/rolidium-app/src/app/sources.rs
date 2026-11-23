// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The sources of a loft or a sweep in the Selection panel: the body made by such a link points to its profiles and its path, which can be
//! selected and edited there (a sketch is edited in the sketch mode, a helix in the fields of the panel).

use super::*;
use crate::i18n::parse_num;
use rolidium_doc::{CurveDef, CurveId, LinkKind, SweepPath};

/// The sketches and curves a link of a loft or a sweep was made from.
struct Source {
    sketches: Vec<SketchId>,
    curves: Vec<CurveId>,
    /// Faces or edges of bodies (a profile that is not a sketch, or a path along edges).
    from_bodies: usize,
}

fn source_of(kind: &LinkKind) -> Source {
    use rolidium_doc::LoftRef;
    let mut s = Source {
        sketches: Vec::new(),
        curves: Vec::new(),
        from_bodies: 0,
    };
    let add = |r: &LoftRef, s: &mut Source| match r {
        LoftRef::Region { sketch, .. } => {
            if !s.sketches.contains(sketch) {
                s.sketches.push(*sketch)
            }
        }
        _ => s.from_bodies += 1,
    };
    match kind {
        LinkKind::Loft { profiles, .. } => profiles.iter().for_each(|p| add(p, &mut s)),
        LinkKind::Sweep { profile, path, .. } => {
            add(profile, &mut s);
            match path {
                SweepPath::Curve(c) => s.curves.push(*c),
                SweepPath::Sketch { sketch, .. } => {
                    if !s.sketches.contains(sketch) {
                        s.sketches.push(*sketch)
                    }
                }
                SweepPath::Edges { .. } => s.from_bodies += 1,
            }
        }
        _ => {}
    }
    s
}

impl App {
    /// Whether the body was made by a loft or a sweep (a link that follows its sources).
    pub(super) fn made_by_profiles(&self, body: BodyId) -> bool {
        rolidium_doc::link_of_result(self.doc.state(), body).is_some_and(|l| l.kind.generates())
    }

    /// The name of the body of a loft or a sweep that follows this sketch (the sketch is one of its profiles or its path).
    pub(super) fn body_following_sketch(&self, sketch: SketchId) -> Option<String> {
        let l = self
            .doc
            .state()
            .links
            .iter()
            .find(|l| l.kind.generates() && source_of(&l.kind).sketches.contains(&sketch))?;
        l.results
            .first()
            .and_then(|b| self.doc.body(*b))
            .map(|b| b.name.clone())
    }

    /// Edits a sketch in the sketch mode (the sketch is shown if it was hidden: the loft or the sweep hides the sketches it was made from).
    pub(super) fn edit_sketch(&mut self, id: SketchId) {
        let Some(s) = self.doc.sketch(id).cloned() else {
            return;
        };
        if !self.idle() {
            return;
        }
        if !s.visible {
            self.run_doc_command(
                Command::SetVisible {
                    target: ItemId::Sketch(id),
                    visible: true,
                },
                "Shown",
            );
        }
        self.cancel_pull();
        self.selection.clear();
        self.body_sel = None;
        self.sketch_sel = Some(id);
        let mut sm = SketchMode::new(s.plane, s.target, Some(id));
        sm.focus = s.plane.origin.as_vec3();
        self.sk = Some(sm);
        self.turn_to_sketch();
        self.layers_changed = true;
        self.set_status(
            "Editing a source of a loft or a sweep: the body follows every change",
            false,
        );
    }

    /// In the Selection panel: where the selected body (or the body of the selected face) comes from, with a button for each source.
    pub(super) fn link_sources_ui(&mut self, ui: &mut egui::Ui, body: BodyId, face: bool) {
        let Some(link) = rolidium_doc::link_of_result(self.doc.state(), body).cloned() else {
            return;
        };
        if !link.kind.generates() {
            return;
        }
        let what = tr(link.kind.title());
        ui.separator();
        if face {
            // a face of such a body has no size to change: it comes from the profile and the path
            ui.label(trf(
                "This face has no size of its own: it comes from the {} '{}', so its size follows the profile and the path. Change the source profile instead.",
                &[what.clone(), link.name.clone()],
            ));
        } else {
            ui.label(trf("Made by the {} '{}'", &[what, link.name.clone()]));
            ui.weak(tr(
                "It follows its sources: change a source and the body is made again.",
            ));
        }
        let src = source_of(&link.kind);
        for id in src.sketches {
            let Some(s) = self.doc.sketch(id).map(|s| s.name.clone()) else {
                continue;
            };
            ui.horizontal(|ui| {
                ui.label(trf("Source: {}", std::slice::from_ref(&s)));
                if ui.small_button(tr("Select")).clicked() {
                    self.clear_selection();
                    self.sketch_sel = Some(id);
                    self.layers_changed = true;
                }
                if ui.small_button(tr("Edit the profile")).clicked() {
                    self.edit_sketch(id);
                }
            });
        }
        for id in src.curves {
            let Some(c) = self.doc.curve(id).map(|c| c.name.clone()) else {
                continue;
            };
            ui.horizontal(|ui| {
                ui.label(trf("Path: {}", std::slice::from_ref(&c)));
                if ui.small_button(tr("Select")).clicked() {
                    self.clear_selection();
                    self.curve_sel = Some(id);
                    self.layers_changed = true;
                }
            });
        }
        if src.from_bodies > 0 {
            ui.weak(tr(
                "A face or edges of a body are also a source: change that body.",
            ));
        }
    }

    /// The numbers of the selected helix in the Selection panel (Enter applies): the bodies made along it follow.
    #[allow(deprecated)]
    pub(super) fn curve_edit_ui(&mut self, ui: &mut egui::Ui, id: CurveId) {
        let Some(CurveDef::Helix(h)) = self.doc.curve(id).map(|c| c.def.clone()) else {
            return;
        };
        let now = [h.radius, h.pitch, h.turns, h.cone_deg];
        if self.curve_edit.0 != Some(id) || self.curve_edit.2 != now {
            self.curve_edit = (Some(id), now.iter().map(|v| fmt_num(*v)).collect(), now);
        }
        let labels = [
            ("Radius", "mm"),
            ("Pitch", "mm"),
            ("Turns", ""),
            ("Cone angle", "°"),
        ];
        let mut apply = false;
        for (i, (label, unit)) in labels.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(tr(label));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.curve_edit.1[i])
                        .id(egui::Id::new(("curve_edit", i)))
                        .desired_width(60.0),
                );
                ui.label(*unit);
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    apply = true;
                }
            });
        }
        if apply {
            let v: Vec<Option<f64>> = self.curve_edit.1.iter().map(|t| parse_num(t)).collect();
            let mut h2 = h;
            let (Some(r), Some(p), Some(t), Some(c)) = (v[0], v[1], v[2], v[3]) else {
                self.set_status("Type numbers into the fields", true);
                return;
            };
            (h2.radius, h2.pitch, h2.turns, h2.cone_deg) = (r, p, t, c);
            self.run_doc_command(
                Command::EditCurve {
                    curve: id,
                    def: CurveDef::Helix(h2),
                },
                "Helix changed",
            );
        }
    }
}
