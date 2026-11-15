// Copyright 2026 Jim Coleman / North Idaho Labs, LLC
// SPDX-License-Identifier: Apache-2.0

//! `grid_points` returns points in (col, row) order without sorting, and the
//! same points whatever the thread count. A 3 x 3 grid of tiles, written in
//! a scrambled order, with a narrow last tile column and empty cells.

mod common;
use common::*;
use trimble_tzf::{GridPoint, Tzf, COMPONENT_HORIZONTAL_ANGLE, COMPONENT_RANGE, COMPONENT_VERTICAL_ANGLE};

const TILE: u32 = 2;
const ROWS: u32 = 6;
const COLS: u32 = 5; // the last tile column is one line wide

/// The angles (µrad) and range (mm) a cell holds. Empty cells have range 0.
fn cell(col: u32, row: u32) -> (i64, i64, u32) {
    let va = 1_000_000 + 50_000 * row as i64 + 7 * col as i64;
    let ha = 200_000 * col as i64 + 3 * row as i64;
    let range = if (col + 2 * row) % 5 == 0 { 0 } else { 1000 + 10 * col + row };
    (va, ha, range)
}

/// One tile's three planes: per line, the first cell holds the absolute
/// angle and the rest hold deltas, as the files store them.
fn tile(tr: u32, tc: u32) -> Vec<Plane> {
    let w = TILE.min(COLS - tc * TILE);
    let (mut r, mut v, mut h) = (vec![], vec![], vec![]);
    for line in 0..w {
        let col = tc * TILE + line;
        let (mut pv, mut ph) = (0i64, 0i64);
        for k in 0..TILE {
            let (va, ha, range) = cell(col, tr * TILE + k);
            r.push(range);
            v.push((va - pv) as i32 as u32);
            h.push((ha - ph) as i32 as u32);
            (pv, ph) = (va, ha);
        }
    }
    vec![
        (COMPONENT_RANGE, (tr, tc), w, TILE, 1.0, cells(&r)),
        (COMPONENT_VERTICAL_ANGLE, (tr, tc), w, TILE, 1.0, cells(&v)),
        (COMPONENT_HORIZONTAL_ANGLE, (tr, tc), w, TILE, 1.0, cells(&h)),
    ]
}

fn scan() -> Tzf {
    let order = [(2, 1), (0, 0), (1, 2), (2, 0), (0, 2), (1, 0), (2, 2), (0, 1), (1, 1)];
    let planes: Vec<Plane> = order.iter().flat_map(|&(tr, tc)| tile(tr, tc)).collect();
    let (mut buf, m) = plane_file(&planes, (ROWS, COLS, TILE, ROWS * COLS));
    set_identity_tilt(&mut buf, m);
    Tzf::from_bytes(buf).unwrap()
}

fn key(g: &GridPoint) -> (u32, u32, [u32; 3]) {
    (g.col, g.row, [g.point.x.to_bits(), g.point.y.to_bits(), g.point.z.to_bits()])
}

#[test]
fn points_come_in_col_row_order_with_the_right_values() {
    let got = scan().grid_points().unwrap();
    assert!(got.windows(2).all(|w| (w[0].col, w[0].row) < (w[1].col, w[1].row)), "not in (col, row) order");
    let mut want = vec![];
    for col in 0..COLS {
        for row in 0..ROWS {
            let (va, ha, range) = cell(col, row);
            if range == 0 {
                continue;
            }
            let (va, ha, r) = (va as f64 * 1e-6, ha as f64 * 1e-6, range as f64 / 1000.0);
            want.push((col, row, [r * va.sin() * ha.cos(), r * va.sin() * ha.sin(), r * va.cos()]));
        }
    }
    assert_eq!(got.len(), want.len());
    for (g, (col, row, p)) in got.iter().zip(&want) {
        assert_eq!((g.col, g.row), (*col, *row));
        for (a, b) in [g.point.x, g.point.y, g.point.z].iter().zip(p) {
            assert!((*a as f64 - b).abs() < 1e-6, "cell ({col},{row}): {a} vs {b}");
        }
    }
}

#[test]
fn the_thread_count_does_not_change_the_points() {
    let one: Vec<_> = scan().with_threads(1).grid_points().unwrap().iter().map(key).collect();
    for n in [0, 2, 3, 8] {
        let many: Vec<_> = scan().with_threads(n).grid_points().unwrap().iter().map(key).collect();
        assert_eq!(one, many, "{n} threads");
    }
}
