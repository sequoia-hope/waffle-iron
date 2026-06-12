//! PR-KV5b RED — cylinder solids through the yang-rs boolean boundary.
//!
//! KV5a built circle-profile → cylinder solids (vertex-anchored closed
//! circle edges, `Surface::Cylinder` laterals) deliberately matching
//! yang-rs's M5 fixture topology. This suite pins the KV5b contract: the
//! kernel-v2 ↔ yang-rs conversion for those solids, end-to-end booleans on
//! the configurations yang-rs's own suite proves (cylinder×box class), and
//! typed loud walls everywhere else.
//!
//! ## What the yang-output survey established (tests/kv5b_survey.rs)
//!
//! Native cylinder×box boolean outputs contain:
//! - surfaces: `Plane` and `Cylinder` only (`reversed == true` exactly on
//!   Subtract cavity walls);
//! - curves: `LineSegment` and `Circle` with `start != end` — i.e. ARC
//!   edges carrying full-circle params (the exact SSI intersection
//!   circles). Original input rims come back as untagged `LineSegment`
//!   chord polylines (yang Stage 1's facet resolution). NO full
//!   (`start == end`) circle edges, no Ellipse/Parabola/Hyperbola on the
//!   right-cylinder × axis-aligned-box class;
//! - cylinder faces are PARTIAL patches: boundary loops mixing arc and
//!   segment edges (wrapping rim cycles, window cycles, chord polylines).
//!
//! Oblique sections (tilted plane × cylinder) DO emit `Curve::Ellipse` —
//! out of the KV5b reassembly vocabulary and pinned here as a typed loud
//! wall naming the curve. Cylinder×cylinder fails INSIDE yang (Stage-3 SSI
//! `AmbiguousCurve` — the degree-4 intersection wall); kernel-v2 surfaces
//! that yang error loudly, never masks it.
//!
//! ## Volume oracle band (documented, not a guess)
//!
//! yang Stage 1 tessellates the input rims at a facet count `N` chosen by
//! its own chord bound (N = 8 on these fixtures, observed in the survey:
//! rim chords come back as 8-segment polylines). The reassembled B-Rep's
//! boundary is therefore faceted at the rims (the chords ARE the edges; the
//! reassembly must not invent geometry yang did not emit), while interior
//! tessellation refinement stays on the analytic surface. The inscribed
//! N-gon area deficit at N = 8 is 1 − (N/2π)·sin(2π/N) ≈ 9.97%, so the
//! mesh volume of the cylinder-contributed term lies within
//! [−10%, 0] (+ rounding) of the analytic value. The oracles below assert
//! |V_mesh − V_exact| ≤ 0.12 · V_cyl_term — the documented faceting band
//! with margin, NOT a tunable tolerance.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, from_yang_brep, tessellate, to_yang_brep, validate_solid, BrepArena,
    ExtrudeResult, Profile, RenderMesh, Surface,
};

// ── fixtures ───────────────────────────────────────────────────────────────

/// Right cylinder: axis +z through (0.5, 0.5), radius 0.25, z ∈ [z0, z0+h].
fn cylinder(arena: &mut BrepArena, z0: f64, h: f64) -> ExtrudeResult {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.5, 0.5),
        0.25,
    )
    .expect("circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), h).expect("cylinder extrude")
}

/// Axis-aligned box [0,1]² × [z0, z0+h] (optionally offset in x/y).
fn boxx(arena: &mut BrepArena, x0: f64, y0: f64, z0: f64, h: f64) -> ExtrudeResult {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x0, y0),
            Point2::new(x0 + 1.0, y0),
            Point2::new(x0 + 1.0, y0 + 1.0),
            Point2::new(x0, y0 + 1.0),
        ],
        Vec::new(),
    )
    .expect("rect profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), h).expect("box extrude")
}

