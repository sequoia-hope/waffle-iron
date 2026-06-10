//! Production validation suite for solids built in the B-Rep arena.
//!
//! `validate_solid` checks the full invariant set on everything reachable
//! from one solid:
//!
//! 1. **Twin pairing** — every half-edge has a live twin, `twin(twin) == h`,
//!    `twin != h`, and twins run between the same two vertices in opposite
//!    directions (`origin(twin(h)) == origin(next(h))`). With pairing intact,
//!    "each edge has exactly 2 half-edges" holds structurally and the edge
//!    count is `half-edges / 2`.
//! 2. **Loop closure** — every loop's `next` orbit returns to its
//!    representative, `prev` is the inverse of `next`, and every member
//!    points back at the loop.
//! 3. **Vertex manifoldness** — the half-edges leaving each vertex form a
//!    single radial fan under the orbit `h ↦ next(twin(h))` (2-manifold
//!    vertex condition).
//! 4. **Euler–Poincaré bookkeeping** — `V − E + F − R = 2(S − G)`
//!    (Stroud 2006 §4 rule 4, with his h/b written R/S).
//! 5. **Newell normal consistency** — every face carries `Some(Surface)` and
//!    its stored unit normal matches the normalized Newell normal of its
//!    outer loop; every inner loop (ring) winds opposite (its Newell normal
//!    is antiparallel to the face normal).
//! 6. **Planarity (debug builds only)** — every loop vertex lies within
//!    [`PLANARITY_DEBUG_TOLERANCE`] of the face plane.
//!
//! ## The planarity boundary (documented per the KV1 mandate)
//!
//! kernel-v2 may not depend on `cherchi-rs`, so no exact coplanarity
//! predicate is available in this crate. That is acceptable for KV1 because
//! planarity of constructed faces is **guaranteed by construction**: the
//! Euler operators create geometry only at caller-supplied points, and the
//! KV2 consumers (planar profile → extrude) supply coplanar points by
//! construction. The f64 residual check below is therefore a *debug-tier
//! tripwire for construction-sequence bugs*, compiled only under
//! `debug_assertions`, and is **not** a production correctness gate —
//! production correctness for planarity rests on the constructors, and any
//! future need for exact coplanarity decisions (e.g. boolean input
//! validation) belongs to the yang-rs/cherchi-rs layers.

use crate::arena::{BrepArena, FaceId, LoopBoundary, LoopKind, SolidId, Surface, VertexId};
use crate::error::KernelV2Error;

/// Debug-tier planarity tripwire: maximum |signed distance| (meters) of a
/// loop vertex from its face plane. Strict — far below `TAU_MODEL` (1e-7 m)
/// and chosen so that exactly-constructed prismatic solids (residual 0.0)
/// pass with nine orders of magnitude to spare while genuinely non-planar
/// loops (feature size ≥ `MIN_FEATURE_SIZE` = 1e-6 m) fail loudly.
/// See the module docs for why this is not a production gate.
pub const PLANARITY_DEBUG_TOLERANCE: f64 = 1e-12;

/// Tolerance on `1 − dot(stored_normal, newell_unit)` for invariant 5.
/// Both vectors are unit-length f64; agreement is by construction, so this
/// only absorbs normalization rounding.
pub const NORMAL_AGREEMENT_TOLERANCE: f64 = 1e-9;

/// Element counts and bookkeeping for a validated solid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyReport {
    /// Vertices reachable from the solid.
    pub vertices: usize,
    /// Edges (half-edge pairs).
    pub edges: usize,
    /// Faces.
    pub faces: usize,
    /// Inner loops (rings / hole-loops).
    pub rings: usize,
    /// Shells.
    pub shells: usize,
    /// Total genus across the solid's shells.
    pub genus: usize,
    /// `V − E + F − R`.
    pub euler_lhs: i64,
    /// `2(S − G)`.
    pub euler_rhs: i64,
}

/// Validate everything reachable from `solid`. See module docs for the
/// checked invariant set. Returns the first violation found, or a
/// [`TopologyReport`] when all checks pass.
pub fn validate_solid(
    _arena: &BrepArena,
    _solid: SolidId,
) -> Result<TopologyReport, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("validate_solid"))
}

// Suppress unused-import warnings while the body above is a RED stub.
#[allow(unused)]
fn _red_stub_uses(_: FaceId, _: LoopBoundary, _: LoopKind, _: Surface, _: VertexId) {}
