// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Icons and help cards. Icons are SVG files with two colour tokens that are replaced per theme.

use crate::assets_index::{HELP_ART, HELP_JSON, ICONS};
use crate::i18n::{tr, trf};
use eframe::egui::{self, load::Bytes, Color32, ImageSource};
use serde::Deserialize;
use std::borrow::Cow;

pub const NEUTRAL_DARK: &str = "#d6dae0";
pub const ACCENT_DARK: &str = "#ffb347";
pub const NEUTRAL_LIGHT: &str = "#3b424c";
pub const ACCENT_LIGHT: &str = "#c46a00";

static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Tells the icon loader which colours to use (set whenever the theme of the window changes).
pub fn set_light(light: bool) {
    LIGHT.store(light, std::sync::atomic::Ordering::Relaxed);
}

fn light() -> bool {
    LIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

fn themed(svg: &str) -> String {
    let (n, a) = if light() {
        (NEUTRAL_LIGHT, ACCENT_LIGHT)
    } else {
        (NEUTRAL_DARK, ACCENT_DARK)
    };
    svg.replace("#NEUTRAL", n).replace("#ACCENT", a)
}

fn theme_name() -> &'static str {
    if light() {
        "light"
    } else {
        "dark"
    }
}

fn find<'a>(table: &'a [(&str, &str)], id: &str) -> Option<&'a str> {
    table.iter().find(|(n, _)| *n == id).map(|(_, s)| *s)
}

pub fn icon(id: &str, size: f32) -> egui::Image<'static> {
    let svg = find(ICONS, id).map(themed).unwrap_or_default();
    egui::Image::new(ImageSource::Bytes {
        uri: Cow::Owned(format!("bytes://icon/{id}/{}.svg", theme_name())),
        bytes: Bytes::from(svg.into_bytes()),
    })
    .fit_to_exact_size(egui::Vec2::splat(size))
}

fn illustration(id: &str) -> egui::Image<'static> {
    let svg = find(HELP_ART, id).map(themed).unwrap_or_default();
    egui::Image::new(ImageSource::Bytes {
        uri: Cow::Owned(format!("bytes://help/{id}/{}.svg", theme_name())),
        bytes: Bytes::from(svg.into_bytes()),
    })
    .fit_to_exact_size(egui::Vec2::new(240.0, 140.0))
}

#[derive(Debug, Clone, Deserialize)]
pub struct HelpEntry {
    pub id: String,
    pub title: String,
    pub shortcut: String,
    pub text: String,
    #[serde(default)]
    pub tips: Vec<String>,
    #[allow(dead_code)]
    pub illustration: String,
    /// Reserved for later animated demonstrations; not used yet.
    #[serde(default)]
    #[allow(dead_code)]
    pub animation: Option<String>,
}

pub struct Help {
    entries: Vec<HelpEntry>,
}

impl Help {
    pub fn load() -> Self {
        let entries = serde_json::from_str(HELP_JSON).unwrap_or_default();
        Help { entries }
    }

    pub fn card(&self, ui: &mut egui::Ui, id: &str) {
        let Some(e) = self.entries.iter().find(|e| e.id == id) else {
            return;
        };
        ui.set_max_width(260.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr(&e.title)).strong().size(16.0));
            if !e.shortcut.is_empty() {
                ui.label(
                    egui::RichText::new(format!("[{}]", e.shortcut)).color(Color32::from_rgb(255, 179, 71)),
                );
            }
        });
        ui.add(illustration(&e.id));
        ui.label(tr(&e.text));
        for t in &e.tips {
            ui.label(egui::RichText::new(trf("Tip: {}", &[tr(t)])).weak().small());
        }
    }
}

/// Hover text of a button that cannot be used now: why, then the help card.
fn disabled_hover(ui: &mut egui::Ui, help: &Help, id: &str, why: &str) {
    ui.label(egui::RichText::new(tr(why)).color(Color32::from_rgb(230, 170, 90)));
    ui.separator();
    help.card(ui, id);
}

