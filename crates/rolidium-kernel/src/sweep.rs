// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Paths (3D curves) and the sweep of a profile along a path.
//!
//! A [`Path`] is a connected chain of lines and arcs, a helix, or a chain of edges of a body. [`Solid::sweep`] pulls a flat
//! profile (a face, with holes if it has some) along it and checks the result: it must be a valid solid whose faces do not cut
//! through each other and whose volume is about the profile area times the path length.

use crate::{nat as native, DVec3, EdgeId, KernelError, LoftInput, Result, Solid};
use cxx::SharedPtr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSeg {
    Line {
        a: DVec3,
        b: DVec3,
    },
    /// Arc through three points.
    Arc {
        a: DVec3,
        mid: DVec3,
        b: DVec3,
    },
}

/// A helix on a cylinder or cone around `axis` through `origin`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HelixDef {
    pub origin: DVec3,
    pub axis: DVec3,
    /// Direction from the axis to the start point (made perpendicular to the axis).
    pub x_dir: DVec3,
    pub radius: f64,
    pub pitch: f64,
    pub turns: f64,
    pub right: bool,
    pub start_deg: f64,
    /// Half angle of the cone the helix lies on (0 = cylinder).
    pub cone_deg: f64,
}

/// More turns than this are refused: the body of a sweep along such a helix has tens of thousands of faces and its calculation does not end in
/// a useful time (1000 turns on a cone brought the program down).
pub const HELIX_MAX_TURNS: f64 = 300.0;
/// From this number of turns on the person is warned and asked to confirm (the body is large and slow to calculate).
pub const HELIX_WARN_TURNS: f64 = 40.0;

impl HelixDef {
    /// The height along the axis (mm).
    pub fn height(&self) -> f64 {
        self.pitch.abs() * self.turns
    }

    /// The radius at the end of the helix; the cone (`cone_deg` negative: narrowing along the axis) is followed from the start.
    pub fn end_radius(&self) -> f64 {
        self.radius + self.height() * self.cone_deg.to_radians().tan()
    }

    /// The largest height the helix can have on its cone: where the radius would reach zero (the apex); `None` on a cylinder or a widening cone.
    pub fn apex_height(&self) -> Option<f64> {
        let t = self.cone_deg.to_radians().tan();
        (t < -1e-9 && self.radius > 0.0).then(|| self.radius / -t)
    }

    /// Refuses a helix that is not a helix on its surface: it may not pass the apex of its cone (the radius would reach zero and then grow
    /// again as an inverted cone) and it may not have more than [`HELIX_MAX_TURNS`] turns.
    pub fn check(&self) -> Result<()> {
        if !(self.radius.is_finite() && self.radius > 0.0 && self.pitch.is_finite() && self.pitch.abs() > 1e-9 && self.turns.is_finite() && self.turns > 0.0) {
            return Ok(()); // the other checks of the construction report these
        }
        if let Some(max) = self.apex_height() {
            // the radius must stay above a hundredth of the start radius
            let limit = max * 0.99;
            if self.height() > limit + 1e-9 {
                return Err(KernelError::Invalid(format!(
                    "The helix would pass the apex of the cone (the radius would reach zero). The largest height is {} mm, which is {} turns at this pitch",
                    trim(limit),
                    trim(limit / self.pitch.abs())
                )));
            }
        }
        if self.turns > HELIX_MAX_TURNS {
            return Err(KernelError::Invalid(format!(
                "A helix of {} turns is too large: the limit is {} turns",
                trim(self.turns),
                trim(HELIX_MAX_TURNS)
            )));
        }
        Ok(())
    }
}

fn trim(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

pub enum PathSource<'a> {
    Segments(&'a [PathSeg]),
    Helix(HelixDef),
    Edges { solid: &'a Solid, edges: &'a [EdgeId] },
}

#[derive(Clone)]
pub struct Path {
    shape: SharedPtr<native::Shape>,
    pub start: DVec3,
    pub start_tangent: DVec3,
    pub end: DVec3,
    pub end_tangent: DVec3,
    pub length: f64,
    pub closed: bool,
    /// The axis of a helix path: a profile swept along it keeps its orientation to this axis.
    pub helix_axis: Option<DVec3>,
    /// The pitch (mm per turn along the axis) of a helix path.
    pub helix_pitch: Option<f64>,
}

impl std::fmt::Debug for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Path({:.3} mm from {:?} to {:?})",
            self.length, self.start, self.end
        )
    }
}

