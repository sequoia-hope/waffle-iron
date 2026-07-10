#[allow(unused_imports)]
use super::*;

// ── M8-intra: exactly-negated intra-solid coplanar exclusion ────────────
// Spec `specs/m8_intra_opposite_plane_canonicalization.md` (FIP Phase 2,
// RED). `scan_near_coplanar` is `pub(crate)`, so these unit tests reach it
// directly.

/// A minimal planar `BRepFace` with a valid CCW square loop in one plane,
/// so `BRep::new`'s Stage-1 tessellation accepts it while `scan` reads the
/// DECLARED `(normal, d)`.
pub(crate) fn m8_intra_square_a() -> BRep {
    // Two coplanar squares (z = 3) with EXACTLY-negated plane values — a
    // stepped solid's shared plane carrying opposite outward normals. The
    // negation is value-exact AND exercises 0.0 == -0.0 in the normal's x/y
    // components (spec B6 / §6): F0 = ((0.0, 0.0, 1.0), -3.0),
    // F1 = ((-0.0, -0.0, -1.0), 3.0).
    let verts = vec![
        // F0 corners (CCW viewed from +z).
        BRepVertex {
            point: Point3::new(0.0, 0.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(2.0, 0.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(2.0, 2.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 2.0, 3.0),
        },
        // F1 corners (same coords; wound CCW viewed from −z).
        BRepVertex {
            point: Point3::new(0.0, 0.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(2.0, 0.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(2.0, 2.0, 3.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 2.0, 3.0),
        },
    ];
    let seg = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let edges = vec![
        seg(0, 1),
        seg(1, 2),
        seg(2, 3),
        seg(3, 0), // F0 (+z winding)
        seg(4, 7),
        seg(7, 6),
        seg(6, 5),
        seg(5, 4), // F1 (−z winding)
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -3.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: 3.0,
            },
            outer_loop: vec![4, 5, 6, 7],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("intra-A BRep::new")
}

/// Solid B: a single tilted triangle whose AABB overlaps solid A's face
/// region (x,y ∈ [0.5,1.5], z ∈ [2.5,3.5]) but shares NO plane with A — the
/// "other operand reaches the shared-plane region" contact condition the
/// intra gate keys on.
pub(crate) fn m8_intra_overlapping_b() -> BRep {
    let verts = vec![
        BRepVertex {
            point: Point3::new(0.5, 0.5, 2.5),
        },
        BRepVertex {
            point: Point3::new(1.5, 0.5, 2.5),
        },
        BRepVertex {
            point: Point3::new(1.0, 1.5, 3.5),
        },
    ];
    let seg = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
    // Tilted plane normal = (v1−v0)×(v2−v0), un-normalized is fine (scan
    // normalizes); it is not parallel to z, so no coplanar cross pair.
    let faces = vec![BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, -1.0, 1.0),
            d: -2.0,
        },
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    BRep::new(verts, edges, faces).expect("intra-B BRep::new")
}

/// Spec B6 (RED): an intra-solid pair on EXACTLY-negated planes (two
/// orientations of ONE plane) is benign and must NOT flag the intra gate,
/// even though the other solid overlaps the region.
///
/// RED today: the two faces' raw bits differ (n vs −n, d vs −d, and
/// 0.0 vs −0.0), so the bit-identity exclusion does not fire and the
/// near-coplanar band flags them → `scan.intra == Some(..)`.
#[test]
pub(crate) fn intra_exactly_negated_pair_is_excluded() {
    let a = m8_intra_square_a();
    let b = m8_intra_overlapping_b();
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_none(),
        "exactly-negated intra pair must be benign (B6), got {:?}",
        scan.intra
    );
}

/// Spec B7 (guard): a near-but-NOT-exactly-negated intra pair (one normal
/// component drifted 1 ULP from exact negation) is the loud residue and
/// MUST still flag. Passes today; pins that the B6 exclusion is exact-only.
#[test]
pub(crate) fn intra_one_ulp_off_negation_still_walls_guard() {
    let mut a = m8_intra_square_a();
    // Drift F1's z-normal component 1 ULP off exact negation.
    {
        let faces = a.faces();
        let Surface::Plane { normal, d } = faces[1].surface else {
            panic!("F1 not planar");
        };
        let n = normal.as_array();
        let drifted = f64::from_bits(n[2].to_bits().wrapping_add(1));
        // Rebuild A with the drifted F1 normal (BRep faces are not mutable
        // in place through the accessor).
        let verts = a.vertices().to_vec();
        let edges = a.edges().to_vec();
        let mut new_faces = a.faces().to_vec();
        new_faces[1].surface = Surface::Plane {
            normal: Vector3::new(n[0], n[1], drifted),
            d,
        };
        a = BRep::new(verts, edges, new_faces).expect("drifted intra-A BRep::new");
    }
    let b = m8_intra_overlapping_b();
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_some(),
        "a 1-ULP-off (not exactly negated) intra pair must still wall loud (B7)"
    );
}

// ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
// Attacks on the exactly-negated intra exclusion in `scan_near_coplanar`.
// Appended here (not in a new `tests/` file) because `scan_near_coplanar`
// is `pub(crate)`. Purely additive; touches no existing test. Reuses the
// `m8_intra_square_a` / `m8_intra_overlapping_b` helpers above.

/// Rebuild solid A with a chosen F1 (upper-plane) normal/offset so an attack
/// can inject exact bit patterns the accessor cannot mutate in place.
pub(crate) fn m8_intra_a_with_f1(normal: Vector3, d: f64) -> BRep {
    let a = m8_intra_square_a();
    let verts = a.vertices().to_vec();
    let edges = a.edges().to_vec();
    let mut faces = a.faces().to_vec();
    faces[1].surface = Surface::Plane { normal, d };
    BRep::new(verts, edges, faces).expect("rebuilt intra-A")
}

/// FINDING (test strength). Spec §6 / B6 claim the exclusion uses f64 VALUE
/// equality "so `0.0 == -0.0` matches — bit compare would not". The existing
/// `intra_exactly_negated_pair_is_excluded` fixture puts −0.0 on F1's x/y,
/// but for a −0.0 vs 0.0 pairing a *sign-flip-bit* compare
/// (`a.to_bits() == b.to_bits() ^ SIGN`) gives the SAME answer as the value
/// compare — so that test does NOT actually distinguish value from bit and
/// SURVIVES the sign-flip-bit mutation. This fixture uses +0.0 on BOTH
/// faces' x/y (0.0 vs 0.0), where value-negation still holds (0.0 == −0.0)
/// but sign-flip-bit does NOT — a producer that emits +0.0 on both
/// orientations (a hand-built / file-loaded solid that never ran
/// `canonicalize_sibling_planes`) is a real input. This is the case that
/// genuinely KILLS a bit-compare mutation.
#[test]
pub(crate) fn adversary_both_positive_zero_negation_excluded() {
    // F0 = ((0,0,1), −3); F1 = ((+0,+0,−1), +3): value-exact negation with
    // +0.0 (NOT −0.0) in x/y on BOTH faces.
    let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -1.0), 3.0);
    let b = m8_intra_overlapping_b();
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_none(),
        "value-exact negation with +0.0/+0.0 must be benign (B6), got {:?}",
        scan.intra
    );
}

