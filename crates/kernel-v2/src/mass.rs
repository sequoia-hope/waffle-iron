//! Q3 of `specs/agent_mechanical_design.md` §4.2: **volume, surface area,
//! centroid and the inertia tensor about the centroid**, in two tiers, the
//! tier reported.
//!
//! ## One integral, ten numbers
//!
//! Everything comes from the divergence theorem over the solid's own faces,
//! exactly as [`crate::geom::signed_volume`] gets the volume. Each integrand
//! is the volume one with a polynomial one or two degrees higher:
//!
//! | wanted | surface integral |
//! |---|---|
//! | `V` | `(1/3)∮ x·n̂ dA` |
//! | `∫xᵢ dV` | `(1/2)∮ xᵢ² n̂ᵢ dA` |
//! | `∫xᵢ² dV` | `(1/3)∮ xᵢ³ n̂ᵢ dA` |
//! | `∫xᵢxⱼ dV` | `(1/2)∮ xᵢ²xⱼ n̂ᵢ dA` |
//! | `A` | `∮ dA` |
//!
//! So one pass over the faces, emitting **quadrature nodes** `(x, n̂ dA)`,
//! answers all of them. The integrands are polynomials of degree ≤ 3 in the
//! coordinates, which is what makes the node rules below exact rather than
//! approximate: a rule that integrates cubics exactly integrates every one of
//! them exactly.
//!
//! The surface area comes out of the same nodes rather than from
//! [`crate::introspect::surface_area`], whose closed form refuses an
//! arc-bounded patch — SI5 measured 411 of 709 exact STEP shells with no
//! closed-form area. A shell with no closed form still gets an area here, at
//! the tier the rest of the answer is at.
//!
//! ## Which tier the answer is
//!
//! A face is read EXACTLY when its chart and its trim are both known in
//! closed form, and the quadrature rule used on it is exact for cubics:
//!
//! - **A planar face**, loop by loop. A loop of straight edges is fanned into
//!   triangles from its first vertex: the fan's SIGNED sum is the loop's own
//!   integral whether or not the polygon is convex, and an inner loop winds
//!   the other way, so holes subtract without being told to. A loop that is a
//!   single full circle is a disk, integrated in its own polar chart. A
//!   plate with round holes is therefore exact, and so is a box.
//! - **A cylinder lateral with exactly two full-circle rims** and **a cone
//!   lateral with one or two** — the validated revolve/extrude vocabulary,
//!   the same shapes `signed_volume` has closed forms for. Their charts are
//!   `(θ, t)` and `(θ, τ)`, and the rules are exact there (see
//!   [`add_cylinder_band`] and [`add_cone_band`]).
//!
//! Every other face — a sphere, a torus, a partial patch of any surface, an
//! arc-bounded planar face, an SSI-curve boundary — is read from **its own
//! render triangles** ([`crate::tessellate::tessellate_face`], the same
//! triangles the app draws), and the whole answer drops to
//! `Mesh { chord_bound }`. The mesh tier's volume is INSCRIBED: it is low by
//! the chord deficit, never high, and `chord_bound` is the band the consumer
//! must carry (the chord-band propagation lesson).
//!
//! The tier is the solid's, not the face's: one mesh-tier face makes the whole
//! answer mesh-tier, because the moments are a sum and an unrefined term
//! contaminates it.
//!
//! ## The cross-check
//!
//! When the tier is exact and `signed_volume` also has a closed form, the two
//! must agree. They are independent derivations — `signed_volume` accumulates
//! an exact `dashu` coefficient of π from rim normals, this module integrates
//! a chart keyed off `Surface::reversed` — so a disagreement is a sign or
//! orientation defect in one of them, and it STOPs loudly
//! ([`crate::error::KernelV2Error::MassIntegratorDisagreement`]) instead of
//! reporting a centroid derived from the wrong sense.
//!
//! ## Not in Q3
//!
//! Density from a material: the document model has none (checked 2026-10-03),
//! so `density` is an argument that defaults to
//! [`waffle_types::kernel::DEFAULT_DENSITY_KG_M3`] = 1 and the answer says
//! which one it used. Mesh-backed imported bodies refuse at the adapter, as
//! they do for Q1.

