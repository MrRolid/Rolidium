// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A mesh made a solid, with round surfaces: the flat and round faces are recognised (`rolidium_mesh::fit`) and handed to the kernel as faces on planes,
//! cylinders, cones, spheres and tori with shared edges that are lines, circles and ellipses. When the kernel does not accept that (a face of an odd shape),
//! fewer kinds of surfaces are tried, and at the last the flat conversion of D63, so the person always gets a solid when one can be made at all.

use rolidium_kernel::{DVec3, EdgeSpec, FaceSpec, KernelError, Solid, SurfaceSpec};
use rolidium_mesh::fit::{self, Allow, FitOptions, FitStats};
use rolidium_mesh::TriMesh;

use crate::progress;

/// What the conversion did.
#[derive(Debug, Clone, Default)]
pub struct MadeSolid {
    pub stats: FitStats,
    /// Single-triangle faces and the share of the area they cover (as for the flat conversion).
    pub single: usize,
    pub single_area: f64,
    pub max_deviation: f64,
    pub rounded: usize,
    /// Which way the solid was made: 0 round surfaces of all kinds, 1 cylinders only, 2 flat faces only.
    pub route: u8,
    /// Why the better ways were not taken (empty when the first one worked).
    pub notes: Vec<String>,
}

pub fn spec_of(f: &fit::Fitted) -> SurfaceSpec {
    SurfaceSpec {
        points: f.points.iter().map(|p| DVec3::new(p[0], p[1], p[2])).collect(),
        point_tol: f.point_tol.clone(),
        surfaces: f.surfaces.clone(),
        edges: f
            .edges
            .iter()
            .map(|e| EdgeSpec {
                kind: e.kind,
                v: e.v,
                params: e.params,
                tol: e.tol,
            })
            .collect(),
        faces: f
            .faces
            .iter()
            .map(|x| FaceSpec {
                surface: x.surface,
                loops: x.loops.clone(),
                area: x.area,
            })
            .collect(),
    }
}

/// The sentence for a free-form model (also translated).
pub const ORGANIC: &str = "Organic model, it stays a mesh";

