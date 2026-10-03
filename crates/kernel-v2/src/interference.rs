//! Q2 of `specs/agent_mechanical_design.md` §4.2: **do these two bodies
//! interfere?** — answered by the kernel's own Intersect boolean, not by a
//! separate collision routine.
//!
//! ## Why the boolean and nothing else
//!
//! "Do they overlap, and by how much" is exactly what an Intersect computes.
//! A second implementation (a mesh overlap test, a separating-axis sweep)
//! would be a second source of truth about the same question, free to
//! disagree with the boolean the user's own Subtract will run a moment later.
//! So the query runs the real pipeline and reports what it found — including,
//! loudly, when the pipeline refused.
//!
//! ## The scratch arena
//!
//! A boolean appends entities to the arena AND pushes an [`crate::journal`]
//! entry, and journal entries move the persistent ids every later reference
//! resolves through. A query must not do that. So both operands are deep-
//! copied into a scratch [`BrepArena`] ([`crate::transform::copy_solid_into`])
//! and the Intersect runs there; the arena is dropped with the answer. That
//! is also why [`interference`] takes `&BrepArena` and not `&mut`.
//!
//! The price is that the overlap region has no id in the live arena, so the
//! spec's `region: Option<SolidId>` and the tool's `keep_region` are not in
//! this increment. What comes back instead is the region's measurable
//! content: per lump a volume, a centroid and bounds, which is what tells an
//! agent WHERE to look.
//!
//! ## The discriminator
//!
//! Three outcomes, decided in this order — and nothing is decided by a
//! tolerance on a distance where the boolean has an answer:
//!
//! 1. **AABB reject.** The two bounding boxes, each inflated by the render
//!    chord band (a curved face's chord polygon lies INSIDE the true
//!    surface, so an uninflated box from the tessellation can be short by
//!    that much), do not meet ⇒ `Disjoint`, with Q1's distance. No boolean is
//!    run: a conservative box separation is a proof.
//! 2. **The Intersect runs.** A result solid whose volume exceeds the
//!    workspace's minimum-feature floor (`MIN_FEATURE_SIZE³`) ⇒ `Interferes`
//!    with that volume. A result solid at or under the floor ⇒ `Contact`
//!    with [`ContactEvidence::SliverIntersection`]: the pipeline produced a
//!    body with no meaningful interior, which is what a shared boundary looks
//!    like when Stage 0's overlay resolves it into one.
//! 3. **An empty Intersect** (`EmptyBooleanResult` — the regularized
//!    intersection has no material) ⇒ ask Q1. A distance of zero means they
//!    meet on a set of measure zero ⇒ `Contact` with
//!    [`ContactEvidence::EmptyIntersectionAtZeroDistance`]. A positive
//!    distance ⇒ `Disjoint`.
//!
//! "A distance of zero" is Q1's own zero, which is exact for the planar
//! pairs that touch on a face, an edge or a vertex: the pair kernels report
//! 0 at a piercing or coincident point rather than a small positive number
//! (see [`crate::measure`]). For a curved touching pair Q1 is at the mesh
//! tier, so the test is `value <= chord_bound` — the only honest zero
//! available there, and the evidence arm says which test was used.
//!
//! ## A refused boolean is NOT `Disjoint`
//!
//! Every other error from the pipeline — the Stage-0 coplanar wall, a curved
//! partial-patch operand, a Stage-3/4/5 STOP — propagates as itself. The spec
//! suggests reporting the coplanar refusal as `Contact` naming the coplanar
//! pair; **this increment does not**, and the deviation is deliberate: the
//! wall fires on a coplanar INPUT face pair, and two boxes that OVERLAP share
//! coplanar side faces just as two boxes that merely touch share one. Calling
//! that `Contact` would report "they only touch" for a pair that collides —
//! a silent wrong answer in the one query whose whole job is to catch a
//! collision. A caller who sees the refusal knows the kernel could not tell;
//! a caller who sees `Contact` believes it could.

use crate::arena::{BrepArena, SolidId};
use crate::error::KernelV2Error;
use crate::measure::{DistanceResult, Target};
use crate::tessellate::RENDER_CHORD_TOLERANCE_REL;
use cad_primitives::{BoolOp, MIN_FEATURE_SIZE};

/// One lump of the intersection region.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionBody {
    /// m³.
    pub volume: f64,
    /// Meters.
    pub centroid: [f64; 3],
    /// `[min, max]`, meters.
    pub aabb: [[f64; 3]; 2],
}

/// Why a pair was judged to be in contact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContactEvidence {
    /// The regularized Intersect was EMPTY and the Q1 distance is zero.
    EmptyIntersectionAtZeroDistance,
    /// The Intersect produced a solid at or under the minimum-feature volume
    /// floor: a sliver, not shared interior.
    SliverIntersection { volume: f64 },
}

