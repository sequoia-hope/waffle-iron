//! Stage 3 — analytical SSI refinement of intersection edges
//! (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 6).

#[allow(clippy::wildcard_imports)]
use crate::*;

// =========================================================================
// PR-YR9 (P3) — Stage 3: analytical SSI refinement of intersection edges
// =========================================================================

/// A position-keyed undirected edge set: sorted pairs of coordinate bit
/// triples (see [`pos_key`]). The producer's per-EDGE intersection
/// provenance (`LabeledArrangement::intersection_edges`) travels to Stage 3
/// in this form — POSITION-keyed through the weld so it survives the
/// compaction between the arrangement and the Stage-3/4 mesh (the
/// `minted_junction_keys` bit-pattern precedent). Position keys are valid
/// only BEFORE Stage-4 relocation moves vertices (spec
/// `specs/yang_s3_intersection_edge_provenance.md`, inc-2 measurement), so
/// only the FIRST `compute_phase_a` pass receives a non-empty set; every
/// post-relocation recompute passes [`NO_EDGE_PROVENANCE`] and keeps
/// today's geometric-gate behavior.
pub(crate) type PosKeyedEdgeSet = std::collections::BTreeSet<([u64; 3], [u64; 3])>;

/// The empty provenance set for call sites where provenance is unavailable
/// or (post-relocation) no longer position-valid.
pub(crate) static NO_EDGE_PROVENANCE: PosKeyedEdgeSet = PosKeyedEdgeSet::new();

/// Position bit-key of a mesh vertex for the provenance lookup.
pub(crate) fn pos_key(p: Point3) -> [u64; 3] {
    let a = p.as_array();
    [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
}

// The inc-2 provenance-first classification (spec
// `specs/yang_s3_intersection_edge_provenance.md` §3b/§3c) is ALWAYS-ON as
// of 2026-07-30: it engages exactly when the producer supplied per-edge
// provenance (`!edge_provenance.is_empty()`), so provenance-less producers
// (the sidecar parity oracle, hand-built fixtures) keep the historical
// geometric-gate behavior byte-identically. Flip measurement, back-to-back
// full-corpus runs: OFF 257C/0W/53E/0T byte-identical; ON 258C/0W/52E/0T —
// exactly one category delta (F0083 ERROR→CORRECT), zero CORRECT→ERROR,
// plus four already-ERROR cases advancing to later ops (F0082 3→1 failing
// ops, F0085 2→1).

/// PR-YR9: convert a yang `Surface` into the analytical `ssi_rs::QuadricSurface`
/// for Stage-3 SSI (spec §5.2).
///
/// `Surface::Plane { normal, d }` uses the convention `n·x + d = 0`, while
/// `QuadricSurface::Plane` is `n·(x − point) = 0`, so a point on the plane is
/// `point = -d · n` (with `n` the stored unit normal). `Cylinder`, `Sphere`,
/// and `Cone` map field-for-field (PR-YR15 wires `Sphere`, enabling the exact
/// `plane ∩ sphere` great-circle rim; PR-YR17 wires `Cone`, enabling the exact
/// `plane ∩ cone` perpendicular-cut `Circle` rim via the `ssi_rs` `plane_cone`
/// C1 branch).
pub(crate) fn surface_to_quadric(s: Surface) -> Result<ssi_rs::QuadricSurface, SsiRefinementError> {
    match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            Ok(ssi_rs::QuadricSurface::Plane {
                point: Point3::new(-d * n[0], -d * n[1], -d * n[2]),
                normal,
            })
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => Ok(ssi_rs::QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        }),
        Surface::Sphere { center, radius } => Ok(ssi_rs::QuadricSurface::Sphere { center, radius }),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => Ok(ssi_rs::QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        }),
        // A torus is a DEGREE-4 surface, not a quadric — its SSI refinement is
        // out of the quadric-solver vocabulary (KV6d boolean increment).
        Surface::Torus { .. } => Err(SsiRefinementError::UnsupportedSurfaceForSsi),
    }
}

/// M5 (Y1): map an `ssi_rs::QuadricSurface` back to a yang `Surface`, the
/// inverse of `surface_to_quadric` for the operands of a `SurfacePair` curve.
/// The M5 producers are cyl×cyl and the cone-pair arms (cyl×cone, cone×cone), so
/// `Cylinder` and `Cone` operands map field-for-field. `Plane`/`Sphere` cannot
/// appear as a surface-pair operand (no producer emits them) and reject loudly
/// (P9 — a `Plane`/`Sphere` here would be a producer fault, not a curve).
pub(crate) fn quadric_to_surface(q: ssi_rs::QuadricSurface) -> Result<Surface, SsiRefinementError> {
    match q {
        ssi_rs::QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => Ok(Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        }),
        ssi_rs::QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => Ok(Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        }),
        // F10 (design review 2026-07-12): the sphere×cylinder / sphere×cone
        // general-position arms now emit a `SurfacePair` with a `Sphere`
        // operand, so this conversion must carry it through to the yang
        // `Surface::Sphere` (kernel-v2 has the matching `PairSurface::Sphere`,
        // and Stage-4 relocation has the sphere gradient arm). Without this
        // the F10 promotion would only move the wall from ssi-rs's ASNA to
        // here.
        ssi_rs::QuadricSurface::Sphere { center, radius } => Ok(Surface::Sphere { center, radius }),
        // No producer emits a bare `Plane` as a surface-pair operand (a
        // plane section is always a conic, never a degree-4 pair).
        ssi_rs::QuadricSurface::Plane { .. } => Err(SsiRefinementError::UnsupportedSurfaceForSsi),
    }
}

