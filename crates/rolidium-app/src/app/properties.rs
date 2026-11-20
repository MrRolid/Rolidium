// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Properties of the selection (volume, area, size, lengths), shown in the Selection panel; a click on a value copies it.

use super::*;
use crate::i18n::tr;
use rolidium_kernel::Solid;

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub label: String,
    pub value: String,
}

/// A number with a few digits, trailing zeros removed: 12.50 -> 12.5.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

pub fn fmt_length(v: f64) -> String {
    format!("{} mm", num(v))
}

pub fn fmt_area(v: f64) -> String {
    if v >= 1e6 {
        format!("{} mm² ({} cm²)", num(v), num(v / 100.0))
    } else {
        format!("{} mm²", num(v))
    }
}

pub fn fmt_volume(v: f64) -> String {
    if v >= 1000.0 {
        format!("{} mm³ ({} cm³)", num(v), num(v / 1000.0))
    } else {
        format!("{} mm³", num(v))
    }
}

/// Grams, or kilograms for heavy bodies.
pub fn fmt_mass(g: f64) -> String {
    if g >= 10_000.0 {
        format!("{} kg", num(g / 1000.0))
    } else {
        format!("{} g", num(g))
    }
}

/// Volume, surface area and the size of the bounding box of a body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyProps {
    pub volume: f64,
    pub area: f64,
    pub size: [f64; 3],
}

pub fn body_props(solid: &Solid) -> Option<BodyProps> {
    let p = solid.props().ok()?;
    let s = p.max - p.min;
    Some(BodyProps {
        volume: p.volume,
        area: p.area,
        size: [s.x, s.y, s.z],
    })
}

fn prop(label: &str, value: String) -> Prop {
    Prop {
        label: tr(label),
        value,
    }
}

