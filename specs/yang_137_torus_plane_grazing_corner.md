# SPEC — #137: torus∩plane grazing-loop CORNER refinement (C0065/R0074)

Status: **DESIGN** (no production code yet). Grounded in the 2026-07-15 probe +
resolution-sweep session (`docs/yang_deviations.md` "#137" + "#137 follow-up",
memory `session_2026_07_15_137_resolution_refutes_refinement`).

This is the faithful fix for the last torus∩plane failures. It is a two-part
increment; **do not ship part (a) without part (b)** — refinement alone converts a
correct loud STOP into a silent `SUPPORTED_WRONG` (proven, see §3).

## 0. Scope

- **In scope:** a torus∩plane intersection loop that is CLIPPED by a *second,
  transversal plane* of the same operand (a box notch face perpendicular to the
  cutting face). Corpus drivers: **C0065** (subtract), **R0074** (union).
- **Out of scope:** torus∩torus (M5, R0044/R0096), torus∩cylinder tangency
  (R0038 — same near-tangency family but no bounded-plane clip), and the general
  §4.5.2 loop for arbitrary surface pairs. This spec is the torus∩plane∩plane
  corner case only.

## 1. Paper requirement (the spec)

Yang 2025 §4.5 "Solving failures handling":
- §4.5.1 Optimize across boundaries (`...txt:672-690`): when correct points
  `v0`,`v1` bound an erroneous region ON THE SAME SURFACE, remove the bad points,
  reinsert a midpoint, and take *truncated* Newton steps that ride the point from
  surface `S2` across the shared boundary curve `Cb` onto `S1`.
- §4.5.2 Local refinement (`...txt:659-671`): when the failure is bounded by two
  points on boundary curves of *different* patches, INCREASE the mesh resolution
  of the surfaces traversed by the partial curve `Cp` (red regions) + a ring of
  neighbours (orange), recompute the mesh intersection ONLY in the refined
  region, and splice the improved curve in.
- Fig. 13 corner point `s` (`...txt:638-657`): where >2 surfaces meet, boundary
  points may glide to `s` and then go the wrong way → topology error. The exact
  corner (a 3-surface junction) must be pinned, not left to glide.

## 2. The case, precisely (C0065)

- Torus `T`: center `[0,0,0.5]`, axis **z**, R=1.2, r=0.3. Outer equator radius
  1.5 in the plane z=0.5.
- Box notch (subtract): `x∈[0.95,1.45]`, `|y|≤0.25`, z∈[−1,2]. Cutting face
  `Px: x=1.45`; clipping faces `Py±: y=±0.25`.
- True intersection `T ∩ Px`: a closed loop in the plane x=1.45, extent
  |y|≤0.384 (equatorial, z=0.5), z∈[0.334,0.666] (at y=0). It POKES OUT of the
  box in y: |y|=0.384 > 0.25.
- The loop crosses `Py−` (y=−0.25) at the exact 3-surface junctions
  **`T ∩ Px ∩ Py− = [1.45,-0.25,0.3723]` and `[1.45,-0.25,0.6277]`** (closed
  form: radial=√(1.45²+0.25²)=1.4714, z=0.5±√(r²−(radial−R)²)=0.5±0.1277).
  Symmetric junctions on `Py+` and on the x=0.95 wall.
- The correct B-Rep: on the `Px` face the surviving edge is the sub-arc with
  |y|≤0.25 (two arcs, top and bottom of the loop), terminating at those four
  corner junctions; the arcs with |y|>0.25 are replaced by the `Py±` face edges.
  Expected χ=2 (through-notch severs the ring).

## 3. Current behaviour & why refinement ALONE is not the fix

The torus∩plane solver (`relocate_onto_implicit_pair`, wired in the KV6d Tier B
block `stage4_correct.rs:3479+`) runs and is correct. The failure is topology:

- **Production density (equator rim n_seg≈12).** ONE mesh vertex lands exactly on
  the outer-equator ring (z=0.5 exact): C0065 v8=`[1.45,-0.219,0.5]`. Its nearest
  true-curve point is the equatorial extreme `[1.45,-0.384,0.5]`, so Newton drags
  it outside the `Px` face → bounded-face containment STOP `OffCurveBeyondChordBand`
  (`stage4_correct.rs:3737-3745`). The whole mesh loop is inside |y|<0.25, so the
  mesh cannot even represent the correct (clipped) topology — it thinks the notch
  fully contains the loop.
- **Forced fine density** (dev-only `YANG_NSEG_FLOOR`, sweep on C0065):

  | equator rim N | outcome |
  |---|---|
  | 12/24/48 | ERROR — loud STOP (correct) |
  | 64 | SUPPORTED_WRONG — 112 unpaired edges, χ=1 |
  | 96 | SUPPORTED_WRONG — 164 unpaired, χ=1 |
  | 160 | SUPPORTED_WRONG — 272 unpaired, χ=−1 |

  At N≥64 no vertex sits on the exact equator ring, so every relocation is tiny
  and the torus block PASSES — but the `Px` loop (mesh max |y|≈0.23) and the
  `Py−` face curve (separate mesh verts, e.g. `[1.44,-0.25,0.64]`) **never share
  an exact corner junction**, so the loop end dangles → unpaired edges → χ≠2.