/// The answer to [`interference`].
#[derive(Debug, Clone, PartialEq)]
pub enum Interference {
    /// Shared interior volume.
    Interferes {
        /// The sum over `bodies`, m³.
        volume: f64,
        bodies: Vec<RegionBody>,
        /// Whether every number above was integrated exactly (the
        /// intersection of two curved operands is a partial-patch B-Rep,
        /// which the moment integrator reads at the mesh tier).
        exact: bool,
        /// The render chord band of the region in meters — the bound on
        /// `volume`'s linear dimensions when `exact` is false, 0 when it is
        /// true. Carried rather than re-derived by the caller: a mesh-tier
        /// number whose band reads as zero is indistinguishable from an exact
        /// one, and a mesh volume is LOW by the chord deficit.
        chord_bound: f64,
    },
    /// Touching, no shared interior.
    Contact {
        evidence: ContactEvidence,
        closest: DistanceResult,
    },
    /// Apart.
    Disjoint { distance: DistanceResult },
}

/// The volume at or under which an Intersect result is a sliver rather than
/// shared interior: the cube of the workspace's minimum feature size. A body
/// thinner than `MIN_FEATURE_SIZE` in any direction is below what the kernel
/// claims to model, so a region that small is not evidence of a collision.
const SLIVER_VOLUME_FLOOR: f64 = MIN_FEATURE_SIZE * MIN_FEATURE_SIZE * MIN_FEATURE_SIZE;

/// Whether `a` and `b` share interior volume, touch, or are apart (Q2).
///
/// `arena` is NOT mutated: the Intersect runs on copies in a scratch arena
/// (see the module docs).
pub fn interference(
    arena: &BrepArena,
    a: SolidId,
    b: SolidId,
) -> Result<Interference, KernelV2Error> {
    let (lo_a, hi_a) = crate::mass::mesh_bounds(arena, a)?;
    let (lo_b, hi_b) = crate::mass::mesh_bounds(arena, b)?;
    let band = RENDER_CHORD_TOLERANCE_REL
        * crate::mass::diagonal(lo_a, hi_a).max(crate::mass::diagonal(lo_b, hi_b));
    let separated = (0..3).any(|k| lo_b[k] - hi_a[k] > band || lo_a[k] - hi_b[k] > band);
    if separated {
        return Ok(Interference::Disjoint {
            distance: crate::measure::distance(arena, Target::Solid(a), Target::Solid(b))?,
        });
    }

    let mut scratch = BrepArena::new();
    let ca = crate::transform::copy_solid_into(arena, a, &mut scratch)?;
    let cb = crate::transform::copy_solid_into(arena, b, &mut scratch)?;
    let region = match crate::boolean_op(&mut scratch, ca, cb, BoolOp::Intersect) {
        Ok(region) => region,
        Err(KernelV2Error::EmptyBooleanResult) => {
            // No material in common. Touching or apart — Q1 decides, on the
            // LIVE arena (the operands there are the ones the caller named).
            let closest = crate::measure::distance(arena, Target::Solid(a), Target::Solid(b))?;
            let zero = if closest.exact {
                closest.value <= 0.0
            } else {
                closest.value <= closest.chord_bound
            };
            return Ok(if zero {
                Interference::Contact {
                    evidence: ContactEvidence::EmptyIntersectionAtZeroDistance,
                    closest,
                }
            } else {
                Interference::Disjoint { distance: closest }
            });
        }
        // Every other pipeline error is itself: a capability wall or a STOP,
        // never an answer about the geometry (see the module docs).
        Err(other) => return Err(other),
    };

    let lumps = crate::split_solid_into_bodies(&mut scratch, region)?;
    let mut bodies = Vec::with_capacity(lumps.len());
    let mut total = 0.0;
    let mut exact = true;
    let mut chord_bound = 0.0f64;
    for lump in lumps {
        let l = crate::mass::lump_of(&scratch, lump)?;
        total += l.volume;
        exact &= l.exact;
        chord_bound = chord_bound.max(l.chord_bound);
        bodies.push(RegionBody {
            volume: l.volume,
            centroid: l.centroid,
            aabb: l.aabb,
        });
    }

    if total <= SLIVER_VOLUME_FLOOR {
        let closest = crate::measure::distance(arena, Target::Solid(a), Target::Solid(b))?;
        return Ok(Interference::Contact {
            evidence: ContactEvidence::SliverIntersection { volume: total },
            closest,
        });
    }
    Ok(Interference::Interferes {
        volume: total,
        bodies,
        exact,
        chord_bound,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sliver floor is the cube of the minimum feature size, not a number
    /// picked to make a case pass — pinned so a later edit has to say why.
    #[test]
    fn the_sliver_floor_is_the_minimum_feature_volume() {
        // `1e-18` to within the rounding of the repeated multiply — the point
        // is the DERIVATION (the minimum feature size cubed), not a literal
        // anyone could nudge.
        assert!((SLIVER_VOLUME_FLOOR / 1e-18 - 1.0).abs() < 1e-15);
        assert_eq!(SLIVER_VOLUME_FLOOR, MIN_FEATURE_SIZE.powi(3));
    }
}
