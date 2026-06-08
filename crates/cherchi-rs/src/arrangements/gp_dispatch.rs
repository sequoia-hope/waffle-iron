//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # Generic-point FFI dispatch (shared, `pub(crate)`)
//!
//! The faithful Rust equivalent of the C++ `genericPoint` runtime-tag dispatch:
//! a `VertexCoords` (Explicit / Lpi / Tpi) is turned into a concrete sized
//! handle (`ExplicitPoint3D` / `ImplicitPoint3DLpi` / `ImplicitPoint3DTpi`) that
//! the safe `genericPoint::`-static predicate wrappers in
//! `indirect-predicates-sidecar-rs` require. The implicit variants borrow their
//! backing explicit generators (kept SEPARATE from the handle so the handle can
//! borrow them without a self-referential struct).
//!
//! Extracted verbatim from `retriangulate.rs` (PR-CR-AR3a sub-step 3a, pure
//! move) so that both `retriangulate.rs` and `enforce.rs` can build their own
//! predicate dispatchers from the same machinery. The `with_gp!` macro is
//! re-exported (`pub(crate) use with_gp;`) for cross-module use.
//!
//! **NEVER** call the `_II` / `_IIII` predicate variants on explicit input —
//! they segfault (CR-IP6). Always go through the `genericPoint::`-static safe
//! wrappers, which this module's dispatchers do.

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::Plane;
use indirect_predicates_sidecar_rs::{
    orient2d_xy, orient2d_yz, orient2d_zx, point_in_triangle, AsGenericPoint, ExplicitPoint3D,
    ImplicitPoint3DLpi, ImplicitPoint3DTpi, Sign as IpSign,
};

// ── FFI handle dispatch (no self-referential structs) ─────────────────

/// Backing explicit generators for a `VertexCoords`. Empty for `Explicit`; the
/// 5 LPI generators for `Lpi`; the 9 TPI generators for `Tpi`. Kept SEPARATE
/// from the handle so the handle can borrow them without self-reference.
pub(crate) struct Backing {
    gens: Vec<ExplicitPoint3D>,
}

pub(crate) fn backing(c: &VertexCoords) -> Backing {
    match c {
        VertexCoords::Explicit(_) => Backing { gens: vec![] },
        VertexCoords::Lpi { line, plane } => Backing {
            gens: vec![
                ExplicitPoint3D::new(line[0].x(), line[0].y(), line[0].z()),
                ExplicitPoint3D::new(line[1].x(), line[1].y(), line[1].z()),
                ExplicitPoint3D::new(plane[0].x(), plane[0].y(), plane[0].z()),
                ExplicitPoint3D::new(plane[1].x(), plane[1].y(), plane[1].z()),
                ExplicitPoint3D::new(plane[2].x(), plane[2].y(), plane[2].z()),
            ],
        },
        // PR-CR-AR2b Cycle C1: the 9 TPI generators (three triangles `v,w,u`,
        // each defining one supporting plane), in the exact arg order
        // `ImplicitPoint3DTpi::new` expects: v[0..3], w[0..3], u[0..3].
        VertexCoords::Tpi { v, w, u } => {
            let mut gens = Vec::with_capacity(9);
            for tri in [v, w, u] {
                for p in tri {
                    gens.push(ExplicitPoint3D::new(p.x(), p.y(), p.z()));
                }
            }
            Backing { gens }
        }
    }
}

/// A generic-point handle over a `VertexCoords`: an owned explicit point, an
/// LPI, or a TPI — the implicit variants borrowing their backing generators.
pub(crate) enum Gp<'a> {
    E(ExplicitPoint3D),
    L(ImplicitPoint3DLpi<'a>),
    T(ImplicitPoint3DTpi<'a>),
}

pub(crate) fn gp<'a>(c: &VertexCoords, b: &'a Backing) -> Gp<'a> {
    match c {
        VertexCoords::Explicit(p) => Gp::E(ExplicitPoint3D::new(p.x(), p.y(), p.z())),
        VertexCoords::Lpi { .. } => Gp::L(ImplicitPoint3DLpi::new(
            &b.gens[0], &b.gens[1], &b.gens[2], &b.gens[3], &b.gens[4],
        )),
        // PR-CR-AR2b Cycle C1: the exact `ImplicitPoint3DTpi` handle over the 9
        // backing generators (three supporting planes), replacing the Cycle-B
        // `sum/9` explicit-centroid stand-in. Borrows the backing generators in
        // the same v[0..3],w[0..3],u[0..3] order `backing` pushed them.
        VertexCoords::Tpi { .. } => Gp::T(ImplicitPoint3DTpi::new(
            &b.gens[0], &b.gens[1], &b.gens[2], &b.gens[3], &b.gens[4], &b.gens[5], &b.gens[6],
            &b.gens[7], &b.gens[8],
        )),
    }
}

/// Destructure one or more `&Gp` into concrete sized handles (binding each to
/// the SAME identifier it came in as), then evaluate `$body`. The nested
/// 3-variant (`E`/`L`/`T`) match monomorphizes `$body` to the concrete static
/// types each arg actually holds — the faithful Rust equivalent of the C++
/// hand-enumerated generic-point dispatch (`genericPoint` runtime-tag switch),
/// here turning `&Gp` into the sized `&impl AsGenericPoint` the underlying safe
/// `genericPoint::`-static predicate wrappers require. DRY: each predicate body
/// is written once; the macro supplies the 3^N concrete instantiations.
macro_rules! with_gp {
    // Bind one `Gp` (named `$id`), then recurse over the rest.
    ($body:expr; $id:ident $(, $rest:ident)*) => {
        match $id {
            Gp::E($id) => with_gp!($body; $($rest),*),
            Gp::L($id) => with_gp!($body; $($rest),*),
            Gp::T($id) => with_gp!($body; $($rest),*),
        }
    };
    // All handles bound to concrete types — evaluate the predicate.
    ($body:expr;) => {
        $body
    };
}

// Re-export the macro for cross-module use (enforce.rs builds its own
// dispatchers; retriangulate.rs uses the dispatch fns below).
pub(crate) use with_gp;

/// `point_in_triangle` over four `Gp` handles (each arg its own static type).
pub(crate) fn dispatch_point_in_triangle(p: &Gp, a: &Gp, b: &Gp, c: &Gp) -> bool {
    with_gp!(point_in_triangle(p, a, b, c); p, a, b, c)
}

/// `orient2d` (in `plane`) over three `Gp` handles.
pub(crate) fn dispatch_orient2d(plane: Plane, a: &Gp, b: &Gp, c: &Gp) -> IpSign {
    fn o2d(
        plane: Plane,
        a: &impl AsGenericPoint,
        b: &impl AsGenericPoint,
        c: &impl AsGenericPoint,
    ) -> IpSign {
        match plane {
            Plane::XY => orient2d_xy(a, b, c),
            Plane::YZ => orient2d_yz(a, b, c),
            Plane::ZX => orient2d_zx(a, b, c),
        }
    }
    with_gp!(o2d(plane, a, b, c); a, b, c)
}
