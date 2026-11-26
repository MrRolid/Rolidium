// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Boolean operations on closed triangle meshes with the Manifold library (Apache-2.0; its source is in `third_party/manifold` and is compiled into the
//! program, no shared library is needed). The meshes are indexed triangles with positions in single precision, as everywhere in Rolidium; the operation is
//! made in double precision.
//!
//! The only `unsafe` code of the program that is not in a graphics or system call is here: the calls of the C interface of `src/shim.cpp`.

use std::ffi::{c_char, c_int, CStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Everything that is in one of the two.
    Union,
    /// What is in the first and not in the second.
    Difference,
    /// What is in both.
    Intersection,
}

/// An indexed triangle mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

extern "C" {
    fn rm_boolean(
        pa: *const f64,
        na: usize,
        ta: *const u32,
        nta: usize,
        pb: *const f64,
        nb: usize,
        tb: *const u32,
        ntb: usize,
        op: c_int,
        out_pos: *mut *mut f64,
        out_np: *mut usize,
        out_tri: *mut *mut u32,
        out_nt: *mut usize,
        err: *mut *const c_char,
    ) -> c_int;
    fn rm_volume(
        pa: *const f64,
        na: usize,
        ta: *const u32,
        nta: usize,
        volume: *mut f64,
        err: *mut *const c_char,
    ) -> c_int;
    fn rm_free(p: *mut std::ffi::c_void);
}

fn doubles(m: &Mesh) -> Vec<f64> {
    m.positions.iter().flat_map(|p| p.map(|c| c as f64)).collect()
}

fn message(p: *const c_char) -> String {
    if p.is_null() {
        "the boolean operation failed".into()
    } else {
        // SAFETY: the pointer is a static string of the C interface
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// `a` combined with `b`. Both must be closed, consistently oriented surfaces (the checks of `rolidium_mesh::check` tell); a mesh that is not is refused with a
/// sentence.
pub fn boolean(a: &Mesh, b: &Mesh, op: Op) -> Result<Mesh, String> {
    if a.triangles.is_empty() || b.triangles.is_empty() {
        return Err("one of the meshes is empty".into());
    }
    let (pa, pb) = (doubles(a), doubles(b));
    let mut out_pos: *mut f64 = std::ptr::null_mut();
    let mut out_tri: *mut u32 = std::ptr::null_mut();
    let (mut np, mut nt) = (0usize, 0usize);
    let mut err: *const c_char = std::ptr::null();
    let code = match op {
        Op::Union => 0,
        Op::Difference => 1,
        Op::Intersection => 2,
    };
    // SAFETY: the pointers and lengths describe the vectors above, which live through the call; the arrays that come back are copied and freed
    let rc = unsafe {
        rm_boolean(
            pa.as_ptr(),
            a.positions.len(),
            a.triangles.as_ptr().cast(),
            a.triangles.len(),
            pb.as_ptr(),
            b.positions.len(),
            b.triangles.as_ptr().cast(),
            b.triangles.len(),
            code,
            &mut out_pos,
            &mut np,
            &mut out_tri,
            &mut nt,
            &mut err,
        )
    };
    if rc != 0 {
        return Err(message(err));
    }
    // SAFETY: on success the arrays hold 3 * np doubles and 3 * nt indices
    let result = unsafe {
        let pos = std::slice::from_raw_parts(out_pos, 3 * np);
        let tri = std::slice::from_raw_parts(out_tri, 3 * nt);
        let r = Mesh {
            positions: pos
                .as_chunks::<3>()
                .0
                .iter()
                .map(|c| [c[0] as f32, c[1] as f32, c[2] as f32])
                .collect(),
            triangles: tri.as_chunks::<3>().0.to_vec(),
        };
        rm_free(out_pos.cast());
        rm_free(out_tri.cast());
        r
    };
    Ok(result)
}

/// The volume of a closed mesh as the library sees it; an error when it is not a closed surface.
pub fn volume(a: &Mesh) -> Result<f64, String> {
    let pa = doubles(a);
    let mut v = 0.0;
    let mut err: *const c_char = std::ptr::null();
    // SAFETY: the pointers and lengths describe the vectors, which live through the call
    let rc = unsafe {
        rm_volume(
            pa.as_ptr(),
            a.positions.len(),
            a.triangles.as_ptr().cast(),
            a.triangles.len(),
            &mut v,
            &mut err,
        )
    };
    if rc != 0 {
        return Err(message(err));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(min: f32, max: f32) -> Mesh {
        let p = |x: f32, y: f32, z: f32| [x, y, z];
        let positions = vec![
            p(min, min, min),
            p(max, min, min),
            p(max, max, min),
            p(min, max, min),
            p(min, min, max),
            p(max, min, max),
            p(max, max, max),
            p(min, max, max),
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh { positions, triangles }
    }

    #[test]
    fn union_difference_and_intersection_of_two_cubes() {
        let a = cube(0.0, 10.0);
        let b = cube(5.0, 15.0);
        assert!((volume(&a).unwrap() - 1000.0).abs() < 1e-6);
        let u = boolean(&a, &b, Op::Union).unwrap();
        assert!((volume(&u).unwrap() - 1875.0).abs() < 1e-3);
        let d = boolean(&a, &b, Op::Difference).unwrap();
        assert!((volume(&d).unwrap() - 875.0).abs() < 1e-3);
        let i = boolean(&a, &b, Op::Intersection).unwrap();
        assert!((volume(&i).unwrap() - 125.0).abs() < 1e-3);
    }

    #[test]
    fn an_open_mesh_is_refused_with_a_sentence() {
        let mut a = cube(0.0, 10.0);
        a.triangles.pop();
        let e = boolean(&a, &cube(1.0, 2.0), Op::Union).unwrap_err();
        assert!(e.contains("closed"), "{e}");
    }
}
