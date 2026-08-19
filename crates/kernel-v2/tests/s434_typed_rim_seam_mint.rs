//! I5-2 adjudication (a) — C0117 anchor (spec `yang_441_trim_cdt_construction.md`
//! §4-I5-1b): TYPED-RIM canonicalization in `recover.rs`.
//!
//! With the I5-1b Stage-6 seam chain-merge on (`YANG_434_MERGE`), yang emits
//! a closed seam rim as analytic ARCS whose split vertices are chosen per
//! rim, so the two rims of one lateral share no azimuth. `recover.rs`'s
//! canonical `[rim, seam, rim, seam]` pairing used to REQUIRE an existing
//! azimuth-aligned vertex pair (an implicit "both rims retain the Stage-1
//! lattice" contract) and otherwise fell back to sub-π arc pieces — a
//! non-canonical lateral whose caps then went through the general planar
//! tessellation path, sampling each arc from its own start. On C0117's
//! 1e-4 coaxial tube wall (sagitta 4.8e-4 at r = 0.5) the out-of-phase cap
//! rings crossed → `TessellationFailed "ring rejected by CDT"` at the
//! boolean's render gate; with a canonical inner lateral but an arbitrary
//! seam azimuth the two laterals' fixed-N render rows interpenetrate →
//! `SelfIntersectingBooleanOutput`.
//!
//! The fix (recover.rs pass 2): when no existing pair aligns, choose the
//! seam foot azimuth COHERENTLY with an already-anchored coaxial lateral
//! (the constructor's own convention for a holed profile), else the face's
//! own deterministic rim-a vertex, and MINT the exact on-circle foot on the
//! rim(s) lacking a vertex there. Pass 1 (existing aligned pairs) is
//! untouched — gate-off outputs are byte-identical.
//!
//! The merge gate is process-global; every test here serialises on one
//! mutex and sets/clears the env var itself.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, BrepArena, Curve, Profile, Surface};
use std::sync::{Mutex, MutexGuard};

static GATE: Mutex<()> = Mutex::new(());

fn merge_on() -> MutexGuard<'static, ()> {
    let g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("YANG_434_MERGE", "1");
    g
}

fn merge_off() -> MutexGuard<'static, ()> {
    let g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("YANG_434_MERGE");
    g
}

