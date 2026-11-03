//! The extent of a solid along a direction: `[min, max]` of `p · d̂` over
//! every point `p` of the solid (its support interval).
//!
//! Consumers ask "where is this body's material along an axis?" — the
//! feature engine's cut auto-reversal and its `ThroughAll` depth. Reading
//! B-Rep VERTICES alone answers it only for polyhedra: a plain cylinder has
//! exactly two vertices, both on its seam, so its vertex interval misses the
//! rims (prospector seed 6, 2026-11-03: rows 00021 / 00122 reversed a cut
//! on a vertex interval of `[-10.7, -7.3]` against the true `[-11.8, 8.4]`).
//!
//! The extremum of the linear function `p · d̂` over a closed solid lies on
//! its boundary surface, and over a face either on the face's boundary or at
//! an interior critical point (surface normal `∥ d̂`). So the interval is the
//! hull of:
//!
//! - every vertex;
//! - each edge curve's own critical points that lie on the edge — closed
//!   form for circles, arcs, ellipse arcs and hyperbola arcs; a surface-pair
//!   curve (no closed form) contributes its render-identical certified
//!   samples, within the render chord band of the curve;
//! - each face's interior critical points that lie IN the face. Planes,
//!   cylinders and cones are ruled: `p · d̂` is linear along every ruling,
//!   so a ruled face's extremum is on its boundary, except a cone's apex.
//!   A sphere has two candidates (`c ± r d̂`), a torus four
//!   (`c ± R ê ± r d̂`, `ê` the unit projection of `d̂` off the axis). A
//!   candidate counts when it lies within twice the render sagitta band of
//!   the face's own render triangles — membership read off the face's
//!   tessellation (kernel-v2 has no exact point-in-face test); a candidate
//!   just outside a boundary moves the extent by second-order amounts.
//!   With `d̂` along a torus's axis the critical set is two whole circles;
//!   the face's render vertices (on the surface, so within the sagitta
//!   band of the true extent) stand in for them.

use crate::arena::{BrepArena, Curve, FaceId, SolidId, Surface};
use crate::error::KernelV2Error;
use crate::tessellate::{
    circle_segment_count, surface_pair_edge_samples, tessellate_face, RenderMesh,
    RENDER_CHORD_TOLERANCE_REL,
};

type V = [f64; 3];

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V) -> Option<V> {
    let n = norm(a);
    (n.is_finite() && n > 0.0).then(|| scale(a, 1.0 / n))
}
/// `a` modulo 2π into `[0, 2π)`.
fn wrap(a: f64) -> f64 {
    a.rem_euclid(std::f64::consts::TAU)
}

/// Running `[lo, hi]` of `p · d̂`.
struct Interval {
    d: V,
    lo: f64,
    hi: f64,
}

impl Interval {
    fn add(&mut self, p: V) {
        let s = dot(p, self.d);
        self.lo = self.lo.min(s);
        self.hi = self.hi.max(s);
    }
}

/// The interval of `p · d̂` over `solid` (see the module docs), `d̂` the unit
/// of `direction`. `Ok(None)` for a degenerate direction or a solid with no
/// boundary point.
pub fn extent_along(
    arena: &BrepArena,
    solid: SolidId,
    direction: [f64; 3],
) -> Result<Option<(f64, f64)>, KernelV2Error> {
    let Some(d) = unit(direction) else {
        return Ok(None);
    };
    let mut iv = Interval {
        d,
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };
    let n_seg = circle_segment_count(RENDER_CHORD_TOLERANCE_REL);
    for &sh in &arena.solid(solid)?.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let loops = std::iter::once(face.outer_loop).chain(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    edge_extremes(arena, h, d, n_seg, &mut iv)?;
                }
            }
            if let Some(surface) = face.surface {
                face_interior_extremes(arena, f, &surface, d, &mut iv)?;
            }
        }
    }
    Ok((iv.lo.is_finite() && iv.hi.is_finite()).then_some((iv.lo, iv.hi)))
}

/// The vertex interval alone — exact for a polyhedron, and what
/// [`extent_along`]'s callers fall back to when it fails.
pub fn vertex_extent_along(
    arena: &BrepArena,
    solid: SolidId,
    direction: [f64; 3],
) -> Result<Option<(f64, f64)>, KernelV2Error> {
    let Some(d) = unit(direction) else {
        return Ok(None);
    };
    let mut iv = Interval {
        d,
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };
    for &sh in &arena.solid(solid)?.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let loops = std::iter::once(face.outer_loop).chain(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    iv.add(arena.vertex(arena.half_edge(h)?.origin)?.point.as_array());
                }
            }
        }
    }
    Ok((iv.lo.is_finite() && iv.hi.is_finite()).then_some((iv.lo, iv.hi)))
}

