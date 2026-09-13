# Yang output — tangency pinch-vertex split at the shell gate

Status: SPEC (2026-07-08, task #86). Corpus driver: C0058 (equal-radius
cylinders crossing at 30°, coplanar axes, UNION; exact Steinmetz-family
volume 2.08191… in its meta) — today `NonManifoldOutput` from the
`s4-shell-euler` gate with χ=1 (v=90, e=267, f=178). This is the banked
KV9-F1 follow-up ("Stage-6 boundary-walk figure-eight (union)").

## 0. Goal

The union of two solids whose surfaces meet TANGENTIALLY has a boundary
that self-touches at isolated pinch points (for C0058: the 2 points where
the equal-R cylinders are tangent; both intersection ellipses pass through
them). The mesh boolean legitimately produces a vertex whose triangle star
is TWO closed fans (an hourglass/pinch) — not 2-manifold as one vertex,
but a perfectly valid solid whose standard B-Rep representation is one
vertex PER SHEET at the same position.

Today's behavior depends on bit-level luck (PREMISE CORRECTED during the
red phase — measured by the Test Author):

- **Corpus path (C0058 via kernel-v2 tessellation):** ASYMMETRIC — one
  junction welded (χ −1), the other ULP-split; the shell gate sees the
  impossible χ=1 and stops loud (`NonManifoldOutput`).
- **Direct symmetric fixtures (yang-rs unit level):** BOTH junctions
  welded → χ=0, which the even-χ gate ACCEPTS as a genus-1 shell — a
  pinched sphere silently reads as a torus. Watertight and volumetrically
  fine, but topologically WRONG output (P9 silent-wrong class).

The split fixes both: every pinch presents uniformly as per-sheet
vertices and the shell measures the honest χ=2.

**Named out-of-scope sibling (roadmap follow-up, not this slice):** the
PERPENDICULAR equal-R union welds the tangency along a shared mesh EDGE
(2 undirected edges with 4 incident triangles) — an EDGE pinch the
vertex-fan split cannot and must not touch (§2 row: non-manifold edges
unchanged). It currently emits Ok with χ=0 + 2 non-manifold edges
(tolerated by the fwd=rev pairing rule).

### 0a. The EDGE pinch, located and specified (2026-09-13, F0060)

Measured with the new `YANG_NM_EDGE_PROBE` census (the over-2 edge list with
each incident triangle's `(input, face)`, printed at the six Stage-4
checkpoints). **F0060 is the corpus driver for this sibling**, and the
measurement moves the work earlier than the earlier ledger reading assumed.

F0060 is A = cylinder r 0.3 with caps at z = ±0.3, minus B = cylinder r 0.3 on
the y-axis. B's lateral is tangent to BOTH cap PLANES along a whole diameter
(x = 0), so `A − B` is genuinely LINE-pinched there: just above a cap the
removed lens has half-width √(0.6ε), so the two halves of the solid meet only
on the line itself.