fn mesh_volume(mesh: &RenderMesh) -> f64 {
    let mut vol = 0.0f64;
    let p = |i: u32| {
        let i = i as usize * 3;
        [
            mesh.positions[i],
            mesh.positions[i + 1],
            mesh.positions[i + 2],
        ]
    };
    for t in mesh.indices.chunks(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    vol
}

const PI: f64 = std::f64::consts::PI;
const R: f64 = 0.25;
/// Documented faceting band (module docs): yang Stage-1 rim facets.
const FACET_BAND: f64 = 0.12;

/// Count the solid's faces by surface kind: (planar, cylinder).
fn face_kinds(arena: &BrepArena, solid: kernel_v2::SolidId) -> (usize, usize) {
    let (mut planar, mut cyl) = (0usize, 0usize);
    let solid_ref = arena.solid(solid).expect("solid");
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            match arena.face(f).expect("face").surface {
                Some(Surface::Plane(_)) => planar += 1,
                Some(Surface::Cylinder { .. }) => cyl += 1,
                Some(other) => panic!("unexpected surface kind {other:?}"),
                None => panic!("face without surface"),
            }
        }
    }
    (planar, cyl)
}

// ── 1. canonical round-trip ────────────────────────────────────────────────

/// A KV5a cylinder solid must convert to yang's BRep (the M5 fixture shape)
/// and reassemble back into a kernel-v2 solid that passes `validate_solid`
/// with the canonical V2/E3/F3 topology and EXACTLY the same analytic
/// signed volume (the round-trip is mechanical by design — identical curve
/// and surface data on both sides, no resampling anywhere).
#[test]
fn cylinder_round_trips_through_yang_brep_exactly() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, 0.0, 2.0);
    let vol_in = kernel_v2::geom::signed_volume(&arena, cyl.solid).expect("analytic volume");

    let ybrep = to_yang_brep(&arena, cyl.solid).expect("cylinder → yang BRep (KV5b)");
    // The M5 fixture shape: 2 seam vertices, 3 edges (2 closed circles +
    // 1 seam ruling), 3 faces.
    assert_eq!(ybrep.vertices().len(), 2, "seam vertices");
    assert_eq!(ybrep.edges().len(), 3, "two rim circles + one seam");
    assert_eq!(ybrep.faces().len(), 3, "two caps + lateral");

    let back = from_yang_brep(&mut arena, &ybrep).expect("yang BRep → cylinder");
    let report = validate_solid(&arena, back).expect("round-tripped solid validates");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.genus),
        (2, 3, 3, 0),
        "canonical cylinder topology V2/E3/F3/G0"
    );
    let vol_out = kernel_v2::geom::signed_volume(&arena, back).expect("analytic volume");
    assert_eq!(
        vol_in.to_bits(),
        vol_out.to_bits(),
        "analytic volume preserved exactly (got {vol_in} vs {vol_out})"
    );
    let (planar, cyl_faces) = face_kinds(&arena, back);
    assert_eq!((planar, cyl_faces), (2, 1));
}

// ── 2. cylinder ∪ box (yr8 configuration) ─────────────────────────────────

/// Union of the yr8-class configuration: cylinder pokes through both caps
/// of the box. Volume = box + the cylinder part outside the box
/// (analytic truth 1 + πr²·(2−1)), within the documented faceting band.
#[test]
fn boolean_union_cylinder_box_volume() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, -0.5, 2.0);
    let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);

    let out = boolean_op(&mut arena, cyl.solid, bx.solid, BoolOp::Union)
        .expect("cylinder ∪ box is a yang-proven configuration (yr8)");
    let report = validate_solid(&arena, out).expect("union validates");
    assert_eq!(report.genus, 0, "union is genus 0");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let (_, cyl_faces) = face_kinds(&arena, out);
    assert!(
        cyl_faces >= 1,
        "the surviving lateral must stay Surface::Cylinder (analytic), got {cyl_faces}"
    );

    let mesh = tessellate(&arena, out).expect("union tessellates");
    let vol = mesh_volume(&mesh);
    let cyl_term = PI * R * R * 1.0; // cylinder volume outside the box
    let exact = 1.0 + cyl_term;
    assert!(
        (vol - exact).abs() <= FACET_BAND * cyl_term,
        "union volume {vol} vs analytic {exact} (band {})",
        FACET_BAND * cyl_term
    );
}