/// The half-edge's origin and its own critical points of `p · d̂` that lie
/// on it (the destination is the next half-edge's origin).
fn edge_extremes(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    d: V,
    n_seg: u32,
    iv: &mut Interval,
) -> Result<(), KernelV2Error> {
    let he = arena.half_edge(h)?;
    let o = arena.vertex(he.origin)?.point.as_array();
    let e = arena
        .vertex(arena.half_edge(he.next)?.origin)?
        .point
        .as_array();
    let closed = he.origin == arena.half_edge(he.next)?.origin;
    iv.add(o);
    match he.curve {
        Curve::LineSegment => {}
        Curve::Circle {
            center,
            normal,
            radius,
        }
        | Curve::Arc {
            center,
            normal,
            radius,
        } => {
            let c = center.as_array();
            let n = [normal.x, normal.y, normal.z];
            // In-plane direction of steepest ascent of p · d̂.
            let Some(s) = unit(sub(d, scale(n, dot(d, n)))) else {
                return Ok(()); // d̂ ∥ the axis: constant along the circle
            };
            let full = matches!(he.curve, Curve::Circle { .. }) || closed;
            let Some(e1) = unit(sub(o, c)) else {
                return Ok(());
            };
            let e2 = cross(n, e1);
            let angle = |p: V| {
                let q = sub(p, c);
                wrap(dot(q, e2).atan2(dot(q, e1)))
            };
            let sweep = if full {
                std::f64::consts::TAU
            } else {
                angle(e)
            };
            for sign in [1.0, -1.0] {
                let p = add(c, scale(s, sign * radius));
                if full || angle(p) <= sweep {
                    iv.add(p);
                }
            }
        }
        Curve::EllipseArc {
            center,
            normal,
            major_axis,
            major_radius: a,
            minor_radius: b,
        } => {
            let c = center.as_array();
            let n = [normal.x, normal.y, normal.z];
            let m = [major_axis.x, major_axis.y, major_axis.z];
            let w = cross(n, m);
            let (alpha, beta) = (dot(m, d), dot(w, d));
            if a * alpha == 0.0 && b * beta == 0.0 {
                return Ok(());
            }
            let param = |p: V| {
                let q = sub(p, c);
                (dot(q, w) / b).atan2(dot(q, m) / a)
            };
            let t0 = param(o);
            let sweep = if closed {
                std::f64::consts::TAU
            } else {
                wrap(param(e) - t0)
            };
            let t_star = (b * beta).atan2(a * alpha);
            for t in [t_star, t_star + std::f64::consts::PI] {
                if closed || wrap(t - t0) <= sweep {
                    iv.add(add(c, add(scale(m, a * t.cos()), scale(w, b * t.sin()))));
                }
            }
        }
        Curve::HyperbolaArc {
            center,
            normal,
            major_axis,
            semi_transverse: a,
            semi_conjugate: b,
        } => {
            let c = center.as_array();
            let n = [normal.x, normal.y, normal.z];
            let m = [major_axis.x, major_axis.y, major_axis.z];
            let w = cross(n, m);
            let (alpha, beta) = (dot(m, d), dot(w, d));
            // f(u) = a·α·cosh u + b·β·sinh u; f'(u) = 0 ⇔ tanh u = −bβ / (aα).
            if a * alpha != 0.0 {
                let r = -(b * beta) / (a * alpha);
                if r.abs() < 1.0 {
                    let u_star = r.atanh();
                    let u = |p: V| (dot(sub(p, c), w) / b).asinh();
                    let (u0, u1) = (u(o), u(e));
                    if u_star >= u0.min(u1) && u_star <= u0.max(u1) {
                        iv.add(add(
                            c,
                            add(scale(m, a * u_star.cosh()), scale(w, b * u_star.sinh())),
                        ));
                    }
                }
            }
        }
        Curve::SurfacePair { .. } => {
            for p in surface_pair_edge_samples(arena, h, n_seg)? {
                iv.add(p.as_array());
            }
        }
    }
    Ok(())
}

/// A face's interior critical points of `p · d̂` that lie in the face.
fn face_interior_extremes(
    arena: &BrepArena,
    f: FaceId,
    surface: &Surface,
    d: V,
    iv: &mut Interval,
) -> Result<(), KernelV2Error> {
    let (candidates, chord_r): (Vec<V>, f64) = match *surface {
        Surface::Sphere { center, radius, .. } => {
            let c = center.as_array();
            (
                vec![add(c, scale(d, radius)), sub(c, scale(d, radius))],
                radius,
            )
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        } => {
            let c = center.as_array();
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            match unit(sub(d, scale(a, dot(d, a)))) {
                Some(e) => {
                    let mut v = Vec::with_capacity(4);
                    for s1 in [1.0, -1.0] {
                        for s2 in [1.0, -1.0] {
                            v.push(add(
                                c,
                                add(scale(e, s1 * major_radius), scale(d, s2 * minor_radius)),
                            ));
                        }
                    }
                    (v, major_radius + minor_radius)
                }
                None => {
                    // d̂ along the axis: the critical set is two circles.
                    let mesh = tessellate_face(arena, f)?;
                    for p in mesh.positions.chunks_exact(3) {
                        iv.add([p[0], p[1], p[2]]);
                    }
                    return Ok(());
                }
            }
        }
        Surface::Cone { apex, .. } => (vec![apex.as_array()], 0.0),
        _ => return Ok(()),
    };
    let mesh = tessellate_face(arena, f)?;
    let chord_r = if chord_r > 0.0 {
        chord_r
    } else {
        // A cone: the largest distance of its render vertices from the apex.
        let apex = candidates[0];
        mesh.positions
            .chunks_exact(3)
            .map(|p| norm(sub([p[0], p[1], p[2]], apex)))
            .fold(0.0, f64::max)
    };
    let tol = 2.0 * RENDER_CHORD_TOLERANCE_REL * chord_r;
    for p in candidates {
        if mesh_distance(&mesh, p) <= tol {
            iv.add(p);
        }
    }
    Ok(())
}

/// Distance from `p` to the nearest triangle of `mesh`.
fn mesh_distance(mesh: &RenderMesh, p: V) -> f64 {
    let pos = |i: u32| {
        let k = i as usize * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    mesh.indices
        .chunks_exact(3)
        .map(|t| {
            norm(sub(
                p,
                closest_on_triangle(p, pos(t[0]), pos(t[1]), pos(t[2])),
            ))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Closest point of triangle `abc` to `p` (Ericson, Real-Time Collision
/// Detection §5.1.5).
fn closest_on_triangle(p: V, a: V, b: V, c: V) -> V {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, scale(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, scale(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, scale(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    add(a, add(scale(ab, vb * denom), scale(ac, vc * denom)))
}
