// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A compact form of a mesh for the .rolidium file: the coordinates as four byte planes, the indices as differences in four byte planes, both deflated.
//! (A mesh from a 3D printer is mostly neighbouring numbers, so the planes pack well: the file is a good deal smaller than the binary STL.)

use crate::TriMesh;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use std::io::{Read, Write};

const MAGIC: &[u8; 5] = b"RMSH1";

fn planes(values: &[u32]) -> Vec<u8> {
    let n = values.len();
    let mut out = vec![0u8; n * 4];
    for (i, v) in values.iter().enumerate() {
        let b = v.to_le_bytes();
        for k in 0..4 {
            out[k * n + i] = b[k];
        }
    }
    out
}

fn unplanes(bytes: &[u8], n: usize) -> Vec<u32> {
    (0..n)
        .map(|i| u32::from_le_bytes([bytes[i], bytes[n + i], bytes[2 * n + i], bytes[3 * n + i]]))
        .collect()
}

pub fn encode(m: &TriMesh) -> Vec<u8> {
    let coords: Vec<u32> = m.positions.iter().flatten().map(|c| c.to_bits()).collect();
    let mut prev = 0u32;
    let deltas: Vec<u32> = m
        .triangles
        .iter()
        .flatten()
        .map(|i| {
            let d = (*i as i64 - prev as i64) as i32;
            prev = *i;
            // zigzag
            ((d << 1) ^ (d >> 31)) as u32
        })
        .collect();
    let mut raw = Vec::new();
    raw.extend_from_slice(&(m.positions.len() as u32).to_le_bytes());
    raw.extend_from_slice(&(m.triangles.len() as u32).to_le_bytes());
    raw.extend_from_slice(&planes(&coords));
    raw.extend_from_slice(&planes(&deltas));
    let mut z = ZlibEncoder::new(Vec::new(), Compression::new(6));
    z.write_all(&raw).unwrap();
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&z.finish().unwrap());
    out
}

pub fn decode(bytes: &[u8]) -> Result<TriMesh, String> {
    if !bytes.starts_with(MAGIC) {
        return Err("not a mesh of Rolidium".into());
    }
    let mut raw = Vec::new();
    ZlibDecoder::new(&bytes[MAGIC.len()..])
        .read_to_end(&mut raw)
        .map_err(|e| format!("the mesh is damaged: {e}"))?;
    if raw.len() < 8 {
        return Err("the mesh is damaged".into());
    }
    let nv = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize;
    let nt = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
    let (nc, ni) = (nv * 3, nt * 3);
    if raw.len() != 8 + nc * 4 + ni * 4 {
        return Err("the mesh is damaged (wrong length)".into());
    }
    let coords = unplanes(&raw[8..8 + nc * 4], nc);
    let deltas = unplanes(&raw[8 + nc * 4..], ni);
    let positions = coords
        .chunks_exact(3)
        .map(|c| [f32::from_bits(c[0]), f32::from_bits(c[1]), f32::from_bits(c[2])])
        .collect();
    let mut prev = 0i64;
    let mut idx = Vec::with_capacity(ni);
    for z in deltas {
        let d = ((z >> 1) as i32) ^ -((z & 1) as i32);
        prev += d as i64;
        if prev < 0 || prev as usize >= nv {
            return Err("the mesh is damaged (index out of range)".into());
        }
        idx.push(prev as u32);
    }
    let triangles = idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
    Ok(TriMesh { positions, triangles })
}
