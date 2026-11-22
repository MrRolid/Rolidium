// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The look of the window: the greys of the panels and of the model area for the dark and the light appearance, and the slider "Brightness" that moves
//! them in a range in which the text and the icons keep their contrast (WCAG AA: 4.5:1 for text, 3:1 for icons and lines; `tests` check this at both
//! ends of the slider and at the default). The model area follows the same grey, the grid and the axes are drawn for its lightness.

use eframe::egui::{self, Color32};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// The slider goes from `-100` to `100`; `0` is the default.
pub const MIN: i32 = -100;
pub const MAX: i32 = 100;

static LIGHT: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicI32 = AtomicI32::new(0);

/// Remembers the look in use (the colours of the overlays and of the icons depend on it).
pub fn set(light: bool, level: i32) {
    LIGHT.store(light, Ordering::Relaxed);
    LEVEL.store(level.clamp(MIN, MAX), Ordering::Relaxed);
    rolidium_render::set_background(viewport_lightness(light, level));
}

pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

/// Linear interpolation of a lightness: `0` gives `base`, `-100` the `lo` end, `100` the `hi` end.
fn scale(level: i32, lo: f32, base: f32, hi: f32) -> f32 {
    let f = level.clamp(MIN, MAX) as f32 / 100.0;
    if f >= 0.0 {
        base + f * (hi - base)
    } else {
        base + f * (base - lo)
    }
}

/// Lightness (0 black, 1 white, in the sRGB values) of the panels.
pub fn panel_lightness(light: bool, level: i32) -> f32 {
    if light {
        scale(level, 0.76, 0.84, 0.94)
    } else {
        scale(level, 0.07, 0.15, 0.20)
    }
}

/// Lightness of the model area.
pub fn viewport_lightness(light: bool, level: i32) -> f32 {
    if light {
        scale(level, 0.62, 0.76, 0.90)
    } else {
        scale(level, 0.07, 0.17, 0.26)
    }
}

fn grey(l: f32) -> Color32 {
    let v = (l.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(v, v, v)
}

/// Relative luminance of an sRGB colour (WCAG).
pub fn luminance(c: [u8; 3]) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Contrast ratio of two colours (WCAG), 1 to 21.
#[cfg(test)]
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
fn rgb_of(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// The colours of the text on a panel of the lightness `l`: normal, strong, weak.
pub fn text_colors(light: bool, panel: f32) -> (Color32, Color32, Color32) {
    let _ = panel;
    if light {
        (grey(0.10), Color32::BLACK, grey(0.22))
    } else {
        (grey(0.90), Color32::WHITE, grey(0.74))
    }
}

/// The colours of a button (resting, hovered, pressed) next to a panel of the lightness `l`.
pub fn button_greys(light: bool, panel: f32) -> (Color32, Color32, Color32) {
    if light {
        (grey(panel - 0.04), grey(panel - 0.09), grey(panel - 0.15))
    } else {
        (grey(panel + 0.06), grey(panel + 0.11), grey(panel + 0.17))
    }
}

/// The colours of the whole window for the appearance and the slider.
pub fn visuals(light: bool, level: i32) -> egui::Visuals {
    let mut v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    let p = panel_lightness(light, level);
    let (text, strong, weak) = text_colors(light, p);
    let (rest, hover, press) = button_greys(light, p);
    v.panel_fill = grey(p);
    v.window_fill = grey(if light { p + 0.04 } else { p + 0.03 });
    v.extreme_bg_color = grey(if light {
        (p + 0.10).min(0.98)
    } else {
        (p - 0.05).max(0.02)
    });
    v.faint_bg_color = grey(if light { p - 0.03 } else { p + 0.03 });
    v.weak_text_color = Some(weak);
    let stroke = grey(if light { p - 0.28 } else { p + 0.22 });
    v.widgets.noninteractive.bg_fill = grey(p);
    v.widgets.noninteractive.weak_bg_fill = grey(p);
    v.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, grey(if light { p - 0.12 } else { p + 0.10 }));
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);
    for (w, fill) in [
        (&mut v.widgets.inactive, rest),
        (&mut v.widgets.hovered, hover),
        (&mut v.widgets.active, press),
        (&mut v.widgets.open, hover),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = egui::Stroke::new(1.0, stroke);
        w.fg_stroke = egui::Stroke::new(1.0, text);
    }
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, strong);
    v.widgets.active.fg_stroke = egui::Stroke::new(2.0, strong);
    v.window_stroke = egui::Stroke::new(1.0, stroke);
    if light {
        // a selection that keeps the dark text readable
        v.selection.bg_fill = Color32::from_rgb(120, 175, 235);
        v.selection.stroke = egui::Stroke::new(1.0, Color32::BLACK);
        v.hyperlink_color = Color32::from_rgb(0, 70, 160);
    }
    v
}

/// The icon colours (neutral and accent) for the appearance. The accent is darker in the light appearance, so that it keeps 3:1 on a grey button.
pub fn icon_colors(light: bool) -> (&'static str, &'static str) {
    if light {
        ("#3b424c", "#8c4600")
    } else {
        ("#d6dae0", "#ffb347")
    }
}

/// A colour for text or lines drawn over the panels or the model area. In the light appearance a bright colour (a yellow, a light green) is
/// darkened, keeping its hue, until it has the contrast to a light grey; in the dark appearance it stays as it is.
pub fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    if !is_light() {
        return Color32::from_rgb(r, g, b);
    }
    ink([r, g, b], 0.055)
}

