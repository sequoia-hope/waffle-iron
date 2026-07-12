# M8 — Intentional merge of a rim-crossing override onto a uniform rim sample

**Status: SPEC (2026-07-12).** Task #143, the named follow-up of the
fused-emission collapse (task #142, `specs/m8_overlay_fused_emission_collapse.md`).
**Crate:** `yang-rs` (`src/stage1_tessellate.rs` only — the two
"silent merge refused" sites; no Stage-0 wiring changes).
**Corpus target:** C0048 (`circle edge 0: rim-crossing override at
angle-offset 5.385587406153931 coincides with uniform sample k=12 (silent
merge refused)`).

## 0. Measured context (Manager diagnosis, 2026-07-12)

C0048 probe (`KV2_RIM_MERGE_PROBE`, this session): the refused override is the
task-#142 fused survivor — an input-loop vertex of the ULP-split mirrored
14-gon twin rim. It sits **3 ULPs** off this body's own uniform sample
(x bits `…66b7` vs `…66ba`, y bits `…e4bb` vs `…e4b8`, 3D distance 7.4e-16),
at uniform slot k=12 of n_seg=14. The two bodies MUST share this one exact
point on the overlap boundary (Yang §4.5.5 identical-mesh requirement) — the
overlay already carries it; the refusal was authored when a coinciding
override always meant an upstream bug, which task #142 made obsolete: the
fusion deliberately produces overrides that land on (within ULPs of) a rim's
own uniform samples.

## 1. Goal

`stage1_tessellate_with_rim_overrides` must **deliberately merge** an
override point that angularly coincides with a uniform rim sample when the
two are identical at the fused-emission scale: the uniform slot's computed
sample is **replaced** by the override's exact bits (the shared fused point),
so cap overlay, rim ring, and lateral all use ONE 3D point. Ring length is
unchanged (replacement, not insertion) so the uniform index-pairing lateral
path stays valid and the rim is NOT routed to azimuth-merge. Coincidences
that are NOT sub-resolution-identical (real-scale) remain the loud typed
wall — fail closed.

## 2. Parameters

No new public inputs. Constants, from the centralized tolerance policy
(A14.3, no ad-hoc epsilon):

- **Angular trigger** (unchanged): `merge_tol = uni_step · 1e-6` — the
  existing coincidence-detection band at both sites.
- **Identity ceiling** (new): the merge is allowed only if the 3D distance
  between the override and the computed uniform sample point is
  `< cad_primitives::TAU_MODEL` (1e-7) — the task-#142 fused-emission
  eligibility constant (A14.2: nothing below TAU_MODEL is a representable
  feature). Computed in f64 on two f64 points (an eligibility ceiling on
  ULP-vs-real-scale, not an exactness predicate; the compared magnitudes are
  ~1e-16 vs 1e-7, nine orders apart).

## 3. Branch table

Both sites — the full-circle rim ring and the arc chain (`e_start != e_end`)
— get the same rows. "Slot" = a uniform Steiner sample; the SEAM (full rim
slot k=0) and the ARC ENDPOINTS (`e_start`/`e_end`) are B-Rep vertices, not
slots — replacing their bits would desync every other edge/face sharing that
vertex.

| # | Condition | Behavior |
|---|-----------|----------|
| 1 | Override angularly clear of every uniform sample (`> merge_tol`) | INSERT (existing path, byte-identical) |
| 2 | Coincides with interior slot k≠0, 3D distance < TAU_MODEL, slot not already merged | **MERGE**: slot k's point becomes the override's exact bits; slot keeps its uniform angular key + theta; ring length unchanged; NOT added to `inserted_rims` |
| 3 | Coincides with interior slot k≠0, 3D distance ≥ TAU_MODEL | Loud `MalformedTopology` (unchanged wall, message now includes the distance) |
| 4 | Coincides with slot k already merged, bit-identical to the merged point | Dedup — skip |
| 5 | Coincides with slot k already merged, DIFFERENT bits | Loud `MalformedTopology` (two distinct overrides claim one sample) |
| 6 | Coincides with the seam / an arc endpoint, bit-identical to that B-Rep vertex's point | Dedup — skip (the point is already in the ring) |
| 7 | Coincides with the seam / an arc endpoint, different bits | Loud `MalformedTopology` (B-Rep vertex is authoritative; no corpus driver, fail closed) |
| 8 | Bit-identical to an already-INSERTED override | Dedup — skip (existing path; dedup keys for insertions stay separate from merge bookkeeping so a pure merge never sets `inserted_rims`) |
| 9 | Empty / absent override map | Byte-identical to `stage1_tessellate` (existing pin) |

Arc-path notes: `k_near = 0` means the arc START endpoint (row 6/7);
`off ≥ sweep − merge_tol` stays the existing outside-sweep refusal (reached
before any coincidence handling, unchanged).

## 4. Invariants

- I1 — A merge never changes ring/chain LENGTH: `n_seg` (or `m−1` interior
  arc slots) is preserved, so the uniform `(N−k)` lateral pairing and the
  cap fan indexing stay valid; a pure-merge rim is absent from
  `inserted_rims`.
- I2 — After a merge the ring contains the override's EXACT bits at slot k
  (§4.5.5 conformality: the ring vertex is bit-identical to the overlay
  mesh's copy of the point).
- I3 — A bit-exact merge (override == computed uniform sample) yields
  byte-identical `verts` and `tris` to the same call without that override.
- I4 — Fail closed: rows 3/5/7 are loud; in particular a real-scale
  (≥ TAU_MODEL) angular coincidence still errors (the pre-#142 wall
  semantics are preserved above the fused-emission scale).
- I5 — The no-override path stays byte-identical (row 9, existing pin).

## 5. Oracles

Unit (in-crate, `tests_unit/boolean_functional.rs`, driving
`stage1_tessellate_inner` directly):

- ULP-twin merge on a cylinder rim (row 2): ring length unchanged, twin bits
  present in the vertex pool, displaced uniform bits absent, edge NOT in the
  returned inserted set, mesh a closed 2-manifold (I1+I2).
- Bit-exact merge (row 2 degenerate): byte-identical verts+tris vs no
  override (I3).
- Real-scale coincidence refused (row 3): large-radius rim where
  `r · Δangle` at the trigger band exceeds TAU_MODEL → still
  `MalformedTopology` (I4).
- Same-slot conflict (row 5) loud; same-slot bit-identical repeat (row 4)
  dedups.
- Seam bit-exact dedup (row 6) + seam ULP-off refusal (row 7).
- Arc-chain interior-slot merge (row 2, arc site): chain length unchanged,
  twin bits present.

Corpus: C0048 leaves this wall (progresses past the rim-override refusal to
its next honest verdict); full assay zero-lost.

## 6. Failure modes

Rows 3/5/7: `YangError::MalformedTopology` with the edge index, angle
offset, slot k, and (row 3) the measured 3D distance vs TAU_MODEL.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.5 — the overlap region must carry IDENTICAL
  meshes on both models; overlap-boundary points are shared exactly. The
  merge propagates the one fused boundary point into this body's rim
  sampling instead of refusing to.
- Task #142 (`specs/m8_overlay_fused_emission_collapse.md`), [#51] Hoppe
  edge-collapse, [#52] snap-rounding family — the producer of these
  coinciding overrides; TAU_MODEL as the sub-resolution identity ceiling
  (A14.2/A14.3), same constant, same fail-closed framing.
- Constitution §7 parameter normalization — the merge resolves at the ONE
  site where overrides enter the ring; downstream (lateral, cap, opposite
  rim) sees ordinary uniform-count rings and needs no new branches.

### 7a. Analytical vs. approximate method

No surface-surface intersection is computed here; this is Stage-1 sampling
bookkeeping (which exact 3D point occupies an existing rim sample slot).
Method: exact bit propagation of an already-minted point; N/A for SSI
coverage.
