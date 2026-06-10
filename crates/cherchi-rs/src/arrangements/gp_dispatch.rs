//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # Generic-point dispatch (shared, `pub(crate)`)
//!
//! The Rust equivalent of the C++ `genericPoint` runtime-tag dispatch: a
//! stored [`VertexCoords`] (Explicit / Lpi / Tpi) is turned into a
//! [`GenericPoint3D`] for the clean-room native indirect predicates
//! (`crate::predicates::indirect`, PR-CR-M7c).
//!
//! Compared to the pre-M7c FFI version, the handle machinery collapses:
//! `GenericPoint3D` is an owned, lifetime-free, generator-based enum, so the
//! old `Backing` (separately-owned explicit generators) / `Gp<'a>` (borrowing
//! handle) split and the `with_gp!` 3^N static-dispatch macro are all gone —
//! conversion is one `match`. `GenericPoint3D` lazily caches its f64 and
//! interval lambdas internally (Attene §5.4), so consumers that previously
//! kept one FFI handle alive per vertex (e.g. the inside_out ray-sort arena)
//! get the same per-vertex caching by constructing one `GenericPoint3D` per
//! vertex and reusing it across predicate calls.

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::Plane;
use crate::predicates::indirect::{
    orient2d_xy_indirect, orient2d_yz_indirect, orient2d_zx_indirect, point_in_triangle_indirect,
    GenericPoint3D, Sign,
};

/// Convert stored typed vertex coordinates into a native generic point.
/// Mechanical: `Explicit`/`Lpi`/`Tpi` carry exactly the generator data the
/// corresponding `GenericPoint3D` constructor expects.
pub(crate) fn to_generic(c: &VertexCoords) -> GenericPoint3D {
    match c {
        VertexCoords::Explicit(p) => GenericPoint3D::explicit(*p),
        VertexCoords::Lpi { line, plane } => {
            GenericPoint3D::lpi(line[0], line[1], plane[0], plane[1], plane[2])
        }
        VertexCoords::Tpi { v, w, u } => GenericPoint3D::tpi(*v, *w, *u),
    }
}

/// `point_in_triangle` over four generic points (boundary-inclusive; the
/// consumer contract has `p` coplanar with `(a, b, c)`).
pub(crate) fn dispatch_point_in_triangle(
    p: &GenericPoint3D,
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> bool {
    point_in_triangle_indirect(p, a, b, c)
}

/// `orient2d` (projected to `plane`) over three generic points.
pub(crate) fn dispatch_orient2d(
    plane: Plane,
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Sign {
    match plane {
        Plane::XY => orient2d_xy_indirect(a, b, c),
        Plane::YZ => orient2d_yz_indirect(a, b, c),
        Plane::ZX => orient2d_zx_indirect(a, b, c),
    }
}
