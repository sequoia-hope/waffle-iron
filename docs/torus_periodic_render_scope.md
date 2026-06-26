# Seam-wrapping (periodic) torus patch render — scope

**Status:** 2026-06-26. Boolean-output torus patches of **disk topology** now
render end-to-end (relocation → reconstruction → UV-CDT), and **seam-wrapping
(periodic) patches are detected and reported loudly**. The periodic render
itself — triangulating a patch that wraps the meridian (cylindrical topology) —
is the remaining piece, scoped here.

---

## 1. What renders today

- **Disk-topology patches** (a bounded `(u,v)` region, optionally with holes):
  `yang_rs::tessellate_torus_patch` projects the boundary to the meridian/
  longitude `(u,v)` plane, conditions to TAU_WORK, and runs the interior-Steiner
  CDT. Green end-to-end:
  `kv6d_torus_boolean_recovery::box_intersect_torus_reconstructs_and_tessellates`
  (a box ∩ tube → a disk patch, watertight + on-tube). The simple-patch render is
  also unit-tested (`tessellate::torus_patch_tess_tests`); holes are supported.
- **Periodic detection:** the consumer computes the net winding of the boundary
  in `u` and `v` (the closure-aware sum of `wrap_to_pi` steps). If either is
  nonzero the patch wraps a seam, and the consumer bails so the caller reports it
  loudly rather than feeding a self-crossing polygon to the CDT. Green:
  `kv6d_torus_boolean_recovery::torus_subtract_seam_cut_is_periodic_and_reported`.

## 2. The periodic case, measured

A subtract that bites the tube **at the outer (φ=0) seam** merges the `u=0` and
`u=2π` edges of the lateral's `(u,v)` rectangle, turning the disk into a
**cylinder** (the meridian `u` becomes periodic). Diagnosed on the KV6d fixture
(`torus − box([3.4,-0.6,-0.6],[1.2,1.2,1.2])`): the surviving torus face's outer
loop, projected and unwrapped, runs `u` from `+0.19` down to `−5.80` — a net
winding of **−2π (one meridian wrap)** — so the closing edge jumps a full period
and the polygon self-intersects. The boundary is genuinely periodic, not a CDT
artifact; the Tier B relocation already placed every vertex on-surface, so
reconstruction succeeds — only the render is blocked.

## 3. The plan — port the cylinder patch's periodic unroll

`kernel-v2`'s `tessellate_cylinder_patch` already solves the analogous problem (a
cylinder lateral is periodic in the azimuth θ, bounded in the height h). For the
**partial torus** the meridian `u` is periodic and the longitude `v` is bounded —
**exactly the cylinder's one-periodic-one-bounded structure** — so its approach
ports directly to the torus `(u,v)`:

- **Pass 1 (winding):** walk each loop accumulating `u` continuously via per-edge
  `wrap_to_pi(Δθ)` deltas (NOT per-vertex `atan2`), recording each loop's net
  `wrap`. (The consumer already computes the net wrap for detection; this extends
  it to a continuous per-loop `u` and keeps the loop structure instead of a flat
  point list — an interface change: the consumer must receive the loops, not a
  pre-flattened boundary + holes.)
- **Pass 2 (assemble):**
  - `0` wrapping loops → the current disk path (CDT of the bounded region).
  - `2` oppositely-wrapping loops → a **band**: cut along a seam meridian and
    bridge the two loops in the universal cover (the cylinder's case-2), then CDT
    the unrolled rectangle. The seam vertices at `u=u_cut` and `u=u_cut+2π` map to
    the same 3D meridian (`face_eval` is periodic) → watertight.
  - The seam-bite case above presents as **one** wrapping loop + a window hole;
    its exact unrolled topology (does the bite split into two seam-edge notches?)
    needs the pass-1 continuous-`u` data to classify — likely it resolves to the
    band case once the wrapping loop is cut at the bite.

- **Doubly-periodic (full torus):** a 360° revolve has no seam, so `v` is periodic
  too; the same machinery runs on `v` (a second seam cut). Out of this increment.

**Effort:** a focused port of the cylinder pass-1/pass-2 (~100–150 lines) plus the
consumer interface change (take loops, not flattened boundary). The math is
proven (cylinder); the care is the seam-bridge watertightness and classifying the
seam-bite topology. Test against a synthetic clean band first (constructible +
exactly verifiable, like `torus_patch_tess_tests`), then the seam-cut booleans.

## 4. References

- `crates/yang-rs/src/lib.rs` — `tessellate_torus_patch` (net-wrap detection at
  the projection; the disk CDT path).
- `crates/kernel-v2/src/tessellate.rs` — `tessellate_cylinder_patch` pass-1
  (`Chain { wrap }`, `total_theta`) and pass-2 (`match wrapping.len()` 0/2, the
  seam-bridge `shift_chain`/universal cover) — the pattern to port.
- `docs/torus_ssi_relocation_scope.md` — the Tier B relocation (the prerequisite,
  done).
- Memory: `kernel_v2_kv6d_torus`.
