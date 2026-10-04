//! N77 — seam feet are anchored per CONNECTED COMPONENT of
//! rims-joined-by-bands, not greedily per face.
//!
//! Deviation `docs/yang_deviations.md` N77; ledger `docs/yang_tail_triage.md`
//! (P0019 row). ALWAYS-ON since 2026-10-03 (corpus 324C/0W/22E/5EE/0T over
//! 351, two moves, zero regressions); `YANG_SEAM_COMPONENT=0|off` is the kill
//! switch, under which `recover.rs` decides exactly as PASS 1 / PASS 2
//! always have — the RED half of every pin below runs under it.
//!
//! `recover.rs` canonicalizes a cylinder/cone face whose two loops are both
//! closed circle rims into the 4-edge `[rim, seam, rim, seam]` lateral — the
//! only two-rim form `validate_cone_face` / `validate_cylinder_face` accept.
//! PASS 1 pairs an already azimuth-aligned vertex pair and PASS 2 mints a foot
//! where a rim has none, both PER FACE in face-index order, and PASS 2 will
//! never move an anchor an earlier face fixed. So a band that reaches PASS 2
//! with BOTH of its rims already pinned, a Stage-1 lattice step apart, keeps
//! the ANNULAR form the validators refuse — and nothing in the greedy order
//! prevents that: it happens whenever the pinning faces INTERLEAVE with the
//! unpaired ones along a run of rim-sharing bands.
//!
//! Measured on the assay case P0019 (`convex4:boss nonconvex5:rev
//! convex4:cut`), a genus-1 ring of FIVE cone bands with no planar cap
//! anywhere: faces 1/3/4 pair at |Δaz| = 0 and in doing so pin all five rim
//! anchors; faces 0 and 2 then see |Δaz| = 2.61799387799148686e-1 rad = π/12
//! EXACTLY — one whole lattice step, not noise — and stay annular, so the
//! subtract STOPped with `CurvedGeometryMismatch { face: FaceId(17), reason:
//! "cone face with inner loops is outside the KV6c vocabulary" }`.
//!
//! The remedy is the rule the OTHER copy of this machinery already states —
//! SI5's STEP ingest (`specs/step_import_si5_exact_analytic_ingestion.md`,
//! "Alignment is not pairwise"): two bands sharing a full-circle rim are
//! necessarily COAXIAL, a shared rim being each surface's own rim, so the
//! constraint is per connected component of rims-joined-by-bands — ONE anchor
//! direction per component, chosen once. It is admissible because a closed
//! edge's anchor is pure representation gauge (Stroud's fake edge): sliding it
//! along its own circle changes no boundary point, the loop being the entire
//! circle either way. §4.4.2 likewise restores a B-Rep face from the surfaces
//! and curves it bounds, never from a seam's phase
//! (`refs/text/yang2025_hybrid_boolean.txt:574-605`).
//!
//! ## The fixtures
//!
//! Both are a lathe put through a REAL boolean — an end-shave subtract that
//! trims only the `axial < 0.1` slab — so recover sees a genuine yang Stage-5
//! output with its seams gone. (A strictly AABB-disjoint union will not do:
//! task #134's passthrough skips yang entirely, so recover is never called.)
//!
//! * **the ring** — a 4-edge ALL-OBLIQUE closed profile revolved a full turn
//!   about an external axis: four cone bands sharing rims in a run. This is
//!   the RED→GREEN pin: gate off it raises P0019's wall VERBATIM, gate on it
//!   assembles and validates.
//! * **the barrel** — two cone bands meeting at a shared rim, closed by an
//!   inner cylinder and two annular caps. Gate off it already assembles; the
//!   pin is that the component rule still produces the SAME SOLID (identical
//!   topology census, volume to 1e-9 relative). It is deliberately NOT a
//!   bitwise pin: the component rule chooses a different representational
//!   seam azimuth, so minted feet DO move. What may not move is the geometry.
//!
//! kernel-v2 cannot carry P0019's own five-band shape as a hand-built
//! fixture: `yang_rs::BRep::new` refuses an annular cone band as INPUT ("cone
//! periodic strip (2 encircling rims) not yet supported") while yang's own
//! Stage 5 emits exactly that form, and `BRep`'s fields are `pub(crate)` — so
//! only a real boolean can produce one. That asymmetry is recorded in the
//! deviations ledger; with the component rule on, every band reaching
//! kernel-v2 is seamed, so it has no customer left.
//!
//! Run: `cargo test -p kernel-v2 --release --test n77_component_seam_anchoring`

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::construct::{extrude, revolve};
use kernel_v2::{
    boolean_op, tessellate, validate_solid, BrepArena, KernelV2Error, Profile, RenderMesh, SolidId,
};