impl App {
    /// What the Selection panel lists for the current selection.
    pub(super) fn selection_properties(&mut self) -> Vec<Prop> {
        // what the properties depend on: the selected bodies and their shapes (not the other bodies: an operation somewhere else
        // in the document does not make them be calculated again)
        let shape_of = |b: BodyId| self.doc.body(b).map(|b| b.solid.shape_id());
        let key = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.body_sel,
            self.body_sel
                .and_then(|b| self.doc.body(b))
                .map(|b| b.material.clone()),
            self.body_sel.and_then(shape_of),
            self.selection,
            self.selection
                .iter()
                .map(|(b, _)| shape_of(*b))
                .collect::<Vec<_>>(),
            self.edge_sel,
        );
        if let Some((k, v)) = &self.prop_cache {
            if *k == key {
                return v.clone();
            }
        }
        let v = self.compute_properties();
        self.prop_cache = Some((key, v.clone()));
        v
    }

    fn compute_properties(&self) -> Vec<Prop> {
        let mut out = Vec::new();
        if let Some(b) = self.body_sel.and_then(|b| self.doc.body(b)) {
            if let Some(p) = body_props(&b.solid) {
                out.push(prop("Volume", fmt_volume(p.volume)));
                out.push(prop("Surface area", fmt_area(p.area)));
                out.push(prop("Size X", fmt_length(p.size[0])));
                out.push(prop("Size Y", fmt_length(p.size[1])));
                out.push(prop("Size Z", fmt_length(p.size[2])));
                if let Some(g) = b.mass_g() {
                    out.push(prop("Mass", fmt_mass(g)));
                }
            }
        }
        if !self.selection.is_empty() {
            let mut total = 0.0;
            for (b, f) in &self.selection {
                if let Some(m) = self.entry(*b).and_then(|m| m.faces.get(*f as usize)) {
                    total += m.area;
                }
            }
            let n = self.selection.len();
            out.push(prop(if n > 1 { "Total area" } else { "Area" }, fmt_area(total)));
            if n == 1 {
                let (b, f) = self.selection[0];
                if let Some(d) = self
                    .entry(b)
                    .and_then(|m| m.faces.get(f as usize))
                    .and_then(|m| m.cyl_diameter)
                {
                    out.push(prop("Diameter", fmt_length(d)));
                } else if let Some(info) = self
                    .doc
                    .body(b)
                    .and_then(|bd| bd.solid.cylinder_info(FaceId(f)).ok().flatten())
                {
                    out.push(prop("Diameter", fmt_length(info.radius * 2.0)));
                }
            }
        }
        if !self.edge_sel.is_empty() {
            let mut total = 0.0;
            for (b, e) in &self.edge_sel {
                if let Some(i) = self
                    .doc
                    .body(*b)
                    .and_then(|bd| bd.solid.edge_info(rolidium_kernel::EdgeId(*e)).ok())
                {
                    total += i.length;
                }
            }
            out.push(prop(
                if self.edge_sel.len() > 1 {
                    "Total length"
                } else {
                    "Length"
                },
                fmt_length(total),
            ));
            if let [(b, e)] = self.edge_sel[..] {
                if let Some(i) = self
                    .doc
                    .body(b)
                    .and_then(|bd| bd.solid.edge_info(rolidium_kernel::EdgeId(e)).ok())
                {
                    if i.kind == rolidium_kernel::EdgeKind::Circle && i.radius > 0.0 {
                        out.push(prop("Diameter", fmt_length(i.radius * 2.0)));
                        out.push(prop("Radius", fmt_length(i.radius)));
                    }
                }
            }
        }
        out
    }

    /// The material of the selected body: a choice of presets and a density of one's own.
    fn material_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.body_sel else { return };
        let Some(body) = self.doc.body(id) else { return };
        let current = body.material_or_default();
        let mut choice: Option<Option<rolidium_doc::Material>> = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}:", tr("Material"))).weak());
            let shown = if current.name == "custom" {
                tr("Custom")
            } else {
                tr(&current.name)
            };
            egui::ComboBox::from_id_salt("material")
                .selected_text(format!("{shown} ({} g/cm³)", num(current.density)))
                .show_ui(ui, |ui| {
                    for (i, (name, d)) in rolidium_doc::MATERIALS.iter().enumerate() {
                        if ui
                            .selectable_label(
                                current.name == *name,
                                format!("{} ({} g/cm³)", tr(name), num(*d)),
                            )
                            .clicked()
                        {
                            // the default material is stored as "never chosen", so such a file stays as it was
                            choice = Some(if i == 0 {
                                None
                            } else {
                                Some(rolidium_doc::Material::preset(i))
                            });
                        }
                    }
                    if ui
                        .selectable_label(current.name == "custom", tr("Custom"))
                        .clicked()
                        && current.name != "custom"
                    {
                        choice = Some(Some(rolidium_doc::Material {
                            name: "custom".into(),
                            density: current.density,
                        }));
                    }
                });
        });
        if current.name == "custom" {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}:", tr("Density"))).weak());
                let text = self
                    .density_text
                    .entry(id)
                    .or_insert_with(|| num(current.density));
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(60.0));
                ui.label("g/cm³");
                if r.lost_focus() {
                    if let Ok(d) = text.replace(',', ".").trim().parse::<f64>() {
                        if d > 0.0 && d < 100.0 {
                            choice = Some(Some(rolidium_doc::Material {
                                name: "custom".into(),
                                density: d,
                            }));
                        }
                    }
                }
            });
        } else {
            self.density_text.remove(&id);
        }
        if let Some(material) = choice {
            self.run_doc_command(Command::SetMaterial { body: id, material }, "Material changed");
        }
    }

    /// The property rows; a click on a value copies it.
    pub(super) fn properties_ui(&mut self, ui: &mut egui::Ui) {
        let mass_label = tr("Mass");
        for p in self.selection_properties() {
            // the material sits right above the mass it gives
            if p.label == mass_label {
                self.material_ui(ui);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}:", p.label)).weak());
                let r = ui
                    .add(egui::Label::new(egui::RichText::new(&p.value).monospace()).sense(Sense::click()))
                    .on_hover_text(tr("Click to copy the value"));
                if r.clicked() {
                    ui.ctx()
                        .copy_text(p.value.split(" (").next().unwrap_or(&p.value).to_string());
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rolidium_kernel::DVec3;

    #[test]
    fn a_box_and_a_cylinder_have_the_known_properties() {
        let b = Solid::cuboid(DVec3::new(1.0, 2.0, 3.0), DVec3::new(41.0, 32.0, 23.0)).unwrap();
        let p = body_props(&b).unwrap();
        assert!((p.volume - 24000.0).abs() < 1e-6);
        assert!((p.area - 2.0 * (40.0 * 30.0 + 40.0 * 20.0 + 30.0 * 20.0)).abs() < 1e-6);
        assert!(
            p.size
                .iter()
                .zip([40.0, 30.0, 20.0])
                .all(|(a, e)| (a - e).abs() < 1e-3),
            "{:?}",
            p.size
        );
        let c = Solid::cylinder(5.0, 20.0, DVec3::ZERO, DVec3::Z).unwrap();
        let q = body_props(&c).unwrap();
        assert!((q.volume - std::f64::consts::PI * 25.0 * 20.0).abs() < 1e-6);
        assert!((q.area - 2.0 * std::f64::consts::PI * 5.0 * 25.0).abs() < 1e-6);
        assert!((q.size[0] - 10.0).abs() < 1e-3 && (q.size[2] - 20.0).abs() < 1e-3);
        // a face and an edge of the cylinder
        let side = c
            .faces()
            .find(|f| c.cylinder_info(*f).ok().flatten().is_some())
            .unwrap();
        assert!((c.cylinder_info(side).unwrap().unwrap().radius * 2.0 - 10.0).abs() < 1e-9);
        let circle = c
            .edges()
            .find(|e| {
                c.edge_info(*e)
                    .map(|i| i.kind == rolidium_kernel::EdgeKind::Circle)
                    .unwrap_or(false)
            })
            .unwrap();
        let i = c.edge_info(circle).unwrap();
        assert!((i.length - std::f64::consts::TAU * 5.0).abs() < 1e-6 && (i.radius - 5.0).abs() < 1e-9);
    }

    #[test]
    fn numbers_are_shown_with_their_units() {
        assert_eq!(fmt_length(12.5), "12.5 mm");
        assert_eq!(fmt_length(20.0), "20 mm");
        assert_eq!(fmt_area(300.25), "300.25 mm²");
        assert_eq!(fmt_volume(999.0), "999 mm³");
        assert_eq!(fmt_volume(24000.0), "24000 mm³ (24 cm³)");
        assert_eq!(fmt_volume(1570.796), "1570.796 mm³ (1.571 cm³)");
    }
}