// ── 3. blind pocket: box − cylinder ────────────────────────────────────────

/// Subtract a cylinder that enters the box top and stops inside (blind
/// pocket, yr13 class). Genus stays 0; the pocket wall must be an analytic
/// `Surface::Cylinder` face; volume = 1 − πr²·depth.
#[test]
fn boolean_subtract_blind_pocket_volume() {
    let mut arena = BrepArena::new();
    let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);
    let cyl = cylinder(&mut arena, 0.4, 1.1); // z ∈ [0.4, 1.5]: pocket depth 0.6

    let out = boolean_op(&mut arena, bx.solid, cyl.solid, BoolOp::Subtract)
        .expect("box − cylinder blind pocket is a yang-proven configuration (yr13)");
    let report = validate_solid(&arena, out).expect("pocket validates");
    assert_eq!(report.genus, 0, "blind pocket is genus 0");

    let (_, cyl_faces) = face_kinds(&arena, out);
    assert!(cyl_faces >= 1, "pocket wall must stay Surface::Cylinder");

    let mesh = tessellate(&arena, out).expect("pocket tessellates");
    let vol = mesh_volume(&mesh);
    let cyl_term = PI * R * R * 0.6;
    let exact = 1.0 - cyl_term;
    assert!(
        (vol - exact).abs() <= FACET_BAND * cyl_term,
        "pocket volume {vol} vs analytic {exact}"
    );
}

// ── 4. through-hole: box − cylinder ────────────────────────────────────────

/// Subtract a cylinder passing fully through the box (yr14 class): the
/// result is one closed genus-1 shell (χ = V−E+F−R = 0 = 2(S−G)), with a
/// curved tube wall.
#[test]
fn boolean_subtract_through_hole_genus() {
    let mut arena = BrepArena::new();
    let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);
    let cyl = cylinder(&mut arena, -0.5, 2.0);

    let out = boolean_op(&mut arena, bx.solid, cyl.solid, BoolOp::Subtract)
        .expect("box − cylinder through-hole is a yang-proven configuration (yr14)");
    let report = validate_solid(&arena, out).expect("through-hole validates");
    assert_eq!(report.shells, 1, "one closed shell");
    assert_eq!(report.genus, 1, "through-hole is genus 1");
    assert_eq!(report.euler_lhs, 0, "χ = 0 for a torus-like shell");
    assert_eq!(report.euler_rhs, 0);

    let (_, cyl_faces) = face_kinds(&arena, out);
    assert!(cyl_faces >= 1, "tube wall must stay Surface::Cylinder");

    let mesh = tessellate(&arena, out).expect("through-hole tessellates");
    let vol = mesh_volume(&mesh);
    let cyl_term = PI * R * R * 1.0;
    let exact = 1.0 - cyl_term;
    assert!(
        (vol - exact).abs() <= FACET_BAND * cyl_term,
        "through-hole volume {vol} vs analytic {exact}"
    );
}

// ── 5. intersect ───────────────────────────────────────────────────────────

/// box ∩ cylinder (the cylinder portion inside the box): a closed barrel
/// slice — two intersection-circle caps + the lateral.
#[test]
fn boolean_intersect_cylinder_box_volume() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, -0.5, 2.0);
    let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);

    let out = boolean_op(&mut arena, bx.solid, cyl.solid, BoolOp::Intersect)
        .expect("box ∩ cylinder is in the proven cylinder×box class");
    let report = validate_solid(&arena, out).expect("intersection validates");
    assert_eq!(report.genus, 0);

    let mesh = tessellate(&arena, out).expect("intersection tessellates");
    let vol = mesh_volume(&mesh);
    let exact = PI * R * R * 1.0;
    assert!(
        (vol - exact).abs() <= FACET_BAND * exact,
        "intersection volume {vol} vs analytic {exact}"
    );
}

