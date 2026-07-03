# M8 — Stage-0 inputcheck-clean overlap emission (general-M8 item (v))

**Status:** spec (FIP Phase 1) — §2 Measured mechanism TBD (Increment 0 of the
cycle; FIP pre-implementation amendment). **Change class:** bug fix
(modeling-related), M8 workstream. **Crates:** `yang-rs` (`stage0.rs` emission
mechanics; the fix), `cherchi-rs` (diagnostic census module; oracle only),
`test-harness` (diagnosis + trackers).

## 1. Goal

Every operand mesh Stage-0 hands to `backend.labeled_arrangement`
(`yang-rs/src/lib.rs`, the native-boolean entry) individually satisfies the
five Cherchi input axioms — manifold, watertight, locally consistent winding,
globally oriented, self-intersection free (`mesh_booleans_inputcheck`,
reference `main-inputcheck.cpp`) — **whenever its solid's Stage-1 mesh does**
(conditional contract: chained inputs can arrive with pre-existing N22-class
subdivision structure Stage-0 neither introduces nor repairs).

Today the rim-carrying crossing configs violate axioms 3+5
(`Local Orientation: failed` + `Intersection: failed`, measured in the
`cherchi_patch_label_tolerance` cycle §6a). Downstream, cherchi's coplanar
dedup then yields a `[A,B]` sheet whose borders to single-label regions are
MANIFOLD (2-incident) instead of non-manifold, the L2a compatible flood
(deviation N23) legitimately crosses them, and the patch-level in/out verdict
keeps a wrong set — R0046/R0088's kept mesh carries open holes along the
A-cylinder risers plus `tris=3` double-cover strips, rejected by kernel-v2 as
`InvalidBooleanOutput("an undirected output edge is not used by exactly two
directed edges")` (`specs/yang_kept_mesh_manifold_gate.md` §2, the P10 record
that names this spec as the root fix). Reference parity proved the C++ release
produces the same defective output on the same operand meshes — the port is
faithful; the INPUT contract is what this spec restores.

**Acceptance cases:** R0046, R0088, F0063 (roadmap §0.2 item 1, remaining
item (v)). F0063 is "chained-input" class: if Increment-0 measurement shows its
violations are entirely inherited from its pre-Stage-0 meshes, it is descoped
here explicitly (recorded in §2) and R0046/R0088 remain the acceptance pair.

**Architecture pinned (pressure-tested):** the duplicate-and-dedup emission —
`mesh_a` = AOnly+Overlap, `mesh_b` = BOnly+Overlap, Overlap triangles
bit-identical from one shared `coords` array, B winding-swapped iff `opposite`
— IS the correct Cherchi adaptation of Yang §4.5.5 ("identical meshes are
generated for both models"): each operand must stay a closed shell for the
per-input ray-cast, and Watertight is itself one of the five axioms, so
"emit the overlap once as a shared sheet" would violate the contract by
construction. This spec does NOT restructure the emission; it makes each
emitted operand individually clean so the dedup'd sheet borders come out
non-manifold and the flood stops there naturally.

## 2. Measured mechanism (2026-07-03, diagnosis harness
## `test-harness/tests/m8_stage0_operand_diagnosis.rs`)

**Every violation is INTRODUCED by Stage-0: all six `_pre` Stage-1 meshes of
the failing ops are five-axiom clean (sidecar + native census agree).** F0063
is therefore fully IN SCOPE (the "chained-input inherited" descope hypothesis
is disproven). Two mechanisms, both measured:

**M-A — cluster-domain split drop (the dominant class; R0046 B-side,
R0088 op(a=718,b=3036), F0063 ops 0-1).** The §2b/§2c in-frame clustering
(`cluster_frame_coords_rim_aware`) rewrites the pair's 2D polygon
coordinates and remaps the corner/rim KEY MAPS — but `va`/`vb` (the snapped
3D vertex arrays) are untouched, and `collect_edge_splits` re-projects each
B-Rep edge's endpoints RAW (`frame.project(coords[lo])`, stage0.rs) into
`ExactPoint2`. At every vertex the clustering moved (even 1 ULP), the exact
collinearity test `dx·wy − dy·wx ≠ 0` fails for every overlay vertex on that
edge's subdivided chain → ALL splits on the edge are silently dropped → the
coplanar face's override carries the subdivided chain while each adjacent
face keeps its original unsplit edge. Census signature (measured): k unpaired
sub-edges on the coplanar face + exactly 1 unpaired (whole) edge per affected
neighbor face + improper T-junction contacts between the neighbor's triangles
and the override's (F0063 op0: 567 boundary edges = 471 on the gear face +
~96 neighbors × 1; 391 improper pairs; R0088 big op: 1204/292; R0046 B: 8+2).
Causal verification: `YANG_CLUSTER_PROBE=1` shows R0046's pair moved exactly
ONE corner (B[3], 1 ULP per axis) and the defects sit precisely on that
corner's two incident edges (neighbor faces 3 and 4); R0088 moved 219
coordinates ↔ 1204 holes. The final proof is F-A itself (GREEN increment 1):
if aligning the domains does not close these holes, P10 STOP.