/// PR-YR9: convert an `ssi_rs::SsiCurve` into a yang `Curve` (spec §5.3).
/// `Circle`/`Ellipse` map field-for-field; `Line` becomes `LineSegment`
/// (the edge's endpoints trim it). `Parabola`/`Hyperbola` cannot arise for the
/// Cylinder∩Plane pair and reject loudly (P9, defensive).
pub(crate) fn ssi_curve_to_curve(c: ssi_rs::SsiCurve) -> Result<Curve, SsiRefinementError> {
    match c {
        ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        } => Ok(Curve::Circle {
            center,
            normal,
            radius,
        }),
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => Ok(Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        }),
        ssi_rs::SsiCurve::Line { .. } => Ok(Curve::LineSegment),
        // PR-YR22: the θ=α cone∩plane section is a Parabola (the single-candidate
        // conic). Map field-for-field.
        ssi_rs::SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => Ok(Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        }),
        // PR-YR23: the axis-parallel (HYPE) cone∩plane section returns TWO
        // Hyperbola candidates (one per nappe). Map field-for-field; the
        // two-branch selection falls out of `curve_contains_point`'s `u > 0`
        // discriminator in `build_intersection_curves`.
        ssi_rs::SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => Ok(Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        }),
        // M5 (Y1): the general degree-4 cyl×cyl curve — carry both operands
        // verbatim as a procedural surface-pair curve.
        ssi_rs::SsiCurve::SurfacePair { a, b } => Ok(Curve::SurfacePair {
            a: quadric_to_surface(a)?,
            b: quadric_to_surface(b)?,
        }),
    }
}

/// PR-YR9: implicit on-curve test (spec §5.4) — does point `p` lie within `tol`
/// of curve `c`? No parameter solving; uses the curve's implicit residual.
/// `tol` is supplied by the caller (the Stage-1 chord bound `d_ε`); no ad-hoc
/// epsilon is introduced. `Parabola`/`Hyperbola` always return `false`.
///
/// PR-YR19 (spec §2/§4): `source_radius` carries the originating sphere radius
/// `R` for a sphere section `Circle`, so the in-plane RADIAL band is scaled by
/// the propagated factor `(R / r_circle)` (the projection of the surface-normal
/// chord error `d_ε` onto the section plane — see spec §2's
/// `dr ≈ (R/r_c)·d_sphere`). The AXIAL (out-of-plane) band stays the unscaled
/// `tol` (the cut plane is exact). `source_radius = None` (every non-sphere
/// path: cylinder / cone / plane) is BYTE-IDENTICAL to the old flat-`tol`
/// behavior. A near-tangent section (`r_circle ≤ MIN_FEATURE_SIZE`) fails closed
/// (keeps the unscaled band) so the factor cannot blow up.
pub(crate) fn curve_contains_point(
    c: &ssi_rs::SsiCurve,
    p: Point3,
    tol: f64,
    source_radius: Option<f64>,
) -> bool {
    let x = p.as_array();
    match c {
        ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = normalize3(normal.as_array());
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let axial = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            let radial_vec = [
                w[0] - axial * n[0],
                w[1] - axial * n[1],
                w[2] - axial * n[2],
            ];
            let radial = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            let radial_tol = match source_radius {
                Some(big_r) if *radius > cad_primitives::MIN_FEATURE_SIZE => {
                    (big_r / *radius) * tol
                }
                _ => tol,
            };
            axial.abs() <= tol && (radial - radius).abs() <= radial_tol
        }
        ssi_rs::SsiCurve::Line { point, dir } => {
            let d = normalize3(dir.as_array());
            let pt = point.as_array();
            let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
            let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
            let perp = [
                w[0] - along * d[0],
                w[1] - along * d[1],
                w[2] - along * d[2],
            ];
            (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= tol
        }
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let min_axis = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
            let v = w[0] * min_axis[0] + w[1] * min_axis[1] + w[2] * min_axis[2];
            let residual = ((u / major_radius).powi(2) + (v / minor_radius).powi(2)).sqrt() - 1.0;
            residual.abs() * major_radius.min(*minor_radius) <= tol
        }
        ssi_rs::SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            // PR-YR22: in-plane implicit membership `y² = 4f·x` for the θ=α
            // cone∩plane parabola. Out-of-plane reject first (the cut plane is
            // exact), then the in-plane relation.
            let n = normalize3(normal.as_array());
            let ax = normalize3(axis_dir.as_array());
            let conj = [
                n[1] * ax[2] - n[2] * ax[1],
                n[2] * ax[0] - n[0] * ax[2],
                n[0] * ax[1] - n[1] * ax[0],
            ];
            let vtx = vertex.as_array();
            let w = [x[0] - vtx[0], x[1] - vtx[1], x[2] - vtx[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let px = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            let py = w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2];
            // The implicit residual `y² − 4f·x` has units length². Convert it to
            // a perpendicular distance (length) by dividing by the in-plane
            // gradient magnitude `|∇(y²−4f·x)| = |(−4f, 2y)| = 2√(4f²+y²)` —
            // the parabola analog of the Ellipse arm's residual→length scaling.
            // Compare that geometric residual against the cone chord band `tol`.
            let implicit = (py * py - 4.0 * focal_length * px).abs();
            let grad = 2.0 * (4.0 * focal_length * focal_length + py * py).sqrt();
            let geo_res = if grad > cad_primitives::MIN_FEATURE_SIZE {
                implicit / grad
            } else {
                implicit
            };
            geo_res <= tol
        }
        ssi_rs::SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            // PR-YR23: in-plane implicit membership `(u/a)² − (v/b)² = 1` for the
            // axis-parallel (HYPE) cone∩plane hyperbola, AND the branch
            // discriminator `u > 0` (the OTHER nappe's branch — opposite
            // major_axis — gives u < 0 here and is rejected, so matched == 1).
            // Out-of-plane reject first (the cut plane is exact), then the
            // in-plane relation + branch test.
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let conj = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let a = *semi_transverse;
            let b = *semi_conjugate;
            let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
            let v = w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2];
            // The implicit residual `F = (u/a)² − (v/b)² − 1` is dimensionless.
            // Convert it to a perpendicular distance (length) by dividing by the
            // in-plane gradient magnitude `|∇F| = |(2u/a², −2v/b²)|` — the
            // hyperbola analog of the Ellipse/Parabola arms' residual→length
            // scaling (NOT a flat widening). Compare against the cone chord band
            // `tol`.
            let implicit = ((u / a).powi(2) - (v / b).powi(2) - 1.0).abs();
            let gu = 2.0 * u / (a * a);
            let gv = 2.0 * v / (b * b);
            let grad = (gu * gu + gv * gv).sqrt();
            let geo_res = if grad > cad_primitives::MIN_FEATURE_SIZE {
                implicit / grad
            } else {
                implicit
            };
            geo_res <= tol && u > 0.0
        }
        // M5 (Y2): membership on a procedural surface-pair curve is the
        // per-point on-BOTH-surfaces test — the point lies within `tol` of the
        // implicit residual of EACH defining surface (the curve IS the common
        // zero set). The producers are cyl×cyl and the cone-pair arms; a
        // Plane/Sphere operand cannot arise and fails closed (defensive; there
        // is no curve to be on).
        ssi_rs::SsiCurve::SurfacePair { a, b } => {
            let quadric_residual = |q: &ssi_rs::QuadricSurface| -> Option<f64> {
                match q {
                    ssi_rs::QuadricSurface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                    } => {
                        let ap = axis_point.as_array();
                        let ad = normalize3(axis_dir.as_array());
                        let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
                        let along = w[0] * ad[0] + w[1] * ad[1] + w[2] * ad[2];
                        let perp = [
                            w[0] - along * ad[0],
                            w[1] - along * ad[1],
                            w[2] - along * ad[2],
                        ];
                        let r = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
                        Some((r - radius).abs())
                    }
                    // Cone: |radial − |h|·tanα| (the signed_distance_to_surface
                    // form; the double-nappe implicit whose zero set is the cone).
                    ssi_rs::QuadricSurface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                    } => {
                        let ap = apex.as_array();
                        let ad = normalize3(axis_dir.as_array());
                        let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
                        let h = w[0] * ad[0] + w[1] * ad[1] + w[2] * ad[2];
                        let perp = [w[0] - h * ad[0], w[1] - h * ad[1], w[2] - h * ad[2]];
                        let r = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
                        Some((r - h.abs() * half_angle.tan()).abs())
                    }
                    _ => None,
                }
            };
            match (quadric_residual(a), quadric_residual(b)) {
                (Some(ra), Some(rb)) => ra <= tol && rb <= tol,
                _ => false,
            }
        }
    }
}

