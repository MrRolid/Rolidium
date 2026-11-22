// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Patterns and mirrors of the shapes of a sketch (lines, rectangles, circles, arcs). They make new shapes at once (no link: a sketch has no history);
//! the regions are found again by the sketch, so several shapes can be pulled together right away.

use super::*;
use rolidium_sketch::{DVec2, EntityId};

/// A pattern of sketch shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchPattern {
    Linear {
        dir: DVec2,
        count: u32,
        spacing: f64,
        by_total: bool,
        /// A second direction (a grid): direction, count, spacing, `by_total`.
        second: Option<(DVec2, u32, f64, bool)>,
    },
    Circular {
        center: DVec2,
        count: u32,
        angle_deg: f64,
        full: bool,
    },
}

#[derive(Debug, Clone, Copy)]
enum Map2 {
    Move(DVec2),
    Turn {
        center: DVec2,
        angle: f64,
    },
    /// Mirror in the line through `a` with the direction angle `theta`.
    Mirror {
        a: DVec2,
        theta: f64,
    },
}

impl Map2 {
    fn point(self, p: DVec2) -> DVec2 {
        match self {
            Map2::Move(d) => p + d,
            Map2::Turn { center, angle } => {
                let (s, c) = angle.sin_cos();
                let v = p - center;
                center + DVec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
            }
            Map2::Mirror { a, theta } => {
                let (s, c) = (2.0 * theta).sin_cos();
                let v = p - a;
                a + DVec2::new(v.x * c + v.y * s, v.x * s - v.y * c)
            }
        }
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The shapes that `e` becomes (a rectangle that is not axis-aligned any more becomes four lines).
fn map_entity(e: &Entity, m: Map2) -> Vec<Entity> {
    match e {
        Entity::Line { a, b } => vec![Entity::Line {
            a: m.point(*a),
            b: m.point(*b),
        }],
        Entity::Circle { center, radius } => vec![Entity::Circle {
            center: m.point(*center),
            radius: *radius,
        }],
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let c = m.point(*center);
            let (start2, sweep2) = match m {
                Map2::Move(_) => (*start, *sweep),
                Map2::Turn { angle, .. } => (*start + angle, *sweep),
                // an angle f becomes 2 theta - f, and the arc is still counter-clockwise: it starts where the old one ended
                Map2::Mirror { theta, .. } => (2.0 * theta - (*start + *sweep), *sweep),
            };
            vec![Entity::Arc {
                center: c,
                radius: *radius,
                start: start2,
                sweep: sweep2,
            }]
        }
        Entity::Rect { a, b } => {
            let corners = [
                DVec2::new(a.x, a.y),
                DVec2::new(b.x, a.y),
                DVec2::new(b.x, b.y),
                DVec2::new(a.x, b.y),
            ];
            let q: Vec<DVec2> = corners.iter().map(|p| m.point(*p)).collect();
            let axis_aligned = (near(q[0].y, q[1].y)
                && near(q[1].x, q[2].x)
                && near(q[2].y, q[3].y)
                && near(q[3].x, q[0].x))
                || (near(q[0].x, q[1].x)
                    && near(q[1].y, q[2].y)
                    && near(q[2].x, q[3].x)
                    && near(q[3].y, q[0].y));
            if axis_aligned {
                vec![Entity::Rect { a: q[0], b: q[2] }]
            } else {
                (0..4)
                    .map(|i| Entity::Line {
                        a: q[i],
                        b: q[(i + 1) % 4],
                    })
                    .collect()
            }
        }
    }
}

fn step_of(count: u32, spacing: f64, by_total: bool) -> f64 {
    if by_total && count > 1 {
        spacing / (count - 1) as f64
    } else {
        spacing
    }
}

fn chosen(sk: &SketchDoc, ids: &[EntityId]) -> Result<Vec<Entity>, DocError> {
    let all = sk.sketch.entities();
    let list: Vec<Entity> = if ids.is_empty() {
        all.iter().map(|(_, e)| *e).collect()
    } else {
        ids.iter()
            .map(|i| {
                all.iter()
                    .find(|(x, _)| x == i)
                    .map(|(_, e)| *e)
                    .ok_or_else(|| KernelError::Invalid("a shape of the sketch no longer exists".into()))
                    .map_err(DocError::from)
            })
            .collect::<Result<_, _>>()?
    };
    if list.is_empty() {
        return Err(KernelError::Invalid("the sketch has no shapes to repeat".into()).into());
    }
    Ok(list)
}

/// Adds the copies of the shapes (`ids`, empty: all of them); returns how many shapes were added.
pub(crate) fn pattern(sk: &mut SketchDoc, ids: &[EntityId], p: &SketchPattern) -> Result<usize, DocError> {
    let src = chosen(sk, ids)?;
    let mut maps = Vec::new();
    match p {
        SketchPattern::Linear {
            dir,
            count,
            spacing,
            by_total,
            second,
        } => {
            if *count < 1 || *count as usize > MAX_PIECES || dir.length() < 1e-12 {
                return Err(KernelError::Invalid(format!("the number of pieces must be between 1 and {MAX_PIECES} and the direction must have a length")).into());
            }
            let d = dir.normalize();
            let step = step_of(*count, *spacing, *by_total);
            let (c2, d2, s2) = match second {
                Some((dd, c, sp, bt)) => {
                    if *c < 1 || dd.length() < 1e-12 || (*c as usize) * (*count as usize) > MAX_PIECES {
                        return Err(KernelError::Invalid(format!(
                            "a pattern can have at most {MAX_PIECES} pieces"
                        ))
                        .into());
                    }
                    (*c, dd.normalize(), step_of(*c, *sp, *bt))
                }
                None => (1, DVec2::Y, 0.0),
            };
            for j in 0..c2 {
                for i in 0..*count {
                    if i == 0 && j == 0 {
                        continue;
                    }
                    maps.push(Map2::Move(d * step * i as f64 + d2 * s2 * j as f64));
                }
            }
        }
        SketchPattern::Circular {
            center,
            count,
            angle_deg,
            full,
        } => {
            if *count < 1 || *count as usize > MAX_PIECES {
                return Err(KernelError::Invalid(format!(
                    "the number of pieces must be between 1 and {MAX_PIECES}"
                ))
                .into());
            }
            let step = if *full {
                std::f64::consts::TAU / *count as f64
            } else {
                angle_deg.to_radians()
            };
            for i in 1..*count {
                maps.push(Map2::Turn {
                    center: *center,
                    angle: step * i as f64,
                });
            }
        }
    }
    let mut added = 0;
    for m in maps {
        for e in &src {
            for n in map_entity(e, m) {
                sk.sketch.add(n)?;
                added += 1;
            }
        }
    }
    Ok(added)
}

/// Mirrors the shapes (`ids`, empty: all of them) in the line through `a` and `b`; with `keep` the originals stay, else they are replaced.
pub(crate) fn mirror(
    sk: &mut SketchDoc,
    ids: &[EntityId],
    a: DVec2,
    b: DVec2,
    keep: bool,
) -> Result<usize, DocError> {
    let d = b - a;
    if d.length() < 1e-9 {
        return Err(KernelError::Invalid("the mirror line has no length".into()).into());
    }
    let src = chosen(sk, ids)?;
    let m = Map2::Mirror {
        a,
        theta: d.y.atan2(d.x),
    };
    if !keep {
        let drop: Vec<EntityId> = if ids.is_empty() {
            sk.sketch.entities().iter().map(|(i, _)| *i).collect()
        } else {
            ids.to_vec()
        };
        sk.sketch.remove(&drop);
    }
    let mut added = 0;
    for e in &src {
        for n in map_entity(e, m) {
            sk.sketch.add(n)?;
            added += 1;
        }
    }
    Ok(added)
}

/// The numbers a person can change for a shape of a sketch, in this order: circle (diameter, centre x, centre y), arc (radius, angle of the
/// arc in degrees), rectangle (width, height), line (length, angle from the x axis in degrees).
pub fn entity_dims(e: &Entity) -> Vec<f64> {
    match *e {
        Entity::Circle { center, radius } => vec![radius * 2.0, center.x, center.y],
        Entity::Arc { radius, sweep, .. } => vec![radius, sweep.abs().to_degrees()],
        Entity::Rect { a, b } => vec![(b.x - a.x).abs(), (b.y - a.y).abs()],
        Entity::Line { a, b } => {
            let d = b - a;
            vec![d.length(), d.y.atan2(d.x).to_degrees()]
        }
    }
}

/// The shape with some of its numbers changed (`None`: unchanged), in the order of [`entity_dims`]. `None` for numbers that make no shape.
pub fn entity_with_dims(old: &Entity, v: &[Option<f64>]) -> Option<Entity> {
    let get = |i: usize, current: f64| -> Option<f64> {
        match v.get(i) {
            Some(Some(x)) if x.is_finite() => Some(*x),
            Some(Some(_)) => None,
            _ => Some(current),
        }
    };
    let cur = entity_dims(old);
    match *old {
        Entity::Circle { .. } => {
            let d = get(0, cur[0])?;
            let (cx, cy) = (get(1, cur[1])?, get(2, cur[2])?);
            (d.abs() > 1e-9).then(|| Entity::Circle {
                center: DVec2::new(cx, cy),
                radius: d.abs() / 2.0,
            })
        }
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let r = get(0, cur[0])?.abs();
            let ang = get(1, cur[1])?.abs();
            if r <= 1e-9 || ang <= 1e-6 || ang > 359.999 {
                return None;
            }
            if (ang - cur[1]).abs() > 1e-9 {
                // a new angle: the centre and the start stay, the arc is as long as the angle says (on the same side)
                let sign = if sweep < 0.0 { -1.0 } else { 1.0 };
                return Some(Entity::Arc {
                    center,
                    radius: r,
                    start,
                    sweep: sign * ang.to_radians(),
                });
            }
            // only the radius: the ends and the side stay
            let seg = Segment::Arc {
                center,
                radius,
                start,
                sweep,
            };
            Entity::arc_chord_radius(seg.start_point(), seg.end_point(), r, seg.mid_point())
        }
        Entity::Rect { a, b } => {
            let (w, h) = (get(0, cur[0])?.abs(), get(1, cur[1])?.abs());
            if w <= 1e-9 || h <= 1e-9 {
                return None;
            }
            let sx = if b.x >= a.x { 1.0 } else { -1.0 };
            let sy = if b.y >= a.y { 1.0 } else { -1.0 };
            Some(Entity::Rect {
                a,
                b: a + DVec2::new(sx * w, sy * h),
            })
        }
        Entity::Line { a, .. } => {
            let l = get(0, cur[0])?.abs();
            let ang = get(1, cur[1])?.to_radians();
            if l <= 1e-9 {
                return None;
            }
            Some(Entity::Line {
                a,
                b: a + DVec2::new(ang.cos(), ang.sin()) * l,
            })
        }
    }
}