use crate::arena::{BrepArena, Curve, FaceId, SolidId, Surface, UnitVector3};
use crate::error::KernelV2Error;
use crate::tessellate::RENDER_CHORD_TOLERANCE_REL;

/// Volume, area, centroid and the inertia tensor of one solid.
#[derive(Debug, Clone, PartialEq)]
pub struct MassResult {
    /// m³.
    pub volume: f64,
    /// m².
    pub surface_area: f64,
    /// Meters.
    pub centroid: [f64; 3],
    /// The inertia tensor about the centroid, world axes, scaled by the
    /// density the caller asked for.
    pub inertia_at_centroid: [[f64; 3]; 3],
    /// Eigenvalues of `inertia_at_centroid`, ascending.
    pub principal_moments: [f64; 3],
    /// The unit eigenvector of each, as rows, right-handed.
    pub principal_axes: [[f64; 3]; 3],
    /// The density used, kg/m³.
    pub density: f64,
    /// `density × volume`, kg.
    pub mass: f64,
    /// Whether every face was read exactly.
    pub exact: bool,
    /// The render chord band in meters — the bound on the numbers when
    /// `exact` is false. Reported either way so a consumer can see the band
    /// it escaped.
    pub chord_bound: f64,
}

// ---------------------------------------------------------------------------
// The accumulator
// ---------------------------------------------------------------------------

/// The ten integrals of §"One integral, ten numbers", accumulated from
/// quadrature nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Moments {
    /// `∮ dA`.
    area: f64,
    /// `∫dV`.
    v: f64,
    /// `∫xᵢ dV`.
    m: [f64; 3],
    /// `∫xᵢ² dV`.
    q: [f64; 3],
    /// `∫xy dV`, `∫yz dV`, `∫zx dV`.
    qxy: f64,
    qyz: f64,
    qzx: f64,
}

impl Moments {
    /// One quadrature node: the point `x`, and `an = n̂ dA` — the oriented
    /// area element already multiplied by the node's weight.
    fn add_node(&mut self, x: [f64; 3], an: [f64; 3]) {
        self.v += (x[0] * an[0] + x[1] * an[1] + x[2] * an[2]) / 3.0;
        for i in 0..3 {
            self.m[i] += 0.5 * x[i] * x[i] * an[i];
            self.q[i] += x[i] * x[i] * x[i] * an[i] / 3.0;
        }
        self.qxy += 0.5 * x[0] * x[0] * x[1] * an[0];
        self.qyz += 0.5 * x[1] * x[1] * x[2] * an[1];
        self.qzx += 0.5 * x[2] * x[2] * x[0] * an[2];
    }
}

/// The 4-point degree-3 rule on a triangle (Hammer–Stroud): barycentric
/// coordinates and weights, the weights summing to exactly 1. Degree 3 is
/// precisely what the integrands need, so the rule is exact for them — the
/// one negative weight is what buys that with four points.
const TRI_RULE: [([f64; 3], f64); 4] = [
    ([1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], -27.0 / 48.0),
    ([0.6, 0.2, 0.2], 25.0 / 48.0),
    ([0.2, 0.6, 0.2], 25.0 / 48.0),
    ([0.2, 0.2, 0.6], 25.0 / 48.0),
];