thread_local! {
    static STATEFUL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks the next button as one whose availability changes, so it gets the indication (a dot when it has no shortcut letter).
pub fn stateful() {
    STATEFUL.with(|s| s.set(true));
}

/// The colour of a shortcut letter or dot: muted green when available, muted red when not. They differ in brightness too.
pub fn avail_color(dark: bool, available: bool) -> Color32 {
    match (dark, available) {
        (true, true) => Color32::from_rgb(130, 205, 150),
        (true, false) => Color32::from_rgb(205, 100, 100),
        (false, true) => Color32::from_rgb(60, 150, 85),
        (false, false) => Color32::from_rgb(170, 45, 45),
    }
}

/// How a button is drawn and answered.
struct Look {
    /// The availability is indicated (a shortcut letter or a dot).
    indicate: bool,
    /// Unavailable and drawn in the traffic light way: readable text, dimmed icon, no click.
    soft_off: bool,
    available: bool,
}

fn look(why_not: Option<&str>, has_key: bool) -> Look {
    let stateful = STATEFUL.with(|s| s.replace(false));
    let indicate = has_key || stateful;
    Look {
        indicate,
        soft_off: why_not.is_some(),
        available: why_not.is_none(),
    }
}

/// Adds the button: greyed out (and not clickable) when unavailable, or the traffic light way.
fn add_button(ui: &mut egui::Ui, b: egui::Button<'_>, why_not: Option<&str>, look: &Look) -> egui::Response {
    if look.indicate && why_not.is_none() {
        // available: plain white (black in the light theme) text
        return ui
            .scope(|ui| {
                let v = ui.visuals_mut();
                let c = v.strong_text_color();
                for w in [
                    &mut v.widgets.inactive,
                    &mut v.widgets.hovered,
                    &mut v.widgets.active,
                ] {
                    w.fg_stroke.color = c;
                }
                ui.add(b)
            })
            .inner;
    }
    if look.soft_off {
        ui.scope(|ui| {
            let v = ui.visuals_mut();
            let c = v.strong_text_color();
            // a button that only senses hovering is drawn with the non-interactive look: its text colour is set too
            for w in [
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.noninteractive,
            ] {
                w.fg_stroke.color = c;
            }
            // the background changes as it did when the button was greyed out; the text stays readable
            let fade = v.panel_fill;
            let bg = v.widgets.inactive.weak_bg_fill.lerp_to_gamma(fade, 0.5);
            for w in [
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
            ] {
                w.weak_bg_fill = bg;
                w.bg_fill = bg;
                w.expansion = 0.0;
            }
            ui.add(b.sense(egui::Sense::hover()))
        })
        .inner
    } else {
        ui.add_enabled(why_not.is_none(), b)
    }
}

fn dim(img: egui::Image<'static>, look: &Look) -> egui::Image<'static> {
    if look.soft_off {
        img.tint(Color32::from_white_alpha(105))
    } else {
        img
    }
}

fn tip(
    r: egui::Response,
    help: &Help,
    id: &str,
    title: Option<&str>,
    why_not: Option<&str>,
    look: &Look,
) -> egui::Response {
    let show = |ui: &mut egui::Ui| {
        if let Some(t) = title {
            ui.strong(t);
        }
        match why_not {
            Some(w) => disabled_hover(ui, help, id, w),
            None => help.card(ui, id),
        }
    };
    if look.soft_off || why_not.is_none() {
        r.on_hover_ui(show)
    } else {
        r.on_disabled_hover_ui(show)
    }
}

/// The shortcut letter in the lower right corner of a button, or a dot (green filled / red ring) when it has none.
fn corner_key(ui: &egui::Ui, r: &egui::Response, key: &str, look: &Look) {
    let dark = ui.visuals().dark_mode;
    let pos = r.rect.right_bottom() + egui::Vec2::new(-3.0, -1.0);
    if look.indicate {
        let col = avail_color(dark, look.available);
        if key.is_empty() {
            let c = r.rect.right_bottom() + egui::Vec2::new(-7.0, -7.0);
            if look.available {
                ui.painter().circle_filled(c, 3.0, col);
            } else {
                ui.painter().circle_stroke(c, 3.0, egui::Stroke::new(1.5, col));
            }
        } else {
            ui.painter().text(
                pos,
                egui::Align2::RIGHT_BOTTOM,
                key,
                egui::FontId::proportional(11.5),
                col,
            );
        }
        return;
    }
    if key.is_empty() {
        return;
    }
    let col = if r.enabled() {
        ui.visuals().text_color().gamma_multiply(0.8)
    } else {
        ui.visuals().weak_text_color().gamma_multiply(0.6)
    };
    ui.painter().text(
        pos,
        egui::Align2::RIGHT_BOTTOM,
        key,
        egui::FontId::proportional(11.5),
        col,
    );
}

/// "Sweep (W)": the letter in the brackets gets the availability colour.
fn label_with_key(ui: &egui::Ui, label: &str, look: &Look) -> egui::WidgetText {
    if look.indicate {
        if let Some((head, rest)) = label.rsplit_once(" (") {
            if let Some(key) = rest.strip_suffix(')') {
                let font = egui::TextStyle::Button.resolve(ui.style());
                let normal = ui.visuals().strong_text_color();
                let mut job = egui::text::LayoutJob::default();
                let fmt = |c: Color32| egui::TextFormat::simple(font.clone(), c);
                job.append(&format!("{head} ("), 0.0, fmt(normal));
                job.append(key, 0.0, fmt(avail_color(ui.visuals().dark_mode, look.available)));
                job.append(")", 0.0, fmt(normal));
                return job.into();
            }
        }
    }
    label.to_string().into()
}

/// Like [`tool_button`], but when `why_not` is set the button is unavailable and its tooltip says why. A label ending in
/// "(K)" gets the availability colour on the letter.
pub fn tool_button_why(
    ui: &mut egui::Ui,
    help: &Help,
    id: &str,
    label: &str,
    selected: bool,
    why_not: Option<&str>,
) -> egui::Response {
    let lk = look(why_not, label.ends_with(')') && label.contains(" ("));
    // a long translated name is cut off instead of making the button taller (the tooltip shows the whole card)
    let b = egui::Button::image_and_text(dim(icon(id, 22.0), &lk), label_with_key(ui, label, &lk))
        .selected(selected)
        .truncate()
        .min_size(egui::Vec2::new(128.0, 28.0));
    let r = add_button(ui, b, why_not, &lk);
    if lk.indicate && !label.contains(" (") {
        corner_key(ui, &r, "", &lk);
    }
    tip(r, help, id, None, why_not, &lk)
}

/// A labelled button with its shortcut in the corner (the tooltip says it in words, too).
pub fn tool_button_key(
    ui: &mut egui::Ui,
    help: &Help,
    id: &str,
    label: &str,
    key: &str,
    selected: bool,
    why_not: Option<&str>,
) -> egui::Response {
    let lk = look(why_not, !key.is_empty());
    let b = egui::Button::image_and_text(dim(icon(id, 22.0), &lk), label)
        .selected(selected)
        .truncate()
        .min_size(egui::Vec2::new(128.0, 28.0));
    let r = add_button(ui, b, why_not, &lk);
    corner_key(ui, &r, key, &lk);
    let title = if key.is_empty() {
        label.to_string()
    } else {
        format!("{label} ({key})")
    };
    tip(r, help, id, Some(&title), why_not, &lk)
}

/// An icon-only button of a given width, with the reason when it is unavailable (the label goes into the tooltip).
#[allow(clippy::too_many_arguments)]
pub fn icon_button_why(
    ui: &mut egui::Ui,
    help: &Help,
    id: &str,
    label: &str,
    selected: bool,
    why_not: Option<&str>,
    width: f32,
    key: &str,
) -> egui::Response {
    let lk = look(why_not, !key.is_empty());
    // the top bar buttons are larger, so their icons are easy to tell apart
    let (size, height) = if width >= 36.0 { (27.0, 34.0) } else { (22.0, 28.0) };
    let b = egui::Button::image(dim(icon(id, size), &lk))
        .selected(selected)
        .min_size(egui::Vec2::new(width, height));
    let r = add_button(ui, b, why_not, &lk);
    corner_key(ui, &r, key, &lk);
    tip(r, help, id, Some(label), why_not, &lk)
}

/// A big icon-only button of the File block: `size` is the side of the icon, the button is a little larger.
pub fn file_button(
    ui: &mut egui::Ui,
    help: &Help,
    id: &str,
    label: &str,
    why_not: Option<&str>,
    size: f32,
) -> egui::Response {
    let lk = look(why_not, false);
    let side = size + 8.0;
    let b = egui::Button::image(dim(icon(id, size), &lk)).min_size(egui::Vec2::splat(side));
    let r = add_button(ui, b, why_not, &lk);
    tip(r, help, id, Some(label), why_not, &lk)
}

/// The colour of the frame around the File block.
pub fn file_frame_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(214, 186, 96)
    } else {
        Color32::from_rgb(176, 138, 30)
    }
}