1. **The pinch is already in the ARRANGEMENT.** At `s4-entry` the census reads
   **0 open edges and 14 over-2 edges** — every one of them on one of the two
   tangent lines, none anywhere else. So no Stage-4 pass creates it; the exact
   mesh boolean hands it over, which is the honest output for a line-pinched
   solid. (The §4.4.1(b) merge then *reduces* the count to 3 — it is not the
   cause, which corrects the 2026-09-13 addendum's reading.)
2. **The split rule is FORCED — no radial sort, no dihedral, no tolerance.**
   Each of the 14 carries exactly four triangles with the signature
   `2 × (A, cap face) + 2 × (B, lateral)`, and within each operand one is
   forward and one reverse on the edge. A sheet is `cap ∪ lateral`, so pairing
   each A triangle with the B triangle of OPPOSITE orientation is the only
   consistent partition. This matters because the two B triangles are ZERO-AREA
   (they lie in the cap plane — the surfaces meet tangentially), so first-order
   dihedral sorting is degenerate here by construction: the attribution +
   orientation certificate is what replaces it.
3. **Therefore the split must run at Stage-4 ENTRY, before the §4.4.1(b)
   merge.** After the merge only 2 of the 3 survivors still carry the
   discriminator: `(3,76)` is `2×A#0 + 2×B#2` and `(22,48)` is
   `2×A#1 + 2×B#2`, but `(48,79)` — the whole top diameter, the merge having
   collapsed that chain into one edge — is `4 × B#2`, with no A/B split left to
   read. The merge is not wrong to fuse ULP twins; it simply destroys evidence
   the split needs, so the split has to come first.
4. **Shape of the operation.** The over-2 edges form two CHAINS along the
   tangent lines (bottom: v1—v0—v20—v18—v16—v14—v11—v10, seven edges; top:
   v21—v22—v27—v29—v31—v33—v35—v36, seven edges), each running rim to rim.
   Splitting a chain duplicates its INTERIOR vertices (the endpoints are where
   the two sheets genuinely rejoin around B's end cap and must stay shared) and
   re-points one sheet's triangles to the copies. The nest of zero-area
   triangles at the cap centre is the part that needs care, and is why this is a
   slice of its own rather than an extension of the vertex-fan split.

After this slice: a **pinch-vertex split** pass runs on the output mesh
before the shell gate — every vertex whose star decomposes into ≥ 2
edge-connected fans, EACH a closed disk, is split into one vertex per fan
(identical positions). Both C0058 junctions then present uniformly as
per-sheet vertices; the shell gate measures the honest χ=2 sphere; stages
5/6 walk per-sheet seam chains.

## 1. Parameters

No new tunables. The split is purely combinatorial (triangle-star fan
decomposition); no positional tolerance is involved.

## 2. Branch table

| Output-mesh vertex star | Today | After |
|---|---|---|
| single closed fan (ordinary manifold vertex) | passes | byte-identical |
| single OPEN fan (boundary/defect) | loud gate failure | unchanged (loud) |
| ≥ 2 fans, ALL closed disks (pinch) | χ gate failure (loud) | **split: one vertex per fan** |
| ≥ 2 fans, any fan open/broken | loud gate failure | unchanged (loud — the guard) |
| non-manifold EDGE (≠2 incident triangles) | loud pairing failure | unchanged (loud, checked before the vertex pass) |

## 3. Invariants

- **I1 (honest split):** a vertex is split ONLY when every fan of its star
  is a closed edge-connected disk. Anything else keeps today's loud error
  (P9 — the split must never mask a genuine defect).
- **I2 (positions):** split copies carry the IDENTICAL position bits; no
  point moves.
- **I3 (Euler accounting):** each split of a k-fan pinch vertex raises V
  by k−1 and χ by k−1; a sphere pinched at one point (χ=1) measures χ=2
  after the split. The shell gate itself is UNCHANGED.
- **I4 (downstream):** stages 5/6 consume the split mesh; seam polylines
  terminate per-sheet at the split copies (an intersection curve passing
  through the pinch is cut there); output faces' trim loops close within
  their sheet.
- **I5 (no-op on manifold outputs):** any output with no pinch vertices is
  BYTE-IDENTICAL through the pass (the entire green corpus).
- **I6 (kernel-v2 re-entry):** from_yang must not positionally re-weld the
  coincident copies (verify; if a weld exists it must be keyed by vertex
  id, not position bits).
- **I7 (determinism):** fan enumeration and split-vertex id assignment in
  deterministic (triangle-index) order.

## 4. Oracles

- **Canonical (yang-rs unit):** two equal-R cylinders at 30°, coplanar
  axes, UNION (adapt the KV9-F1 steinmetz fixture from subtract to union):
  boolean succeeds; output watertight; per-shell χ=2; mesh volume within
  the chord band of the analytic union volume (V₁ + V₂ − V∩ with the
  Steinmetz-form intersection); exactly 2 position-duplicate vertex pairs
  (the pinches).
- **Corpus (P9 gate):** C0058 ERROR → SUPPORTED_CORRECT on its exact
  meta volume; the assay pin flips. Zero CORRECT lost.
- **Branch coverage:** a broken-fan fixture (hand-built mesh with an open
  fan at a shared vertex) still fails loud (the I1 guard); an ordinary
  green boolean byte-identical (I5).
- **Mutation (adversary):** weaken the closed-fan guard (split
  unconditionally) → the broken-fan fixture must catch it; skip the split
  → the canonical test catches NonManifoldOutput.

## 5. Failure modes

- Broken star (open fans, isolated triangles): today's loud
  `NonManifoldOutput` with the `NONMANIFOLD_SITE_PROBE` site preserved.
- Non-manifold edges: unchanged loud pairing failure.

## 6. Research basis

- [#24 Yang 2025 §4.3.3] tangent points are first-class (collinear-normal
  test at intersection optimization) — the pipeline already computes and
  relocates them (KV9-F1 tangency junction band, shipped).
- Pinched-boundary solids in manifold B-Rep kernels are canonically
  represented by per-sheet coincident vertices (Mäntylä [#23] — manifold
  data structures represent non-manifold point-set solids by topological
  duplication). The split is that representation at the mesh level, applied
  uniformly instead of by ULP luck.
- The output 2-manifold CONTRACT (yang crate rule 4) is preserved — the
  split output IS 2-manifold.

## 7. Analytical vs. approximate

No geometry changes; combinatorial topology only. The tangent points
themselves come from the existing exact junction machinery (KV9-F1).

## 8. Design

One pass over the FINAL output mesh (the same mesh the shell gate and
stages 5/6 consume), immediately BEFORE `check_watertight_2manifold`'s
shell-euler accounting (after directed-edge pairing, which must still run
first and stay loud on unpaired edges):

1. Build vertex → incident-triangle lists.
2. For each vertex v: group its triangles into edge-connected components
   via shared v-incident edges. One component → skip.
3. For each component, verify the fan closes: the v-incident edges of the
   component each appear in exactly 2 of its triangles (a closed disk
   around v). Any violation → keep today's loud path untouched.
4. Split: component 0 keeps v; each further component gets a fresh vertex
   with v's position bits; rewrite its triangles' indices.
5. Re-run the split until fixpoint (a split cannot create new pinches, so
   one pass suffices — assert in debug).

The pass lives next to the shell gate so every consumer (gate + stage 5/6
walks) sees the same split mesh.