**M-B — many-to-one 2D→3D resolution collapse emitted as degenerate
triangles (R0046 both sides).** The overlay-vertex resolution (corners → rim
maps → `snap_eps2` rim snap → exact rim-chord minting) deliberately
identifies femto-split 2D vertices to ONE 3D point (the §2c "2D twins resolve
to shared 3D coords" measurement). `tris_for` then emits triangles whose
resolved corners coincide: `[u,u,v]` index-degenerate triangles (R0046: 4 on
A/face 0, 10 on B/face 1, in mirror pairs `[2,7,7]`/`[7,2,2]` — collapsed
femto-quads), which carry zero area, so the region they should cover is
missing → holes + a pinch vertex. The N2-3a fold-validity revert loop cannot
repair these: it only reverts MINTED vertices, and the collapse also arrives
via the un-minted snap/corner/rim paths.

**Sidecar↔census calibration (measured on all 24 operand meshes):** verdicts
agree at the clean/dirty level everywhere; Global Orientation sign convention
confirmed (all-positive component volumes ⇔ sidecar pass). Per-axiom
differences are explained and intended: cinolib lets degenerate tris corrupt
its Manifold/LocalOrientation/Intersection verdicts, while the native census
reports them in their own `index_degenerate` bucket and keeps the edge census
over well-formed triangles.

**Scoping caveats (measured):**
- R0088's SECOND failing subtract (a=50/94, b=32/58) emits five-axiom-CLEAN
  operands yet also walls on kernel-v2 edge pairing — that op's root is NOT
  operand cleanliness and stays a residual (re-measure after GREEN; I5 for
  R0088 accordingly means "the defective-operand op's wall falls"; the case
  may retain a second, separately-diagnosed wall).
- The dump's process-global op counter produced DIFFERENT op orderings for
  R0088 between the combined and standalone runs — ops must be identified by
  content (mesh sizes/meta), not counter; whether this reflects true engine
  nondeterminism (A4.2) is a separate question, out of scope here.
- Banked fixtures for the RED census tests:
  `cherchi-rs/tests/fixtures/{r0046,f0063}_stage0_emission_{a,b}.obj`
  (R0046 = M-B + small M-A; F0063 op0 = massive M-A; R0088 adds no distinct
  mechanism and is covered by the E2E tracker). The older
  `r0046_stage0_{a,b}.obj` bank is the patch-label cycle's pinned input
  (pre-§2c emission) and stays untouched.

## 3. Parameters

None new. No tolerances, no epsilons (A14.3) — the emission fix must be
combinatorial/exact. `YANG_STAGE0_DUMP_DIR` (new, diagnostic-only env var) is
not a modeling parameter.

## 4. Branch table (emission path × contract)

| # | Path | Trigger | Contract row |
|---|------|---------|--------------|
| E1 | Pure-polygon overlay pair | `rim_a`/`rim_b` empty | Five axioms preserved; Overlap bit-identical across operands (already the passing population — byte-identical emission, I4) |
| E2 | Rim-carrying overlay pair | disc rim in pair; `collect_rim_crossings` → `rim_overrides` → `stage1_tessellate_with_rim_overrides` | Lateral + opposite-cap re-tessellation conformal with the cap override: no riser T-junctions, no folds, wholesale replacement of each rewritten face's tris |
| E3 | Fold-validity revert (N2-3a, `stage0.rs` revert loop) | minted rim vertex reverted to chord lift | ONE coordinate per overlay vertex per solid — every consumer (cap override, rim overrides, edge splits) reads the same final coordinate |
| E4 | Disc-pair direct builder (`build_disc_pair` → `DiscPair::Handled`) | disc×polygon / disc×disc containment | Winding parity with `opposite`; no duplicate cap triangles |
| E5 | Edge-split neighbor re-triangulation (`collect_edge_splits` → `triangulate_ring` / `edge_split_curved_face`) | overlay subdivides a shared B-Rep boundary edge | Ring tessellation consumes exactly the overlay's boundary points, oriented with the face normal; conformal with the rewritten coplanar face |
| E6 | Coincident-cylinder membrane (`coincident_cylinder_stage0`) | lateral×lateral coincidence | OUT OF SCOPE this cycle — non-regression only (gear-flange suite green) |
| E7 | **[M-A fix]** Pair whose clustering moved ≥1 polygon coordinate | `collect_edge_splits` (and any other consumer re-deriving 2D from `va`/`vb`) operates in the CLUSTERED domain — endpoint projections pass through the same pre→post key map as the overlay input, so the exact collinearity test sees one coordinate world and splits propagate. Pairs with zero moved coords: byte-identical path. |
| E8 | **[M-B fix]** Emission triangle whose resolved 3D corners carry <3 distinct bit-patterns | DROPPED from that mesh's override (never emitted as `[u,u,v]`); the identification means the 2D sliver's 3D image is degenerate, so its neighbors' edges pair directly. Triangles with 3 distinct resolved corners: emitted unchanged. |

(Measurement selected M-A/M-B (§2); rows E2/E5's rim-lateral and
ring-retriangulation machinery were NOT implicated beyond consuming the
outputs of the two roots. GREEN must audit the remaining raw-projection
consumers (`collect_rim_crossings`, `rim_chord_ctx` inputs) for the same
M-A domain mismatch and either fix under E7 or record why exempt.)

## 5. Invariants

- **I1 (conditional cleanliness):** Stage-0 introduces NO new five-axiom
  violations: for each solid, every defect class count in the emitted operand
  ≤ the count in that solid's `_pre` Stage-1 mesh; and when the `_pre` mesh is
  clean, the emitted operand passes all five axioms.
- **I2 (Overlap bit-identity):** Overlap-region triangles are bit-identical
  across the two operands (same f64 coordinate triples, same connectivity),
  modulo the single documented winding swap iff `opposite`. (This is what
  cherchi's exact dedup keys on; the `r0046_patch_label_parity` volume/area
  parity gates it end-to-end.)
- **I3 (intra-operand conformality):** within each operand, borders between
  Stage-0-rewritten faces and their neighbors are conformal — every shared
  boundary edge appears with identical vertex chains on both sides (no
  T-junctions, no folds across the border).
- **I4 (non-regression, byte-identical):** pairs whose current emission is
  already inputcheck-clean emit byte-identical meshes after the fix.
- **I5 (E2E acceptance):** R0046 and R0088 lose the kernel-v2
  `InvalidBooleanOutput` undirected-edge wall (success or a DIFFERENT loud
  typed error); F0063 per its §2 scoping. Full assay: 0 SUPPORTED_WRONG, no
  SUPPORTED_CORRECT lost vs the cycle baseline.
- **I6 (determinism):** dump/census are read-only observers; emission remains
  deterministic (no iteration-order dependence introduced).

## 6. Oracles

- **Native census** (`cherchi_rs::inputcheck`, new module — diagnostic oracle,
  NOT a production gate): localized defect lists — non-manifold edges,
  boundary edges, misoriented (same-traversal) manifold edge pairs, duplicate/
  degenerate triangles, bit-identical vertex twins, improper exact
  intersections among non-vertex-sharing triangle pairs, component count +
  per-component signed orientation. Unit-tested on synthetic fixtures (closed
  cube passes; punched hole; flipped tri; overlapping pair).
- **Sidecar** `mesh_booleans_inputcheck` via `cherchi_sidecar_rs::inputcheck`
  (binding reference; `#[ignore]` + loud-panic-if-missing per the established
  parity convention). Oracle-vs-oracle: native census verdict must agree with
  the sidecar's five-line verdict on the banked fixtures (also calibrates the
  Global-Orientation sign convention).