/// The main button of the program (Sketch / Finish sketch): larger, with an accent that is not the blue of a chosen tool.
pub fn main_button(
    ui: &mut egui::Ui,
    help: &Help,
    id: &str,
    label: &str,
    key: &str,
    full: bool,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let (fill, text) = if dark {
        (Color32::from_rgb(98, 60, 156), Color32::WHITE)
    } else {
        (Color32::from_rgb(206, 188, 240), Color32::from_rgb(40, 24, 70))
    };
    let lk = look(None, !key.is_empty());
    let img = icon(id, 28.0);
    let b = if full {
        egui::Button::image_and_text(img, egui::RichText::new(label).strong().color(text))
            .min_size(egui::Vec2::new(MAIN_FULL_W, 36.0))
    } else {
        egui::Button::image(img).min_size(egui::Vec2::new(MAIN_ICON_W, 36.0))
    }
    .fill(fill)
    .stroke(egui::Stroke::new(
        1.5,
        if dark {
            Color32::from_rgb(170, 140, 230)
        } else {
            Color32::from_rgb(110, 70, 180)
        },
    ));
    let r = ui.add(b);
    corner_key(ui, &r, key, &lk);
    tip(r, help, id, Some(&format!("{label} ({key})")), None, &lk)
}

pub const MAIN_ICON_W: f32 = 56.0;
pub const MAIN_FULL_W: f32 = 132.0;