/// Attack 5 (non-unit normals). Two faces on ONE geometric plane whose raw
/// stored normals differ in magnitude (n vs −2n) are NOT exact value
/// negations, so the B6 exclusion must NOT fire; the pair then normalizes to
/// parallel-opposite-coplanar and — since B reaches the region — walls LOUD.
/// The documented conservative residue; nothing crashes.
#[test]
pub(crate) fn adversary_nonunit_opposite_normals_still_wall() {
    // F1 = ((0,0,−2), 6): plane −2z + 6 = 0 ⇒ z = 3, opposite orientation of
    // F0's z = 3 plane, but stored non-unit.
    let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -2.0), 6.0);
    let b = m8_intra_overlapping_b();
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_some(),
        "non-unit opposite normals must not be excluded (conservative residue)"
    );
}

/// Attack 4 (plane through the origin). Both faces carry d = 0.0 and a zero
/// x/y normal component; F1's normal is the value-negation of F0's. The
/// value compare (0.0 == −0.0, and 0.0 == −0.0 on d) excludes it.
#[test]
pub(crate) fn adversary_plane_through_origin_negation_excluded() {
    // Move both squares to z = 0 so d = 0 on both faces, then negate F1.
    let mut a = m8_intra_square_a();
    {
        let mut verts = a.vertices().to_vec();
        for v in verts.iter_mut() {
            v.point = Point3::new(v.point.x(), v.point.y(), 0.0);
        }
        let edges = a.edges().to_vec();
        let mut faces = a.faces().to_vec();
        faces[0].surface = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        faces[1].surface = Surface::Plane {
            normal: Vector3::new(-0.0, -0.0, -1.0),
            d: -0.0,
        };
        a = BRep::new(verts, edges, faces).expect("origin-plane intra-A");
    }
    // B straddles z = 0 so its AABB overlaps the shared plane region.
    let b = {
        let verts = vec![
            BRepVertex {
                point: Point3::new(0.5, 0.5, -0.5),
            },
            BRepVertex {
                point: Point3::new(1.5, 0.5, -0.5),
            },
            BRepVertex {
                point: Point3::new(1.0, 1.5, 0.5),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("origin-plane B")
    };
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_none(),
        "through-origin value-negation (d = 0.0/−0.0) must be benign (B6), got {:?}",
        scan.intra
    );
}

/// Attack (asymmetry). The B6 exclusion is orientation-blind to which face
/// is listed first: swapping F0/F1 (rep negated first) is still excluded.
#[test]
pub(crate) fn adversary_negation_exclusion_is_symmetric() {
    // A with F0 negated instead of F1: F0 = ((−0,−0,−1), 3), F1 = ((0,0,1), −3).
    let a = {
        let base = m8_intra_square_a();
        let verts = base.vertices().to_vec();
        let edges = base.edges().to_vec();
        let mut faces = base.faces().to_vec();
        faces[0].surface = Surface::Plane {
            normal: Vector3::new(-0.0, -0.0, -1.0),
            d: 3.0,
        };
        faces[1].surface = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -3.0,
        };
        BRep::new(verts, edges, faces).expect("swapped intra-A")
    };
    let b = m8_intra_overlapping_b();
    let scan = scan_near_coplanar(&a, &b);
    assert!(
        scan.intra.is_none(),
        "negation exclusion must be symmetric in face order, got {:?}",
        scan.intra
    );
}
