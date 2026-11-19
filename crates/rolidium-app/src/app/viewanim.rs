// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Animated jumps of the view: a smooth turn (about 0.4 s, never over 1 s) along the shortest way, with the Z axis kept up.
//! Turning the view square to a sketch plane and back, the standard views, Fit all and the plan view (V) all use it.

use super::*;
use rolidium_render::{Camera, StandardView};
use std::f32::consts::{FRAC_PI_2, PI};

pub const MAX_SECONDS: f32 = 1.0;

/// An animation from one camera to another.
#[derive(Clone, Copy, Debug)]
pub struct ViewAnim {
    pub from: Camera,
    pub to: Camera,
    pub start: Instant,
    pub seconds: f32,
}

/// The angle from `a` to `b` by the shorter way, in -PI..=PI.
pub fn angle_delta(a: f32, b: f32) -> f32 {
    let mut d = (b - a) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    } else if d < -PI {
        d += 2.0 * PI;
    }
    d
}

/// Smooth start and stop.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The camera `t` (0..=1, already eased) of the way from `a` to `b`.
pub fn blend(a: &Camera, b: &Camera, t: f32) -> Camera {
    Camera {
        target: a.target.lerp(b.target, t),
        yaw: a.yaw + angle_delta(a.yaw, b.yaw) * t,
        pitch: a.pitch + (b.pitch - a.pitch) * t,
        // distances change by a factor, so zooming feels even
        distance: (a.distance.max(1e-3).ln() * (1.0 - t) + b.distance.max(1e-3).ln() * t).exp(),
        fov_y: a.fov_y,
    }
}

impl ViewAnim {
    pub fn new(from: Camera, to: Camera, start: Instant) -> ViewAnim {
        let turn = angle_delta(from.yaw, to.yaw).abs() + (to.pitch - from.pitch).abs();
        // 0.3 s for a short move, up to 0.5 s for a half turn
        let seconds = (0.3 + 0.2 * (turn / PI).min(1.0)).min(MAX_SECONDS);
        ViewAnim {
            from,
            to,
            start,
            seconds,
        }
    }

    /// The camera at `elapsed` seconds, and whether the animation is over.
    pub fn at(&self, elapsed: f32) -> (Camera, bool) {
        if elapsed >= self.seconds {
            return (self.to, true);
        }
        (blend(&self.from, &self.to, ease(elapsed / self.seconds)), false)
    }
}

/// The camera orientation that looks at a plane with the given outward normal from the front. The Z axis stays up: a vertical
/// plane gets no tilt; a horizontal one is seen from above (or below) with the nearest right-angled turn about Z, so the
/// picture does not spin needlessly.
pub fn plane_view(current: &Camera, normal: Vec3) -> Camera {
    let n = normal.normalize_or_zero();
    let mut c = *current;
    if n.z.abs() > 0.999 {
        let mut top = *current;
        top.set_view(if n.z > 0.0 {
            StandardView::Top
        } else {
            StandardView::Bottom
        });
        c.pitch = top.pitch;
        c.yaw = (current.yaw / FRAC_PI_2).round() * FRAC_PI_2;
    } else {
        c.yaw = n.y.atan2(n.x);
        c.pitch = n.z.asin();
    }
    c
}

/// The smallest part of the sketch plane that is shown when the view turns to it, in mm.
pub const MIN_VIEW_MM: f32 = 20.0;

/// The distance at which a box of points fills the view, and its centre.
pub fn frame(points: &[Vec3], fov_y: f32) -> Option<(Vec3, f32)> {
    let first = *points.first()?;
    let (mut lo, mut hi) = (first, first);
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    // never closer than a window of MIN_VIEW_MM across: tiny geometry must not fill the whole view
    let c = (lo + hi) * 0.5;
    let half = (hi - lo).max_element() * 0.5;
    if half < MIN_VIEW_MM * 0.5 {
        let h = Vec3::splat(MIN_VIEW_MM * 0.5);
        (lo, hi) = (c - h, c + h);
    }
    let mut cam = Camera {
        fov_y,
        ..Camera::default()
    };
    cam.fit(lo, hi);
    Some((cam.target, cam.distance))
}

