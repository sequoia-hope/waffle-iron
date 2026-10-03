//! P0019: a full-circle rim shared by two CONE faces and no planar cap.
//!
//! The assay case P0019 (`convex4:boss nonconvex5:rev convex4:cut`) revolves a
//! 5-vertex non-convex profile about an external axis, which is a genus-1 ring
//! of FIVE cone bands joined rim to rim — no planar cap anywhere on the body.
//! `from_yang_brep`'s full-circle sense derivation had exactly two witnesses, a
//! planar cap use and the leaving-edge reading of a cylinder or torus use, and
//! neither can speak for a cone band: the first because there is no cap, the
//! second because `derive_curved` had no `FaceSurf::Cone` arm at all, and
//! because a band in its ANNULAR form (outer loop = one rim, inner loop = the
//! other) has no edge leaving the rim's anchor to read. The whole subtract
//! STOPped with `InvalidBooleanOutput("full-circle edge sense is underivable
//! …")` — the wall R0004 once hit.
//!
//! The sense is derivable, and from the band itself: `validate_cone_face` and
//! `validate_cylinder_face` both state and ENFORCE the rule that on an outward
//! (solid) band each rim's traversal axis points TOWARD the opposite rim, and
//! AWAY from it on a cavity wall — the same law the SI5 STEP ingest derives a
//! rim's traversal from (`specs/step_import_si5_exact_analytic_ingestion.md`,
//! "Which way a rim circle is traversed is derived, never read"). Reading it
//! needs only the face's own two rim centres.
//!
//! Fixture: the smallest closed solid carrying a rim with NO planar cap use and
//! NO cylinder/torus use — a "barrel", two cone frustums meeting at a shared
//! rim, closed by a planar disc at each end:
//!
//! ```text
//!    rim2 ------=========------   z = 2, r = 1     top disc, normal +ẑ
//!              /         \        frustum B, apex (0,0,4)
//!    rim1 ----/-----------\----   z = 1, r = 3/2  <-- two CONE uses, no cap
//!              \         /        frustum A, apex (0,0,−2)
//!    rim0 ------=========------   z = 0, r = 1     bottom disc, normal −ẑ
//! ```
//!
//! rim1's two uses are both cone bands, so neither witness could speak for it;
//! rim0 and rim2 each have a planar cap use and were always derivable.
//!
//! The P0019 document itself does NOT convert on this fix alone: its five rims
//! form a CYCLE (a closed profile revolved about an external axis), and
//! `recover.rs` anchors seam feet greedily per face, which in a rim cycle
//! leaves two bands whose rims were anchored 15° apart and therefore stay in
//! the annular form that `validate_cone_face` refuses. See the P0019 row of
//! `docs/yang_tail_triage.md` and deviation N73.

use cad_primitives::{Point3, Vector3};
use kernel_v2::{from_yang_brep, tessellate, validate_solid, BrepArena, RenderMesh};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// Positional watertightness: snap each render-mesh vertex to a 1e-9 grid and
/// require every undirected edge to be shared by exactly two triangles.
fn assert_render_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::BTreeMap;
    let key = |i: u32| -> [i64; 3] {
        let k = i as usize * 3;
        [
            (mesh.positions[k] * 1e9).round() as i64,
            (mesh.positions[k + 1] * 1e9).round() as i64,
            (mesh.positions[k + 2] * 1e9).round() as i64,
        ]
    };
    let mut ec: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *ec.entry(e).or_insert(0) += 1;
        }
    }
    for (e, n) in &ec {
        assert_eq!(*n, 2, "{what}: edge {e:?} shared by {n} tris (not 2)");
    }
}

/// Enclosed volume of a kernel-v2 render mesh.
fn render_volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [m.positions[k], m.positions[k + 1], m.positions[k + 2]]
    };
    let mut s = 0.0;
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        s += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    (s / 6.0).abs()
}