/// 4-point Gauss–Legendre on `[-1, 1]` (nodes, weights) — exact to degree 7,
/// where the integrands need 4 (a cone's `ρ(τ)` adds one degree to the cubic).
const GL4: [(f64, f64); 4] = [
    (-0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
    (-0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
];

/// Nodes of the uniform (midpoint) rule around a FULL period, which is exact
/// for every trigonometric polynomial of degree `< N`. The integrands reach
/// trigonometric degree 4 (a cubic in `x`, which is degree 1 in the angle,
/// times the normal's own degree 1), so 32 is exact with eight times the
/// margin and costs nothing.
const N_THETA: usize = 32;

/// `[a, b]` mapped Gauss–Legendre nodes.
fn gl(a: f64, b: f64) -> [(f64, f64); 4] {
    let mid = 0.5 * (a + b);
    let half = 0.5 * (b - a);
    let mut out = [(0.0, 0.0); 4];
    for (slot, (xi, wi)) in out.iter_mut().zip(GL4) {
        *slot = (mid + half * xi, half * wi);
    }
    out
}

/// Two unit vectors orthogonal to `u` and to each other, chosen
/// deterministically (off `u`'s smallest component, so the cross product is
/// never near-degenerate).
fn frame(u: UnitVector3) -> ([f64; 3], [f64; 3]) {
    let a = [u.x, u.y, u.z];
    let smallest = (0..3)
        .min_by(|&i, &j| a[i].abs().total_cmp(&a[j].abs()))
        .unwrap_or(0);
    let mut seed = [0.0; 3];
    seed[smallest] = 1.0;
    let e1 = normalize(cross(a, seed));
    (e1, cross(a, e1))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    if n > 0.0 {
        [a[0] / n, a[1] / n, a[2] / n]
    } else {
        a
    }
}
fn scaled(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn axpy(p: [f64; 3], v: [f64; 3], t: f64) -> [f64; 3] {
    [p[0] + v[0] * t, p[1] + v[1] * t, p[2] + v[2] * t]
}

// ---------------------------------------------------------------------------
// Face arms
// ---------------------------------------------------------------------------

/// One triangle of a tessellation: its own winding is its orientation, and
/// its area is unsigned (render triangles neither overlap nor fold back).
fn add_mesh_triangle(acc: &mut Moments, p: [[f64; 3]; 3]) {
    let vector_area = scaled(
        cross(
            [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]],
            [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]],
        ),
        0.5,
    );
    acc.area += dot(vector_area, vector_area).sqrt();
    add_triangle_nodes(acc, p, vector_area);
}

/// One triangle of a planar face's fan: the SIGNED area along the face normal
/// is the contribution, so a reflex fan triangle cancels and an inner loop
/// subtracts.
fn add_fan_triangle(acc: &mut Moments, p: [[f64; 3]; 3], normal: [f64; 3]) {
    let vector_area = scaled(
        cross(
            [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]],
            [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]],
        ),
        0.5,
    );
    acc.area += dot(vector_area, normal);
    add_triangle_nodes(acc, p, vector_area);
}

fn add_triangle_nodes(acc: &mut Moments, p: [[f64; 3]; 3], vector_area: [f64; 3]) {
    for (bary, w) in TRI_RULE {
        let x = [0, 1, 2].map(|k| bary[0] * p[0][k] + bary[1] * p[1][k] + bary[2] * p[2][k]);
        acc.add_node(x, scaled(vector_area, w));
    }
}

/// A disk of radius `radius` centred at `center` in the plane of unit
/// `normal`, oriented along `sign · normal` (`−1` for a hole, whose rim winds
/// against the face normal).
///
/// Polar chart: `x(r, θ) = center + r(cosθ e₁ + sinθ e₂)`, `dA = r dr dθ`.
/// Exact: the integrand is a cubic in `x` times the `r` of the area element,
/// so degree 4 in `r` (Gauss–Legendre 4 is exact to 7) and trigonometric
/// degree 3 in `θ` (the uniform rule is exact to 31).
fn add_disk(acc: &mut Moments, center: [f64; 3], radius: f64, normal: [f64; 3], sign: f64) {
    let (e1, e2) = frame(UnitVector3 {
        x: normal[0],
        y: normal[1],
        z: normal[2],
    });
    let dtheta = std::f64::consts::TAU / N_THETA as f64;
    for (r, wr) in gl(0.0, radius) {
        for j in 0..N_THETA {
            let theta = (j as f64 + 0.5) * dtheta;
            let dir = axpy(scaled(e1, theta.cos()), e2, theta.sin());
            let x = axpy(center, dir, r);
            let w = sign * r * wr * dtheta;
            acc.area += w;
            acc.add_node(x, scaled(normal, w));
        }
    }
}