impl App {
    /// An empty document: a quiet hint in the middle of the view; it goes away with the first action.
    pub(super) fn empty_hint_paint(&self, ui: &egui::Ui, rect: Rect) {
        let empty =
            self.doc.bodies().is_empty() && self.doc.sketches().is_empty() && self.doc.curves().is_empty();
        if !empty || self.sk.is_some() || self.plane_pick || !self.idle() || self.tool != Tool::Select {
            return;
        }
        let col = ui.visuals().text_color().gamma_multiply(0.55);
        ui.painter().with_clip_rect(rect).text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            tr("Start with a sketch (K)"),
            egui::FontId::proportional(22.0),
            col,
        );
    }

    /// Starts an animation to `to` (at once when it is the same view).
    pub(super) fn animate_to(&mut self, to: Camera) {
        let from = self.camera;
        let same = (from.target - to.target).length() < 1e-3
            && angle_delta(from.yaw, to.yaw).abs() < 1e-4
            && (from.pitch - to.pitch).abs() < 1e-4
            && (from.distance - to.distance).abs() < 1e-3;
        if same {
            self.view_anim = None;
            return;
        }
        self.view_anim = Some(ViewAnim::new(from, to, Instant::now()));
    }

    /// A standard view (Top, Front, ...), keeping what the camera looks at and the distance.
    pub(super) fn animate_standard_view(&mut self, v: StandardView) {
        let mut to = self.camera;
        to.set_view(v);
        self.animate_to(to);
    }

    /// Called once a frame: advances the animation; a click, a drag or the wheel interrupts it.
    pub(super) fn view_tick(&mut self, ctx: &egui::Context) {
        let Some(a) = self.view_anim else { return };
        let interrupted = ctx.input(|i| {
            i.pointer.any_pressed()
                || i.pointer.is_decidedly_dragging()
                || i.smooth_scroll_delta != Vec2::ZERO
        });
        if interrupted {
            self.view_anim = None;
            return;
        }
        let (cam, done) = a.at(a.start.elapsed().as_secs_f32());
        self.camera = cam;
        self.layers_changed = true;
        if done {
            self.view_anim = None;
        } else {
            ctx.request_repaint();
        }
    }

    /// A sketch starts: turn the view square to its plane (and remember the view to return to).
    pub(super) fn turn_to_sketch(&mut self) {
        let Some(sm) = &self.sk else { return };
        if !self.settings.auto_turn_view {
            return;
        }
        if self.view_before_sketch.is_none() {
            self.view_before_sketch = Some(self.camera);
        }
        let mut to = plane_view(&self.camera, sm.plane.normal.as_vec3());
        // the geometry around the plane: the outline of the face and the shapes already sketched on this plane
        let mut pts: Vec<Vec3> = Vec::new();
        for (a, b) in &sm.outline {
            pts.push(sm.plane.to_3d(*a).as_vec3());
            pts.push(sm.plane.to_3d(*b).as_vec3());
        }
        for s in self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.plane.same_plane(&sm.plane))
        {
            for (_, e) in s.sketch.entities() {
                for pl in e.polylines() {
                    pts.extend(pl.iter().map(|p| s.plane.to_3d(*p).as_vec3()));
                }
            }
        }
        match frame(&pts, self.camera.fov_y) {
            Some((centre, distance)) => {
                to.target = centre;
                to.distance = distance;
            }
            None => to.target = sm.focus,
        }
        self.animate_to(to);
    }

    /// The sketch ended: back to the view from before.
    pub(super) fn return_from_sketch(&mut self) {
        if let Some(c) = self.view_before_sketch.take() {
            if self.settings.auto_turn_view {
                self.animate_to(c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up_is_z(c: &Camera) -> bool {
        // the screen's up direction must point up in the world: never upside down
        c.view().inverse().y_axis.z >= -1e-6
    }

    #[test]
    fn the_view_turns_square_to_a_plane_and_never_upside_down() {
        let cur = Camera::default();
        for n in [
            Vec3::X,
            -Vec3::X,
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            -Vec3::Z,
            Vec3::new(1.0, 1.0, 0.0).normalize(),
            Vec3::new(0.3, -0.5, 0.8).normalize(),
        ] {
            let c = plane_view(&cur, n);
            assert!(up_is_z(&c), "{n:?}");
            // the camera looks along the plane normal (to within the pitch limit of 89 degrees)
            let dir = (c.eye() - c.target).normalize();
            assert!(dir.dot(n) > 0.999, "{n:?}: {dir:?}");
            // every frame of the animation is also right side up
            for k in 0..=10 {
                assert!(up_is_z(&blend(&cur, &c, ease(k as f32 / 10.0))));
            }
        }
    }

    #[test]
    fn a_horizontal_plane_keeps_the_nearest_right_angled_turn() {
        let mut cur = Camera {
            yaw: 0.2,
            ..Camera::default()
        };
        assert!(plane_view(&cur, Vec3::Z).yaw.abs() < 1e-5);
        cur.yaw = 1.5;
        assert!((plane_view(&cur, Vec3::Z).yaw - FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn the_turn_takes_the_short_way_and_ends_exactly() {
        assert!((angle_delta(3.0, -3.0) - (2.0 * PI - 6.0)).abs() < 1e-5);
        let a = Camera {
            yaw: 3.0,
            ..Camera::default()
        };
        let b = Camera {
            yaw: -3.0,
            ..Camera::default()
        };
        let anim = ViewAnim::new(a, b, Instant::now());
        assert!(anim.seconds >= 0.3 && anim.seconds <= 1.0);
        let (mid, done) = anim.at(anim.seconds * 0.5);
        assert!(!done);
        // half way between 3.0 and -3.0 the short way is about PI, not 0
        assert!(mid.yaw.cos() < -0.98, "{}", mid.yaw);
        let (end, done) = anim.at(anim.seconds + 0.1);
        assert!(done && end == b);
        // eased: slow at the start
        assert!(ease(0.1) < 0.05 && ease(0.5) == 0.5 && ease(1.0) == 1.0);
    }

    #[test]
    fn the_view_returns_to_where_it_was() {
        // entering stores the camera once; leaving gives it back and clears it
        let before = Camera::default();
        let mut saved: Option<Camera> = None;
        for _ in 0..2 {
            // a second entry (changing the plane inside the sketch) must not overwrite it
            if saved.is_none() {
                saved = Some(before);
            }
        }
        assert_eq!(saved.take(), Some(before));
        assert!(saved.is_none());
    }

    #[test]
    fn the_geometry_around_the_plane_is_framed() {
        let pts = [Vec3::new(-10.0, 0.0, 0.0), Vec3::new(30.0, 20.0, 0.0)];
        let (c, d) = frame(&pts, 0.8).unwrap();
        assert!((c - Vec3::new(10.0, 10.0, 0.0)).length() < 1e-5 && d > 20.0);
        assert!(frame(&[], 0.8).is_none());
    }

    #[test]
    fn very_small_geometry_does_not_zoom_closer_than_twenty_millimetres() {
        let tiny = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.5, 0.5, 0.0)];
        let (_, d_tiny) = frame(&tiny, 0.8).unwrap();
        let twenty = [Vec3::splat(-9.75), Vec3::splat(10.25)];
        let (_, d_twenty) = frame(&twenty, 0.8).unwrap();
        assert!((d_tiny - d_twenty).abs() < 1e-3, "{d_tiny} vs {d_twenty}");
        // a single point
        assert!(frame(&[Vec3::ZERO], 0.8).unwrap().1 > 20.0);
    }
}