/// A small icon-only button (help card on hover), for the dense file section.
pub fn icon_button(ui: &mut egui::Ui, help: &Help, id: &str, enabled: bool) -> egui::Response {
    let lk = Look {
        indicate: false,
        soft_off: !enabled,
        available: enabled,
    };
    let b = egui::Button::image(dim(icon(id, 20.0), &lk)).min_size(egui::Vec2::new(30.0, 28.0));
    let r = add_button(ui, b, if enabled { None } else { Some("") }, &lk);
    r.on_hover_ui(|ui| help.card(ui, id))
}

/// The eye of the document tree: shown (`visible`) or hidden.
pub fn eye_button(ui: &mut egui::Ui, visible: bool) -> egui::Response {
    let b = egui::Button::image(icon(if visible { "eye_on" } else { "eye_off" }, 16.0)).frame(false);
    ui.add(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let s = v as f32 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    #[test]
    fn green_and_red_differ_in_brightness_in_both_themes() {
        for dark in [true, false] {
            let (g, r) = (
                luminance(avail_color(dark, true)),
                luminance(avail_color(dark, false)),
            );
            let ratio = (g.max(r) + 0.05) / (g.min(r) + 0.05);
            assert!(ratio > 1.4, "dark={dark}: {g} vs {r} ({ratio})");
        }
    }

    #[test]
    fn the_three_states_of_a_button() {
        // a button with a shortcut letter that is unavailable: soft off, indicated, not available
        let l = look(Some("why"), true);
        assert!(l.indicate && l.soft_off && !l.available);
        let l = look(None, true);
        assert!(l.indicate && !l.soft_off && l.available);
        // no letter and not marked: no indication at all (views, files)
        let l = look(None, false);
        assert!(!l.indicate && !l.soft_off);
        // no letter but marked (undo): a dot
        stateful();
        let l = look(Some("why"), false);
        assert!(l.indicate && l.soft_off);
        // the flag is used up by one button
        assert!(!look(None, false).indicate);
    }
}