/// A full cylinder lateral band between two rims.
///
/// `x(θ, t) = axis_point + t·u + R(cosθ e₁ + sinθ e₂)`, outward
/// `n̂ = ±(cosθ e₁ + sinθ e₂)`, `dA = R dθ dt`. The sign is the surface's own
/// `reversed` flag, which the arena DEFINES as which side is outward — not
/// the rim traversal `signed_volume` reads it off instead. The two must agree,
/// and [`mass_properties`] STOPs if they do not.
///
/// Exact: `x` is linear in `t`, so the cubic integrands are degree 3 there
/// (Gauss–Legendre 4 is exact to 7), and trigonometric degree 4 in `θ`.
fn add_cylinder_band(
    acc: &mut Moments,
    axis_point: [f64; 3],
    axis: UnitVector3,
    radius: f64,
    t0: f64,
    t1: f64,
    sign: f64,
) {
    let u = [axis.x, axis.y, axis.z];
    let (e1, e2) = frame(axis);
    let dtheta = std::f64::consts::TAU / N_THETA as f64;
    for (t, wt) in gl(t0, t1) {
        let on_axis = axpy(axis_point, u, t);
        for j in 0..N_THETA {
            let theta = (j as f64 + 0.5) * dtheta;
            let radial = axpy(scaled(e1, theta.cos()), e2, theta.sin());
            let x = axpy(on_axis, radial, radius);
            let w = radius * wt * dtheta;
            acc.area += w;
            acc.add_node(x, scaled(radial, sign * w));
        }
    }
}

/// A full cone lateral band between axial coordinates `τ₀` and `τ₁` measured
/// from the apex (`τ₀ = 0` is the apex form).
///
/// `x(θ, τ) = apex + τ·u + ρ(τ)(cosθ e₁ + sinθ e₂)` with `ρ(τ) = τ tanα`,
/// outward `n̂ = ±(cosα(cosθ e₁ + sinθ e₂) − sinα u)`, `dA = ρ(τ) secα dτ dθ`.
///
/// Exact: degree 4 in `τ` (the cubic integrand times `ρ`'s own degree), which
/// Gauss–Legendre 4 integrates exactly, and trigonometric degree 4 in `θ`.
fn add_cone_band(
    acc: &mut Moments,
    apex: [f64; 3],
    axis: UnitVector3,
    half_angle: f64,
    tau0: f64,
    tau1: f64,
    sign: f64,
) {
    let u = [axis.x, axis.y, axis.z];
    let (e1, e2) = frame(axis);
    let (sa, ca) = half_angle.sin_cos();
    let tan = sa / ca;
    let sec = 1.0 / ca;
    let dtheta = std::f64::consts::TAU / N_THETA as f64;
    for (tau, wtau) in gl(tau0, tau1) {
        let on_axis = axpy(apex, u, tau);
        let rho = tau * tan;
        for j in 0..N_THETA {
            let theta = (j as f64 + 0.5) * dtheta;
            let radial = axpy(scaled(e1, theta.cos()), e2, theta.sin());
            let x = axpy(on_axis, radial, rho);
            let n = axpy(scaled(radial, ca), u, -sa);
            let w = rho * sec * wtau * dtheta;
            acc.area += w;
            acc.add_node(x, scaled(n, sign * w));
        }
    }
}

/// Every full-circle edge of a loop, with the loop's half-edge count — what
/// tells a disk cap (one circle, one half-edge) from a polygon (no circles)
/// from an arc patch (neither).
struct LoopShape {
    circles: Vec<([f64; 3], [f64; 3], f64)>,
    half_edges: usize,
    /// Every half-edge is a straight line: a polygon.
    straight: bool,
    /// Every half-edge that is NOT a full circle is a straight line — the
    /// shape of a curved lateral's slit loop, whose seam edges are rulings.
    rest_straight: bool,
}

