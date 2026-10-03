//! N72 (P0016): a CONE band with no `Curve::Circle` rim of its own reads the
//! OWNER's Stage-1 chord budget back, instead of calling its own absence a
//! producer fault.
//!
//! The defect. `stage1_tessellate` sizes every curved chart against one
//! operand-level value — `curved_chord_bound`, else `ellipse_rim_chord_bound`
//! — whose own comment there names it "the operand's chord budget **as Stage
//! 3/4 read it back**". Two arms read it back: Stage 3's
//! `chord_tol_for_curved_owner` (cylinder/sphere selection band) and Stage 4's
//! `input_curved_chord_bound`. The two CONE arms did not: both
//! `cone_chord_tol_for_owner` (Stage-3 selection) and
//! `cone_chord_budget_from_owner` (Stage-4 cone-ellipse relocation) demanded a
//! `Curve::Circle` rim on the band's own face and, failing to find one, called
//! it a producer fault — a loud STOP on a band the operand demonstrably
//! carries.
//!
//! A cone patch re-entering from a PRIOR boolean is bounded by conic chains
//! alone. P0016 measured exactly that: its cone-owning operand's edge census
//! was 2 ellipses + 6 hyperbolas + 10 segments, **zero** circles. The Stage-3
//! arm STOPped first, reporting `AmbiguousCurve { candidates: 0, matched: 0 }`
//! — which is not an ambiguity (no candidate was proposed because
//! `ssi_rs::intersect` was never reached) and not a producer fault either;
//! with that arm fixed the op STOPped again at the Stage-4 twin
//! (`LocalRefinementRequired`). One defect, two error texts: fixing one arm
//! alone only silences the twin.
//!
//! The fixture is a cone PATCH bounded by two oblique `cone ∩ plane` section
//! ellipses and two rulings — P0016's operand shape with the smallest possible
//! topology. It is built TOPOLOGY-ONLY ([`topology_only`]): the real article
//! comes out of a boolean, which a unit test cannot run, and the conic-bounded
//! cone shapes Stage 1 accepts from hand-authored topology do not include it
//! (a single encircling conic rim is refused outright, two is the deferred
//! "KV14 Slice E holed frustum band" sub-slice). Every helper under test takes
//! only `faces()` / `edges()` / `vertices()`, so the topology arrays are the
//! whole input and the donor's mesh is never consulted.
//!
//! The ellipses are closed-form (see [`oblique_section_ellipse`]), so every
//! expected bound is computed here from the same single sources the production
//! code reads, never from a recorded output.

use super::*;
use crate::stage3_ssi::cone_chord_tol_for_owner;
use crate::stage4_relocate::cone_chord_budget_from_owner;
use crate::{
    cone_band_chord_bound, curved_chord_bound, ellipse_chord_bound, ellipse_rim_chord_bound,
    owner_stage1_chord_budget,
};

const ALPHA: f64 = std::f64::consts::FRAC_PI_6; // cone half-angle 30°
const APEX: [f64; 3] = [0.0, 0.0, 0.0];
const AXIS: [f64; 3] = [0.0, 0.0, 1.0];

fn cone_surface() -> Surface {
    Surface::Cone {
        apex: Point3::new(APEX[0], APEX[1], APEX[2]),
        axis_dir: Vector3::new(AXIS[0], AXIS[1], AXIS[2]),
        half_angle: ALPHA,
    }
}

