// Copyright 2026 Jim Coleman / North Idaho Labs, LLC
// SPDX-License-Identifier: Apache-2.0

//! Runs the `tzfdump` and `tzf2ply` binaries on small synthetic `.tzf` files
//! (built in `tests/common`) and checks their output, exit codes and errors.

mod common;

use common::*;
use std::path::Path;
use std::process::{Command, Output};

const TZFDUMP: &str = env!("CARGO_BIN_EXE_tzfdump");
const TZF2PLY: &str = env!("CARGO_BIN_EXE_tzf2ply");

fn run<S: AsRef<std::ffi::OsStr>>(bin: &str, args: &[S]) -> Output {
    Command::new(bin).args(args).output().expect("binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("exited normally")
}

/// A file that is not a TZF: long enough to have a header, wrong magic.
fn not_tzf(name: &str) -> std::path::PathBuf {
    write_tmp(name, &[b'x'; 256])
}

// ---------------------------------------------------------------- tzfdump

#[test]
fn tzfdump_without_a_path_prints_usage() {
    for args in [&[][..], &["--blocks", "--verify"][..]] {
        let o = run(TZFDUMP, args);
        assert_eq!(code(&o), 2, "args {args:?}");
        assert!(
            stderr(&o).contains("usage: tzfdump <file.tzf>"),
            "{}",
            stderr(&o)
        );
        assert!(o.stdout.is_empty());
    }
}

#[test]
fn tzfdump_missing_file() {
    let p = tmp("dump-missing.tzf");
    let o = run(TZFDUMP, &[&p]);
    assert_eq!(code(&o), 1);
    assert!(stderr(&o).starts_with("error: I/O:"), "{}", stderr(&o));
    assert!(o.stdout.is_empty());
}

#[test]
fn tzfdump_non_tzf_file() {
    let o = run(TZFDUMP, &[not_tzf("dump-not.tzf")]);
    assert_eq!(code(&o), 1);
    assert_eq!(stderr(&o).trim(), "error: not a .tzf file: bad magic");
}

#[test]
fn tzfdump_file_too_short_for_a_header() {
    let o = run(TZFDUMP, &[write_tmp("dump-short.tzf", &trimble_tzf::MAGIC)]);
    assert_eq!(code(&o), 1);
    assert_eq!(stderr(&o).trim(), "error: truncated: need 88 bytes, have 8");
}

#[test]
fn tzfdump_header_and_scan_meta() {
    let buf = good_scan();
    let p = write_tmp("dump-good.tzf", &buf);
    let o = run(TZFDUMP, &[&p]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    let m = trimble_tzf::Tzf::from_bytes(buf.clone())
        .unwrap()
        .header()
        .meta_offset();
    assert!(
        out.contains(&format!("file      : {}", p.display())),
        "{out}"
    );
    assert!(
        out.contains(&format!("size      : {} bytes", buf.len())),
        "{out}"
    );
    assert!(out.contains(&format!("  @0x10 -> {m} (0x{m:x})")), "{out}");
    assert!(out.contains("  @0x50 -> 0 (0x0)"), "{out}");
    assert!(
        out.contains("scan meta : grid 2x2 (rows x cols), tile 2, points 4"),
        "{out}"
    );
    assert!(out.contains("            start HA "), "{out}");
    assert!(out.contains("            angle steps "), "{out}");
    // No block listing without a flag.
    assert!(!out.contains("blocks    :"), "{out}");
    assert!(!out.contains("verify    :"), "{out}");
}

#[test]
fn tzfdump_reports_unreadable_scan_meta() {
    // The metadata offset points past the end of the file.
    let mut buf = header();
    buf[0x10..0x18].copy_from_slice(&1000u64.to_le_bytes());
    let o = run(TZFDUMP, &[write_tmp("dump-nometa.tzf", &buf)]);
    assert_eq!(code(&o), 0);
    assert!(
        stdout(&o).contains("scan meta : <truncated: need 1064 bytes, have 92>"),
        "{}",
        stdout(&o)
    );
}

#[test]
fn tzfdump_blocks() {
    let p = write_tmp("dump-blocks.tzf", &good_scan());
    for flag in ["--blocks", "--patches"] {
        let o = run(TZFDUMP, &[p.as_os_str(), flag.as_ref()]);
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        let out = stdout(&o);
        assert_eq!(out.matches("  block @0x").count(), 5, "{out}");
        assert!(
            out.contains("tile (0,0) component 1 Deflate 2x2 raw 16"),
            "{out}"
        );
        assert!(
            out.contains("component 4 Jpeg 2x2 raw 4 payload 12 B"),
            "{out}"
        );
        assert!(
            out.contains("component 5 Snappy 2x2 raw 4 payload 6 B"),
            "{out}"
        );
        assert!(
            out.contains("blocks    : 5 in 1 tiles (1 rows x 1 cols)"),
            "{out}"
        );
        assert!(
            out.contains("  component 1 Deflate 2x2 raw       16  x 1"),
            "{out}"
        );
        assert!(!out.contains("verify    :"), "{out}");
    }
}

#[test]
fn tzfdump_lists_only_the_first_six_blocks() {
    // Eight tiles of plane blocks, 24 blocks in all.
    let mut planes = Vec::new();
    for t in 0..8u32 {
        for mut p in two_line_planes() {
            p.1 = (t / 4, t % 4);
            planes.push(p);
        }
    }
    let (buf, _) = plane_file(&planes, (4, 8, 2, 32));
    let o = run(
        TZFDUMP,
        &[
            write_tmp("dump-many.tzf", &buf).as_os_str(),
            "--blocks".as_ref(),
        ],
    );
    assert_eq!(code(&o), 0);
    let out = stdout(&o);
    assert_eq!(out.matches("  block @0x").count(), 6, "{out}");
    assert!(
        out.contains("blocks    : 24 in 8 tiles (2 rows x 4 cols)"),
        "{out}"
    );
    assert!(
        out.contains("  component 2 Deflate 2x2 raw       16  x 8"),
        "{out}"
    );
}

#[test]
fn tzfdump_verify_ok() {
    // The flag may come before the path.
    let p = write_tmp("dump-verify.tzf", &good_scan());
    let o = run(TZFDUMP, &["--verify".as_ref(), p.as_os_str()]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("blocks    : 5 in 1 tiles"), "{out}");
    assert!(
        out.contains("verify    : deflate/snappy 4 ok / 0 failed; jpeg 1 ok / 0 bad"),
        "{out}"
    );
    assert!(o.stderr.is_empty(), "{}", stderr(&o));
}

#[test]
fn tzfdump_verify_counts_failures() {
    let mut buf = header();
    push_planes(&mut buf, &two_line_planes());
    // A DEFLATE block whose stream ends early: inflates 8 of the declared 16 bytes.
    push_block(&mut buf, 5, (0, 0), 2, 2, 16, &deflate_payload(&[7u8; 8]));
    // A Snappy block whose literal runs past the payload.
    push_block(&mut buf, 6, (0, 0), 2, 2, 4, &[4, (4 - 1) << 2, b'a']);
    // A JPEG that starts right but has no end-of-image marker.
    push_block(&mut buf, 4, (0, 0), 2, 2, 4, &JPEG[..10]);
    // A payload of no known kind.
    push_block(&mut buf, 7, (0, 0), 2, 2, 4, &[1, 2, 3, 4, 5, 6]);
    finish(&mut buf, (2, 2, 2, 4));
    let o = run(
        TZFDUMP,
        &[
            write_tmp("dump-bad.tzf", &buf).as_os_str(),
            "--verify".as_ref(),
        ],
    );
    assert_eq!(code(&o), 1, "{}", stdout(&o));
    let out = stdout(&o);
    assert!(out.contains("component 7 Unknown 2x2"), "{out}");
    assert!(
        out.contains("verify    : deflate/snappy 3 ok / 3 failed; jpeg 0 ok / 1 bad"),
        "{out}"
    );
    let err = stderr(&o);
    assert_eq!(
        err.matches("  decode failed: decompress:").count(),
        2,
        "{err}"
    );
    assert!(err.contains("inflated 8 of 16 bytes"), "{err}");
    assert!(err.contains("did not decode"), "{err}");
}

#[test]
fn tzfdump_verify_fails_on_a_bad_jpeg_alone() {
    let mut buf = header();
    push_block(&mut buf, 4, (0, 0), 2, 2, 4, &JPEG[..10]);
    finish(&mut buf, (2, 2, 2, 4));
    let o = run(
        TZFDUMP,
        &[
            write_tmp("dump-badjpeg.tzf", &buf).as_os_str(),
            "--verify".as_ref(),
        ],
    );
    assert_eq!(code(&o), 1);
    assert!(
        stdout(&o).contains("verify    : deflate/snappy 0 ok / 0 failed; jpeg 0 ok / 1 bad"),
        "{}",
        stdout(&o)
    );
}

// ---------------------------------------------------------------- tzf2ply

/// Run tzf2ply on `tzf`, writing to a fresh `<name>` base. Returns the output
/// and the base path.
fn ply(name: &str, tzf: &Path, extra: &[&str]) -> (Output, String) {
    let base = tmp(name).to_str().unwrap().to_string();
    let _ = std::fs::remove_file(format!("{base}.ply"));
    let _ = std::fs::remove_file(format!("{base}.las"));
    let mut args = vec![tzf.to_str().unwrap().to_string(), base.clone()];
    args.extend(extra.iter().map(|s| s.to_string()));
    (run(TZF2PLY, &args), base)
}

/// Parse a tzf2ply PLY: header lines, then (x, y, z, gray) per vertex.
fn read_ply(path: &str) -> (Vec<String>, Vec<([f32; 3], u8)>) {
    let d = std::fs::read(path).unwrap();
    let end = d.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
    let head: Vec<String> = std::str::from_utf8(&d[..end])
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    let body = &d[end..];
    assert_eq!(body.len() % 15, 0);
    let verts = body
        .chunks(15)
        .map(|c| {
            let f = |i: usize| f32::from_le_bytes(c[i..i + 4].try_into().unwrap());
            assert_eq!(c[12], c[13]);
            assert_eq!(c[13], c[14]);
            ([f(0), f(4), f(8)], c[12])
        })
        .collect();
    (head, verts)
}

struct Las {
    n: u32,
    scale: [f64; 3],
    offset: [f64; 3],
    max: [f64; 3],
    min: [f64; 3],
    /// (X, Y, Z, intensity) per record.
    points: Vec<([i32; 3], u16)>,
}

fn read_las(path: &str) -> Las {
    let d = std::fs::read(path).unwrap();
    assert_eq!(&d[..4], b"LASF");
    assert_eq!(&d[24..26], &[1, 2]);
    assert_eq!(&d[26..29], b"tzf");
    assert_eq!(&d[58..65], b"tzf2ply");
    let u16_at = |o: usize| u16::from_le_bytes(d[o..o + 2].try_into().unwrap());
    let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let f64_at = |o: usize| f64::from_le_bytes(d[o..o + 8].try_into().unwrap());
    assert_eq!(u16_at(94), 227); // header size
    assert_eq!(u32_at(96), 227); // offset to point data
    assert_eq!(u32_at(100), 0); // VLRs
    assert_eq!(d[104], 0); // point format 0
    assert_eq!(u16_at(105), 20); // record length
    let n = u32_at(107);
    assert_eq!(u32_at(111), n); // first returns
    assert_eq!(d.len(), 227 + 20 * n as usize);
    let three = |o: usize| [f64_at(o), f64_at(o + 8), f64_at(o + 16)];
    let scale = three(131);
    let offset = three(155);
    let max = [f64_at(179), f64_at(195), f64_at(211)];
    let min = [f64_at(187), f64_at(203), f64_at(219)];
    let points = d[227..]
        .chunks(20)
        .map(|r| {
            let i = |o: usize| i32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            assert_eq!(&r[14..20], &[0b0000_1001, 0, 0, 0, 0, 0]);
            ([i(0), i(4), i(8)], u16::from_le_bytes([r[12], r[13]]))
        })
        .collect();
    Las {
        n,
        scale,
        offset,
        max,
        min,
        points,
    }
}

const EXPECTED: [[f32; 3]; 4] = [
    [1.0, 0.0, 0.0],
    [2.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 2.0, 0.0],
];

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn tzf2ply_bad_arguments() {
    for args in [&[][..], &["only-one.tzf"][..]] {
        let o = run(TZF2PLY, args);
        assert_eq!(code(&o), 2, "args {args:?}");
        assert!(
            stderr(&o).contains("usage: tzf2ply <in.tzf> <out-base>"),
            "{}",
            stderr(&o)
        );
    }
}

#[test]
fn tzf2ply_missing_file() {
    let (o, base) = ply("ply-missing", &tmp("ply-missing.tzf"), &[]);
    assert_eq!(code(&o), 1);
    assert!(stderr(&o).starts_with("error: I/O:"), "{}", stderr(&o));
    assert!(!Path::new(&format!("{base}.ply")).exists());
}

#[test]
fn tzf2ply_non_tzf_file() {
    let (o, base) = ply("ply-not", &not_tzf("ply-not.tzf"), &[]);
    assert_eq!(code(&o), 1);
    assert_eq!(stderr(&o).trim(), "error: not a .tzf file: bad magic");
    assert!(!Path::new(&format!("{base}.ply")).exists());
}

#[test]
fn tzf2ply_truncated_scan_meta() {
    let mut buf = header();
    buf[0x10..0x18].copy_from_slice(&1000u64.to_le_bytes());
    let (o, _) = ply("ply-nometa", &write_tmp("ply-nometa.tzf", &buf), &[]);
    assert_eq!(code(&o), 1);
    assert_eq!(
        stderr(&o).trim(),
        "error: truncated: need 1064 bytes, have 92"
    );
}

#[test]
fn tzf2ply_no_point_blocks() {
    let mut buf = header();
    push_block(&mut buf, 4, (0, 0), 2, 2, 4, &JPEG);
    let m = finish(&mut buf, (2, 2, 2, 4));
    set_identity_tilt(&mut buf, m);
    let (o, _) = ply("ply-noblocks", &write_tmp("ply-noblocks.tzf", &buf), &[]);
    assert_eq!(code(&o), 1);
    assert_eq!(
        stderr(&o).trim(),
        "error: malformed scan: no range/angle blocks found"
    );
}

#[test]
fn tzf2ply_writes_ply_and_las() {
    let (o, base) = ply("ply-good", &write_tmp("ply-good.tzf", &good_scan()), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(o.stdout.is_empty());
    assert_eq!(
        stderr(&o).trim(),
        format!("4 points (4 expected) -> {base}.ply, {base}.las")
    );

    let (head, verts) = read_ply(&format!("{base}.ply"));
    assert_eq!(head[0], "ply");
    assert_eq!(head[1], "format binary_little_endian 1.0");
    assert!(head.contains(&"element vertex 4".to_string()));
    assert_eq!(verts.len(), 4);
    for ((xyz, g), want) in verts.iter().zip(EXPECTED) {
        assert_eq!(*g, 128);
        assert!((0..3).all(|k| near(xyz[k], want[k])), "{xyz:?} vs {want:?}");
    }

    let las = read_las(&format!("{base}.las"));
    assert_eq!(las.n, 4);
    assert_eq!(las.scale, [0.0001; 3]);
    assert_eq!(las.offset, las.min);
    for k in 0..3 {
        let lo = EXPECTED.iter().map(|p| p[k]).fold(f32::MAX, f32::min) as f64;
        let hi = EXPECTED.iter().map(|p| p[k]).fold(f32::MIN, f32::max) as f64;
        assert!(
            (las.min[k] - lo).abs() < 1e-4 && (las.max[k] - hi).abs() < 1e-4,
            "axis {k}"
        );
    }
    for ((xyz, intensity), want) in las.points.iter().zip(EXPECTED) {
        assert_eq!(*intensity, 128 * 257);
        for k in 0..3 {
            let v = las.offset[k] + xyz[k] as f64 * las.scale[k];
            assert!((v - want[k] as f64).abs() < 2e-4, "{v} vs {}", want[k]);
        }
    }
}

#[test]
fn tzf2ply_warns_when_tilt_is_not_a_rotation() {
    // plane_file leaves an all-zero tilt.
    let (buf, _) = plane_file(&two_line_planes(), (2, 2, 2, 4));
    let (o, base) = ply("ply-zerotilt", &write_tmp("ply-zerotilt.tzf", &buf), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let err = stderr(&o);
    assert!(
        err.starts_with(
            "warning: stored levelling matrix is not a proper rotation; points are NOT levelled"
        ),
        "{err}"
    );
    assert!(err.contains("4 points (4 expected)"), "{err}");
    assert_eq!(read_ply(&format!("{base}.ply")).1.len(), 4);
}

#[test]
fn tzf2ply_warns_when_tilt_is_missing() {
    let (buf, m) = plane_file(&two_line_planes(), (2, 2, 2, 4));
    let buf = &buf[..m + 0x44];
    let (o, _) = ply("ply-notilt", &write_tmp("ply-notilt.tzf", buf), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stderr(&o)
            .starts_with("warning: no levelling rotation in the file; points are NOT levelled"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn tzf2ply_writes_empty_clouds() {
    // Every range cell is 0 (no return): the planes are valid, no points.
    let mut planes = two_line_planes();
    planes[0].5 = cells(&[0; 4]);
    let (mut buf, m) = plane_file(&planes, (2, 2, 2, 4));
    set_identity_tilt(&mut buf, m);
    let (o, base) = ply("ply-empty", &write_tmp("ply-empty.tzf", &buf), &[]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("0 points (4 expected)"),
        "{}",
        stderr(&o)
    );
    let (head, verts) = read_ply(&format!("{base}.ply"));
    assert!(head.contains(&"element vertex 0".to_string()));
    assert!(verts.is_empty());
    let las = read_las(&format!("{base}.las"));
    assert_eq!(las.n, 0);
    assert_eq!((las.min, las.max), ([0.0; 3], [0.0; 3]));
}

#[test]
fn tzf2ply_fails_when_output_cannot_be_written() {
    let tzf = write_tmp("ply-nodir.tzf", &good_scan());
    let base = tmp("no-such-dir").join("out");
    let o = run(TZF2PLY, &[tzf.as_os_str(), base.as_os_str()]);
    assert_eq!(code(&o), 1);
    assert!(stderr(&o).starts_with("error: "), "{}", stderr(&o));
    assert!(stderr(&o).contains("os error"), "{}", stderr(&o));
}

/// A binary PGM with a comment line in the header.
fn pgm(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let mut d = format!("P5\n# grid intensity\n{w} {h}\n255\n").into_bytes();
    d.extend_from_slice(px);
    d
}

#[test]
fn tzf2ply_intensity_from_pgm() {
    let tzf = write_tmp("ply-pgm.tzf", &good_scan());
    // Row-major: (row 0: col0 col1), (row 1: col0 col1).
    let img = write_tmp("ply-pgm.pgm", &pgm(2, 2, &[10, 30, 20, 40]));
    let (o, base) = ply("ply-pgm", &tzf, &["--intensity-pgm", img.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    // Points come in (col, row) order: (0,0) (0,1) (1,0) (1,1).
    let want = [10u8, 20, 30, 40];
    let (_, verts) = read_ply(&format!("{base}.ply"));
    assert_eq!(verts.iter().map(|v| v.1).collect::<Vec<_>>(), want);
    let las = read_las(&format!("{base}.las"));
    let got: Vec<u16> = las.points.iter().map(|p| p.1).collect();
    assert_eq!(got, want.map(|g| g as u16 * 257));
}

fn pgm_error(name: &str, extra: &[&str]) -> String {
    let tzf = write_tmp(&format!("{name}.tzf"), &good_scan());
    let (o, base) = ply(name, &tzf, extra);
    assert_eq!(code(&o), 1, "{}", stderr(&o));
    assert!(!Path::new(&format!("{base}.ply")).exists());
    stderr(&o).trim().to_string()
}

#[test]
fn tzf2ply_intensity_flag_without_path() {
    assert_eq!(
        pgm_error("pgm-nopath", &["--intensity-pgm"]),
        "error: --intensity-pgm needs a path"
    );
}

#[test]
fn tzf2ply_intensity_pgm_errors() {
    let missing = tmp("pgm-missing.pgm");
    let err = pgm_error(
        "pgm-missing",
        &["--intensity-pgm", missing.to_str().unwrap()],
    );
    assert!(
        err.starts_with("error: ") && err.contains("os error"),
        "{err}"
    );
    let cases: [(&str, Vec<u8>, &str); 6] = [
        (
            "pgm-size",
            pgm(3, 2, &[0; 6]),
            "error: intensity image is 3x2, grid is 2x2",
        ),
        (
            "pgm-ascii",
            b"P2\n2 2\n255\n1 2 3 4\n".to_vec(),
            "error: expected 8-bit binary PGM (P5)",
        ),
        (
            "pgm-16bit",
            b"P5\n2 2\n65535\n\0\0\0\0\0\0\0\0".to_vec(),
            "error: expected 8-bit binary PGM (P5)",
        ),
        (
            "pgm-header",
            b"P5\n2 2".to_vec(),
            "error: truncated PGM header",
        ),
        (
            "pgm-pixels",
            pgm(2, 2, &[1, 2]),
            "error: truncated PGM pixels",
        ),
        (
            "pgm-number",
            b"P5\n2 two\n255\n\0\0\0\0".to_vec(),
            "error: invalid digit found in string",
        ),
    ];
    for (name, bytes, want) in cases {
        let img = write_tmp(&format!("{name}.pgm"), &bytes);
        assert_eq!(
            pgm_error(name, &["--intensity-pgm", img.to_str().unwrap()]),
            want,
            "{name}"
        );
    }
}