/// Selection tolerance for a CYLINDER-owning intersection edge: the cylinder
/// input's Stage-1 chord bound via `curved_chord_bound` (the SINGLE source for
/// the cylinder band). A cylinder-bearing input with NO circle rims is a
/// producer fault → LOUD `AmbiguousCurve { matched: 0 }` (never silently
/// default to `TAU_WORK` for a curved selection). Factored out of
/// `build_intersection_curves` (PR-YR15) so the sphere arm can sit beside it
/// without duplicating the producer-fault path; sphere uses its OWN
/// `sphere_chord_bound` (2r√3), not this cylinder/rim-AABB band.
pub(crate) fn chord_tol_for_curved_owner(
    input: InputId,
    a: &BRep,
    b: &BRep,
    candidates: usize,
    edge: (u32, u32),
) -> Result<f64, YangError> {
    let owner = match input {
        InputId::A => a,
        InputId::B => b,
    };
    match curved_chord_bound(owner.edges()) {
        Some(t) => Ok(t),
        // Spec `yang_s3_ellipse_rim_chord_bound` T2: an owner with NO Circle
        // rim but ellipse rims (obliquely-trimmed cylinder re-entering from a
        // prior boolean, KV14 vocabulary) gets the Stage-1 ellipse-chain
        // bound — the guarantee its samples actually carry, not a widening.
        None => match ellipse_rim_chord_bound(owner.edges()) {
            Some(t) => Ok(t),
            None => {
                // Stage-3 diagnosis probe (read-only, env-gated): the producer-
                // fault census — a curved-owning edge whose owner B-Rep carries
                // NO Circle or Ellipse rim. Prints the owner's censuses.
                if std::env::var_os("YANG_S3_AMBIG_PROBE").is_some() {
                    let mut surf_census: std::collections::BTreeMap<&'static str, usize> =
                        std::collections::BTreeMap::new();
                    for f in owner.faces() {
                        let k = match f.surface {
                            Surface::Plane { .. } => "plane",
                            Surface::Cylinder { .. } => "cylinder",
                            Surface::Cone { .. } => "cone",
                            Surface::Sphere { .. } => "sphere",
                            Surface::Torus { .. } => "torus",
                        };
                        *surf_census.entry(k).or_default() += 1;
                    }
                    let mut curve_census: std::collections::BTreeMap<&'static str, usize> =
                        std::collections::BTreeMap::new();
                    for e in owner.edges() {
                        let k = match e.curve {
                            Curve::LineSegment => "seg",
                            Curve::Circle { .. } => "circle",
                            Curve::Ellipse { .. } => "ellipse",
                            Curve::Parabola { .. } => "parabola",
                            Curve::Hyperbola { .. } => "hyperbola",
                            Curve::SurfacePair { .. } => "surface-pair",
                        };
                        *curve_census.entry(k).or_default() += 1;
                    }
                    eprintln!(
                        "[s3-ambig-probe] PRODUCER FAULT edge {edge:?}: cylinder-owning input \
                     {input:?} has NO Circle or Ellipse rim; faces {surf_census:?} edges \
                     {curve_census:?}"
                    );
                }
                Err(YangError::SsiRefinementFailed {
                    edge,
                    reason: SsiRefinementError::AmbiguousCurve {
                        candidates,
                        matched: 0,
                    },
                })
            }
        },
    }
}