/// P0019's wall, verbatim.
const KV6C_ANNULAR_WALL: &str = "cone face with inner loops is outside the KV6c vocabulary";

/// `YANG_SEAM_COMPONENT` is process-global, so each test below holds this lock
/// for its WHOLE body — the gate-OFF half included, which is why it cannot be
/// folded into the `Gate` guard: the other test must not switch the gate on
/// underneath a RED phase.
static GATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    let g = GATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // The rule is on by default; the RED half needs the kill switch.
    std::env::set_var("YANG_SEAM_COMPONENT", "0");
    g
}

struct Gate;
impl Gate {
    fn on() -> Self {
        std::env::remove_var("YANG_SEAM_COMPONENT");
        Gate
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        std::env::set_var("YANG_SEAM_COMPONENT", "0");
    }
}

/// A closed 4-edge profile, EVERY edge oblique to the axis (so every revolved
/// band is a CONE), strictly on the +radial side of it (so the full turn is a
/// genus-1 ring with no planar face at all). CCW in the (axial, radial) frame;
/// area 4, radial centroid 64.5/24 = 2.6875.
fn ring_profile() -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 2.0),
            Point2::new(1.5, 1.5),
            Point2::new(3.0, 3.0),
            Point2::new(1.0, 4.0),
        ],
        vec![],
    )
    .expect("ring profile")
}

/// A "barrel" washer: the outer wall is TWO oblique edges meeting at the rim
/// (axial 1, radial 3/2) — two cone bands sharing a rim with no planar cap
/// use, the N73 shape — closed by an inner cylinder and two annular caps.
/// Area 1.5, radial centroid 8/9.
fn barrel_profile() -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.5),
            Point2::new(2.0, 0.5),
            Point2::new(2.0, 1.0),
            Point2::new(1.0, 1.5),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("barrel profile")
}