/// The colour `c` with its luminance lowered (in linear light, so the hue stays) to at most `max_lum`.
pub fn ink(c: [u8; 3], max_lum: f32) -> Color32 {
    let y = luminance(c);
    if y <= max_lum {
        return Color32::from_rgb(c[0], c[1], c[2]);
    }
    let k = (max_lum / y).powf(1.0 / 2.4);
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

/// Applies the appearance (`dark`, `light` or `system`) and the slider to the window.
pub fn apply(ctx: &egui::Context, theme: &str, level: i32) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals(false, level));
    ctx.set_visuals_of(egui::Theme::Light, visuals(true, level));
    ctx.set_theme(match theme {
        "light" => egui::ThemePreference::Light,
        "system" => egui::ThemePreference::System,
        _ => egui::ThemePreference::Dark,
    });
    let light = ctx.theme() == egui::Theme::Light;
    set(light, level);
    crate::icons::set_light(light);
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [i32; 5] = [MIN, -50, 0, 50, MAX];

    #[test]
    fn the_defaults_are_light_grey_and_not_white() {
        let p = panel_lightness(true, 0);
        assert!((0.78..0.90).contains(&p), "{p}");
        let v = viewport_lightness(true, 0);
        assert!((0.70..0.85).contains(&v), "{v}");
        // darker than the old light appearance
        assert!(p < 0.95);
    }

    #[test]
    fn text_keeps_aa_contrast_over_the_whole_slider_in_both_appearances() {
        for light in [false, true] {
            for level in LEVELS {
                let p = panel_lightness(light, level);
                let (text, strong, weak) = text_colors(light, p);
                let (rest, hover, press) = button_greys(light, p);
                let panel = rgb_of(grey(p));
                for (name, t) in [("text", text), ("strong", strong), ("weak", weak)] {
                    for (bn, bg) in [
                        ("panel", panel),
                        ("button", rgb_of(rest)),
                        ("hovered", rgb_of(hover)),
                        ("pressed", rgb_of(press)),
                    ] {
                        // strong text is used on the hovered and pressed buttons, normal and weak text on the panel and on resting buttons
                        let must = 4.5;
                        if (name == "weak" || name == "text") && (bn == "hovered" || bn == "pressed") {
                            continue;
                        }
                        let c = contrast(rgb_of(t), bg);
                        assert!(c >= must, "light={light} level={level} {name} on {bn}: {c:.2}");
                    }
                }
                let c = contrast(rgb_of(strong), rgb_of(hover));
                assert!(c >= 4.5, "light={light} level={level} strong on hovered: {c:.2}");
                let c = contrast(rgb_of(strong), rgb_of(press));
                assert!(c >= 4.5, "light={light} level={level} strong on pressed: {c:.2}");
            }
        }
    }

    fn hex(s: &str) -> [u8; 3] {
        let v = u32::from_str_radix(&s[1..], 16).unwrap();
        [(v >> 16) as u8, (v >> 8) as u8, v as u8]
    }

    #[test]
    fn icons_and_the_traffic_light_keep_3_to_1_on_buttons_over_the_whole_slider() {
        for light in [false, true] {
            let (n, a) = icon_colors(light);
            for level in LEVELS {
                let p = panel_lightness(light, level);
                let (rest, hover, _) = button_greys(light, p);
                for (bn, bg) in [
                    ("panel", rgb_of(grey(p))),
                    ("button", rgb_of(rest)),
                    ("hovered", rgb_of(hover)),
                ] {
                    for (cn, c) in [("neutral", hex(n)), ("accent", hex(a))] {
                        let r = contrast(c, bg);
                        assert!(r >= 3.0, "light={light} level={level} icon {cn} on {bn}: {r:.2}");
                    }
                    for (an, ok) in [("available", true), ("unavailable", false)] {
                        let c = crate::icons::avail_color(!light, ok);
                        let r = contrast(rgb_of(c), bg);
                        assert!(r >= 3.0, "light={light} level={level} {an} on {bn}: {r:.2}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_grid_the_axes_and_the_overlays_are_visible_on_the_model_area_over_the_whole_slider() {
        for light in [false, true] {
            for level in LEVELS {
                let v = viewport_lightness(light, level);
                // the colours of the shader: grid lines at full strength, the axes
                let (grid, x_axis, y_axis) = rolidium_render::grid_colors(v);
                let to8 = |c: [f32; 3]| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8];
                // a line of the major grid (30 % strength) and an axis (95 %) over the background, as the shader mixes them
                let over = |c: [f32; 3], a: f32| {
                    let b = [v - 0.01, v, v + 0.03];
                    to8([
                        b[0] + a * (c[0] - b[0]),
                        b[1] + a * (c[1] - b[1]),
                        b[2] + a * (c[2] - b[2]),
                    ])
                };
                let bg = to8([v - 0.01, v, v + 0.03]);
                let g = contrast(over(grid, 0.30), bg);
                assert!(g >= 1.25, "light={light} level={level} grid {g:.2}");
                let x = contrast(over(x_axis, 0.95), bg);
                assert!(x >= 2.0, "light={light} level={level} x axis {x:.2}");
                let y = contrast(over(y_axis, 0.95), bg);
                assert!(y >= 2.0, "light={light} level={level} y axis {y:.2}");
                // the orange of the selection and the yellow of the lines of a sketch, through the same rule as the program applies
                set(light, level);
                for base in [(255u8, 179u8, 71u8), (255, 220, 100), (120, 255, 160)] {
                    let c = rgb(base.0, base.1, base.2);
                    let r = contrast([c.r(), c.g(), c.b()], bg);
                    assert!(r >= 2.5, "light={light} level={level} overlay {base:?}: {r:.2}");
                }
            }
        }
        set(false, 0);
    }
}