**Conclusion:** the blocker is exact grazing-CORNER junction assembly, not
sampling. The coarse STOP is LOAD-BEARING — keep it until part (b) exists.

## 4. Design

### Part (a) — local grazing-band refinement (§4.5.2)

Trigger: the KV6d Tier B block detects a torus∩plane relocation escaping the
bounded partner face (today's STOP site, `stage4_correct.rs:3740`). Instead of an
immediate STOP:
1. Identify the grazing band on the torus: the toroidal θ-interval where the
   cutting plane `Px` is within `~2·d_ε` of tangency to the tube (the small-loop
   region; for C0065 θ∈[−16.6°,16.6°] about the +x equator).
2. Locally subdivide the torus tessellation in that θ-band (and a one-ring of
   neighbours) so the mesh `Px` loop reaches its true extent and crosses the
   clip planes `Py±`. Reuse `tessellate_torus_closed`'s (θ×φ) grid — insert extra
   θ-columns only in the band (do NOT globally raise n_seg: that is the sweep
   above, and it is both wasteful and, without (b), wrong).
3. Recompute the mesh intersection in the refined region only, splice the curve.

Guardrail: the band subdivision MUST stay conformal — the box mesh's `Px`/`Py`
faces must be re-sampled identically where they meet the refined torus band, or
Cherchi produces mismatched arrangements. (This is the §4.5.2 "orange ring".)

### Part (b) — exact corner-junction insertion + stitch (§4.5.1 + Fig. 13)

After (a) the mesh `Px` loop crosses `Py±`. For each crossing:
1. Compute the exact 3-surface junction `T ∩ Px ∩ Py` via
   `relocate_onto_implicit_triple(seed, T, Px, Py)` (already exists,
   `stage4_relocate.rs:268`). Seed = the mesh crossing vertex. Validate `F=0` on
   all three surfaces and that the junction lies on both bounded faces.
2. Split BOTH incident curve chains (the `T∩Px` loop edge and the `T∩Py` face
   edge) at that junction and WELD them to one shared arrangement vertex — the
   corner point `s`. This is the Fig. 13 pin: the corner is a fixed 3-surface
   vertex, not a gliding boundary point.
3. Trim the `T∩Px` loop arc with |y|>0.25 (it is replaced by the `T∩Py` face
   edge between the two junctions on that face).

Conformality (watertight key): steps 2–3 must be applied to BOTH operands'
attributions so the bijective map stays 1:1 and the half-edge pairing stays
balanced (cf. the N2/CDT two-sided conformality blocker,
`yang_n2_stage4_cdt_mesh_updating.md` §5c.8). Use the shared-vertex identity path
(`yang_rim_junction_insertion.md`) rather than re-deriving each side.

## 5. Acceptance criteria

1. **C0065 → SUPPORTED_CORRECT** (χ=2, watertight, positive volume, monotone).
2. **R0074 → SUPPORTED_CORRECT** (analogous, finer d_ε=8.56e-3).
3. **Full assay 0 WRONG**, no CORRECT regressions (baseline 241C/0W/49E; the two
   targets move ERROR→CORRECT ⇒ 243C/0W/47E).
4. Reference parity: the refined arrangement matches the Cherchi C++ sidecar in
   the refined region (`reference_sidecar_available_here`).
5. The load-bearing STOP still fires for any residual escape that (a)+(b) do NOT
   resolve (never silent-wrong).

## 6. Decomposition (ordered; each gates the next)

- **N-137.1** — corner-junction primitive: `torus_plane_clip_junctions(T, Px,
  Py, seed) -> Option<[Point3;≤2]>` around `relocate_onto_implicit_triple`, with
  unit tests pinning C0065's `[1.45,-0.25,{0.3723,0.6277}]`. Test-only, unwired,
  byte-identical production. (Safe foundation — the only piece whose signature is
  certain.)
- **N-137.2** — grazing-band detector: from the escape site, compute the toroidal
  θ-band and the clip planes. Probe `YANG_GRAZE_PROBE`. Test-only.
- **N-137.3** — local band re-tessellation (part a), gated
  `YANG_137_GRAZE_ENABLE`, off by default; prove full-assay byte-identical off.
- **N-137.4** — corner insert+stitch (part b) behind the same gate; the
  conformality core. Green C0065 with the gate ON before flipping default.
- **N-137.5** — flip the gate on; full assay + sidecar parity; un-quarantine.

## 7. Risks & guardrails (P9/P10)

- Part (a) without (b) = silent `SUPPORTED_WRONG` (§3). The gate must enable BOTH
  together; never ship (a) alone.
- No tolerance widening to "contain" the escaped vertex (the deleted-kernel S-H
  trap). The junction is EXACT (triple Newton to F=0); the arc trim is
  topological, not a band relaxation.
- Do not globally raise n_seg — it is wasteful and, per §3, wrong without (b).
- The current STOP stays until N-137.4 greens the case with the gate ON; only
  N-137.5 removes it as the default path.