// ── 6. typed walls ─────────────────────────────────────────────────────────

/// PR-KV9 flip (was the named Ellipse output-curve wall): oblique plane ×
/// cylinder sections are ELLIPSES; yang emits exact `Curve::Ellipse` arcs
/// (PR-YR11 relocation) and kernel-v2 now carries the `EllipseArc`
/// vocabulary end-to-end (classification, twin rules, validation,
/// tessellation, closed-form volume terms). The cut plane passes through
/// the cylinder's centroid, so by point symmetry the result is EXACTLY
/// half the cylinder.
#[test]
fn oblique_section_ellipse_subtract_succeeds() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, 0.0, 1.0);
    // Oblique slab: unit normal (1, 2, 2)/3, base plane passing through the
    // cylinder mid — its planar faces cut the lateral in ellipses.
    let n = Vector3::new(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0);
    let s5 = 5.0f64.sqrt();
    let u = Vector3::new(2.0 / s5, -1.0 / s5, 0.0);
    let v = Vector3::new(
        n.y() * u.z() - n.z() * u.y(),
        n.z() * u.x() - n.x() * u.z(),
        n.x() * u.y() - n.y() * u.x(),
    );
    let profile = Profile::new(
        Point3::new(0.5, 0.5, 0.5),
        u,
        v,
        vec![
            Point2::new(-2.0, -2.0),
            Point2::new(2.0, -2.0),
            Point2::new(2.0, 2.0),
            Point2::new(-2.0, 2.0),
        ],
        Vec::new(),
    )
    .expect("oblique rect profile");
    let slab = extrude(&mut arena, &profile, n, 1.0).expect("oblique slab");

    let out = boolean_op(&mut arena, cyl.solid, slab.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("oblique ellipse-section subtract: {e:?}"));
    validate_solid(&arena, out).expect("validates");
    let vol = mesh_volume(&tessellate(&arena, out).expect("tessellate"));
    // The slab's near face passes through the cylinder centroid and its far
    // face clears the cylinder entirely; the cylinder is point-symmetric
    // about its centroid, so the kept region is EXACTLY half:
    let expect = std::f64::consts::PI * 0.25 * 0.25 * 1.0 / 2.0;
    assert!(
        vol <= expect * 1.005 && vol >= 0.93 * expect,
        "half-cylinder volume {vol} vs exact {expect}"
    );
}

/// PR-KV9 flip (was the Stage-3 SSI wall pin): PARALLEL cylinder×cylinder
/// is the analytic special case ssi-rs solves exactly (cross-section
/// circle∩circle → two ruling lines), and Stage-3/4 now carry the
/// propagated membership bands + relocation for cyl×cyl line incidence.
/// The union works end-to-end with CRESCENT (lune) caps — exact lens
/// volume oracle. (The IRREDUCIBLE quartic — skew / unequal non-parallel
/// axes — stays loudly walled; pinned in kv9_cyl_cyl_special.)
#[test]
fn cylinder_cylinder_parallel_union_succeeds() {
    let mut arena = BrepArena::new();
    let c1 = cylinder(&mut arena, -0.5, 2.0);
    // Second cylinder: offset axis, overlapping laterals.
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, -0.7),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.6, 0.5),
        0.3,
    )
    .expect("circle profile");
    let c2 =
        extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.5).expect("second cylinder");

    let out = boolean_op(&mut arena, c1.solid, c2.solid, BoolOp::Union)
        .unwrap_or_else(|e| panic!("parallel cylinder∪cylinder: {e:?}"));
    validate_solid(&arena, out).expect("validates");
    let mesh = tessellate(&arena, out).expect("tessellate");
    let vol = mesh_volume(&mesh);
    // Exact: π·r1²·2.0 + π·r2²·2.5 − lens(r1, r2, d)·overlap_height where
    // the z-overlap is [−0.5, 1.5] (height 2.0) and d = 0.1.
    let (r1, r2, d) = (0.25_f64, 0.3_f64, 0.1_f64);
    let a1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0);
    let a2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0);
    let (t1, t2) = (a1.acos(), a2.acos());
    let lens = r1 * r1 * (t1 - t1.sin() * t1.cos()) + r2 * r2 * (t2 - t2.sin() * t2.cos());
    let expect = std::f64::consts::PI * (r1 * r1 * 2.0 + r2 * r2 * 2.5) - lens * 2.0;
    // Chord under-fill band on the two laterals (not geometric tolerance).
    assert!(
        vol <= expect * 1.001 && vol >= 0.92 * expect,
        "volume {vol} vs exact {expect}"
    );
}