/// PR-YR17: selection tolerance for a CONE-owning intersection edge. A cone
/// edge is a `plane ∩ cone` cut whose exact `ssi_rs` curve is a `Circle` (⊥
/// section) or `Ellipse` (oblique section); the mesh endpoints sit on the
/// cone's Stage-1 chord approximation, off that exact curve by up to the
/// cone's Stage-1 chord bound (A14.3 single source — the SAME bound Stage 1
/// guarantees, NOT tolerance widening).
///
/// **N38 fix:** the band is the EDGE's OWN cone band's `cone_chord_bound`
/// (`cone_band_chord_bound`, matched by exact `Surface`), max-height rim. The
/// pre-fix code paired the edge band's apex/half_angle with an ARBITRARY first
/// cone face's rim; on a multi-band gear revolve that mixes one band's apex
/// with another's rim → a nonsense height → a too-tight band that UNDERESTIMATES
/// the band the edge actually lies on, raising a spurious `AmbiguousCurve` on
/// legitimate chord-error endpoints (R0003). Every single-cone case stays
/// byte-identical (the matched face is the only cone face). A cone-bearing
/// input with NO rim Circle is a producer fault → LOUD
/// `AmbiguousCurve { matched: 0 }` (never silently default to `TAU_WORK` for a
/// curved selection), mirroring `chord_tol_for_curved_owner`.
pub(crate) fn cone_chord_tol_for_owner(
    cone_surface: Surface,
    input: InputId,
    a: &BRep,
    b: &BRep,
    candidates: usize,
    edge: (u32, u32),
) -> Result<f64, YangError> {
    let owner = match input {
        InputId::A => a,
        InputId::B => b,
    };
    match cone_band_chord_bound(cone_surface, owner.faces(), owner.edges()) {
        Some(t) => Ok(t),
        // Not a cone surface, or the cone band carries no `Curve::Circle` rim
        // → producer fault (never silently default to `TAU_WORK`).
        None => Err(YangError::SsiRefinementFailed {
            edge,
            reason: SsiRefinementError::AmbiguousCurve {
                candidates,
                matched: 0,
            },
        }),
    }
}