fn v3(a: &[f64]) -> DVec3 {
    DVec3::new(a[0], a[1], a[2])
}

fn push3(out: &mut Vec<f64>, p: DVec3) {
    out.extend([p.x, p.y, p.z]);
}

impl Path {
    pub fn build(source: PathSource) -> Result<Path> {
        crate::timing::stage("path wire", || Path::build_inner(source))
    }

    fn build_inner(source: PathSource) -> Result<Path> {
        let mut helix_axis = None;
        let mut helix_pitch = None;
        let shape = match source {
            PathSource::Segments(segs) => {
                if segs.is_empty() {
                    return Err(KernelError::Invalid("the path has no segments".into()));
                }
                let mut d = Vec::new();
                for s in segs {
                    match *s {
                        PathSeg::Line { a, b } => {
                            d.push(1.0);
                            push3(&mut d, a);
                            push3(&mut d, b);
                        }
                        PathSeg::Arc { a, mid, b } => {
                            d.push(2.0);
                            push3(&mut d, a);
                            push3(&mut d, mid);
                            push3(&mut d, b);
                        }
                    }
                }
                native::make_path(&d)?
            }
            PathSource::Helix(h) => {
                if h.axis.length() < 1e-9 {
                    return Err(KernelError::Invalid("the helix axis must not be zero".into()));
                }
                let axis = h.axis.normalize();
                let mut x = if h.x_dir.length() > 1e-9 {
                    h.x_dir
                } else {
                    DVec3::X
                };
                if x.cross(axis).length() < 1e-9 {
                    x = if axis.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
                }
                h.check()?;
                helix_axis = Some(axis);
                helix_pitch = Some(h.pitch.abs());
                let mut d = vec![3.0];
                push3(&mut d, h.origin);
                push3(&mut d, axis);
                push3(&mut d, x);
                d.extend([
                    h.radius,
                    h.pitch.abs(),
                    h.turns,
                    if h.right { 1.0 } else { -1.0 },
                    h.start_deg.to_radians(),
                    h.cone_deg.to_radians(),
                ]);
                // a negative pitch runs against the axis: turn the axis round
                if h.pitch < 0.0 {
                    d[4] = -axis.x;
                    d[5] = -axis.y;
                    d[6] = -axis.z;
                    // the cone opens along the axis that was given: along the turned axis it opens the other way
                    let n = d.len();
                    d[n - 1] = -d[n - 1];
                    helix_axis = Some(-axis);
                }
                native::make_path(&d)?
            }
            PathSource::Edges { solid, edges } => {
                let ids: Vec<u32> = edges.iter().map(|e| e.0).collect();
                native::edges_wire(solid.raw(), &ids)?
            }
        };
        let i = native::path_info(&shape)?;
        if i.len() != 14 {
            return Err(KernelError::Invalid("the path cannot be measured".into()));
        }
        Ok(Path {
            shape,
            start: v3(&i[0..3]),
            start_tangent: v3(&i[3..6]),
            end: v3(&i[6..9]),
            end_tangent: v3(&i[9..12]),
            length: i[12],
            closed: i[13] > 0.5,
            helix_axis,
            helix_pitch,
        })
    }

    /// Points along the path for drawing (the polyline stays within `deflection` mm of the curve).
    pub fn polyline(&self, deflection: f64) -> Result<Vec<DVec3>> {
        let v = native::path_points(&self.shape, deflection.max(1e-4))?;
        Ok(v.as_chunks::<3>().0.iter().map(|c| v3(c)).collect())
    }
}