fn loop_shape(arena: &BrepArena, lid: crate::arena::LoopId) -> Result<LoopShape, KernelV2Error> {
    let hes = arena.loop_half_edges(lid)?;
    let mut circles = Vec::new();
    let mut straight = true;
    let mut rest_straight = true;
    for &h in &hes {
        match arena.half_edge(h)?.curve {
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                straight = false;
                circles.push((center.as_array(), [normal.x, normal.y, normal.z], radius));
            }
            Curve::LineSegment => {}
            _ => {
                straight = false;
                rest_straight = false;
            }
        }
    }
    Ok(LoopShape {
        circles,
        half_edges: hes.len(),
        straight,
        rest_straight,
    })
}

/// Integrate one face exactly if its chart and trim are known in closed form.
/// `Ok(false)` means "no exact arm for this face" and leaves `acc` untouched.
fn try_exact_face(arena: &BrepArena, f: FaceId, acc: &mut Moments) -> Result<bool, KernelV2Error> {
    let face = arena.face(f)?;
    let mut lids = vec![face.outer_loop];
    lids.extend(face.inner_loops.iter().copied());
    let mut shapes = Vec::with_capacity(lids.len());
    for &lid in &lids {
        shapes.push(loop_shape(arena, lid)?);
    }
    let rims: Vec<_> = shapes.iter().flat_map(|s| s.circles.iter()).collect();

    match face.surface {
        Some(Surface::Plane(plane)) => {
            let n = [plane.normal.x, plane.normal.y, plane.normal.z];
            // Per loop: a straight polygon fans, a lone full circle is a
            // disk. A loop that is neither (an arc, a mixed circle/segment
            // loop) has no closed form, so the whole face falls back.
            let mut staged = Moments::default();
            for (&lid, shape) in lids.iter().zip(&shapes) {
                if shape.straight {
                    let pts = arena.loop_points(lid)?;
                    if pts.len() < 3 {
                        return Ok(false);
                    }
                    let p0 = pts[0].as_array();
                    for w in pts[1..].windows(2) {
                        add_fan_triangle(&mut staged, [p0, w[0].as_array(), w[1].as_array()], n);
                    }
                } else if shape.half_edges == 1 && shape.circles.len() == 1 {
                    let (center, nu, radius) = shape.circles[0];
                    let sign = if dot(nu, n) > 0.0 { 1.0 } else { -1.0 };
                    add_disk(&mut staged, center, radius, n, sign);
                } else {
                    return Ok(false);
                }
            }
            *acc = merge(*acc, staged);
            Ok(true)
        }
        Some(Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            reversed,
        }) => {
            // The validated lateral shape: ONE loop carrying exactly two
            // full-circle rims, the rest of it the straight parameter-domain
            // seam. An extruded cylinder's lateral is a single 4-half-edge
            // slit loop — bottom rim, seam line, top rim, seam line — which
            // is why this counts rims rather than half-edges (what
            // `signed_volume` does too). A PARTIAL patch never gets here: its
            // boundary carries arcs, not full circles, so `rims` is short.
            if rims.len() != 2 || shapes.len() != 1 || !shapes[0].rest_straight {
                return Ok(false);
            }
            let a = axis_point.as_array();
            let u = [axis_dir.x, axis_dir.y, axis_dir.z];
            let t = |c: &[f64; 3]| dot([c[0] - a[0], c[1] - a[1], c[2] - a[2]], u);
            let (t0, t1) = (t(&rims[0].0), t(&rims[1].0));
            let mut staged = Moments::default();
            add_cylinder_band(
                &mut staged,
                a,
                axis_dir,
                radius,
                t0.min(t1),
                t0.max(t1),
                if reversed { -1.0 } else { 1.0 },
            );
            *acc = merge(*acc, staged);
            Ok(true)
        }
        Some(Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            reversed,
        }) => {
            // The frustum lateral (two rims) and the apex form (one): the
            // apex is the τ = 0 end, the same convention `signed_volume`
            // uses. One loop, its non-circle edges the straight seam — as
            // for the cylinder.
            if rims.is_empty() || rims.len() > 2 || shapes.len() != 1 || !shapes[0].rest_straight {
                return Ok(false);
            }
            let ap = apex.as_array();
            let u = [axis_dir.x, axis_dir.y, axis_dir.z];
            let tau = |c: &[f64; 3]| dot([c[0] - ap[0], c[1] - ap[1], c[2] - ap[2]], u);
            let taus: Vec<f64> = rims.iter().map(|r| tau(&r.0)).collect();
            let hi = taus.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let lo = if taus.len() == 2 {
                taus.iter().copied().fold(f64::INFINITY, f64::min)
            } else {
                0.0
            };
            let mut staged = Moments::default();
            add_cone_band(
                &mut staged,
                ap,
                axis_dir,
                half_angle,
                lo,
                hi,
                if reversed { -1.0 } else { 1.0 },
            );
            *acc = merge(*acc, staged);
            Ok(true)
        }
        // A sphere, a torus, or a face still under construction: the mesh
        // tier. Spheres and tori have closed forms for a FULL band only, and
        // detecting the parameter rectangle of a partial one from its loops
        // is the next slice of Q3 — until then the render triangles answer,
        // labelled.
        _ => Ok(false),
    }
}