/// PR-YR9: build the EXACT analytical `Curve` for each output intersection edge
/// (spec §5.5). An intersection edge is an undirected mesh boundary edge whose
/// incidence list has EXACTLY TWO entries with DIFFERENT `InputId` — it lies on
/// one surface of input A and one of input B.
///
/// For each such edge: convert both surfaces to `QuadricSurface`, call
/// `ssi_rs::intersect`, derive the selection tolerance `tol` from the
/// CURVED-owning input's Stage-1 chord bound (cylinder via `curved_chord_bound`,
/// sphere via `sphere_chord_bound` — PR-YR15, cone via `cone_chord_tol_for_owner`
/// — PR-YR17), and select the UNIQUE returned curve passing through BOTH mesh
/// endpoints within `tol`. `matched != 1` is a
/// P9/P10 LOUD stop (`AmbiguousCurve`) — never a silent polyline fallback.
///
/// Plane∩Plane edges yield a `Line` → `LineSegment` (equal to the caller's
/// fallback, so the planar corpus is unchanged); their `tol` is `TAU_WORK`
/// (a plane∩plane line has zero chord error).
pub(crate) fn build_intersection_curves(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    mesh: &Mesh,
    a: &BRep,
    b: &BRep,
    edge_provenance: &PosKeyedEdgeSet,
) -> Result<std::collections::BTreeMap<(u32, u32), Curve>, YangError> {
    let mut out: std::collections::BTreeMap<(u32, u32), Curve> = std::collections::BTreeMap::new();
    // Producer per-EDGE provenance (spec §3b): a CONFIRMED edge is one the
    // arrangement itself minted as an intersection constraint. Behavior
    // changes only behind `edge_provenance_enabled()`; the
    // `YANG_S3_PROVENANCE_PROBE` reporting observes either way.
    let probe_on = std::env::var_os("YANG_S3_PROVENANCE_PROBE").is_some();
    let prov_enabled = !edge_provenance.is_empty();
    let prov_of = |s: u32, e: u32| -> Option<bool> {
        if edge_provenance.is_empty() {
            return None;
        }
        let (ka, kb) = (
            pos_key(mesh.verts[s as usize]),
            pos_key(mesh.verts[e as usize]),
        );
        Some(edge_provenance.contains(&(ka.min(kb), ka.max(kb))))
    };
    let mut probe_counts = [0usize; 6]; // [seen, confirmed, c_len, c_same_input, c_onboth, c_other]
    for (&(s, e), entries) in incidence {
        let prov = prov_of(s, e);
        if probe_on {
            probe_counts[0] += 1;
            if prov == Some(true) {
                probe_counts[1] += 1;
            }
        }
        if entries.len() != 2 {
            if probe_on && prov == Some(true) {
                probe_counts[2] += 1;
                eprintln!(
                    "YANG_S3_PROV confirmed-SKIP site=len edge=({s},{e}) n_entries={}",
                    entries.len()
                );
            }
            continue;
        }
        let (input0, surf0) = entries[0];
        let (input1, surf1) = entries[1];
        if input0 == input1 {
            if probe_on && prov == Some(true) {
                probe_counts[3] += 1;
                eprintln!(
                    "YANG_S3_PROV confirmed-SKIP site=same_input edge=({s},{e}) surf={surf0:?}"
                );
            }
            continue;
        }

        // Selection tolerance: the Stage-1 chord bound of the CURVED-owning
        // input (A14.3 single source). The mesh edge endpoints sit on the
        // curved surface's Stage-1 chord approximation, off the EXACT analytic
        // curve by up to that surface's own chord bound — so the on-curve test
        // must admit them at that bound (the SAME bound Stage 1 guarantees, NOT
        // tolerance widening). Plane∩Plane → no curved surface → TAU_WORK
        // (zero chord error). PR-YR15 extends the cylinder-only logic to a
        // SPHERE edge: a sphere uses its OWN bound `sphere_chord_bound(radius)`
        // (2r√3), NOT the rim-AABB `curved_chord_bound` (2r√2, which would
        // underestimate — I-sphere-band).
        //
        // PR-YR18: `tol` is computed FIRST (before `surface_to_quadric` /
        // `ssi_rs::intersect`) so it can drive the on-both-surfaces gate below.
        // The producer-fault helpers' `candidates` argument is diagnostic-only
        // (untested); in this pre-intersect position we have no `returned.len()`
        // yet, so we pass `0`.
        // PR-YR19: alongside `tol`, derive `source_radius` — `Some(R)` ONLY for
        // a sphere-owning edge, so `curve_contains_point` scales the section
        // `Circle`'s in-plane radial band by the propagated factor `(R/r_c)`
        // (spec §2). Cylinder / cone / plane arms keep `None` (byte-identical to
        // the pre-YR19 flat-band membership test).
        let (tol, source_radius): (f64, Option<f64>) = if matches!(surf0, Surface::Cylinder { .. })
            && matches!(surf1, Surface::Cylinder { .. })
        {
            // PR-KV9: cylinder × cylinder edge — the arrangement vertex sits
            // on the crossing of BOTH meshes' facet chords, off the exact
            // curve by up to the SUM of the two inputs' own Stage-1 chord
            // bounds (each endpoint is within its owner's band of its
            // surface; the crossing inherits both). The sum is the derived
            // combined bound, not widening.
            (
                chord_tol_for_curved_owner(input0, a, b, 0, (s, e))?
                    + chord_tol_for_curved_owner(input1, a, b, 0, (s, e))?,
                None,
            )
        } else if matches!(surf0, Surface::Cylinder { .. }) {
            (chord_tol_for_curved_owner(input0, a, b, 0, (s, e))?, None)
        } else if matches!(surf1, Surface::Cylinder { .. }) {
            (chord_tol_for_curved_owner(input1, a, b, 0, (s, e))?, None)
        } else if let Surface::Sphere { radius, .. } = surf0 {
            (sphere_chord_bound(radius), Some(radius))
        } else if let Surface::Sphere { radius, .. } = surf1 {
            (sphere_chord_bound(radius), Some(radius))
        } else if matches!(surf0, Surface::Cone { .. }) {
            (
                cone_chord_tol_for_owner(surf0, input0, a, b, 0, (s, e))?,
                None,
            )
        } else if matches!(surf1, Surface::Cone { .. }) {
            (
                cone_chord_tol_for_owner(surf1, input1, a, b, 0, (s, e))?,
                None,
            )
        } else {
            (cad_primitives::TAU_WORK, None)
        };

        let p_s = mesh.verts[s as usize];
        let p_e = mesh.verts[e as usize];

        // PR-YR18 (spec §2/§3): on-both-surfaces gate. An edge handed to
        // `ssi_rs::intersect` as a `(surf0, surf1)` intersection edge must have
        // BOTH endpoints on BOTH attributed surfaces within the edge's Stage-1
        // chord band `tol`. `compute_phase_a` pushes a patch's single inherited
        // surface onto every boundary edge of the patch cycle, so a seam edge
        // can be tagged `(surfA, surfB)` while one endpoint is genuinely off one
        // surface — that is a single-surface internal edge, NOT a true
        // intersection edge. Skip it (→ `Curve::LineSegment` fallback in
        // `emit_topology`) before it reaches the SSI. Reuses the SAME `tol` the
        // selection uses (no widening): the intersection curve lies ON both
        // surfaces, so every edge that currently selects `matched == 1`
        // necessarily passes this gate — it can only reclassify edges that today
        // raise `AmbiguousCurve` with an endpoint off a surface beyond `tol`.
        let on_both = |pt: Point3| -> Result<bool, YangError> {
            Ok(signed_distance_to_surface(surf0, pt)?.abs() <= tol
                && signed_distance_to_surface(surf1, pt)?.abs() <= tol)
        };
        let s_on = on_both(p_s)?;
        let e_on = on_both(p_e)?;
        // Provenance override (spec §3b, gated): a producer-CONFIRMED edge is
        // an intersection edge by construction; the on-both gate must not
        // veto it because of the very endpoint drift Stage-4 relocation
        // exists to fix. It proceeds in WITNESS mode: the curve is selected
        // through the endpoint(s) that ARE on both surfaces, and the drifted
        // endpoint becomes a Stage-4 relocation obligation onto that curve.
        // A confirmed edge with NO witness still reaches the loud
        // `AmbiguousCurve` below (P9 — never silently guess).
        let overridden = prov_enabled && prov == Some(true) && !(s_on && e_on);
        if !(s_on && e_on) && !overridden {
            if let Ok(list) = std::env::var("YANG_V_PROBE") {
                if list
                    .split(',')
                    .any(|t| t.trim().parse::<u32>() == Ok(s) || t.trim().parse::<u32>() == Ok(e))
                {
                    // §16 tangent test: at a point on BOTH surfaces the
                    // intersection curve's tangent is exactly n0 x n1 (no SSI
                    // needed). A TRUE intersection edge is a chord of that
                    // curve, so its direction matches the tangent; a
                    // misclassified internal edge runs away from it.
                    for (wname, w) in [("s", p_s), ("e", p_e)] {
                        let wa = w.as_array();
                        let scale = wa[0].abs().max(wa[1].abs()).max(wa[2].abs()).max(1.0);
                        let eps = cad_primitives::TAU_WORK * scale;
                        if signed_distance_to_surface(surf0, w)?.abs() > eps
                            || signed_distance_to_surface(surf1, w)?.abs() > eps
                        {
                            continue; // not a witness
                        }
                        let (n0, n1) = (
                            crate::stage4_relocate::surface_value_and_normal(surf0, wa),
                            crate::stage4_relocate::surface_value_and_normal(surf1, wa),
                        );
                        if let (Some((_, a0)), Some((_, a1))) = (n0, n1) {
                            let t = [
                                a0[1] * a1[2] - a0[2] * a1[1],
                                a0[2] * a1[0] - a0[0] * a1[2],
                                a0[0] * a1[1] - a0[1] * a1[0],
                            ];
                            let tl = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
                            let (sa, ea) = (p_s.as_array(), p_e.as_array());
                            let d = [ea[0] - sa[0], ea[1] - sa[1], ea[2] - sa[2]];
                            let dl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                            if tl > 0.0 && dl > 0.0 {
                                let cos = ((t[0] * d[0] + t[1] * d[1] + t[2] * d[2]) / (tl * dl))
                                    .clamp(-1.0, 1.0);
                                let ang = cos.abs().acos().to_degrees();
                                eprintln!(
                                    "YANG_V_PROBE tangent-test edge ({s},{e}) witness={wname} \
                                     angle(edge, n0xn1)={ang:.6}deg edge_len={dl:.3e}"
                                );
                            }
                        }
                    }
                    eprintln!(
                        "YANG_V_PROBE on-both gate SKIP edge ({s},{e}) tol={tol:.3e} \
                         surf0={surf0:?} surf1={surf1:?} \
                         d_s=({:.3e},{:.3e}) d_e=({:.3e},{:.3e})",
                        signed_distance_to_surface(surf0, p_s)?.abs(),
                        signed_distance_to_surface(surf1, p_s)?.abs(),
                        signed_distance_to_surface(surf0, p_e)?.abs(),
                        signed_distance_to_surface(surf1, p_e)?.abs(),
                    );
                }
            }
            if probe_on && prov == Some(true) {
                probe_counts[4] += 1;
                eprintln!(
                    "YANG_S3_PROV confirmed-SKIP site=on_both edge=({s},{e}) tol={tol:.3e} \
                     d_s=({:.3e},{:.3e}) d_e=({:.3e},{:.3e})",
                    signed_distance_to_surface(surf0, p_s)?.abs(),
                    signed_distance_to_surface(surf1, p_s)?.abs(),
                    signed_distance_to_surface(surf0, p_e)?.abs(),
                    signed_distance_to_surface(surf1, p_e)?.abs(),
                );
            }
            continue;
        }
        if probe_on && prov == Some(false) {
            // Past the gate with NO producer provenance: either a plane∩plane
            // seam (legitimately curve-bearing without a constraint — the
            // §4.5.5 overlay route), or a gate admission the producer would
            // refuse. Count under "other" and report for the census.
            probe_counts[5] += 1;
            eprintln!("YANG_S3_PROV unconfirmed-ADMIT edge=({s},{e}) surfs=({surf0:?},{surf1:?})");
        }
        if probe_on && overridden {
            eprintln!(
                "YANG_S3_PROV override-ADMIT edge=({s},{e}) witness=({s_on},{e_on}) tol={tol:.3e} \
                 d_s=({:.3e},{:.3e}) d_e=({:.3e},{:.3e})",
                signed_distance_to_surface(surf0, p_s)?.abs(),
                signed_distance_to_surface(surf1, p_s)?.abs(),
                signed_distance_to_surface(surf0, p_e)?.abs(),
                signed_distance_to_surface(surf1, p_e)?.abs(),
            );
        }

        // Plane∩Plane: the curve is the unique line through the two (gate-
        // verified on-both-planes) endpoints — `Curve::LineSegment`, exact,
        // zero chord error. This short-circuit is byte-equivalent to the
        // SSI route for TRANSVERSAL planes (ssi returns the Line, which
        // maps to `LineSegment`) and is REQUIRED for the §4.5.5 coplanar
        // seams (PR-YR26): the boundary of a trimmed common planar surface
        // is an intersection curve between two COINCIDENT planes ("The
        // boundaries of the common surface are regarded as intersection
        // curves between the two models"), where `ssi_rs::intersect`
        // correctly refuses the parallel-plane pair (`DegenerateInput`) —
        // the curve comes from the 2D overlay, not from SSI.
        if matches!(surf0, Surface::Plane { .. }) && matches!(surf1, Surface::Plane { .. }) {
            out.insert((s, e), Curve::LineSegment);
            continue;
        }

        if cylinders_are_coincident(surf0, surf1, tol) {
            // PR-5: COINCIDENT cylinders (the §4.5.5 membrane analog for a coaxial
            // flange wall == gear bore, `err.waffle`). `ssi_rs::intersect`
            // correctly refuses an identical-quadric pair (`DegenerateInput`):
            // the two cylinders do not intersect transversally, so the edge
            // curve does NOT come from SSI but from the overlap boundary on the
            // shared cylinder — exactly the coincident-PLANE case above.
            //
            // Every such edge (rim, generator, or interior tessellation chord)
            // is left to the `Curve::LineSegment` fallback in `emit_topology`
            // (the mesh chord — the analog of the plane case handing every seam
            // edge a straight segment, refinement-free). Emitting a rim
            // `Curve::Circle` here would instead mark its endpoints for Stage-4
            // relocation onto the analytic circle, collapsing adjacent
            // membrane-region triangles (`DegenerateTriangle`): those vertices
            // already sit on the shared cylinder's exact chord rim (the lateral
            // tessellation put them there) and need no relocation. It is NEVER
            // sent to the degenerate SSI (which would be a loud `DegenerateInput`).
            continue;
        }

        // KV6d Tier B: a TORUS intersection edge is degree-4 — there is no
        // analytic SSI curve (`surface_to_quadric` refuses a torus). Leave it as
        // the `Curve::LineSegment` fallback (the `emit_topology` default); Stage
        // 4 relocates its endpoints onto the exact torus∩surface curve via the
        // implicit-pair Newton (`relocate_onto_implicit_pair`/`_triple`).
        if matches!(surf0, Surface::Torus { .. }) || matches!(surf1, Surface::Torus { .. }) {
            continue;
        }

        let q0 = surface_to_quadric(surf0).map_err(|reason| YangError::SsiRefinementFailed {
            edge: (s, e),
            reason,
        })?;
        let q1 = surface_to_quadric(surf1).map_err(|reason| YangError::SsiRefinementFailed {
            edge: (s, e),
            reason,
        })?;

        let returned =
            ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                edge: (s, e),
                reason: SsiRefinementError::IntersectFailed(err),
            })?;

        // PR-F3b: a `Line` candidate's membership band carries the propagated
        // factor `r/√(r² − d²)` (the radial chord deficit measured in the
        // cutting plane's in-plane metric) — same derivation as the PR-YR19
        // sphere section circle's `(R/r_c)` scaling. Conic candidates keep
        // the unscaled `tol` byte-for-byte.
        let line_amp = line_band_amplification(surf0, surf1);
        // N46 (task #164): a `cylinder ∩ plane` generator LINE uses the EXACT
        // worst-case band `√(B_in² + tol²)` (the concave radial→perpendicular
        // metric), superseding `line_amp`'s first-order tangent estimate which
        // UNDER-admits near tangency (R0026's spurious `AmbiguousCurve{2,0}`).
        // `None` (non-cyl/plane, plane-misses, or merged-generator tangency)
        // falls back to the `line_amp` path unchanged. Scoped here so the
        // cyl∩cyl Steinmetz and cone-apex line paths keep their current band.
        let cyl_plane_gen_band = cyl_plane_generator_band(surf0, surf1, tol);
        // PR-KV9: cylinder×cylinder pairs carry the per-point gradient
        // amplification (membership measured against the band intersection
        // of BOTH surfaces; diverges at surface tangency — the Steinmetz
        // crossing points — where the tangent-direction discriminator below
        // takes over).
        let cyl_pair: Option<((Point3, Vector3), (Point3, Vector3))> = match (surf0, surf1) {
            (
                Surface::Cylinder {
                    axis_point: p1,
                    axis_dir: d1,
                    ..
                },
                Surface::Cylinder {
                    axis_point: p2,
                    axis_dir: d2,
                    ..
                },
            ) => Some(((p1, d1), (p2, d2))),
            _ => None,
        };
        // N39 (task #161): a CONE∩PLANE conic (ellipse ⊥/oblique section,
        // hyperbola for a plane ∥ axis, parabola for a plane ∥ generator)
        // carries the per-point gradient-angle amplification `1/sin α` between
        // the cone normal and the plane normal — the conic analog of the
        // `cyl_cyl` factor. A grazing (small-α) cut places the mesh chord point
        // legitimately further from the exact curve than the raw cone chord
        // sagitta; without this factor the flat band under-admits and Stage-3
        // raises a spurious `AmbiguousCurve`. `None` (apex singularity /
        // tangency) keeps the flat band + tangent discriminator.
        let cone_plane: Option<(Surface, Surface)> = match (surf0, surf1) {
            (c @ Surface::Cone { .. }, p @ Surface::Plane { .. })
            | (p @ Surface::Plane { .. }, c @ Surface::Cone { .. }) => Some((c, p)),
            _ => None,
        };
        let point_tol = |x: Point3, curve: &ssi_rs::SsiCurve| -> f64 {
            match curve {
                ssi_rs::SsiCurve::Line { .. } => {
                    cyl_plane_gen_band.unwrap_or_else(|| line_amp.map_or(tol, |a| a * tol))
                }
                ssi_rs::SsiCurve::Ellipse { .. }
                | ssi_rs::SsiCurve::Hyperbola { .. }
                | ssi_rs::SsiCurve::Parabola { .. } => {
                    if let Some((c1, c2)) = cyl_pair {
                        // Steinmetz ellipse: two-cylinder radial amplification.
                        cyl_cyl_point_amplification(x, c1, c2).map_or(f64::INFINITY, |a| a * tol)
                    } else if let Some((cone, plane)) = cone_plane {
                        surface_pair_point_amplification(x, cone, plane).map_or(tol, |a| a * tol)
                    } else {
                        tol
                    }
                }
                _ => tol,
            }
        };
        // Witness-aware selection points (spec §3c): in provenance-override
        // mode only the endpoint(s) verified on both surfaces vouch for the
        // curve — the drifted endpoint cannot (its position is the defect) —
        // and it becomes a Stage-4 relocation obligation onto the selected
        // curve. Off-override this is exactly the historical both-endpoint
        // rule. An overridden edge with NO witness keeps both points, so no
        // candidate matches and the loud `AmbiguousCurve` below reports it.
        let sel_both = [p_s, p_e];
        let sel_s = [p_s];
        let sel_e = [p_e];
        let sel_pts: &[Point3] = if !overridden {
            &sel_both
        } else if s_on {
            &sel_s
        } else if e_on {
            &sel_e
        } else {
            &sel_both
        };
        let mut matched_idx: Option<usize> = None;
        let mut matched = 0usize;
        for (i, curve) in returned.iter().enumerate() {
            if sel_pts
                .iter()
                .all(|&p| curve_contains_point(curve, p, point_tol(p, curve), source_radius))
            {
                matched += 1;
                matched_idx = Some(i);
            }
        }

        // PR-KV9: tangent-direction discrimination for multi-matches. Two
        // curves through one region (the Steinmetz ellipses near their
        // crossing) CROSS transversally, so the mesh edge's direction
        // aligns with exactly one curve's tangent. Selected only with a
        // clear margin; otherwise the loud ambiguity stands (P9 — a
        // tie-break, never a band widening).
        // All three tie-break blocks below are BOTH-endpoint machinery
        // (their re-tests and their geometric premises assume two on-curve
        // endpoints), so a provenance-overridden edge never enters them —
        // a witness multi-match stays a loud `AmbiguousCurve` (P9).
        if matched > 1 && !overridden {
            let edge_dir = {
                let d = [p_e.x() - p_s.x(), p_e.y() - p_s.y(), p_e.z() - p_s.z()];
                normalize3(d)
            };
            let mid = Point3::new(
                (p_s.x() + p_e.x()) / 2.0,
                (p_s.y() + p_e.y()) / 2.0,
                (p_s.z() + p_e.z()) / 2.0,
            );
            let mut scored: Vec<(f64, usize)> = Vec::new();
            for (i, curve) in returned.iter().enumerate() {
                if !(curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                    && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius))
                {
                    continue;
                }
                if let Some(t) = curve_tangent_at(curve, mid) {
                    let c = (t[0] * edge_dir[0] + t[1] * edge_dir[1] + t[2] * edge_dir[2]).abs();
                    scored.push((c, i));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            if scored.len() >= 2 && scored[0].0 > scored[1].0 + 0.1 {
                matched = 1;
                matched_idx = Some(scored[0].1);
            }
        }

        // R0072: POSITION tie-break for near-coincident PARALLEL-line matches.
        // A near-tangent `plane ∩ cylinder` secant returns two near-coincident
        // parallel generators; both pass `curve_contains_point` (the `line_amp`
        // near-tangency band inflation admits both), and the tangent pass above
        // cannot separate them — parallel lines share a direction, so the
        // cosine margin never fires. But the mesh edge lies on exactly ONE
        // generator, which is nearer to BOTH endpoints. Select the candidate
        // whose endpoint-distance interval lies strictly below every other
        // matched candidate's (`hi_w < lo_j ∀ j≠w`): the winner's WORST endpoint
        // still beats every rival's BEST, so the endpoints unambiguously lie on
        // it. Margin-free and scale-free. If the intervals overlap (generators
        // merged below mesh resolution — true-tangency territory), no candidate
        // qualifies and the loud `AmbiguousCurve` stands (P9 — a proximity
        // tie-break on geometry the on-both gate already verified, never a band
        // widening). Spec: `specs/yr_r0072_parallel_line_position_tiebreak.md`.
        if matched > 1 && !overridden {
            // Collect the matched candidates that are lines, paired with their
            // `returned` index; bail if any matched candidate is NOT a line (a
            // mixed line/conic multi-match is not the parallel-generator case).
            let mut cands: Vec<(Point3, Vector3)> = Vec::new();
            let mut cand_idx: Vec<usize> = Vec::new();
            let mut all_matched_are_lines = true;
            for (i, curve) in returned.iter().enumerate() {
                if !(curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                    && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius))
                {
                    continue;
                }
                if let ssi_rs::SsiCurve::Line { point, dir } = curve {
                    cands.push((*point, *dir));
                    cand_idx.push(i);
                } else {
                    all_matched_are_lines = false;
                    break;
                }
            }
            if all_matched_are_lines && cands.len() == matched {
                if let Some(wk) = select_disjoint_parallel_line(&cands, p_s, p_e) {
                    matched = 1;
                    matched_idx = Some(cand_idx[wk]);
                }
            }
        }

        // R0008: POSITION tie-break for CROSSING generator lines. A cutting
        // plane through a cone's APEX sections it into TWO generator lines that
        // CROSS at the apex — NOT parallel (so the R0072 block above bails), and
        // when the mesh edge lies near the apex-plane, both generators are
        // nearly aligned with the edge (so the tangent discriminator's 0.1
        // cosine margin never fires). The edge lies on exactly ONE generator;
        // the other is admitted only by the large cone chord band `tol`
        // (R0008: a near-flat 88.95° cone, tol ≈ 2.81). The same disjoint
        // perpendicular-distance interval criterion as R0072 selects it — sound
        // for crossing lines because the endpoints lie on the true generator to
        // chord accuracy (perp dist ~0) while the false one is a full band away
        // (~2.6). Overlapping intervals (edge sitting AT the apex crossing) →
        // the loud `AmbiguousCurve` stands (P9 — a proximity tie-break on
        // geometry the on-both gate already verified, never a band widening).
        // Spec `specs/yr_r0008_cone_apex_generators.md`.
        if matched > 1 && !overridden {
            let mut cands: Vec<(Point3, Vector3)> = Vec::new();
            let mut cand_idx: Vec<usize> = Vec::new();
            let mut all_matched_are_lines = true;
            for (i, curve) in returned.iter().enumerate() {
                if !(curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                    && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius))
                {
                    continue;
                }
                if let ssi_rs::SsiCurve::Line { point, dir } = curve {
                    cands.push((*point, *dir));
                    cand_idx.push(i);
                } else {
                    all_matched_are_lines = false;
                    break;
                }
            }
            if all_matched_are_lines && cands.len() == matched {
                if let Some(wk) = select_disjoint_line_by_distance(&cands, p_s, p_e) {
                    matched = 1;
                    matched_idx = Some(cand_idx[wk]);
                }
            }
        }

        let idx = match (matched, matched_idx) {
            (1, Some(idx)) => idx,
            // Provenance-confirmed edge with NO witness endpoint (both
            // endpoints drifted — the chain-interior case, e.g. F0083's
            // (80,118) joining two mis-seated vertices): no containment test
            // can anchor the selection, but with exactly ONE candidate there
            // is nothing to be ambiguous about — the producer vouches the
            // edge lies on this pair's intersection, and this is that
            // intersection's only curve. Both endpoints become Stage-4
            // relocation obligations onto it. Multi-candidate no-witness
            // stays loud (P9 — never guess between branches).
            (0, None) if overridden && !s_on && !e_on && returned.len() == 1 => {
                if probe_on {
                    eprintln!("YANG_S3_PROV no-witness-single-candidate edge=({s},{e}) selected");
                }
                0
            }
            _ => {
                // Stage-3 diagnosis probe (read-only, env-gated): full selector
                // context at the loud stop — surfaces, band, candidates, and
                // per-endpoint membership — for the AmbiguousCurve class census.
                if std::env::var_os("YANG_S3_AMBIG_PROBE").is_some() {
                    eprintln!(
                        "[s3-ambig-probe] edge ({s},{e}) candidates={} matched={matched} \
                         tol={tol:.3e}\n  surf0={surf0:?}\n  surf1={surf1:?}\n  p_s=({},{},{}) \
                         p_e=({},{},{})",
                        returned.len(),
                        p_s.x(),
                        p_s.y(),
                        p_s.z(),
                        p_e.x(),
                        p_e.y(),
                        p_e.z()
                    );
                    for (i, curve) in returned.iter().enumerate() {
                        eprintln!(
                            "  cand {i}: contains(s)={} contains(e)={} tol_s={:.3e} tol_e={:.3e} {curve:?}",
                            curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius),
                            curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius),
                            point_tol(p_s, curve),
                            point_tol(p_e, curve)
                        );
                    }
                }
                return Err(YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason: SsiRefinementError::AmbiguousCurve {
                        candidates: returned.len(),
                        matched,
                    },
                });
            }
        };
        let curve =
            ssi_curve_to_curve(returned[idx]).map_err(|reason| YangError::SsiRefinementFailed {
                edge: (s, e),
                reason,
            })?;
        if std::env::var_os("YANG_S3_ELLIPSE_PROBE").is_some() {
            if let Curve::Ellipse {
                center,
                normal,
                major_radius,
                minor_radius,
                ..
            } = curve
            {
                eprintln!(
                    "[s3-ellipse] edge ({s},{e}) center={:?} n={:?} a={major_radius:.6} \
                     b={minor_radius:.6}\n  surf0={surf0:?}\n  surf1={surf1:?}",
                    center.as_array(),
                    normal.as_array(),
                );
            }
        }
        out.insert((s, e), curve);
    }
    if probe_on {
        let have_set = !edge_provenance.is_empty();
        eprintln!(
            "YANG_S3_PROV summary have_set={have_set} edges_seen={} confirmed={} \
             confirmed_skip_len={} confirmed_skip_same_input={} confirmed_skip_on_both={} \
             unconfirmed_admit={} curves_built={}",
            probe_counts[0],
            probe_counts[1],
            probe_counts[2],
            probe_counts[3],
            probe_counts[4],
            probe_counts[5],
            out.len(),
        );
    }
    Ok(out)
}