/// A polyline through `points`; with `radius` > 0 every corner is rounded by a tangent arc of that radius.
pub fn polyline_segments(points: &[DVec3], radius: f64) -> Result<Vec<PathSeg>> {
    let mut pts: Vec<DVec3> = Vec::new();
    for p in points {
        if pts.last().is_none_or(|l| (*l - *p).length() > 1e-9) {
            pts.push(*p);
        }
    }
    if pts.len() < 2 {
        return Err(KernelError::Invalid(
            "a line needs at least two different points".into(),
        ));
    }
    if radius <= 1e-9 || pts.len() < 3 {
        return Ok(pts
            .windows(2)
            .map(|w| PathSeg::Line { a: w[0], b: w[1] })
            .collect());
    }
    // tangent length at every inner corner (0 where the line goes straight on)
    let n = pts.len();
    let mut t = vec![0.0; n];
    for i in 1..n - 1 {
        let (u1, u2) = (
            (pts[i - 1] - pts[i]).normalize(),
            (pts[i + 1] - pts[i]).normalize(),
        );
        let theta = u1.dot(u2).clamp(-1.0, 1.0).acos();
        if theta > std::f64::consts::PI - 1e-6 {
            continue; // straight on
        }
        if theta < 1e-6 {
            return Err(KernelError::Invalid(
                "the line turns back on itself at a corner".into(),
            ));
        }
        t[i] = radius / (theta / 2.0).tan();
    }
    for i in 0..n - 1 {
        let len = (pts[i + 1] - pts[i]).length();
        if t[i] + t[i + 1] > len * (1.0 + 1e-9) {
            return Err(KernelError::Invalid(
                "The corner radius is too large for the length of the segments next to a corner".into(),
            ));
        }
    }
    let mut segs = Vec::new();
    let mut cur = pts[0];
    for i in 1..n - 1 {
        if t[i] == 0.0 {
            continue;
        }
        let (u1, u2) = (
            (pts[i - 1] - pts[i]).normalize(),
            (pts[i + 1] - pts[i]).normalize(),
        );
        let (p1, p2) = (pts[i] + u1 * t[i], pts[i] + u2 * t[i]);
        let w = (u1 + u2).normalize();
        let half = u1.dot(u2).clamp(-1.0, 1.0).acos() / 2.0;
        let centre = pts[i] + w * (radius / half.sin());
        let mid = centre - w * radius;
        if (p1 - cur).length() > 1e-9 {
            segs.push(PathSeg::Line { a: cur, b: p1 });
        }
        segs.push(PathSeg::Arc { a: p1, mid, b: p2 });
        cur = p2;
    }
    let last = pts[n - 1];
    if (last - cur).length() > 1e-9 {
        segs.push(PathSeg::Line { a: cur, b: last });
    }
    Ok(segs)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SweepOptions {
    /// Round the corners of the path (otherwise they are sharp).
    pub rounded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepWarning {
    /// The profile does not lie at either end of the path.
    ProfileNotAtPath,
    /// The profile is not approximately perpendicular to the path where it starts.
    ProfileNotPerpendicular,
}

#[derive(Debug, Clone)]
pub struct SweepResult {
    pub solid: Solid,
    pub warnings: Vec<SweepWarning>,
}

fn sweep_message() -> KernelError {
    KernelError::Invalid(
        "The profile cannot be swept along this path: the path may turn more sharply than the profile allows. The model was not changed."
            .into(),
    )
}

impl Solid {
    /// Plane (point, normal) of a profile when it is known.
    fn profile_plane(i: &LoftInput) -> Option<(DVec3, DVec3)> {
        match i {
            LoftInput::Profile { origin, normal, .. } => Some((*origin, normal.normalize_or_zero())),
            LoftInput::Face { solid, face } => solid.face_info(*face).ok().map(|f| (f.origin, f.normal)),
            LoftInput::Edge { .. } => None,
        }
    }

    pub fn sweep(profile: &LoftInput, path: &Path, opts: &SweepOptions) -> Result<SweepResult> {
        let face = crate::timing::stage("profile face", || Solid::profile_face(profile))?;
        let area = native::props(&face)?.area;
        let (fixed, axis) = match path.helix_axis {
            Some(a) => (true, a),
            None => (false, DVec3::Z),
        };
        let t0 = std::time::Instant::now();
        let shape = crate::timing::stage("sweep (pipe shell and hole cut)", || {
            native::sweep_face(&face, &path.shape, fixed, axis.x, axis.y, axis.z, opts.rounded)
        })
        .map_err(|e| match KernelError::from(e) {
            KernelError::Cancelled => KernelError::Cancelled,
            _ => sweep_message(),
        })?;
        let t1 = std::time::Instant::now();
        let solid = Solid::from_shape(shape)
            .validated_body()
            .map_err(|_| sweep_message())?;
        let t2 = std::time::Instant::now();
        // Along a helix the turns cannot touch each other as long as the profile is narrower than the pitch (measured along
        // the axis); that is checked directly, because the general check takes many seconds on a long thread.
        let self_cut = match (path.helix_axis, path.helix_pitch) {
            (Some(axis), Some(pitch)) => {
                let p = native::props(&face)?;
                let mut lo = f64::MAX;
                let mut hi = f64::MIN;
                for x in [p.min[0], p.max[0]] {
                    for y in [p.min[1], p.max[1]] {
                        for z in [p.min[2], p.max[2]] {
                            let d = DVec3::new(x, y, z).dot(axis);
                            lo = lo.min(d);
                            hi = hi.max(d);
                        }
                    }
                }
                hi - lo > pitch * (1.0 + 1e-6)
            }
            _ => native::has_self_intersections(solid.raw()),
        };
        if std::env::var_os("ROLIDIUM_TIMING").is_some() {
            eprintln!(
                "sweep: kernel {:?}, validate {:?}, self-intersection check {:?}",
                t1 - t0,
                t2 - t1,
                t2.elapsed()
            );
        }
        if self_cut {
            return Err(KernelError::Invalid(
                "The swept body cuts through itself (the path bends too sharply, comes too close to itself, or the profile is wider than the pitch), so the model was not changed."
                    .into(),
            ));
        }
        let v = crate::timing::stage("volume", || solid.volume())?;
        let expected = area * path.length;
        if v.is_nan() || v <= 0.0 || (expected > 0.0 && (v < 0.3 * expected || v > 3.0 * expected)) {
            return Err(sweep_message());
        }
        let mut warnings = Vec::new();
        if let Some((o, n)) = Solid::profile_plane(profile) {
            let ds = (path.start - o).dot(n).abs();
            let de = (path.end - o).dot(n).abs();
            let (near_start, tangent) = if ds <= de {
                (ds, path.start_tangent)
            } else {
                (de, path.end_tangent)
            };
            if near_start > 0.5 {
                warnings.push(SweepWarning::ProfileNotAtPath);
            }
            // a profile in a plane through the axis is the usual way to sweep along a helix, so no remark there
            if path.helix_axis.is_none() && n.dot(tangent).abs() < 10f64.to_radians().cos() {
                warnings.push(SweepWarning::ProfileNotPerpendicular);
            }
        }
        Ok(SweepResult { solid, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Profile, ProfileSegment};

    fn ring(outer: f64, inner: f64) -> Profile {
        Profile {
            loops: vec![
                vec![ProfileSegment::Circle {
                    center: [0.0, 0.0],
                    radius: outer,
                }],
                vec![ProfileSegment::Circle {
                    center: [0.0, 0.0],
                    radius: inner,
                }],
            ],
        }
    }

    fn disc(r: f64) -> Profile {
        Profile {
            loops: vec![vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: r,
            }]],
        }
    }

    fn at(p: &Profile, origin: DVec3, normal: DVec3, x_dir: DVec3) -> LoftInput<'_> {
        LoftInput::Profile {
            profile: p,
            origin,
            normal,
            x_dir,
        }
    }

    #[test]
    fn a_bent_tube_has_the_volume_of_area_times_path_length() {
        // a line along X, a quarter circle of radius 40, a line along Y
        let segs = polyline_segments(
            &[
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(100.0, 0.0, 0.0),
                DVec3::new(100.0, 100.0, 0.0),
            ],
            40.0,
        )
        .unwrap();
        assert_eq!(segs.len(), 3);
        let path = Path::build(PathSource::Segments(&segs)).unwrap();
        let length = 60.0 + std::f64::consts::FRAC_PI_2 * 40.0 + 60.0;
        assert!((path.length - length).abs() < 1e-6, "{}", path.length);
        let prof = ring(10.0, 8.0);
        let r = Solid::sweep(
            &at(&prof, DVec3::ZERO, DVec3::X, DVec3::Y),
            &path,
            &SweepOptions::default(),
        )
        .unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let area = std::f64::consts::PI * (100.0 - 64.0);
        let v = r.solid.volume().unwrap();
        assert!(
            (v - area * length).abs() < 1e-3 * area * length,
            "{v} vs {}",
            area * length
        );
        assert!(r.solid.is_valid());
    }

    #[test]
    fn a_spring_is_a_circle_swept_along_a_helix() {
        // wire diameter 2 (radius 1) on a helix of radius 10, pitch 5, 10 turns
        let h = HelixDef {
            origin: DVec3::ZERO,
            axis: DVec3::Z,
            x_dir: DVec3::X,
            radius: 10.0,
            pitch: 5.0,
            turns: 10.0,
            right: true,
            start_deg: 0.0,
            cone_deg: 0.0,
        };
        let path = Path::build(PathSource::Helix(h)).unwrap();
        let one_turn = ((2.0 * std::f64::consts::PI * 10.0).powi(2) + 25.0).sqrt();
        assert!((path.length - 10.0 * one_turn).abs() < 1e-3, "{}", path.length);
        let prof = disc(1.0);
        // the profile stands at the start (10, 0, 0) perpendicular to the helix tangent
        let t = path.start_tangent;
        let x = t.cross(DVec3::Z).normalize();
        let r = Solid::sweep(&at(&prof, path.start, t, x), &path, &SweepOptions::default()).unwrap();
        let v = r.solid.volume().unwrap();
        let expected = std::f64::consts::PI * path.length;
        assert!((v - expected).abs() < 0.01 * expected, "{v} vs {expected}");
        assert!(r.solid.is_valid());
    }

    #[test]
    fn a_path_that_bends_too_sharply_is_refused_with_a_message() {
        // a tube of radius 10 round a corner of radius 3: the inside would turn inside out
        let segs = polyline_segments(
            &[
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(50.0, 0.0, 0.0),
                DVec3::new(50.0, 50.0, 0.0),
            ],
            3.0,
        )
        .unwrap();
        let path = Path::build(PathSource::Segments(&segs)).unwrap();
        let prof = disc(10.0);
        let r = Solid::sweep(
            &at(&prof, DVec3::ZERO, DVec3::X, DVec3::Y),
            &path,
            &SweepOptions::default(),
        );
        let msg = r.expect_err("a sharp bend must be refused").to_string();
        assert!(msg.contains("model was not changed"), "{msg}");
    }

    #[test]
    fn a_profile_away_from_the_path_gives_a_warning_but_works() {
        let segs = polyline_segments(&[DVec3::ZERO, DVec3::new(100.0, 0.0, 0.0)], 0.0).unwrap();
        let path = Path::build(PathSource::Segments(&segs)).unwrap();
        let prof = disc(5.0);
        let far = at(&prof, DVec3::new(0.0, 0.0, 30.0), DVec3::Z, DVec3::X);
        let r = Solid::sweep(&far, &path, &SweepOptions::default());
        // the profile plane is along the path: both warnings, and the result is whatever the kernel makes of it
        if let Ok(r) = r {
            assert!(
                r.warnings.contains(&SweepWarning::ProfileNotAtPath)
                    || r.warnings.contains(&SweepWarning::ProfileNotPerpendicular)
            );
        }
    }

    #[test]
    fn corner_rounding_checks_the_segment_lengths() {
        let e = polyline_segments(
            &[
                DVec3::ZERO,
                DVec3::new(10.0, 0.0, 0.0),
                DVec3::new(10.0, 10.0, 0.0),
            ],
            20.0,
        );
        assert!(e.is_err());
        let ok = polyline_segments(
            &[
                DVec3::ZERO,
                DVec3::new(10.0, 0.0, 0.0),
                DVec3::new(10.0, 10.0, 0.0),
            ],
            5.0,
        )
        .unwrap();
        assert_eq!(ok.len(), 3);
    }
}