fn merge(a: Moments, b: Moments) -> Moments {
    Moments {
        area: a.area + b.area,
        v: a.v + b.v,
        m: [0, 1, 2].map(|i| a.m[i] + b.m[i]),
        q: [0, 1, 2].map(|i| a.q[i] + b.q[i]),
        qxy: a.qxy + b.qxy,
        qyz: a.qyz + b.qyz,
        qzx: a.qzx + b.qzx,
    }
}

/// Every face's triangles, for the chord band and the bounding extent.
fn mesh_face(arena: &BrepArena, f: FaceId, acc: &mut Moments) -> Result<(), KernelV2Error> {
    let mesh = crate::tessellate::tessellate_face(arena, f)?;
    let point = |i: u32| -> [f64; 3] {
        let i = i as usize * 3;
        [
            mesh.positions[i],
            mesh.positions[i + 1],
            mesh.positions[i + 2],
        ]
    };
    for t in mesh.indices.chunks_exact(3) {
        add_mesh_triangle(acc, [point(t[0]), point(t[1]), point(t[2])]);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Mass properties of `solid` at `density` kg/m³ (Q3).
pub fn mass_properties(
    arena: &BrepArena,
    solid: SolidId,
    density: f64,
) -> Result<MassResult, KernelV2Error> {
    if !density.is_finite() || density <= 0.0 {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "mass properties: density must be a positive finite number",
        });
    }
    let mut acc = Moments::default();
    let mut exact = true;
    let mut faces = 0usize;
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            faces += 1;
            if !try_exact_face(arena, f, &mut acc)? {
                exact = false;
                mesh_face(arena, f, &mut acc)?;
            }
        }
    }
    if faces == 0 {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "mass properties: the solid has no faces to integrate over",
        });
    }
    // The band is only meaningful for a mesh-tier answer, and measuring it
    // costs a pass over the tessellation — so an exact answer reports 0: no
    // band, nothing for a consumer to carry.
    let chord_bound = if exact {
        0.0
    } else {
        let (lo, hi) = mesh_bounds(arena, solid)?;
        RENDER_CHORD_TOLERANCE_REL * diagonal(lo, hi)
    };

    // P10 net: two independent exact integrators must agree. `signed_volume`
    // takes the lateral sense from the rim normals, this module from
    // `Surface::reversed`; a disagreement is a defect in one of them, and a
    // wrong sense would move the centroid without moving the volume much, so
    // it must STOP rather than answer.
    if exact {
        if let Ok(reference) = crate::geom::signed_volume(arena, solid) {
            let scale = reference.abs().max(acc.v.abs());
            if (reference - acc.v).abs() > 1e-9 * scale.max(f64::MIN_POSITIVE) {
                return Err(KernelV2Error::MassIntegratorDisagreement {
                    solid,
                    volumes: format!("moment integration {} vs signed_volume {reference}", acc.v),
                });
            }
        }
    }

    if acc.v <= 0.0 {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "mass properties: the solid integrates to a non-positive volume (an open or \
                     inward-oriented shell is not a body)",
        });
    }
    let centroid = [0, 1, 2].map(|i| acc.m[i] / acc.v);

    // Second moments about the centroid: `∫(xᵢ−cᵢ)(xⱼ−cⱼ)dV = Qᵢⱼ − V cᵢcⱼ`.
    let c = centroid;
    let q = [0, 1, 2].map(|i| acc.q[i] - acc.v * c[i] * c[i]);
    let qxy = acc.qxy - acc.v * c[0] * c[1];
    let qyz = acc.qyz - acc.v * c[1] * c[2];
    let qzx = acc.qzx - acc.v * c[2] * c[0];
    let d = density;
    let inertia = [
        [d * (q[1] + q[2]), -d * qxy, -d * qzx],
        [-d * qxy, d * (q[0] + q[2]), -d * qyz],
        [-d * qzx, -d * qyz, d * (q[0] + q[1])],
    ];
    let (principal_moments, principal_axes) = eigen_symmetric_3(inertia);

    Ok(MassResult {
        volume: acc.v,
        surface_area: acc.area,
        centroid,
        inertia_at_centroid: inertia,
        principal_moments,
        principal_axes,
        density,
        mass: density * acc.v,
        exact,
        chord_bound,
    })
}

