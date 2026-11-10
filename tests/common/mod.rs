//! Synthetic `.tzf` builder for the integration tests.
//!
//! Copied from the unit tests in `src/lib.rs` (`push_block`, `plane_file`,
//! `two_line_planes`), which are private to the library. Keep the two in step
//! if the observed layout changes. Uses only the public constants, so the
//! library's public API is unchanged.

#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use trimble_tzf::{
    COMPONENT_HORIZONTAL_ANGLE, COMPONENT_RANGE, COMPONENT_VERTICAL_ANGLE, DESCRIPTOR_LEN,
    FIRST_DESCRIPTOR, MAGIC,
};

/// One plane block: component, tile, width, height, descriptor scale,
/// decoded bytes.
pub type Plane = (u32, (u32, u32), u32, u32, f32, Vec<u8>);

pub fn deflate_raw(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// Append one block: 60-byte descriptor + payload.
pub fn push_block(
    buf: &mut Vec<u8>,
    component: u32,
    tile: (u32, u32),
    w: u32,
    h: u32,
    raw: u32,
    payload: &[u8],
) {
    let comp = (payload.len() + 28) as u32;
    buf.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes()); // id
    for v in [200u32, 16, comp, component, tile.0, tile.1, 100, w, h, raw] {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    buf.extend_from_slice(&0u64.to_le_bytes()); // hash
    buf.extend_from_slice(payload);
}

/// A DEFLATE payload as the files store it: `[u32 raw size][0x78 0x01][raw deflate]`.
pub fn deflate_payload(raw: &[u8]) -> Vec<u8> {
    let mut p = (raw.len() as u32).to_le_bytes().to_vec();
    p.extend_from_slice(&[0x78, 0x01]);
    p.extend_from_slice(&deflate_raw(raw));
    p
}

pub fn cells(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|c| c.to_le_bytes()).collect()
}

/// A file header with no blocks yet.
pub fn header() -> Vec<u8> {
    let mut buf = vec![0u8; FIRST_DESCRIPTOR];
    buf[..8].copy_from_slice(&MAGIC);
    buf
}

/// Append the DEFLATE plane blocks to `buf`.
pub fn push_planes(buf: &mut Vec<u8>, planes: &[Plane]) {
    for (comp, tile, w, h, scale, raw) in planes {
        let p = deflate_payload(raw);
        push_block(buf, *comp, *tile, *w, *h, raw.len() as u32, &p);
        let at = buf.len() - p.len() - DESCRIPTOR_LEN;
        buf[at + 48..at + 52].copy_from_slice(&scale.to_le_bytes());
    }
}

/// Close the data region at the current end of `buf` and append a metadata
/// block with the given grid (rows, cols, tile size, point count) and an
/// all-zero tilt. Returns the metadata offset.
pub fn finish(buf: &mut Vec<u8>, grid: (u32, u32, u32, u32)) -> usize {
    let m = buf.len();
    buf[0x10..0x18].copy_from_slice(&(m as u64).to_le_bytes());
    buf.extend_from_slice(&[0u8; 0x44 + 72]);
    let (rows, cols, tile, n) = grid;
    for (off, v) in [(0x30, rows), (0x34, cols), (0x38, tile), (0x3c, n)] {
        buf[m + off..m + off + 4].copy_from_slice(&v.to_le_bytes());
    }
    m
}

/// Build a file from DEFLATE plane blocks plus a metadata block with the
/// given grid (rows, cols, tile size, point count) and an all-zero tilt.
/// Returns the file and the metadata offset.
pub fn plane_file(planes: &[Plane], grid: (u32, u32, u32, u32)) -> (Vec<u8>, usize) {
    let mut buf = header();
    push_planes(&mut buf, planes);
    let m = finish(&mut buf, grid);
    (buf, m)
}

/// Store the identity as the levelling rotation, so points are levelled.
pub fn set_identity_tilt(buf: &mut [u8], m: usize) {
    for k in 0..9 {
        let v = if k % 4 == 0 { 1.0f64 } else { 0.0 };
        let o = m + 0x44 + 8 * k;
        buf[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }
}

/// One 2x2 tile: two vertical lines at HA 0 and 90°, both rows at VA 90°
/// (horizontal), range 1 m and 2 m. Decodes (identity tilt) to, in
/// (col, row) order: (1,0,0), (2,0,0), (0,1,0), (0,2,0).
pub fn two_line_planes() -> Vec<Plane> {
    let q = 1_570_796u32; // π/2 in µrad
    vec![
        (
            COMPONENT_RANGE,
            (0, 0),
            2,
            2,
            10.0,
            cells(&[10_000, 20_000, 10_000, 20_000]),
        ),
        (
            COMPONENT_VERTICAL_ANGLE,
            (0, 0),
            2,
            2,
            1.0,
            cells(&[q, 0, q, 0]),
        ),
        (
            COMPONENT_HORIZONTAL_ANGLE,
            (0, 0),
            2,
            2,
            1.0,
            cells(&[0, 0, q, 0]),
        ),
    ]
}

/// A minimal JPEG-shaped payload (SOI ... EOI).
pub const JPEG: [u8; 12] = [
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0xff, 0xd9,
];

/// A raw Snappy payload for 4 bytes: length 4, one 4-byte literal.
pub const SNAPPY: [u8; 6] = [4, (4 - 1) << 2, b'a', b'b', b'c', b'd'];

/// The two-line scan (identity tilt, 2x2 grid, 4 points) plus a JPEG
/// intensity block (component 4) and a Snappy block (component 5), so every
/// block kind that decodes cleanly is present.
pub fn good_scan() -> Vec<u8> {
    let mut buf = header();
    push_planes(&mut buf, &two_line_planes());
    push_block(&mut buf, 4, (0, 0), 2, 2, 4, &JPEG);
    push_block(&mut buf, 5, (0, 0), 2, 2, 4, &SNAPPY);
    let m = finish(&mut buf, (2, 2, 2, 4));
    set_identity_tilt(&mut buf, m);
    buf
}

/// A fresh path under Cargo's per-target temp directory.
pub fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bins");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    let _ = std::fs::remove_file(&p);
    p
}

/// Write `bytes` to a fresh temp file and return its path.
pub fn write_tmp(name: &str, bytes: &[u8]) -> PathBuf {
    let p = tmp(name);
    std::fs::write(&p, bytes).unwrap();
    p
}
