// Copyright (C) 2026 North Idaho Labs, LLC
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Decodes a real scan named by `TZF_FIXTURE` (scan files are not committed).
//! Checks the point count against the file metadata and that the points form
//! a plausible scene. Ignored by default, so a run without a scan reports it
//! as ignored instead of passing it:
//!
//!     TZF_FIXTURE=/path/to/scan.tzf cargo test --release -- --ignored
#[test]
#[ignore = "needs a real scan: set TZF_FIXTURE and run with --ignored"]
fn decodes_fixture() {
    let path = std::env::var("TZF_FIXTURE").expect("set TZF_FIXTURE to the path of a .tzf file");
    let tzf = trimble_tzf::Tzf::open(std::path::Path::new(&path)).unwrap();
    let meta = tzf.scan_meta().unwrap();
    assert!(meta.levelling.is_applied(), "levelling: {:?}", meta.levelling);
    let pts = tzf.points().unwrap();
    assert_eq!(pts.len() as u64, meta.n_points as u64);
    for p in &pts {
        let r = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
        assert!(r > 0.1 && r < 400.0, "range {r} out of bounds");
    }
}
