// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! STL, binary and ASCII: reading (the vertices are welded), writing, and the units (STL has none).

use crate::TriMesh;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StlError {
    #[error("The file is not an STL file (no triangles were found)")]
    NoTriangles,
    #[error("The binary STL file is cut short: it says {0} triangles but is too short")]
    Truncated(usize),
    #[error("The file contains numbers that are not finite")]
    NotFinite,
}

/// What a number of the file means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Units {
    Millimetres,
    Inches,
    /// One unit of the file is this many millimetres.
    Scale(f64),
}

impl Units {
    pub fn factor(self) -> f64 {
        match self {
            Units::Millimetres => 1.0,
            Units::Inches => 25.4,
            Units::Scale(s) => s,
        }
    }
}

fn weld(raw: &[[f32; 3]]) -> TriMesh {
    // three raw vertices per triangle; equal ones (bit for bit, -0 as 0) are one vertex
    let mut map: HashMap<[u32; 3], u32> = HashMap::with_capacity(raw.len() / 5 + 16);
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(raw.len() / 5 + 16);
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(raw.len() / 3);
    let mut id = |p: [f32; 3]| -> u32 {
        let key = [
            (p[0] + 0.0).to_bits(),
            (p[1] + 0.0).to_bits(),
            (p[2] + 0.0).to_bits(),
        ];
        *map.entry(key).or_insert_with(|| {
            positions.push(p);
            (positions.len() - 1) as u32
        })
    };
    for t in raw.as_chunks::<3>().0.iter() {
        triangles.push([id(t[0]), id(t[1]), id(t[2])]);
    }
    TriMesh { positions, triangles }
}

/// Reads a binary or an ASCII STL file.
pub fn parse(bytes: &[u8]) -> Result<TriMesh, StlError> {
    let mut raw: Vec<[f32; 3]> = Vec::new();
    let binary = bytes.len() >= 84 && {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        bytes.len() == 84 + 50 * n
    };
    if binary {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        raw.reserve(n * 3);
        for t in 0..n {
            let at = 84 + 50 * t + 12;
            for v in 0..3 {
                let f = |k: usize| {
                    let o = at + v * 12 + k * 4;
                    f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap())
                };
                raw.push([f(0), f(1), f(2)]);
            }
        }
    } else {
        // a binary file that is cut short would not be read as text
        if bytes.len() >= 84 && !bytes[..80].starts_with(b"solid") {
            let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
            if n > 0 && bytes.len() < 84 + 50 * n {
                return Err(StlError::Truncated(n));
            }
        }
        let text = String::from_utf8_lossy(bytes);
        for l in text.lines() {
            let mut w = l.split_whitespace();
            if w.next() == Some("vertex") {
                let p: Vec<f32> = w.filter_map(|x| x.parse().ok()).collect();
                if let [x, y, z] = p[..] {
                    raw.push([x, y, z]);
                }
            }
        }
        raw.truncate(raw.len() / 3 * 3);
    }
    if raw.is_empty() {
        return Err(StlError::NoTriangles);
    }
    if raw.iter().any(|p| p.iter().any(|c| !c.is_finite())) {
        return Err(StlError::NotFinite);
    }
    Ok(weld(&raw))
}

/// Reads the file and scales it to millimetres.
pub fn parse_units(bytes: &[u8], units: Units) -> Result<TriMesh, StlError> {
    let mut m = parse(bytes)?;
    let k = units.factor();
    if (k - 1.0).abs() > 1e-12 {
        for p in &mut m.positions {
            for c in p.iter_mut() {
                *c = (*c as f64 * k) as f32;
            }
        }
    }
    Ok(m)
}

/// Binary STL (the normals are worked out from the triangles).
pub fn write_binary(m: &TriMesh) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + 50 * m.triangles.len());
    let mut header = [0u8; 80];
    let text = b"Rolidium mesh, millimetres";
    header[..text.len()].copy_from_slice(text);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(m.triangles.len() as u32).to_le_bytes());
    for t in 0..m.triangles.len() {
        let (n, _) = m.normal_area(t);
        for c in n {
            out.extend_from_slice(&(c as f32).to_le_bytes());
        }
        for i in m.triangles[t] {
            for c in m.positions[i as usize] {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

pub fn write_ascii(m: &TriMesh, name: &str) -> String {
    let mut s = format!("solid {name}\n");
    for t in 0..m.triangles.len() {
        let (n, _) = m.normal_area(t);
        s.push_str(&format!(
            "facet normal {:e} {:e} {:e}\n outer loop\n",
            n[0], n[1], n[2]
        ));
        for i in m.triangles[t] {
            let p = m.positions[i as usize];
            s.push_str(&format!("  vertex {:e} {:e} {:e}\n", p[0], p[1], p[2]));
        }
        s.push_str(" endloop\nendfacet\n");
    }
    s.push_str(&format!("endsolid {name}\n"));
    s
}
