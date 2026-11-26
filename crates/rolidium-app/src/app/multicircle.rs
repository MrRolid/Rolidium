// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The quick multiplication of a circle that is being drawn: while the diameter is set (the dimensions to the walls are shown), the right button shows
//! the same circle in all the corners of the closed shape the circle is inside (the same distances from the two nearest walls), or, inside a circle,
//! evenly round its centre (every right click adds a piece, Shift + right click takes one away). The left click draws them all as one step of Undo.

use super::*;
use crate::sketchmode::Multi;
use rolidium_doc::rolidium_sketch::multi::{
    around_copies, circle_of_points, closed_loops, container_of, corner_copies, Container,
};

/// The most pieces round a circle.
pub const MULTI_MAX: usize = 10;

impl App {
    /// While a circle is drawn and its centre is placed: what it is inside (found once for the centre).
    pub(super) fn update_container(&mut self) {
        let Some(sm) = &self.sk else { return };
        let Some(centre) = sm
            .drawing
            .as_ref()
            .map(|d| d.start)
            .filter(|_| sm.tool == SkTool::Circle)
        else {
            return;
        };
        if sm.container.as_ref().is_some_and(|(c, _)| *c == centre) {
            return;
        }
        let plane = sm.plane;
        let mut segs = sm.straight.clone();
        segs.extend(self.sketch_segments(&plane, sm.body));
        let mut circles: Vec<(DVec2, f64)> = Vec::new();
        for s in self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.visible && s.plane.same_plane(&plane) && s.target == sm.body)
        {
            for (_, e) in s.sketch.entities() {
                if let Entity::Circle { center, radius } = *e {
                    circles.push((center, radius));
                }
            }
        }
        // a round face: its outline is a polyline that lies on a circle
        for l in closed_loops(&sm.outline, 1e-4) {
            if let Some(c) = circle_of_points(&l) {
                circles.push(c);
            }
        }
        let loops = closed_loops(&segs, 1e-4);
        let found = container_of(centre, &loops, &circles);
        if let Some(sm) = &mut self.sk {
            sm.container = Some((centre, found));
        }
    }

    /// Whether the right button would make copies now.
    pub(super) fn multi_available(&self) -> bool {
        self.sk.as_ref().is_some_and(|sm| {
            sm.tool == SkTool::Circle
                && sm.drawing.is_some()
                && sm.container.as_ref().is_some_and(|(_, c)| c.is_some())
        })
    }

    /// The right button while a circle is drawn: copies in the corners of the closed shape (a second click takes them away), or one piece more (Shift:
    /// one less) round the circle. `false` when the circle is inside no closed shape: the right button then cancels the drawing as before.
    pub(super) fn multi_right_click(&mut self, shift: bool) -> bool {
        if !self.multi_available() {
            return false;
        }
        let Some(sm) = &mut self.sk else { return false };
        let Some(container) = sm.container.as_ref().and_then(|(_, c)| c.clone()) else {
            return false;
        };
        sm.multi = match (container, sm.multi.take()) {
            (Container::Polygon(_), Some(_)) => None,
            (c @ Container::Polygon(_), None) => Some(Multi { container: c, n: 0 }),
            (c @ Container::Circle(..), old) => {
                let n = old.map_or(1, |m| m.n);
                let n = if shift { n.saturating_sub(1) } else { n + 1 }.clamp(1, MULTI_MAX);
                (n > 1).then_some(Multi { container: c, n })
            }
        };
        self.layers_changed = true;
        true
    }

    /// The centres of the copies (not the original) that the right button made.
    pub(super) fn multi_centres(&self) -> Vec<DVec2> {
        let Some(sm) = &self.sk else { return Vec::new() };
        let (Some(m), Some(d)) = (&sm.multi, &sm.drawing) else {
            return Vec::new();
        };
        match &m.container {
            Container::Polygon(p) => corner_copies(p, d.start),
            Container::Circle(c, _) => around_copies(*c, d.start, m.n),
        }
    }

    /// The copies of the circle that is drawn, for the preview and for the drawing.
    pub(super) fn multi_copies(&self, e: Entity) -> Vec<Entity> {
        let Entity::Circle { radius, .. } = e else {
            return Vec::new();
        };
        self.multi_centres()
            .into_iter()
            .map(|center| Entity::Circle { center, radius })
            .collect()
    }

    /// The text under the diameter box.
    pub(super) fn multi_hint(&self) -> Option<String> {
        if !self.multi_available() {
            return None;
        }
        let sm = self.sk.as_ref()?;
        Some(match &sm.multi {
            None => tr("Right button: more pieces"),
            Some(m) if matches!(m.container, Container::Polygon(_)) => tr("Right button: back to one"),
            Some(m) => trf(
                "{} pieces, right button: one more, Shift: one less",
                &[m.n.to_string()],
            ),
        })
    }
}