/// A solid's axis-aligned bounds from its render triangles — the same samples
/// Q1's BVH is built over, so measurement and interference agree about where a
/// body is. The loop VERTICES are not enough: a full-turn revolve carries two
/// of them for a whole sphere.
pub(crate) fn mesh_bounds(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<([f64; 3], [f64; 3]), KernelV2Error> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let mesh = crate::tessellate::tessellate_face(arena, f)?;
            for p in mesh.positions.chunks_exact(3) {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
    }
    if !lo[0].is_finite() {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "mass properties: the solid has no geometry to bound",
        });
    }
    Ok((lo, hi))
}

/// The bounding-box diagonal — the `extent` the documented chord band
/// `d_ε(r) = rel · r` is relative to.
pub(crate) fn diagonal(lo: [f64; 3], hi: [f64; 3]) -> f64 {
    ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt()
}

/// Eigenvalues (ascending) and unit eigenvectors (as rows, in the same order,
/// right-handed) of a symmetric 3×3 matrix, by cyclic Jacobi rotations.
///
/// Jacobi, not a characteristic-polynomial root solve: it is backward stable
/// and it stays accurate on the degenerate spectra a symmetric part carries
/// all the time (a cylinder's two equal transverse moments, a cube's three
/// equal ones), where the cubic's discriminant loses most of its digits.
fn eigen_symmetric_3(matrix: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut a = matrix;
    // `v` holds eigenvectors as COLUMNS while rotating; transposed at the end.
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..64 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        let scale = a[0][0].abs() + a[1][1].abs() + a[2][2].abs();
        if off <= f64::EPSILON * scale.max(f64::MIN_POSITIVE) {
            break;
        }
        for (p, q) in [(0usize, 1usize), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            // The rotation that zeroes a[p][q] exactly (Golub & Van Loan §8.4).
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let cs = 1.0 / (t * t + 1.0).sqrt();
            let sn = t * cs;
            // Columns p and q of A, then rows p and q of A, then columns p
            // and q of the accumulated rotation: A ← Jᵀ A J, V ← V J.
            for row in a.iter_mut() {
                let (akp, akq) = (row[p], row[q]);
                row[p] = cs * akp - sn * akq;
                row[q] = sn * akp + cs * akq;
            }
            let (mut rp, mut rq) = (a[p], a[q]);
            for (x, y) in rp.iter_mut().zip(rq.iter_mut()) {
                let (apk, aqk) = (*x, *y);
                *x = cs * apk - sn * aqk;
                *y = sn * apk + cs * aqk;
            }
            a[p] = rp;
            a[q] = rq;
            for row in v.iter_mut() {
                let (vkp, vkq) = (row[p], row[q]);
                row[p] = cs * vkp - sn * vkq;
                row[q] = sn * vkp + cs * vkq;
            }
        }
    }
    let mut order = [0usize, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = order.map(|i| a[i][i]);
    let mut axes = order.map(|i| normalize([v[0][i], v[1][i], v[2][i]]));
    // Right-handed, so the triple is a usable frame.
    if dot(cross(axes[0], axes[1]), axes[2]) < 0.0 {
        axes[2] = scaled(axes[2], -1.0);
    }
    (values, axes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube's twelve triangles, outward-wound, as a raw node-level
    /// check of the accumulator and the triangle rule — before any arena is
    /// involved. The closed forms are `V = 1`, `∫x dV = 1/2`,
    /// `∫x² dV = 1/3`, `∫xy dV = 1/4`.
    #[test]
    fn the_triangle_rule_integrates_a_unit_cube_exactly() {
        let v = |i: usize| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64];
        // Faces as outward-wound quads (CCW seen from outside).
        let quads: [[usize; 4]; 6] = [
            [0, 2, 3, 1], // z = 0, outward -z
            [4, 5, 7, 6], // z = 1, outward +z
            [0, 1, 5, 4], // y = 0, outward -y
            [2, 6, 7, 3], // y = 1, outward +y
            [0, 4, 6, 2], // x = 0, outward -x
            [1, 3, 7, 5], // x = 1, outward +x
        ];
        let mut acc = Moments::default();
        for q in quads {
            add_mesh_triangle(&mut acc, [v(q[0]), v(q[1]), v(q[2])]);
            add_mesh_triangle(&mut acc, [v(q[0]), v(q[2]), v(q[3])]);
        }
        assert!((acc.v - 1.0).abs() < 1e-14, "volume {}", acc.v);
        assert!((acc.area - 6.0).abs() < 1e-14, "area {}", acc.area);
        for i in 0..3 {
            assert!((acc.m[i] - 0.5).abs() < 1e-14, "first moment {i}");
            assert!((acc.q[i] - 1.0 / 3.0).abs() < 1e-14, "second moment {i}");
        }
        for (name, got) in [("xy", acc.qxy), ("yz", acc.qyz), ("zx", acc.qzx)] {
            assert!((got - 0.25).abs() < 1e-14, "mixed moment {name}: {got}");
        }
    }

    /// The disk chart against the closed forms of a unit disk in the z = 0
    /// plane: area π, and — as the cap of nothing — the flux integrals a
    /// cylinder's caps must supply.
    #[test]
    fn the_disk_chart_integrates_a_unit_disk_exactly() {
        let mut acc = Moments::default();
        add_disk(&mut acc, [0.0, 0.0, 0.0], 1.0, [0.0, 0.0, 1.0], 1.0);
        assert!(
            (acc.area - std::f64::consts::PI).abs() < 1e-13,
            "area {}",
            acc.area
        );
        // `∮ x²·n_x dA` over a disk whose normal is z is zero, and the z
        // integrands vanish on the z = 0 plane: the only non-trivial check is
        // that nothing leaks.
        assert!(
            acc.v.abs() < 1e-15,
            "a flat disk at the origin bounds nothing"
        );
    }

    /// Jacobi on a matrix whose eigenvalues and axes are known by hand, and
    /// on a degenerate spectrum (two equal eigenvalues, a cylinder's).
    #[test]
    fn eigen_symmetric_handles_known_and_degenerate_spectra() {
        let (vals, axes) = eigen_symmetric_3([[3.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]]);
        assert!((vals[0] - 1.0).abs() < 1e-14);
        assert!((vals[1] - 2.0).abs() < 1e-14);
        assert!((vals[2] - 3.0).abs() < 1e-14);
        assert!(
            axes[0][1].abs() > 1.0 - 1e-12,
            "the 1.0 axis is y: {axes:?}"
        );

        let (vals, axes) = eigen_symmetric_3([[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 2.0]]);
        assert!((vals[0] - 2.0).abs() < 1e-14 && (vals[2] - 5.0).abs() < 1e-14);
        assert!(
            dot(cross(axes[0], axes[1]), axes[2]) > 1.0 - 1e-12,
            "the frame stays right-handed: {axes:?}"
        );
    }
}