/// PR-KV7 flip (was the typed `UnsupportedCurvedBoolean` re-entry wall):
/// output curve recovery restores B-Rep granularity, so a boolean result
/// carrying cylinder faces re-enters yang Stage 1. This geometry leaves TWO
/// faces on the SAME infinite cylinder (the drill-through stubs above and
/// below the box), exercising the PR-KV7 axial-span tie-break in Stage-6
/// face resolution.
#[test]
fn curved_result_reentry_succeeds() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, -0.5, 2.0);
    let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);
    let out =
        boolean_op(&mut arena, cyl.solid, bx.solid, BoolOp::Union).expect("first union succeeds");

    let bx2 = boxx(&mut arena, 3.0, 3.0, 0.0, 1.0); // disjoint second operand
    let out2 = boolean_op(&mut arena, out, bx2.solid, BoolOp::Union)
        .unwrap_or_else(|e| panic!("recovered curved result re-enters yang Stage 1: {e:?}"));
    validate_solid(&arena, out2).expect("chained result validates");
    // box 1 + stub volume π·0.25²·(0.5 + 0.5) + disjoint box 1, with the
    // chord under-fill band on the stubs only.
    let stub_v = std::f64::consts::PI * 0.25 * 0.25 * 1.0;
    let expect = 2.0 + stub_v;
    let vol = mesh_volume(&tessellate(&arena, out2).expect("tessellate"));
    assert!(
        vol <= expect + 1e-9 && vol >= expect - 0.06 * stub_v,
        "vol {vol} vs {expect}"
    );
}

// ── 7. determinism ─────────────────────────────────────────────────────────

/// Two identical construction+boolean sequences produce bit-identical
/// arenas (the KV determinism contract extends to the curved boolean path).
#[test]
fn curved_boolean_is_deterministic() {
    let run = || -> (BrepArena, kernel_v2::SolidId) {
        let mut arena = BrepArena::new();
        let bx = boxx(&mut arena, 0.0, 0.0, 0.0, 1.0);
        let cyl = cylinder(&mut arena, -0.5, 2.0);
        let out = boolean_op(&mut arena, bx.solid, cyl.solid, BoolOp::Subtract)
            .expect("through-hole subtract");
        (arena, out)
    };
    let (a1, s1) = run();
    let (a2, s2) = run();
    assert_eq!(s1, s2);
    assert_eq!(a1, a2, "curved boolean arenas must be bit-identical");
}

// ── 8. mixed-input conversion sanity ───────────────────────────────────────

/// `to_yang_brep` must handle a solid arena holding BOTH planar and
/// cylinder solids (conversion walks one solid; the other's entities must
/// not leak in), and two cylinders in one arena convert independently.
#[test]
fn to_yang_brep_converts_each_solid_independently() {
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, 0.0, 2.0);
    let bx = boxx(&mut arena, 0.0, 0.0, 5.0, 1.0);

    let yc = to_yang_brep(&arena, cyl.solid).expect("cylinder converts");
    assert_eq!(yc.faces().len(), 3);
    let yb = to_yang_brep(&arena, bx.solid).expect("box converts");
    assert_eq!(yb.faces().len(), 6);
    assert_eq!(yb.vertices().len(), 8);
}
