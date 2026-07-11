# Stage-6 arc orientation + twin-copy chain identity (task #133)

Status: IMPLEMENTING
Driver: yang-DIRECT chained booleans (test fixtures; production goes
through kernel-v2 which re-derives arc senses). A partial-depth pocket
operand (cyl − channel stopping mid-body) re-enters a plain boolean and
dies `NonManifoldOutput`: its Stage-1 mesh carries ~90 unbalanced edges
along the split arcs.

## 1. Root cause (two independent defects)

1. **Orientation**: `emit_topology`'s `push_loop` (both the curved and the
   planar branch) creates one DIRECTED edge copy per face loop, taking the
   loop traversal as `(start, end)` but copying the intersection curve —
   normal included — verbatim from the UNDIRECTED mesh-edge map. The
   Stage-1 input convention is "the arc is the CCW sweep around the stored
   normal from start to end"; the copy whose traversal is clockwise then
   declares the COMPLEMENTARY (≈ 2π) arc. Stage-1 faithfully samples ~a
   full circle for it (probe: lateral z=1 chains spanning all u in the
   unrolled ribbon).
2. **Twin-chain bit identity**: even with truthful orientation, the two
   per-face copies of one geometric arc sample from OPPOSITE ends in
   opposite frames (`ortho_basis(n)` vs `ortho_basis(−n)`); the interior
   Steiner angles are not bit-equal (`φ0 + sweep` ≠ `φ1` in f64), so the
   two chains' points differ by ULPs and the bit-exact mesh interner
   leaves them unwelded — femto tears.

## 2. Branch table

| # | Branch | Behavior |
|---|---|---|
| B1 | Stage-6 directed edge copy, periodic curve (Circle/Ellipse), start≠end, CCW sweep around stored normal > π | negate the stored normal on THAT copy (the kernel-v2 twin convention: same point set, opposite traversal) — every emitted arc spans one mesh chord, so the geometric piece is always the minor side |
| B2 | sweep < π | curve copied unchanged (today's path) |
| B3 | sweep within 1e-6 of π | ambiguous — leave unchanged (matches kernel-v2's `ARC_MINOR_AMBIGUITY_BAND` posture; mesh chords are orders of magnitude below π) |
| B4 | Stage-1 circle-arc chain: edge has a TWIN (another edge, same undirected endpoint ids, same center/radius geometry, complementary orientation) | build the chain ONCE for the canonical member (the copy whose `start < end` by vertex id); the twin receives the REVERSED chain vector — same vertex ids ⇒ bit-identical shared samples |
| B5 | Stage-1 circle-arc chain: no twin (kernel-v2-shared input edge) | today's path byte-identical |
| B6 | LineSegment / Hyperbola / SurfacePair curves | untouched (line: no sampling ambiguity in ids; hyperbola/surface-pair twins do not occur in the failing class — future increment if a case demands it) |

## 3. Invariants

- I1: every Stage-6 output Circle/Ellipse arc edge satisfies the Stage-1
  input convention (CCW sweep < π) — a yang boolean output is a valid yang
  boolean input.
- I2: twin arc copies of one geometric chord produce chains with the SAME
  vertex ids in reversed order (shared samples, weld-free conformality).
- I3: kernel-v2-converted inputs (shared single arc edges) tessellate
  byte-identically to pre-fix.
- I4: corpus zero-lost (kernel-v2's `from_yang_brep` derives arc sense
  itself — `<π → as stored, >π → negated` — so B1's flipped normals are
  transparent to the adapter path).

## 4. Oracles

- `yang-rs/tests/stage6_arc_orientation.rs`:
  - `output_arc_edges_satisfy_ccw_minor_convention` (RED → green): every
    split-arc edge of the pocket operand sweeps < π.
  - `pocket_operand_reenters_plain_boolean`: watertight re-entry.
- stage0 unit `t133_floor_conformality_probe` → promoted to a permanent
  watertight-emission assertion for the rj-fixture pocket operand (was:
  92 unbalanced edges).
- full assay P9 gate zero-lost (I4).

## 5. Research basis

- kernel-v2 `boolean.rs` module docs (PR-KV5b): the directional-normal twin
  convention and the minor-arc derivation this fix mirrors.
- [#24] Yang 2025 §4.4/§4.5: intersection curves carry exact geometry;
  shared boundary sampling is the watertightness mechanism.

## 6. Ledger

- 2026-07-11: spec written after probe-driven diagnosis (t133 probe:
  complementary-arc chains in the unrolled ribbon dump).
- 2026-07-11: B1–B3 implemented (`orient_directed_curve` in
  stage5_topology, both push sites). **B4/B5 (twin-shared chains) turned
  out UNNECESSARY**: with truthful orientation the twin copies' chains
  weld bit-exactly in the driver — `ortho_basis(−n) = (e1, −e2)` exactly
  and `atan2`'s odd symmetry makes the mirrored frames' sweeps bit-equal;
  interior samples agree at least within the Sterbenz band of the short
  per-chord arcs. B4/B5 stay documented as the contingency if a future
  direct-chain case tears at femto scale. The env-gated `YANG_T133_PROBE`
  ribbon dump is banked in `tessellate_lateral_holed_cdt`.
