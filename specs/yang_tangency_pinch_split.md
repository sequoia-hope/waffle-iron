# Yang output — tangency pinch-vertex split at the shell gate

Status: SPEC (2026-07-08, task #86); §0c FLIPPED ALWAYS-ON 2026-09-13. Corpus driver: C0058 (equal-radius
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
   re-points one sheet's triangles to the copies.

### 0b. BUILT and GATED OFF (`YANG_EDGE_PINCH_SPLIT=1`), 2026-09-13

The operation turned out to be an extension of the existing vertex-fan split,
not a separate pass. `split_pinch_vertices` already separates a vertex whose
star falls into ≥ 2 closed fans; all it lacked was a way to PAIR the triangles
on a 4-valent star edge, where it previously bailed outright. `edge_pinch_sheets`
supplies that pairing from the §0a certificate, and the existing union-find then
separates the chain on its own. Two things had to be right, and both are now
measured rather than assumed:

- **The certificate.** Four triangles, 2 + 2 by `InputId`, one forward and one
  reverse within each operand ⇒ pair each operand's forward with the other's
  reverse. Anything else returns `None` and the vertex is left to today's loud
  gates. Pinned by four unit tests in `tests_unit::m4_substitute` (the pairing,
  plus refusal of a same-operand 4-valent edge, of two forward triangles from
  one operand, and of an unattributed triangle).
- **The placement.** It must run at Stage-4 ENTRY. Run at the existing (4a2)
  site it is inert on F0060: the §4.4.1(b) merge has by then collapsed each
  chain, so only `(3,76)` and `(22,48)` still certify while `(48,79)` is
  `4 × B#2`, and a HALF-certified chain does not separate at all — a
  chain-interior vertex's fan ring needs BOTH of its pinch edges paired before
  it falls into two components. Measured: at (4a2) the certificate fires on the
  two and refuses the third, and the shell still reads χ = 3 with the same
  `v=45 e=128 f=86` as the baseline. At entry it splits **18 vertex copies** and
  F0060 stops being a `NonManifoldOutput` altogether.

**Why the gate is still OFF, measured — the gap is DOWNSTREAM, in the B-Rep
emission.** With the split armed F0060 COMPLETES but grades `SUPPORTED_WRONG`
(`V(1438) − E(4292) + F(2860) = 6` over what the oracle groups as 2 shells,
against the authored `euler_target` of 2). The first reading of that number —
"the split is partial, the line contacts separate and the point contacts do
not" — is REFUTED by the mesh itself: the split separates **all four** contacts.
`YANG_EDGE_PINCH_PROBE` reads *entry split 18 vertex copies* (the two seven-edge
line chains) and *4a2 split 2 vertex copies* (the two lateral tangent POINTS,
which the original vertex-fan rule handles once the chains are out of its way).

The loss happens after Stage 4. Dumping the assay's own final mesh
(`ASSAY_DUMP_STL`) and welding it independently at 1e-12: 2860 triangles,
V = 1434, E = 4294, χ = 0, with **two valence-4 edges and twelve valence-1
edges**. The two valence-4 edges are the tangent lines *at full length*,
(0, −0.3, ±0.3) → (0, +0.3, ±0.3), each carrying two cap triangles (one per
side) and two lateral triangles — and the whole bottom tangent line carries
exactly **two** vertices in the render, its endpoints. So kernel-v2's per-face
re-tessellation rebuilt the cap from its analytic surface and boundary loops as
geometry sharing ONE edge: the per-sheet split never reached the B-Rep. The
twelve valence-1 edges are four small triangular holes at the four lobe corners
by the point tangencies — the same story at a vertex.

**So the next increment is Stage-6 emission, not more mesh work.** The split
sheets have to become separate FACES with their own edges and loops, and the
point split has to close the loops it opens; only then does the χ question even
arise. The mesh-level operation is done and certified, which is why it is banked
here rather than abandoned.

(The oracle's own hybrid complex reads V = 1438, E = 4292, χ = 6 — a different
weld granularity from the independent 1e-12 one above. They disagree on details
and agree on the verdict: the armed output is not yet a clean set of closed
surfaces, so the gate stays off. A `SUPPORTED_WRONG` is strictly worse than the
honest `ERROR` it replaces, and 0W is enforced.)

The nest of zero-area triangles at the cap centre needed no special handling:
they are ordinary star triangles, and the certificate never looks at area.

### 0c. ADJUDICATED and FLIPPED ALWAYS-ON (2026-09-13, later) — the χ = 6 was the ORACLE's, not the output's

§0b left the split off because the armed output "reads χ = 6 against the
authored χ = 2" and concluded the per-sheet split "never reached the B-Rep".
Both readings are REFUTED by measuring one layer earlier and one layer later
than they did. Every number below comes from a command that ran.

**What yang emits (`KV2_RECOVER_PROBE`, the raw output before curve
recovery).** With the split armed, F0060's `A − B` leaves yang as **12 faces
in FOUR closed shells** — per lobe one cap half-disc (plane), one A-lateral
curvilinear triangle and one B-lateral curvilinear triangle (both cylinder
patches). Each lobe's tangent LINE is its own `LineSegment` edge (bottom-left
`e0` v53→v80 / `e4` v80→v53; bottom-right `e1` v22→v48 / `e5` v48→v22); the
two lobes' copies end at DIFFERENT vertices, femto-twins the arrangement
minted 6.3e-17 … 1.9e-16 apart (v48/v53, v22/v80, v10/v26, v50/v57) — not
split copies, ULP luck the ledger had already recorded. The tangent POINTS
are the split's copies proper: v33/v86 = (0.3, 0, 0) and v64/v87 =
(−0.3, 0, 0) at distance exactly 0. Per shell V − E + F = 4 − 5 + 3 = 2;
total 8. kernel-v2's `from_yang` pairs edges by (vertex INDEX pair, curve
key) and `recover_output_curves` reports `loop_chains=[None]` on all twelve
faces, so nothing is re-welded (I6 VERIFIED) and the solid assembles as four
shells.