fn box_solid(arena: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> SolidId {
    let sq = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .expect("square");
    extrude(arena, &sq, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box")
        .solid
}

/// What the end-shave subtract produces: the topology census and the render
/// mesh.
#[derive(Debug)]
struct Shaved {
    vertices: usize,
    edges: usize,
    faces: usize,
    rings: usize,
    mesh: RenderMesh,
}

/// Revolve `profile` a full turn about the x-axis, then subtract the slab
/// `axial < 0.1`. The subtract is what strips the lathe's seams: yang
/// re-emits each surviving band as two closed rims, which is the form recover
/// has to canonicalize.
fn end_shave(profile: &Profile) -> Result<Shaved, KernelV2Error> {
    let mut arena = BrepArena::new();
    let lathe = revolve(
        &mut arena,
        profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * std::f64::consts::PI,
    )
    .expect("full-turn revolve of the lathe profile")
    .solid;
    validate_solid(&arena, lathe).expect("the lathe validates before the boolean");
    let shave = box_solid(&mut arena, (-1.0, 0.1), (-9.0, 9.0), (-9.0, 9.0));
    let out = boolean_op(&mut arena, lathe, shave, BoolOp::Subtract)?;
    let r = validate_solid(&arena, out)?;
    let mesh = tessellate(&arena, out)?;
    Ok(Shaved {
        vertices: r.vertices,
        edges: r.edges,
        faces: r.faces,
        rings: r.rings,
        mesh,
    })
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

/// Pappus for the full turn minus the shaved slab, in closed form.
/// Ring: `2π·2.6875·4 = 21.5π`; the slab `t ∈ [0, 0.1]` spans radially
/// `[2 − t/3, 2 + 2t]`, so it removes `π∫₀^0.1 ((28/3)t + (35/9)t²) dt`.
fn ring_exact_volume() -> f64 {
    let pi = std::f64::consts::PI;
    21.5 * pi - pi * ((28.0 / 3.0) * 0.005 + (35.0 / 9.0) * (0.001 / 3.0))
}

/// Barrel: `2π·(8/9)·1.5 = (8/3)π`; the slab spans radially `[0.5, 1 + t/2]`,
/// removing `π∫₀^0.1 (0.75 + t + t²/4) dt`.
fn barrel_exact_volume() -> f64 {
    let pi = std::f64::consts::PI;
    (8.0 / 3.0) * pi - pi * (0.75 * 0.1 + 0.005 + 0.001 / 12.0)
}

/// RED→GREEN. Four cone bands sharing rims in a run: PASS 1 pairs faces 0 and
/// 2 and pins four rim anchors, face 1 then arrives with BOTH of its rims
/// fixed |Δaz| = 2.51327412287182916e-1 rad apart and keeps the annular form,
/// and the assembler raises P0019's wall verbatim. With one azimuth per rim
/// component every band is seamed and the solid assembles.
#[test]
fn a_rim_run_that_defeats_the_greedy_order_converts_under_component_anchoring() {
    let _serialized = serialize();
    // RED: the greedy order leaves an annular cone band.
    let err = end_shave(&ring_profile()).expect_err(
        "without component anchoring the interleaved rim run must leave a cone \
         band ANNULAR — if this now succeeds the greedy order was changed and \
         the pin below no longer proves anything",
    );
    let text = format!("{err:?}");
    assert!(
        text.contains(KV6C_ANNULAR_WALL),
        "the greedy order must fail with P0019's own wall, not something else; got {text}"
    );

    // GREEN: one seam azimuth per component.
    let on = {
        let _g = Gate::on();
        end_shave(&ring_profile()).expect("component anchoring must seam every band")
    };
    // Every band seamed ⇒ no cone face carries an inner loop. The lathe has no
    // planar face except the shaved annular cap, whose inner rim is the one
    // ring the solid is allowed to have.
    assert_eq!(
        on.rings, 1,
        "the only inner loop left is the shaved annular cap's inner rim; a cone \
         band still in the annular form would add one and the assembler would \
         have refused it"
    );
    assert!(
        on.vertices > 0 && on.edges > 0 && on.faces >= 5,
        "the converted ring keeps its four cone bands plus the shaved cap: \
         V={} E={} F={}",
        on.vertices,
        on.edges,
        on.faces
    );
    let v = render_volume(&on.mesh);
    let exact = ring_exact_volume();
    assert!(
        v <= exact && v >= exact * 0.97,
        "the ring's faceted volume {v:.9e} must sit just BELOW the closed-form \
         {exact:.9e} (an inscribed lathe under-reports by its chord deficit)"
    );
}

/// The no-regression half: a rim CHAIN the greedy order already handles must
/// still produce the SAME SOLID with the component rule on. Not bitwise — the
/// component rule picks a different representational seam azimuth, so minted
/// feet move by construction — but the census and the geometry must not.
#[test]
fn a_rim_chain_the_greedy_order_already_handles_keeps_its_solid() {
    let _serialized = serialize();
    let off = end_shave(&barrel_profile()).expect("the barrel assembles without the gate");
    let on = {
        let _g = Gate::on();
        end_shave(&barrel_profile()).expect("the barrel assembles with the gate")
    };
    assert_eq!(
        (off.vertices, off.edges, off.faces, off.rings),
        (on.vertices, on.edges, on.faces, on.rings),
        "the barrel's topology census moved with the gate: V/E/F/R {:?} -> {:?}",
        (off.vertices, off.edges, off.faces, off.rings),
        (on.vertices, on.edges, on.faces, on.rings)
    );
    let (vo, vn) = (render_volume(&off.mesh), render_volume(&on.mesh));
    assert!(
        (vo - vn).abs() <= 1e-9 * vo.max(1.0),
        "the barrel's volume moved with the gate: {vo:.17e} -> {vn:.17e}"
    );
    let exact = barrel_exact_volume();
    assert!(
        vn <= exact && vn >= exact * 0.97,
        "the barrel's faceted volume {vn:.9e} must sit just below the \
         closed-form {exact:.9e}"
    );
}
