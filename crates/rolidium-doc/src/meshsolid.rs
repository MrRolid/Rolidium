// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A mesh made a solid, with round surfaces: the flat and round faces are recognised (`rolidium_mesh::fit`) and handed to the kernel as faces on planes,
//! cylinders, cones, spheres and tori with shared edges that are lines, circles and ellipses. When the kernel does not accept that (a face of an odd shape),
//! fewer kinds of surfaces are tried, and at the last the flat conversion of D63, so the person always gets a solid when one can be made at all.

use rolidium_kernel::{DVec3, EdgeSpec, FaceSpec, KernelError, Solid, SurfaceSpec};
use rolidium_mesh::fit::{self, Allow, FitOptions, FitStats};
use rolidium_mesh::TriMesh;

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
        let Some(fitted) = fit::fit(mesh, &opts, stop) else {
            return Err(KernelError::Cancelled);
        };
        if debug {
            eprintln!("convert: route {route} fit: {:?}", fitted.stats);
        }
        // nothing round was found: the flat conversion below is the same and has more ways to mend a face
        if fitted.stats.cylinders + fitted.stats.cones + fitted.stats.spheres + fitted.stats.tori == 0 {
            break;
        }
        let spec = spec_of(&fitted);
        match Solid::from_surfaces(&spec, 1e-5) {
            Ok(s) => {
                let v = s.volume().unwrap_or(0.0);
                if (v - volume).abs() <= 0.02 * volume.abs() + 1e-9 {
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
                notes.push(format!("route {route}: the volume {v:.3} is not that of the mesh {volume:.3}"));
            }
            Err(KernelError::Cancelled) => return Err(KernelError::Cancelled),
            Err(e) => notes.push(format!("route {route}: {e}")),
        }
        if debug {
            eprintln!("convert: {}", notes.last().cloned().unwrap_or_default());
        }
    }
    // flat faces only: groups of any size are faces; when the solid cannot be made so (a group that is not flat enough, a pinched outline) the small groups
    // are taken apart, and at the last every triangle is a face: a triangle is always flat
    let mut failure = String::new();
    let attempts: &[usize] = if mesh.triangles.len() <= 20_000 { &[2, 4, usize::MAX] } else { &[2, 4] };
    for &merge_from in attempts {
        let pf = rolidium_mesh::convert::polyfaces(mesh, tolerance, round, merge_from, stop).ok_or(KernelError::Cancelled)?;
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
                    eprintln!("convert: merge from {merge_from} failed: {e} ({} faces)", pf.faces.len());
                }
                failure = e.to_string()
            }
        }
    }
    Err(KernelError::Invalid(format!(
        "The mesh could not be made a solid ({failure}). It stays a mesh; repair it (holes, turned triangles) or try a larger accuracy"
    )))
}