/// The exact `cone ∩ plane` ellipse for the cone (apex at the origin, axis
/// +z, half-angle [`ALPHA`]) and the plane tilted `beta` from the
/// axis-perpendicular that meets the axis at `z = h`. Requires
/// `|tan α · tan β| < 1` (a closed ellipse, not a parabola or hyperbola).
///
/// Derived, not recorded. In the plane of symmetry `y = 0` the cone's
/// generators are `x = ± z·tan α` and the plane is `z = h + x·tan β`, so the
/// two major-axis endpoints are at `z = h / (1 ∓ tan α·tan β)` with
/// `x = ± z·tan α`. The centre is their midpoint, the major radius half their
/// separation, and the minor radius is the cone's own radius at the centre's
/// height reduced by its in-plane offset: `b = √((z_c·tan α)² − x_c²)`.
/// Returns the curve plus the plane's unit upward normal.
fn oblique_section_ellipse(beta: f64, h: f64) -> (Curve, Vector3, Point3, f64) {
    let (ta, tb) = (ALPHA.tan(), beta.tan());
    assert!((ta * tb).abs() < 1.0, "fixture: the section must be closed");
    let (z_a, z_b) = (h / (1.0 - ta * tb), h / (1.0 + ta * tb));
    let (x_a, x_b) = (z_a * ta, -z_b * ta);
    let center = Point3::new((x_a + x_b) / 2.0, 0.0, (z_a + z_b) / 2.0);
    let (dx, dz) = (x_a - x_b, z_a - z_b);
    let span = (dx * dx + dz * dz).sqrt();
    let major_radius = span / 2.0;
    let major_axis = Vector3::new(dx / span, 0.0, dz / span);
    let r_at_center = center.z() * ta;
    let minor_radius = (r_at_center * r_at_center - center.x() * center.x()).sqrt();
    let n = Vector3::new(-beta.sin(), 0.0, beta.cos());
    let seam = Point3::new(
        center.x() + major_radius * major_axis.x(),
        center.y() + major_radius * major_axis.y(),
        center.z() + major_radius * major_axis.z(),
    );
    (
        Curve::Ellipse {
            center,
            normal: n,
            major_axis,
            major_radius,
            minor_radius,
        },
        n,
        seam,
        major_radius,
    )
}

/// The cone PATCH re-entry shape, as a single-face B-Rep: one `Surface::Cone`
/// face whose outer loop is [ellipse arc, ruling, ellipse arc, ruling] —
/// bounded between two OBLIQUE section planes (tilts +20° at `z = 10` and
/// −15° at `z = 20`, which do not cross over the patch) and between two
/// azimuths. No `Curve::Circle` exists anywhere, which is the whole point.
///
/// Built through [`topology_only`] — see the module header for why it cannot
/// go through `BRep::new`.
///
/// Returns the B-Rep and the LARGER section's major radius — the value
/// `ellipse_rim_chord_bound` maximizes to over the two rims.
fn conic_bounded_cone_patch() -> (BRep, f64) {
    let deg = |d: f64| d * std::f64::consts::PI / 180.0;
    let (beta_lo, h_lo) = (deg(20.0), 10.0);
    let (beta_hi, h_hi) = (deg(-15.0), 20.0);
    let (c_lo, _, _, a_lo) = oblique_section_ellipse(beta_lo, h_lo);
    let (c_hi, _, _, a_hi) = oblique_section_ellipse(beta_hi, h_hi);
    let ta = ALPHA.tan();
    // The generator at azimuth `theta` meets the plane `n̂·x = h·cos β` at
    // `z = h·cos β / (cos β − sin β·tan α·cos θ)`: substitute the on-cone
    // point `(z·tan α·cos θ, z·tan α·sin θ, z)` into the plane equation.
    let on_plane = |beta: f64, h: f64, theta: f64| -> Point3 {
        let z = h * beta.cos() / (beta.cos() - beta.sin() * ta * theta.cos());
        assert!(
            z > 0.0,
            "fixture: the generator must meet the plane ahead of the apex"
        );
        Point3::new(z * ta * theta.cos(), z * ta * theta.sin(), z)
    };
    let (t0, t1) = (0.6, 1.5);
    let verts = vec![
        BRepVertex {
            point: on_plane(beta_lo, h_lo, t0),
        },
        BRepVertex {
            point: on_plane(beta_lo, h_lo, t1),
        },
        BRepVertex {
            point: on_plane(beta_hi, h_hi, t1),
        },
        BRepVertex {
            point: on_plane(beta_hi, h_hi, t0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: c_lo,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 3,
            curve: c_hi,
        },
        BRepEdge {
            start: 3,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: cone_surface(),
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: vec![],
        reversed: false,
    }];
    (topology_only(verts, edges, faces), a_lo.max(a_hi))
}

/// A B-Rep carrying exactly the given TOPOLOGY, with a throwaway donor's
/// tessellation left in place. Used only for the pure chord-bound readbacks,
/// each of which takes `faces()` / `edges()` / `vertices()` and nothing else;
/// a boolean-output cone patch cannot be re-tessellated from hand-authored
/// topology (see the module header), and this fixture does not pretend to be
/// one — it is the input half of those functions, exactly.
fn topology_only(verts: Vec<BRepVertex>, edges: Vec<BRepEdge>, faces: Vec<BRepFace>) -> BRep {
    let mut brep = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    brep.vertices = verts;
    brep.edges = edges;
    brep.faces = faces;
    brep
}

/// An axis-aligned box: an owner with NO curved rim of any kind. The producer
/// fault must SURVIVE for it (P9 — never invent a curved band). Same
/// construction as `n178_subres_coplanar::box_brep`.
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let ([x0, y0, z0], [x1, y1, z1]) = (lo, hi);
    let pt = |x: f64, y: f64, z: f64| BRepVertex {
        point: Point3::new(x, y, z),
    };
    let verts = vec![
        pt(x0, y0, z0),
        pt(x1, y0, z0),
        pt(x1, y1, z0),
        pt(x0, y1, z0),
        pt(x0, y0, z1),
        pt(x1, y0, z1),
        pt(x1, y1, z1),
        pt(x0, y1, z1),
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let mut edges = Vec::new();
    let mut loops = Vec::new();
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [z0, -z1, y0, -x1, -y1, x0];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).expect("box is valid topology")
}