/// A "barrel" lathe as a yang B-Rep output: two cone frustums meeting at a
/// shared rim, closed by a planar disc at each end.
///
/// Both frustums are given in the SEAMED `[rim, seam, rim, seam]` form. The
/// P0019 output face arrives ANNULAR instead (outer loop = one rim, inner loop
/// = the other) — but `yang_rs::BRep::new` refuses that form as INPUT ("cone
/// periodic strip (2 encircling rims) not yet supported"), so a hand-built
/// fixture cannot carry it. The seam makes no difference to what is under test:
/// `derive_curved` has no `FaceSurf::Cone` arm either way, so a cone band's rim
/// sense comes from the two-rim rule in both forms.
fn barrel_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.5, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 2.0),
        },
    ];
    let edges = vec![
        // rim0: z = 0, r = 1 — frustum A ∩ bottom disc.
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        },
        // rim1: z = 1, r = 3/2 — frustum A ∩ frustum B. NO planar cap use.
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 1.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 1.5,
            },
        },
        // rim2: z = 2, r = 1 — frustum B ∩ top disc.
        BRepEdge {
            start: 2,
            end: 2,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 2.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        },
        // Seam rulings (slant generators), one per band.
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
    ];
    let half = (0.5f64).atan();
    let faces = vec![
        // Frustum A: apex (0,0,−2), axis +ẑ, tan = 1/2 — rim0 at τ = 2 carries
        // r = 1, rim1 at τ = 3 carries r = 3/2.
        BRepFace {
            surface: Surface::Cone {
                apex: p(0.0, 0.0, -2.0),
                axis_dir: v3(0.0, 0.0, 1.0),
                half_angle: half,
            },
            outer_loop: vec![0, 3, 1, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Frustum B: apex (0,0,4), axis −ẑ — rim1 at τ = 3 carries r = 3/2,
        // rim2 at τ = 2 carries r = 1.
        BRepFace {
            surface: Surface::Cone {
                apex: p(0.0, 0.0, 4.0),
                axis_dir: v3(0.0, 0.0, -1.0),
                half_angle: half,
            },
            outer_loop: vec![1, 4, 2, 4],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Bottom disc, outward normal −ẑ: n·x + d = 0 through the origin.
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Top disc, outward normal +ẑ through z = 2.
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, 0.0, 1.0),
                d: -2.0,
            },
            outer_loop: vec![2],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("barrel B-Rep")
}

/// RED before the two-rim band rule: `from_yang_brep` returned
/// `InvalidBooleanOutput("full-circle edge sense is underivable (no planar cap
/// use with an aligned plane, no curved use with a readable material sense)")`.
#[test]
fn cone_rim_with_no_planar_cap_derives_its_sense_from_the_band() {
    let brep = barrel_brep();
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &brep)
        .unwrap_or_else(|e| panic!("from_yang_brep(frustum + frustum + 2 discs): {e:?}"));
    validate_solid(&arena, solid).expect("the reconstructed barrel validates");
}

/// The derived senses are the ones the two validators' material-sense rule
/// demands, so the solid's own geometry is right: the exact volume is the apex
/// cone plus the frustum,
/// `π/3 · 1²·1 + π/3 · 1 · (1.5² + 1.5·1 + 1²) = 5.75·π/3`.
#[test]
fn barrel_volume_matches_the_analytic_lathe() {
    let brep = barrel_brep();
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &brep).expect("barrel reconstructs");
    let mesh = tessellate(&arena, solid).expect("render mesh");
    assert_render_watertight(&mesh, "barrel");
    let exact = 9.5 * std::f64::consts::PI / 3.0;
    let v = render_volume(&mesh);
    // A faceted lathe UNDER-reports a convex-in-azimuth body by its chord
    // deficit; the band is one-sided and generous (the pin is the sense, not
    // the tessellator's chord density).
    assert!(
        v > 0.0 && v <= exact && (exact - v) / exact < 0.05,
        "render volume {v:.17e} must sit just under the analytic {exact:.17e}"
    );
}
