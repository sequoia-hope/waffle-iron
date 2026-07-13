//! yang-output edge → `EdgeKind` classification and surface mapping for the
//! from_yang assembler (move-only F9 split from `from_yang.rs`; byte-identical).
//! Per-curve endpoint certification bands and the surface-pair operand mapping.
//! See `super`'s module docs and `super::keys` for the target vocabulary.

use super::*;

/// Classify one yang output edge into the KV5b vocabulary, applying the
/// named-curve walls and the minor-arc sense derivation (module docs).
/// `from`/`to` are the loop's TRAVERSAL endpoints (the stored
/// `(start, end)` or its reverse — see the loop walk in
/// [`from_yang_brep`]); the derived arc direction is for that traversal.
pub(crate) fn classify_edge(
    e: &yang_rs::BRepEdge,
    yverts: &[yang_rs::BRepVertex],
    from: u32,
    to: u32,
) -> Result<EdgeKind, KernelV2Error> {
    match e.curve {
        yang_rs::Curve::LineSegment => {
            if e.start == e.end {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "degenerate output edge (start == end)",
                ));
            }
            Ok(EdgeKind::Seg)
        }
        yang_rs::Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = normal.as_array();
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge with a non-positive radius",
                ));
            }
            if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge normal is not unit-length",
                ));
            }
            if e.start == e.end {
                return Ok(EdgeKind::Full {
                    center,
                    normal: n,
                    radius,
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            // Endpoints on the circle (f64-construction allowance — the
            // relocated vertices are computed in closed form).
            for p in [ps, pe] {
                let d = sub(p, center);
                let on_plane = dot3(d, n);
                let radial = (dot3(d, d) - on_plane * on_plane).max(0.0).sqrt();
                let band = cad_primitives::TAU_EVAL
                    * (1.0 + radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if (radial - radius).abs() > band || on_plane.abs() > band {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output arc endpoint does not lie on its circle",
                    ));
                }
            }
            let Some(sweep) = crate::geom::ccw_sweep(center, n, ps, pe) else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output arc endpoint has no radial direction",
                ));
            };
            let pi = std::f64::consts::PI;
            if (sweep - pi).abs() <= ARC_MINOR_AMBIGUITY_BAND {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "near-half-circle arc (minor side ambiguous)",
                });
            }
            let forward_normal = if sweep < pi { n } else { [-n[0], -n[1], -n[2]] };
            Ok(EdgeKind::Arc {
                center,
                forward_normal,
                radius,
            })
        }
        yang_rs::Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            // PR-KV9: the exact oblique-section piece. Same minor-side
            // derivation as circles, in the PARAMETRIC frame: each
            // arrangement mesh edge subtends ≈ one Stage-1 facet, far below
            // π; a near-half sweep is rejected loudly rather than guessed.
            let n = normalize3_arr(normal.as_array());
            let m = normalize3_arr(major_axis.as_array());
            if !(major_radius.is_finite()
                && minor_radius.is_finite()
                && major_radius > 0.0
                && minor_radius > 0.0)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output ellipse edge with non-positive radii",
                ));
            }
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "full Ellipse (no producer constructs closed ellipse edges)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            // Endpoints on the ellipse (import band, in-plane residual
            // scaled by the minor radius, out-of-plane direct).
            for p in [ps, pe] {
                let d = sub(p, center);
                let out_of_plane = dot3(d, n);
                let w = [
                    n[1] * m[2] - n[2] * m[1],
                    n[2] * m[0] - n[0] * m[2],
                    n[0] * m[1] - n[1] * m[0],
                ];
                let u = dot3(d, m) / major_radius;
                let v = dot3(d, w) / minor_radius;
                let band = cad_primitives::TAU_EVAL
                    * (1.0 + major_radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if out_of_plane.abs() > band || (u.hypot(v) - 1.0).abs() * minor_radius > band {
                    if std::env::var("KV_ELLIPSE_PROBE").is_ok() {
                        eprintln!(
                            "KV_ELLIPSE_PROBE reject: from={from} to={to} start={} end={} \
                             p={p:?} center={center:?} n={n:?} m={m:?} \
                             a={major_radius:.17e} b={minor_radius:.17e} \
                             out_of_plane={out_of_plane:.3e} in_plane_resid={:.3e} band={band:.3e} \
                             u={u:.17} v={v:.17}",
                            e.start,
                            e.end,
                            (u.hypot(v) - 1.0).abs() * minor_radius,
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output ellipse-arc endpoint does not lie on its ellipse",
                    ));
                }
            }
            let Some(sweep) =
                crate::geom::ellipse_ccw_sweep(center, n, m, major_radius, minor_radius, ps, pe)
            else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output ellipse-arc endpoint has no parametric direction",
                ));
            };
            let pi = std::f64::consts::PI;
            if (sweep - pi).abs() <= ARC_MINOR_AMBIGUITY_BAND {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "near-half-ellipse arc (minor side ambiguous)",
                });
            }
            let forward_normal = if sweep < pi { n } else { [-n[0], -n[1], -n[2]] };
            Ok(EdgeKind::EllipseArc {
                center,
                forward_normal,
                major_axis: m,
                major_radius,
                minor_radius,
            })
        }
        yang_rs::Curve::Parabola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Parabola" })
        }
        // KV16 (spec `kv16_hyperbola_arc_vocabulary`): the axis-steep
        // plane∩cone section piece. Endpoint-determined traversal (the open
        // branch is injective — no minor-arc derivation, no directional
        // normal); each use copies the yang edge descriptor verbatim, so
        // twins come out BIT-IDENTICAL. K-checks: positive finite semi-axes,
        // unit frame, open (`start != end`), both endpoints ON the branch
        // (`u > 0`, first-order residual within the import band).
        yang_rs::Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            if !(semi_transverse.is_finite()
                && semi_conjugate.is_finite()
                && semi_transverse > 0.0
                && semi_conjugate > 0.0)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output hyperbola edge with non-positive semi-axes",
                ));
            }
            let n = normalize3_arr(normal.as_array());
            let m = normalize3_arr(major_axis.as_array());
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "closed hyperbola loop edge (the branch is unbounded — impossible)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            let scale = semi_transverse.max(semi_conjugate);
            for p in [ps, pe] {
                let (in_plane, out_of_plane, u) = crate::geom::hyperbola_branch_residual(
                    center,
                    n,
                    m,
                    semi_transverse,
                    semi_conjugate,
                    p,
                );
                let mag = p.x().abs().max(p.y().abs()).max(p.z().abs());
                let band = cad_primitives::TAU_EVAL * (1.0 + scale.max(mag));
                if std::env::var("KV_HYPERBOLA_PROBE").is_ok() {
                    eprintln!(
                        "KV_HYPERBOLA_PROBE edge ({},{}) p=({:.6},{:.6},{:.6}) u={u:.3e} \
                         in_plane={in_plane:.3e} oop={out_of_plane:.3e} band={band:.3e} ok={}",
                        e.start,
                        e.end,
                        p.x(),
                        p.y(),
                        p.z(),
                        !(u <= 0.0 || in_plane > band || out_of_plane.abs() > band),
                    );
                }
                if u <= 0.0 || in_plane > band || out_of_plane.abs() > band {
                    if std::env::var("KV_HYPERBOLA_PROBE").is_ok() {
                        eprintln!(
                            "KV_HYPERBOLA_PROBE reject: from={from} to={to} start={} end={} \
                             p={p:?} center={center:?} n={n:?} m={m:?} \
                             a={semi_transverse:.17e} b={semi_conjugate:.17e} \
                             u={u:.3e} in_plane={in_plane:.3e} oop={out_of_plane:.3e} \
                             band={band:.3e}",
                            e.start, e.end,
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output hyperbola-arc endpoint does not lie on the +major-axis branch",
                    ));
                }
            }
            Ok(EdgeKind::HyperbolaArc {
                center,
                normal: n,
                major_axis: m,
                semi_transverse,
                semi_conjugate,
            })
        }
        // M5 (K1–K3): the procedural surface-pair curve. Operands are cylinders
        // and/or cones (the cyl×cyl and cone-pair producers); K2 rejects a
        // closed single-edge loop; K3 requires each endpoint on BOTH defining
        // surfaces within the import band (the per-point certification contract,
        // mirroring the circle/ellipse endpoint checks).
        yang_rs::Curve::SurfacePair { a, b } => {
            let pa = yang_surface_to_pair_surface(a)?;
            let pb = yang_surface_to_pair_surface(b)?;
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "closed surface-pair loop edge (no producer constructs them)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            for p in [ps, pe] {
                let xa = [p.x(), p.y(), p.z()];
                let mag = p.x().abs().max(p.y().abs()).max(p.z().abs());
                for s in [&pa, &pb] {
                    let Some((residual, _)) = crate::geom::pair_surface_residual_gradient(s, xa)
                    else {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "surface-pair endpoint lies on a defining surface's axis",
                        ));
                    };
                    let band = cad_primitives::TAU_EVAL
                        * (1.0 + crate::geom::pair_surface_scale(s).max(mag));
                    if residual.abs() > band {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output surface-pair endpoint does not lie on both surfaces",
                        ));
                    }
                }
            }
            Ok(EdgeKind::SurfacePair { a: pa, b: pb })
        }
    }
}