**What the render carries (new `ASSAY_DUMP_OBJ=<dir>`, one `g face_<id>`
group per kernel face, f32 positions at full precision; exact-bit census in
Python).** 2860 triangles, 1438 unique vertices, 4296 edges, **12 valence-1
edges and nothing above 2**. The twelve are four zero-width T-junctions, one
at each lobe's corner near its tangent point: e.g. p1 = (−0.2954423,
−0.052094452, −0.052094452), p3 = (−0.28977776, −0.07764571, −0.07764571) on
the ellipse, and p2 = (−0.29261005, −0.06487008, −0.06487008), which is
bit-for-bit the MIDPOINT of p1p3 and lies 2.9e-4 off both cylinders. The
A-lateral face carries p1–p2–p3, the B-lateral face sharing that ellipse arc
carries p1–p3. That is the developable patch's DESIGNED boundary rule
(`tessellate/developable.rs`: "Boundary edges split ON their own straight 3D
geometry … the neighboring face's unsplit copy of the chord remains
closure-safe (T-junction)"), and the assay oracle heals exactly this class by
subdivision (`subdivide_t_junctions`). So §0b's "two valence-4 edges" were
the 1e-12 weld fusing the femto-twin line endpoints, and its "twelve
valence-1 edges" are the render's normal T-junctions. Neither is a defect.

**Where the 6 came from.** Under exact-bit keys the render has TWO
edge-connected pairs: the bottom-left and top-left lobes share exactly one
vertex, (−0.3, 0, 0) (faces 340, 341, 342, 345 all use it), likewise the
right pair at (0.3, 0, 0). The χ oracle
(`check_mesh_euler_characteristic_with_shells`) counted shells as
VERTEX-connected components of the position-welded complex, so each pair
read as one shell of χ = 3 (two spheres identified at a point): 2 shells,
χ = 6, expected 2 + 2·(2 − 1) = 4 ⇒ `SUPPORTED_WRONG`. The position weld is
erasing Mäntylä duplication — the representation §6 says a manifold kernel
MUST use — and a render mesh carries no vertex ids with which to keep it.

**The fix is in the oracle (test-harness), and it is a representation
correction, not a band.** `shell_decomposition`: shells are the
EDGE-connected components of the triangles (two triangles are in one shell
iff they share an edge key: the exact key where the edge pairs exactly, its
T-subdivided cell keys on the residue path — so the walk crosses a
one-sided chord split the same way the pairing does); each welded vertex is
counted once PER SHELL it touches (`pinch_extra` = Σ_v (shells at v − 1),
the copies the weld removed). Both the exact-bits path and the hybrid path
use it. It never demotes a currently-correct verdict: for genus-g shells,
old-correct means 2C_new − 2g − P = target + 2(C_old − meta) with
P = pinch copies and Q = C_new − C_old, i.e. P = 2Q, which is exactly the
new-correct condition. A pinch INSIDE one shell (two closed fans of the same
component) still reads one χ short; two sheets sharing an EDGE key stay one
non-manifold component (and unpaired for the watertight oracle). Pinned by
four unit tests: two cubes touching at a corner are two shells of χ = 2
(exact path, and the hybrid path with a one-sided T-vertex), two cubes
sharing an edge still fail, a corner-touching third cube next to an
edge-sharing pair credits only the distinct-component corner. Detail strings
gain `+N pinch` ONLY when N > 0, so every other verdict's detail is
byte-identical.

**Result.** Armed F0060: 4 shells, V 1438 + 2 pinch, E 4292, F 2860,
χ = 8 = 2 + 2·(4 − 1) ⇒ **`SUPPORTED_CORRECT` (2.0 s release)**. So the
answer to §0b's "one body or four?" is: four closed shells of one solid,
which is what a manifold B-Rep kernel has to say about a line- and
point-pinched point set, and the corpus oracle now says it too. The gate is
FLIPPED: `edge_pinch_split_enabled()` is on unless `YANG_EDGE_PINCH_SPLIT=0`.
Corpus after the flip (release, 8 jobs, 600 s; wall 745.8 s at host load
< 1; F0085 328.3 s, R0044 294.7 s, F0065 111.2 s): **290C / 0W / 15E / 4EE /
0T + 3 UNSUPPORTED(coplanar-boolean)** — exactly ONE move (F0060 ERROR →
SUPPORTED_CORRECT), zero detail moves; the split is inert wherever no edge
carries the 2 + 2 certificate, and the oracle's shell rule changes no verdict
without a pinch.

**Known fragility, named and not fixed.** The two lobes' line edges are
distinct in the render only because their endpoints are femto-twins. A line
pinch whose chain ENDPOINTS were bit-identical per sheet would render as one
4-valent exact edge and fail the watertight oracle: the oracle has no
per-sheet pairing for EDGES (the certificate problem `edge_pinch_sheets`
solves in the mesh has no render-side counterpart, because a render mesh has
no attribution). No corpus case exercises it; if one appears, the render-side
answer is per-shell vertex emission, not a weld band.

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