fn cyl_rot(a: &mut BrepArena, r: f64, z: (f64, f64), rot_deg: f64) -> kernel_v2::SolidId {
    let (s, c) = rot_deg.to_radians().sin_cos();
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(c, s, 0.0),
        Vector3::new(-s, c, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

/// Every cylinder lateral of `solid`: (radius, seam-foot azimuths in
/// degrees, half-edge curve kinds of the outer loop). A canonical lateral
/// reads `["C","L","C","L"]` (rim, seam, rim, seam).
fn laterals(
    arena: &BrepArena,
    solid: kernel_v2::SolidId,
) -> Vec<(f64, Vec<f64>, Vec<&'static str>)> {
    let mut out = Vec::new();
    let sol = arena.solid(solid).unwrap();
    for &sh in &sol.shells {
        for &f in &arena.shell(sh).unwrap().faces {
            let face = arena.face(f).unwrap();
            let Some(Surface::Cylinder { radius, .. }) = face.surface else {
                continue;
            };
            let mut kinds = Vec::new();
            let mut seams = Vec::new();
            for h in arena.loop_half_edges(face.outer_loop).unwrap() {
                let he = arena.half_edge(h).unwrap();
                let p = arena.vertex(he.origin).unwrap().point;
                let az = p.y().atan2(p.x()).to_degrees();
                kinds.push(match he.curve {
                    Curve::LineSegment => {
                        seams.push(az);
                        "L"
                    }
                    Curve::Circle { .. } => "C",
                    Curve::Arc { .. } => "A",
                    _ => "?",
                });
            }
            out.push((radius, seams, kinds));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out
}

fn tube(
    arena: &mut BrepArena,
    r_in: f64,
    tool_rot_deg: f64,
) -> Result<kernel_v2::SolidId, kernel_v2::KernelV2Error> {
    let a = cyl_rot(arena, 0.5, (0.0, 2.0), 0.0);
    let b = cyl_rot(arena, r_in, (-1.0, 3.0), tool_rot_deg);
    boolean_op(arena, a, b, BoolOp::Subtract)
}

fn assert_canonical_in_phase(arena: &BrepArena, s: kernel_v2::SolidId, r_in: f64) {
    let lats = laterals(arena, s);
    assert_eq!(lats.len(), 2, "tube has two laterals: {lats:?}");
    for (r, seams, kinds) in &lats {
        assert_eq!(
            kinds,
            &["C", "L", "C", "L"],
            "lateral r={r} canonical: {kinds:?}"
        );
        assert_eq!(seams.len(), 2);
    }
    assert!((lats[0].0 - r_in).abs() < 1e-12 && (lats[1].0 - 0.5).abs() < 1e-12);
    // Coaxial phase lock: both laterals' seams sit at the same azimuth.
    let d = (lats[0].1[0] - lats[1].1[0]).abs();
    let d = d.min(360.0 - d);
    assert!(
        d < 1e-9,
        "coaxial laterals share the seam azimuth: inner {:?} outer {:?}",
        lats[0].1,
        lats[1].1
    );
}

/// The C0117 configuration under the merge gate: 1e-4 coaxial wall at
/// r = 0.5. Before the fix: `TessellationFailed "ring rejected by CDT"`
/// (then `SelfIntersectingBooleanOutput` with an arbitrary seam azimuth).
#[test]
fn merged_thin_coaxial_tube_canonicalizes_in_phase() {
    let _g = merge_on();
    let mut arena = BrepArena::new();
    let s = tube(&mut arena, 0.4999, 0.0).expect("merged 1e-4 tube assembles");
    assert_canonical_in_phase(&arena, s, 0.4999);
    let m = tessellate(&arena, s).expect("renders");
    assert_eq!(
        m.indices.len() / 3,
        568,
        "same render vocabulary as gate-off"
    );
}

/// Sub-sagitta gaps across the band: all render, all canonical.
#[test]
fn merged_tube_gap_sweep() {
    let _g = merge_on();
    for r_in in [0.499, 0.4995, 0.4999, 0.49999] {
        let mut arena = BrepArena::new();
        let s = tube(&mut arena, r_in, 0.0)
            .unwrap_or_else(|e| panic!("merged tube r_in={r_in} assembles: {e:?}"));
        assert_canonical_in_phase(&arena, s, r_in);
        tessellate(&arena, s).expect("renders");
    }
}

/// The tool's own lattice phase (30° rotated sketch frame) does NOT leak
/// into the seam: the bore locks to the boss lateral's azimuth (the
/// coaxial reference), not to the tool's frame.
#[test]
fn merged_tube_rotated_tool_locks_to_coaxial_reference() {
    let _g = merge_on();
    let mut arena = BrepArena::new();
    let s = tube(&mut arena, 0.4999, 30.0).expect("merged rotated-tool tube assembles");
    assert_canonical_in_phase(&arena, s, 0.4999);
    tessellate(&arena, s).expect("renders");
}

/// The minted seam foot is the EXACT on-circle point: radius error at
/// rounding, not band, level.
#[test]
fn minted_seam_foot_is_on_circle() {
    let _g = merge_on();
    let mut arena = BrepArena::new();
    let s = tube(&mut arena, 0.4999, 0.0).expect("assembles");
    let lats = laterals(&arena, s);
    let sol = arena.solid(s).unwrap();
    let mut checked = 0;
    for &sh in &sol.shells {
        for &f in &arena.shell(sh).unwrap().faces {
            let face = arena.face(f).unwrap();
            let Some(Surface::Cylinder { radius, .. }) = face.surface else {
                continue;
            };
            for h in arena.loop_half_edges(face.outer_loop).unwrap() {
                let he = arena.half_edge(h).unwrap();
                let p = arena.vertex(he.origin).unwrap().point;
                let r = (p.x() * p.x() + p.y() * p.y()).sqrt();
                assert!(
                    (r - radius).abs() <= 4.0 * f64::EPSILON * radius,
                    "vertex on rim circle: r={r} radius={radius}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 8 && lats.len() == 2);
}

/// Gate-off control: the same tube through the mesh-granular path stays
/// canonical and in phase (pass 1, byte-identical to before this change).
#[test]
fn gate_off_tube_control() {
    let _g = merge_off();
    let mut arena = BrepArena::new();
    let s = tube(&mut arena, 0.4999, 0.0).expect("gate-off tube assembles");
    assert_canonical_in_phase(&arena, s, 0.4999);
    let m = tessellate(&arena, s).expect("renders");
    assert_eq!(m.indices.len() / 3, 568);
}