/// The PREMISE of the defect, measured on the fixture rather than assumed: the
/// cone band carries no `Curve::Circle` rim, so its per-band bound
/// (`cone_band_chord_bound`, the N38 single source) resolves NOTHING — and
/// neither does the circle-rim AABB the cylinder arm's first rung reads. What
/// the operand DOES carry is the ellipse chain bound.
#[test]
fn n71_conic_bounded_cone_band_has_no_own_rim_but_the_owner_has_a_budget() {
    let (brep, major_radius) = conic_bounded_cone_patch();
    let cone = cone_surface();
    assert!(
        !brep
            .edges()
            .iter()
            .any(|e| matches!(e.curve, Curve::Circle { .. })),
        "fixture invalid: the conic-bounded band must carry no Circle edge"
    );
    assert_eq!(
        cone_band_chord_bound(cone, brep.faces(), brep.edges()),
        None,
        "the band has no Circle rim, so the per-band (N38) bound resolves nothing"
    );
    assert_eq!(
        curved_chord_bound(brep.edges()),
        None,
        "no Circle rim ⇒ no rim-AABB bound either"
    );
    let expected = ellipse_chord_bound(major_radius);
    assert_eq!(
        ellipse_rim_chord_bound(brep.edges()),
        Some(expected),
        "the ellipse chain bound at the section's own major radius IS the budget"
    );
    assert_eq!(
        owner_stage1_chord_budget(&brep),
        Some(expected),
        "the owner-level ladder resolves to that same single source"
    );
}

/// RED→GREEN, Stage-3 arm: `cone_chord_tol_for_owner` on a band-less cone
/// owner used to return `Err(AmbiguousCurve { candidates: 0, matched: 0 })`.
/// It now returns the owner's Stage-1 budget — the sag that operand's
/// tessellation actually carries, from the one source Stage 1 sized it with.
#[test]
fn n71_stage3_cone_selection_band_reads_the_owner_budget_back() {
    let (brep, major_radius) = conic_bounded_cone_patch();
    let cone = cone_surface();
    let other = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let got = cone_chord_tol_for_owner(cone, InputId::A, &brep, &other, 0, (6, 13))
        .expect("N72: a conic-bounded cone band resolves a selection band");
    assert_eq!(got, ellipse_chord_bound(major_radius));
    // The same owner reached as input B resolves identically (the arm picks
    // the owner by `InputId`, not by position).
    let got_b = cone_chord_tol_for_owner(cone, InputId::B, &other, &brep, 0, (6, 13))
        .expect("N72: same band, owner reached as B");
    assert_eq!(got_b, got);
}

