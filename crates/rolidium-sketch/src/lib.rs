// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! 2D sketch: entities (line, rectangle, circle) and detection of the closed regions they form.
//!
//! Regions are found from the planar arrangement of all curves: every curve is split at its
//! intersections with every other curve, bridges and dangling chains are dropped, the faces of the
//! remaining graph are walked, and components lying inside other faces become holes of those faces.
//! Overlapping, nested and touching shapes therefore all give the regions one expects.

pub mod edgedims;
pub mod join;
mod plane;
mod regions;

pub use plane::PlaneDef;
pub use regions::Region;
pub use rolidium_path2d::{Contour, DVec2, Segment};

pub type EntityId = u32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Entity {
    Line {
        a: DVec2,
        b: DVec2,
    },
    /// Axis-aligned rectangle (in sketch coordinates) given by two opposite corners.
    Rect {
        a: DVec2,
        b: DVec2,
    },
    Circle {
        center: DVec2,
        radius: f64,
    },
    /// An auxiliary axis (drawn dash-dotted): the straight line through `a` and `b`. It makes no regions, is no part of a profile or a
    /// path, and serves as the axis of a mirror, a circular pattern or a revolve. The two points only give the line a direction and a
    /// drawn length.
    Axis {
        a: DVec2,
        b: DVec2,
    },
    /// Circular arc: from angle `start` (radians) by `sweep` (positive counter-clockwise).
    Arc {
        center: DVec2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SketchError {
    #[error("The shape has no size (zero length, width, height or radius)")]
    Degenerate,
    #[error("The shape contains invalid numbers")]
    NotFinite,
    #[error("No such shape in the sketch")]
    NoSuchEntity,
}

pub const MIN_SIZE: f64 = 1e-6;

impl Entity {
    pub fn validate(&self) -> Result<(), SketchError> {
        let finite = match *self {
            Entity::Line { a, b } | Entity::Rect { a, b } | Entity::Axis { a, b } => {
                a.is_finite() && b.is_finite()
            }
            Entity::Circle { center, radius } => center.is_finite() && radius.is_finite(),
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => center.is_finite() && radius.is_finite() && start.is_finite() && sweep.is_finite(),
        };
        if !finite {
            return Err(SketchError::NotFinite);
        }
        let ok = match *self {
            Entity::Line { a, b } | Entity::Axis { a, b } => a.distance(b) > MIN_SIZE,
            Entity::Rect { a, b } => (a.x - b.x).abs() > MIN_SIZE && (a.y - b.y).abs() > MIN_SIZE,
            Entity::Circle { radius, .. } => radius > MIN_SIZE,
            Entity::Arc { radius, sweep, .. } => {
                radius > MIN_SIZE && sweep.abs() > 1e-6 && sweep.abs() <= std::f64::consts::TAU + 1e-9
            }
        };
        if ok {
            Ok(())
        } else {
            Err(SketchError::Degenerate)
        }
    }

    /// An auxiliary axis (no region, no profile, no path).
    pub fn is_axis(&self) -> bool {
        matches!(self, Entity::Axis { .. })
    }

    /// The line through the two ends of a line or an axis: a point and the unit direction.
    pub fn axis_line(&self) -> Option<(DVec2, DVec2)> {
        match *self {
            Entity::Line { a, b } | Entity::Axis { a, b } if a.distance(b) > MIN_SIZE => {
                Some((a, (b - a).normalize()))
            }
            _ => None,
        }
    }

    /// The same line as an auxiliary axis (a line becomes an axis; anything else is not changed).
    pub fn as_axis(&self) -> Entity {
        match *self {
            Entity::Line { a, b } => Entity::Axis { a, b },
            e => e,
        }
    }

    /// The same axis as an ordinary line.
    pub fn as_line(&self) -> Entity {
        match *self {
            Entity::Axis { a, b } => Entity::Line { a, b },
            e => e,
        }
    }

    /// Corner points of a rectangle in drawing order.
    pub fn rect_corners(a: DVec2, b: DVec2) -> [DVec2; 4] {
        [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)]
    }

    /// Points other tools can snap to.
    pub fn snap_points(&self) -> Vec<(DVec2, SnapKind)> {
        match *self {
            Entity::Line { a, b } | Entity::Axis { a, b } => vec![
                (a, SnapKind::Endpoint),
                (b, SnapKind::Endpoint),
                ((a + b) * 0.5, SnapKind::Midpoint),
            ],
            Entity::Rect { a, b } => {
                let c = Self::rect_corners(a, b);
                let mut v: Vec<_> = c.iter().map(|p| (*p, SnapKind::Endpoint)).collect();
                v.extend((0..4).map(|i| ((c[i] + c[(i + 1) % 4]) * 0.5, SnapKind::Midpoint)));
                v
            }
            Entity::Circle { center, .. } => vec![(center, SnapKind::Center)],
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let seg = Segment::Arc {
                    center,
                    radius,
                    start,
                    sweep,
                };
                vec![
                    (seg.start_point(), SnapKind::Endpoint),
                    (seg.end_point(), SnapKind::Endpoint),
                    (seg.mid_point(), SnapKind::Midpoint),
                    (center, SnapKind::Center),
                ]
            }
        }
    }

    /// Drawable outline as polylines (arcs of circles are sampled finely).
    pub fn polylines(&self) -> Vec<Vec<DVec2>> {
        match *self {
            Entity::Line { a, b } | Entity::Axis { a, b } => vec![vec![a, b]],
            Entity::Rect { a, b } => {
                let c = Self::rect_corners(a, b);
                vec![vec![c[0], c[1], c[2], c[3], c[0]]]
            }
            Entity::Circle { center, radius } => {
                let mut p = Segment::Arc {
                    center,
                    radius,
                    start: 0.0,
                    sweep: std::f64::consts::TAU,
                }
                .polyline(0.05);
                if let Some(f) = p.first().copied() {
                    *p.last_mut().unwrap() = f;
                }
                vec![p]
            }
            Entity::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                vec![Segment::Arc {
                    center,
                    radius,
                    start,
                    sweep,
                }
                .polyline(0.05)]
            }
        }
    }

    /// The arc from `a` through `mid` to `b` (`None` if the three points are in a line).
    pub fn arc_through(a: DVec2, mid: DVec2, b: DVec2) -> Option<Entity> {
        let d = 2.0 * (a.x * (mid.y - b.y) + mid.x * (b.y - a.y) + b.x * (a.y - mid.y));
        if d.abs() < 1e-12 {
            return None;
        }
        let (a2, m2, b2) = (a.length_squared(), mid.length_squared(), b.length_squared());
        let center = DVec2::new(
            (a2 * (mid.y - b.y) + m2 * (b.y - a.y) + b2 * (a.y - mid.y)) / d,
            (a2 * (b.x - mid.x) + m2 * (a.x - b.x) + b2 * (mid.x - a.x)) / d,
        );
        let radius = center.distance(a);
        let ang = |p: DVec2| (p - center).to_angle();
        let (sa, sm, sb) = (ang(a), ang(mid), ang(b));
        let ccw = |from: f64, to: f64| (to - from).rem_euclid(std::f64::consts::TAU);
        let mut sweep = ccw(sa, sb);
        if ccw(sa, sm) > sweep {
            sweep -= std::f64::consts::TAU;
        }
        Some(Entity::Arc {
            center,
            radius,
            start: sa,
            sweep,
        })
    }

    /// An arc from `a` to `b` with the given radius; of the four possible arcs the one whose middle is
    /// nearest to `near`. `None` if the radius is smaller than half the chord.
    pub fn arc_chord_radius(a: DVec2, b: DVec2, radius: f64, near: DVec2) -> Option<Entity> {
        let chord = a.distance(b);
        if radius < chord * 0.5 - 1e-9 || chord < 1e-9 {
            return None;
        }
        let m = (a + b) * 0.5;
        let h = (radius * radius - chord * chord * 0.25).max(0.0).sqrt();
        let n = DVec2::new(-(b - a).y, (b - a).x).normalize();
        let mut best: Option<(f64, Entity)> = None;
        for c in [m + n * h, m - n * h] {
            for ccw in [true, false] {
                let (sa, sb) = ((a - c).to_angle(), (b - c).to_angle());
                let mut sweep = (sb - sa).rem_euclid(std::f64::consts::TAU);
                if !ccw {
                    sweep -= std::f64::consts::TAU;
                }
                if sweep.abs() < 1e-9 {
                    continue;
                }
                let e = Entity::Arc {
                    center: c,
                    radius,
                    start: sa,
                    sweep,
                };
                let mid = Segment::Arc {
                    center: c,
                    radius,
                    start: sa,
                    sweep,
                }
                .mid_point();
                let d = mid.distance(near);
                if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                    best = Some((d, e));
                }
            }
        }
        best.map(|b| b.1)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Sketch {
    entities: Vec<(EntityId, Entity)>,
    next_id: EntityId,
}

impl Sketch {
    pub fn new() -> Self {
        Sketch {
            entities: Vec::new(),
            next_id: 1,
        }
    }

    pub fn entities(&self) -> &[(EntityId, Entity)] {
        &self.entities
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn add(&mut self, e: Entity) -> Result<EntityId, SketchError> {
        e.validate()?;
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.entities.push((id, e));
        Ok(id)
    }

    /// Replaces the geometry of an existing entity (its identity is kept).
    pub fn replace(&mut self, id: EntityId, e: Entity) -> Result<(), SketchError> {
        e.validate()?;
        let slot = self
            .entities
            .iter_mut()
            .find(|(i, _)| *i == id)
            .ok_or(SketchError::NoSuchEntity)?;
        slot.1 = e;
        Ok(())
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|(i, _)| *i == id).map(|(_, e)| e)
    }

    pub fn remove(&mut self, ids: &[EntityId]) {
        self.entities.retain(|(i, _)| !ids.contains(i));
    }

    /// Closed regions formed by the curves (open chains are ignored).
    pub fn regions(&self) -> Vec<Region> {
        regions::find_regions(&self.entities)
    }
}

#[cfg(test)]
mod tests;