/// Makes the solid; `mesh` must be closed and consistent (the caller has checked and mended it).
pub fn solid_from_mesh(
    mesh: &TriMesh,
    tolerance: f64,
    round: bool,
    stop: &dyn Fn() -> bool,
) -> Result<(Solid, MadeSolid), KernelError> {
    let mut notes = Vec::new();
    let volume = mesh.signed_volume();
    let debug = std::env::var_os("ROLIDIUM_DEBUG_CONVERT").is_some();
    let t0 = std::time::Instant::now();
    let budget = 30.0 + mesh.triangles.len() as f64 / 4000.0;
    progress::set("Looking for flat and round faces", 0.05);
    let rungs: [(u8, Allow); 2] = [(0, Allow::ALL), (1, Allow::CYLINDERS)];
    for (route, allow) in rungs {
        if stop() {
            return Err(KernelError::Cancelled);
        }
        let opts = FitOptions {
            tolerance,
            round,
            allow,
        };
        progress::set(
            if route == 0 {
                "Looking for flat and round faces"
            } else {
                "Looking for flat faces and cylinders only"
            },
            0.05 + 0.2 * route as f32,
        );
        let Some(fitted) = fit::fit(mesh, &opts, stop) else {
            return Err(KernelError::Cancelled);
        };
        progress::set(
            if route == 0 {
                "Making the solid from the faces found"
            } else {
                "Making the solid from flat faces and cylinders"
            },
            0.15 + 0.2 * route as f32,
        );
        if debug {
            eprintln!(
                "convert: [{:.1} s] route {route} fit: {:?}",
                t0.elapsed().as_secs_f64(),
                fitted.stats
            );
        }
        // a free shape (a cat, a figure): nothing to make faces of; the flat conversion would take minutes and give a million faces
        if fitted.stats.organic {
            return Err(KernelError::Invalid(ORGANIC.into()));
        }
        // nothing round was found: the flat conversion below is the same and has more ways to mend a face
        if fitted.stats.cylinders + fitted.stats.cones + fitted.stats.spheres + fitted.stats.tori == 0 {
            break;
        }
        let spec = spec_of(&fitted);
        match Solid::from_surfaces(&spec, 1e-5) {
            Ok(s) => {
                let v = s.volume().unwrap_or(0.0);
                // the volume of the solid may differ from that of the mesh by what the facets of a coarse mesh lose against the true round surfaces (more where more of the
                // surface is round), not by more: a solid that is much bigger or smaller has a face in the wrong place
                let limit = 0.003 + 0.05 * fitted.stats.round_area;
                if (v - volume).abs() <= limit * volume.abs() + 1e-9 {
                    let st = fitted.stats.clone();
                    return Ok((
                        s,
                        MadeSolid {
                            single: st.singles,
                            single_area: st.single_area,
                            max_deviation: st.max_deviation,
                            rounded: st.rounded,
                            stats: st,
                            route,
                            notes,
                        },
                    ));
                }
                notes.push(format!(
                    "route {route}: the volume {v:.3} is not that of the mesh {volume:.3}"
                ));
            }
            Err(KernelError::Cancelled) => return Err(KernelError::Cancelled),
            Err(e) => notes.push(format!("route {route}: {e}")),
        }
        if debug {
            eprintln!(
                "convert: [{:.1} s] {}",
                t0.elapsed().as_secs_f64(),
                notes.last().cloned().unwrap_or_default()
            );
        }
    }
    // flat faces only: groups of any size are faces; when the solid cannot be made so (a group that is not flat enough, a pinched outline, a part with details as
    // small as the accuracy) the small groups are taken apart, then the accuracy is made finer, and at the last every triangle is a face: a triangle is always flat
    let mut failure = String::new();
    let n = mesh.triangles.len();
    let finer = (tolerance / 5.0).max(2e-4);
    let mut attempts: Vec<(f64, usize)> = vec![(tolerance, 2), (tolerance, 4)];
    if finer < tolerance * 0.7 {
        attempts.extend([(finer, 2), (finer, 4)]);
    }
    if n <= 150_000 {
        attempts.push((finer.min(tolerance), usize::MAX));
    }
    let total = attempts.len();
    for (k, (tol, merge_from)) in attempts.into_iter().enumerate() {
        if stop() {
            return Err(KernelError::Cancelled);
        }
        // a conversion that does not come out is given up after a time that grows with the size of the mesh, with the reason, not after minutes of nothing
        if k > 0 && t0.elapsed().as_secs_f64() > budget {
            failure = format!(
                "no way worked within {:.0} s; the last one said: {failure}",
                budget
            );
            break;
        }
        progress::set(
            if merge_from == usize::MAX {
                "Making a face of every triangle"
            } else if tol < tolerance {
                "Making flat faces at a finer accuracy"
            } else {
                "Making flat faces"
            },
            0.5 + 0.5 * k as f32 / total as f32,
        );
        let pf = rolidium_mesh::convert::polyfaces(mesh, tol, round, merge_from, stop)
            .ok_or(KernelError::Cancelled)?;
        if debug {
            eprintln!(
                "convert: [{:.1} s] the flat faces are found ({} faces)",
                t0.elapsed().as_secs_f64(),
                pf.faces.len()
            );
        }
        let points: Vec<DVec3> = pf.points.iter().map(|p| DVec3::new(p[0], p[1], p[2])).collect();
        match Solid::from_polygons(&points, &pf.faces, &pf.point_tolerance, &pf.fallback, 1e-5) {
            Ok(s) => {
                let stats = FitStats {
                    planes: pf.single + pf.flat,
                    singles: pf.single,
                    single_area: pf.single_area,
                    max_deviation: pf.max_deviation,
                    rounded: pf.rounded,
                    ..Default::default()
                };
                if (tol - tolerance).abs() > 1e-12 {
                    notes.push(format!(
                        "the solid was made with an accuracy of {tol:.4} mm instead of {tolerance:.4} mm: the part has details as small as that"
                    ));
                }
                return Ok((
                    s,
                    MadeSolid {
                        single: pf.single,
                        single_area: pf.single_area,
                        max_deviation: pf.max_deviation,
                        rounded: pf.rounded,
                        stats,
                        route: 2,
                        notes,
                    },
                ));
            }
            Err(KernelError::Cancelled) => return Err(KernelError::Cancelled),
            Err(e) => {
                if debug {
                    eprintln!(
                        "convert: [{:.1} s] flat faces at {tol} from {merge_from} failed: {e} ({} faces)",
                        t0.elapsed().as_secs_f64(),
                        pf.faces.len()
                    );
                }
                failure = e.to_string()
            }
        }
    }
    Err(KernelError::Invalid(format!(
        "The mesh could not be made a solid ({failure}). It stays a mesh; repair it (holes, turned triangles) or try a larger accuracy"
    )))
}
