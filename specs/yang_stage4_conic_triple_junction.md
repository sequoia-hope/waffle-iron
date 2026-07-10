# Spec: Stage-4 general conic triple-surface junction relocation

> **Status (2026-07-10, increment 1 of the N2/F0059 epic COMPLETE — task
> #121): the Stage-6 "double-cover" origin is FOUND and FIXED, and both
> candidate origins in the 2026-07-08 block below are WRONG.** Measured on
> live code (KV15/KV15b/S7 landed) via the banked env-gated handler
> (`YANG_TRIPLE_JUNCTION_EXPERIMENT`, kept in `stage4_relocate_and_correct`)
> plus the `YANG_DOUBLECOVER_PROBE` chain:
>
> 1. The coincident opposite-winding triangle pair is **minted by Stage-4's
>    own PR-KV9 junction-twin collapse**, not by the Stage-2 arrangement or
>    Stage-5 flood-fill (the kept submesh is I6-guarded duplicate-free). When
>    the two arrangement vertices minted for ONE Steinmetz seam apex are
>    identified (F0059: victims 96→82 and 128→120), the two-triangle pleat
>    spanning the twin gap maps to an exact duplicate pair with opposite
>    windings — a zero-volume flap that put count-4 edges under the Stage-6
>    wedge walk (`s6-wedge-walk-not-outgoing`).
> 2. **FIXED at the mint site**: `collapse_vertex` membrane cancellation
>    (spec `yang_collapse_membrane_cancellation`, SHIPPED 2026-07-10) drops
>    both copies; unit red→green.
> 3. **The χ=4 "two-shell stitching gap" (layer 2 of the old diagnosis) does
>    NOT exist** with the mint-site fix — it was an artifact of the old
>    experiment's exclusion-style workaround (excluding the membrane from
>    boundary derivation while keeping it in the mesh). With handler +
>    cancellation, F0059's boolean COMPLETES and emits a valid B-Rep.
> 4. **F0059's remaining wall (the epic's next increment)** is downstream in
>    kernel-v2 render tessellation: `TessellationFailed FaceId(7) ring
>    rejected by CDT`. Face 7 is a cap disc whose kept material is four
>    circular-segment lobes meeting the trim chords EXACTLY at four rim
>    junction points (`(±0.25, ±0.245)` in cap frame; the four corners lie
>    exactly on the r=0.35 rim). The chord-sampled rim polyline cuts inside
>    the exact circle and crosses the trim chords near those junctions → the
>    emitted single loop self-intersects. This is the §4.3.3 Case-IV /
>    rim-junction-insertion class (M8 increment-15 `forced_rim_n` machinery
>    is the precedent; S7's §4B split arm cannot catch it — the junction sits
>    a full sagitta off the chord, far beyond TAU_WORK).
> 5. Census re-confirmed: the OTHER 13 Stage-4 LRR cases do NOT convert with
>    the handler (junction configs outside the ≥2-maps + exactly-3-surfaces
>    trigger; R0019 resolves v25 then bails at v61, R0047 at v38, R0070
>    advances to OffCurveBeyondChordBand at v1028). F0059 is the only case
>    in the class this handler + cancellation unblocks, and it still needs
>    increment 2 (rim junction insertion) to go green — so the handler STAYS
>    banked-unwired per P4.
>
> **Fix order (revised): (1) ~~double-cover origin~~ DONE (mint-site
> cancellation, shipped); (2) cap-rim junction insertion (the ring-reject
> class — also the F0045/R0011 TessellationFailed family's suspected wall);
> (3) wire this handler (its green reproduction becomes F0059 end-to-end).**

> **Status (2026-07-08, REVISED): DESIGN — prototyped twice, reverted twice.
> The original "land WITH N2 CDT" plan is DISPROVEN.** The handler below is
> correct (0 WRONG, no CORRECT lost; F0059's 8 corner ellipse×circle junctions
> relocate cleanly onto all 3 surfaces, ρ≈1e-2 ≪ gate 2.2e-2) but converts ZERO
> cases alone. The 2nd session instrumented F0059's downstream wall on live code
> and found it is **NOT a T-junction at the relocated junction** — it is a
> PRE-EXISTING Stage-6 wall independent of the relocation:
>
> 1. `s6-wedge-walk-not-outgoing` at mesh vertex 85 = [0,0,0.35] (the TOP of the
>    Steinmetz seam, on BOTH cylinders), caused by a **non-degenerate coincident
>    opposite-oriented triangle pair** {31,48,85} in one cap-cylinder patch — a
>    zero-volume double-cover membrane (count-4 non-manifold edges). Since Stage-4
>    relocation moves POSITIONS only (never topology), this pair is present with or
>    without the handler. **`stage4_mesh_update` on the relocated vertex's patch
>    would never touch vertex 85's patch — wrong target.**
> 2. Excluding coincident opposite pairs (generalizing `patch_fold_slivers`) clears
>    the walk but exposes `s4-shell-euler chi=4` (two shells) — a Stage-5/6 seam-
>    stitching gap where the cyl×cyl union assembles as two disconnected shells.
>
> The wall is ≥3 layers deep; NOT a clean red→green. **REAL fix order:** resolve
> the Stage-6 double-cover origin (Stage-2 arrangement / Stage-5 flood-fill —
> why one patch carries both windings of {31,48,85}) and the χ=4 two-shell
> stitching FIRST; only then can this handler + a clean downstream convert F0059.
> Also note R0004/R0017 still bail Stage-4 WITH the handler (their junction
> vertices fall outside the "≥2 single-curve maps + exactly-3-surface" trigger).
> Full trail: memory `stage4_lrr_conic_triple_junction_diagnosis` +
> `docs/yang_deviations.md` N2.

## Goal

Retire the dominant Stage-4 `LocalRefinementRequired` error class (12 assay
cases, `target/assay_kv2_report.json`) for the sub-family where a mesh vertex is
the crossing of **two analytical intersection curves** — i.e. an
"over-determined junction". Today such a vertex trips a loud P10 STOP in one of
the no-skip audits (`lib.rs` sites 9801/9817/9832/9849/9867: "a vertex shared by
BOTH a circle and an ellipse edge … loud STOP rather than silently picking
one"). That STOP is conservative but wrong-in-spirit: the vertex's exact
position is *not* ambiguous — it is the unique transversal common point of the
distinct surfaces incident at the vertex.

This is the same problem the **torus block already solves** for degree-4 torus
junctions (`lib.rs` ~10560–10657): aggregate the distinct incident surfaces per
vertex and, for exactly three, relocate onto all three via
`relocate_onto_implicit_triple` (Newton on the 3×3 unit-normal Jacobian). This
spec generalizes that block to **conic** junctions (circle × ellipse,
cone × cone, cone × cylinder, …) so ONE handler covers every curve-type mix,
per §0.1 "general over piecemeal".

## Parameters (inputs)

- `mesh`, `attribution`, `a`, `b` — as `stage4_relocate_and_correct`.
- The per-vertex conic maps already built earlier in the function
  (`vert_circle`, `vert_ellipse`, `vert_cone_ellipse`, `vert_parabola`,
  `vert_cone_hyperbola`, `vert_line`).
- `inc0` — the incidence map `(start,end) → [(InputId, Surface)]` from
  `compute_phase_a`: the source of the DISTINCT incident surfaces per vertex.
- `d_eps` — the Stage-4 chord band from `stage4_chord_band`.

## Branch table

For each vertex present in **≥ 2** of the six single-curve conic maps (the
existing line+circle extraction at `lib.rs:9774` runs first and removes those):

| # distinct incident surfaces (deduped from `inc0`) | Action |
|---|---|
| exactly 3 | **NEW**: transversal triple point → `relocate_onto_implicit_triple`; remove vertex from the six maps and from `endpoints`; move it; add to `moved`. |
| 2 | leave in maps → existing over-determined audit STOPs (unchanged). |
| ≥ 4 | leave in maps → existing over-determined audit STOPs (unchanged). |

In the 3-surface relocation:

| condition | Action |
|---|---|
| Newton diverges / degenerate 3×3 (`relocate_onto_implicit_triple` → `None`) | STOP `LocalRefinementRequired` |
| `surface_value_and_normal` fails at proj | STOP `LocalRefinementRequired` |
| displacement `ρ = |proj−p|` > `2·d_eps / sinθ` (θ = angle between two incident normals at proj) | STOP `OffCurveBeyondChordBand` |
| `ρ ≤ TAU_WORK` | retag only (no move) |
| otherwise | move vertex to proj, add to `moved` |

## Invariants

- **I1 (exactness)**: the relocated vertex satisfies `Fᵢ(proj) = 0` for all
  three surfaces to Newton tolerance (1e-13), hence lies exactly on BOTH
  incident intersection curves.
- **I2 (no silent pick)**: a vertex is relocated only when the incident-surface
  set has exactly 3 members and Newton converges within the derived
  displacement band; every other configuration keeps a loud STOP (P9/P10).
- **I3 (bookkeeping balance)**: resolved vertices are removed from `endpoints`
  and are NOT added to `relocations`/`processed`, so the no-skip audit
  (`processed == endpoint_set == relocation_keys`, `lib.rs:10475`) stays
  balanced — identical treatment to the torus junction block (source stays
  `BRepVertex`; position is exact).
- **I4 (no double relocation)**: removed from the six single-curve maps, so no
  per-curve loop touches the vertex afterward.
- **I5 (determinism)**: `BTreeMap` iteration + structural `Surface` dedup in
  incidence order ⇒ deterministic surface triple.

## Oracles

- **Unit**: a cylinder × cylinder 90° union fixture (F0059 shape) whose Steinmetz
  ellipse meets a cap circle; assert the boolean succeeds and the shared vertex
  lies on all three surfaces to ≤ 1e-10 (was `Err(LocalRefinementRequired)`).
- **Assay** (`assay_kv2`): **0 WRONG preserved**, `supported_correct` strictly
  increases (target cases F0059, R0004, R0017, R0019 and any other 3-surface
  conic junctions), no previously-CORRECT case regresses.

## Failure modes

- Non-transversal triple (parallel/tangent normals): `relocate_onto_implicit_triple`
  returns `None` → `LocalRefinementRequired` (unchanged loudness).
- Off-band displacement (real off-curve error, not a Stage-1 chord artifact):
  `OffCurveBeyondChordBand`.
- ≥ 4 incident surfaces (genuinely over-determined) / 2 surfaces (handled by the
  closed-form ell×ell / disc∩disc / line+circle junctions elsewhere): existing
  STOPs untouched.

## Research Basis

- [#24] Yang et al. 2025 §4.4.1 (mesh updating; relocation breaks bijectivity,
  intersection points relocated onto the exact surfaces),
  `refs/text/yang2025_hybrid_boolean.txt:531–573`.
- [#1] Patrikalakis Ch.5 (surface intersection / Newton projection).
- Reuses the existing `relocate_onto_implicit_triple` primitive and mirrors the
  KV6d Tier-B torus junction block (`lib.rs` ~10560), the disc∩disc / cyl×cyl /
  line+circle junction displacement-band derivation (1/sinθ gradient metric,
  NOT tolerance widening).