- **Banked operand fixtures:** `cherchi-rs/tests/fixtures/r0046_stage0_{a,b}.obj`
  (existing; verify = failing op via byte-diff) + newly banked R0088 (and
  F0063 if in scope) failing-op operands. RED census tests on these are the
  sidecar-independent red phase.
- **E2E trackers:** new `test-harness/tests/m8_stage0_inputcheck_campaign.rs`
  pinning the verbatim current walls (kernel-v2 undirected-edge string for
  R0046/R0088; F0063's measured wall). GREEN = success or a different loud
  typed error. The retired-LabelMismatch trackers in
  `m8_rim_clustering_campaign.rs` stay as permanent non-regression.
- **Parity:** `r0046_patch_label_parity` (native vs C++ volume+area) re-run
  after any emission-byte change, on re-banked fixtures (a fixture refresh is
  recorded here, never a tolerance change).
- **Full assay** on a quiet box vs the banked baseline
  (`target/assay_kv2_report.baseline-m8stage0.json`): ≥83 SUPPORTED_CORRECT,
  0 SUPPORTED_WRONG, zero CORRECT lost; F0016/F0024 (N22-sensitive sentinels)
  explicitly diffed.
- **Corpus operand sweep** (dev-only example, post-GREEN):
  per-case Stage-0 operand five-axiom TSV →
  `docs/audits/stage0_operand_inputcheck_sweep_<date>.tsv`, quantifying the
  introduced-vs-inherited residue corpus-wide.

## 7. Failure modes / P10 stop criteria

- **Measurement stops (Increment 0):** operands measured CLEAN on the actual
  failing ops → the named root is falsified → ABORT the cycle and record here
  (the `yang_kept_mesh_manifold_gate.md` §2b pattern). Violations entirely
  inherited from `_pre` meshes → root is Stage-1/chained-input, not Stage-0
  emission → re-scope under a new spec. (F0063-only-inherited is NOT a stop:
  descope F0063, proceed on R0046/R0088.)
- **GREEN stops:** operands reach sidecar-verified cleanliness but the
  kernel-v2 wall persists → the §6a causal chain (dirty operands → L2a flood
  → wrong kept set) is falsified at its last link → STOP, amend §2; do NOT
  chase the kept set with output-side repair (that is the aborted gate's
  territory). Any fix shape requiring a tolerance/epsilon to choose which
  triangles to emit/drop → STOP (P9/A14.3; curved-rim bit-sharing is
  load-bearing — kv9 lens-tip lesson). Overlap bit-identity broken (parity
  volume/area diverges) → wrong fix shape, revert.
- **Expected loud residue:** inputs already carrying five-axiom violations
  (inherited) keep their downstream walls unchanged; E6 membrane path
  unchanged; the general-M8 remainder (curved pairs, n-ary, holed discs)
  keeps its typed `CoplanarFacesUnsupported` walls.
- **No production gate** (design decision, mirrors two P10 records): a runtime
  five-axiom check before the backend call would (a) pay an O(n²)-with-AABB
  exact intersection test on every boolean, and (b) false-positive on
  legitimately-chained inputs whose collinear chains subdivide differently
  (N22 class — the exact population that killed the kept-mesh gate,
  `yang_kept_mesh_manifold_gate.md` §2a/§2b). kernel-v2's post-subdivision
  edge pairing remains the honest production wall; enforcement here is
  dev-only oracles + trackers.

## 8. Research basis

- Yang 2025 §4.5.5 [#24] (`refs/text/yang2025_hybrid_boolean.txt:718-732`,
  Fig. 16 caption `:752-758`): the 2D Boolean before discretization; "identical
  meshes are generated for both models in this part"; "The common part and the
  other two parts share identical sampling points on their boundaries" — the
  shared-boundary-sampling invariant I2/I3 restore.
- Cherchi 2022 [#38] input contract
  (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:236-237`:
  "manifold, watertight and with no self-intersections"; multi-mesh coplanar
  overlap explicitly supported `:242-245`; patches bounded by non-manifold
  intersection edges `:249-256`). The five-axiom operationalization is the
  reference `main-inputcheck.cpp` (Manifold / Watertight / Local Orientation
  = opposite traversal across each shared edge / Global Orientation = signed
  volume / Intersection = cinolib `find_intersections` empty).
- Prior records this spec completes: `specs/cherchi_patch_label_tolerance.md`
  §6a (names this fix), `specs/yang_kept_mesh_manifold_gate.md` §2/§2b (P10
  abort; why no mesh-level gate), `specs/m8_shared_boundary_identity.md` §8c
  (kept-set diagnosis), deviation N23 (L2a flood semantics), N2-3a (fold-revert
  history).

### 8a. Analytical vs approximate

Not applicable — no SSI, no surface approximation. The fix is exact/
combinatorial emission hygiene; all geometry stays on the existing exact
overlay + exact rim-mint machinery.