/// M5 (K1): map a yang output `Surface` to a kernel-v2 [`PairSurface`]. The
/// producers are `Cylinder` (cyl×cyl) and `Cone` (the cone-pair arms: cyl×cone,
/// cone×cone); a `Plane`/`Sphere`/`Torus` operand is a typed wall (no producer
/// emits them onto a surface-pair curve).
pub(crate) fn yang_surface_to_pair_surface(
    s: yang_rs::Surface,
) -> Result<PairSurface, KernelV2Error> {
    match s {
        yang_rs::Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair cylinder operand has a non-positive radius",
                ));
            }
            let ad = normalize3_arr(axis_dir.as_array());
            Ok(PairSurface::Cylinder {
                axis_point,
                axis_dir: UnitVector3 {
                    x: ad[0],
                    y: ad[1],
                    z: ad[2],
                },
                radius,
            })
        }
        yang_rs::Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // α ∈ (0, π/2): a line at α→0, a plane at α→π/2 — both reject.
            if !(half_angle.is_finite()
                && half_angle > 0.0
                && half_angle < std::f64::consts::FRAC_PI_2)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair cone operand has a half-angle outside (0, π/2)",
                ));
            }
            let ad = normalize3_arr(axis_dir.as_array());
            Ok(PairSurface::Cone {
                apex,
                axis_dir: UnitVector3 {
                    x: ad[0],
                    y: ad[1],
                    z: ad[2],
                },
                half_angle,
            })
        }
        yang_rs::Surface::Sphere { center, radius } => {
            // F10: general-position sphere×cyl / sphere×cone degree-4 pairs.
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair sphere operand has a non-positive radius",
                ));
            }
            Ok(PairSurface::Sphere { center, radius })
        }
        _ => Err(KernelV2Error::UnsupportedBooleanOutputCurve {
            curve: "surface-pair with a plane/torus operand (only cyl/cone/sphere are produced)",
        }),
    }
}