/// The shapes a pattern of the shapes `ids` (empty: all) would add to the sketch, without changing it (for the preview in the model area).
pub fn preview_pattern(sk: &SketchDoc, ids: &[EntityId], p: &SketchPattern) -> Result<Vec<Entity>, String> {
    let mut copy = sk.clone();
    let before: Vec<EntityId> = copy.sketch.entities().iter().map(|(i, _)| *i).collect();
    pattern(&mut copy, ids, p).map_err(|e| e.to_string())?;
    Ok(copy
        .sketch
        .entities()
        .iter()
        .filter(|(i, _)| !before.contains(i))
        .map(|(_, e)| *e)
        .collect())
}

/// The shapes a mirror in the line through `a` and `b` would add (with `keep`; without it the image replaces the originals, so all of the
/// shapes of the result are returned), without changing the sketch.
pub fn preview_mirror(
    sk: &SketchDoc,
    ids: &[EntityId],
    a: DVec2,
    b: DVec2,
    keep: bool,
) -> Result<Vec<Entity>, String> {
    let mut copy = sk.clone();
    let before: Vec<EntityId> = copy.sketch.entities().iter().map(|(i, _)| *i).collect();
    mirror(&mut copy, ids, a, b, keep).map_err(|e| e.to_string())?;
    Ok(copy
        .sketch
        .entities()
        .iter()
        .filter(|(i, _)| !before.contains(i))
        .map(|(_, e)| *e)
        .collect())
}