/// RED→GREEN, Stage-4 arm (the twin): `cone_chord_budget_from_owner` used to
/// return `None` for the same owner, and its caller raised
/// `LocalRefinementRequired`. P0016 STOPped there the moment the Stage-3 arm
/// was fixed, which is why both arms are pinned.
#[test]
fn n71_stage4_cone_relocation_budget_reads_the_owner_budget_back() {
    let (brep, major_radius) = conic_bounded_cone_patch();
    let got = cone_chord_budget_from_owner(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        ALPHA,
        &brep,
    )
    .expect("N72: a conic-bounded cone owner resolves a relocation budget");
    assert_eq!(got, ellipse_chord_bound(major_radius));
}

/// The loud wall SURVIVES where there is genuinely nothing to read back: an
/// all-planar owner carries no chord error at all, so inventing a band for it
/// would be the `TAU_WORK` default P9/P10 forbid. Both arms must still refuse.
#[test]
fn n71_an_all_planar_owner_keeps_the_loud_producer_fault() {
    let boxy = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let other = box_brep([2.0, 0.0, 0.0], [3.0, 1.0, 1.0]);
    assert_eq!(owner_stage1_chord_budget(&boxy), None);
    let cone = cone_surface();
    let err = cone_chord_tol_for_owner(cone, InputId::A, &boxy, &other, 3, (6, 13))
        .expect_err("an all-planar owner has no curved band to read back");
    assert!(
        matches!(
            err,
            YangError::SsiRefinementFailed {
                edge: (6, 13),
                reason: SsiRefinementError::AmbiguousCurve {
                    candidates: 3,
                    matched: 0
                }
            }
        ),
        "the producer fault must stay LOUD and carry its edge/candidates: {err:?}"
    );
    assert_eq!(
        cone_chord_budget_from_owner(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            ALPHA,
            &boxy,
        ),
        None,
        "the Stage-4 twin must refuse too"
    );
}

/// A cone band that DOES carry its own `Curve::Circle` rim is untouched: the
/// per-band N38 value wins over the owner ladder, so every circle-rimmed cone
/// case — the multi-band gear revolve N38 was written for included — keeps the
/// bound it had. Mutation guard: the fixture's owner ladder resolves a
/// DIFFERENT number, so a fix that preferred the fallback would fail here.
#[test]
fn n71_a_circle_rimmed_cone_band_still_takes_its_own_per_band_bound() {
    // A solid cone: apex at the origin, axis +z, base rim circle at z = H.
    const H: f64 = 10.0;
    let cone = cone_surface();
    let radius = H * ALPHA.tan();
    let verts = vec![
        BRepVertex {
            point: Point3::new(radius, 0.0, H),
        },
        BRepVertex {
            point: Point3::new(0.0, 0.0, 0.0),
        },
    ];
    let edges = vec![BRepEdge {
        start: 0,
        end: 0,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, H),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius,
        },
    }];
    let faces = vec![
        BRepFace {
            surface: cone,
            outer_loop: vec![0],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -H,
            },
            outer_loop: vec![0],
            inner_loops: vec![],
            reversed: false,
        },
    ];
    let brep = BRep::new(verts, edges, faces).expect("solid cone is valid topology");
    let per_band = cone_band_chord_bound(cone, brep.faces(), brep.edges())
        .expect("a circle-rimmed band resolves its own bound");
    let other = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    assert_eq!(
        cone_chord_tol_for_owner(cone, InputId::A, &brep, &other, 0, (0, 1)).unwrap(),
        per_band,
        "the per-band bound must win wherever it resolves (N38 unchanged)"
    );
    assert_eq!(
        cone_chord_budget_from_owner(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            ALPHA,
            &brep,
        ),
        Some(per_band),
        "the Stage-4 twin keeps its rim-Circle derivation too"
    );
    assert_ne!(
        owner_stage1_chord_budget(&brep),
        Some(per_band),
        "guard: the owner ladder reads a DIFFERENT number here, so preferring \
         the fallback would be observable"
    );
}
