# Spec: the Case-III graze depth must be measured on the FACES, not on the axis lines (P0021, deviation N75)

**Status: ANCHORED and CONVERTED **GATED** 2026-10-03. The ladder is built,
unit-pinned, mutation-checked, and MEASURED to convert P0021
(ERROR → SUPPORTED_CORRECT, 2.0 s release) and its un-minimized 8-op
lineage (ERROR → SUPPORTED_CORRECT, χ 8 → 6, 5 → 4 bodies). It ships
GATED OFF behind `YANG_172_EXTENT=1|on`; the always-on flip owes the full
release categorized assay (P10), which the session that built it was not
permitted to run. Corpus NOT re-measured — P0021 therefore still counts
as E and its pin has NOT moved.**

Owner of this spec's defect: `specs/yang_172_case_iii_graze_guard.md` §2–3
(the Case-III graze guard's derived rim-N). Paper: Yang §4.2.1 / Fig. 8
Case III (`refs/text/yang2025_hybrid_boolean.txt:436-447`) and the §4.5.2
refinement-termination theorem (`:668`).

## 1. The anchor, measured

P0021 (`convex4:boss circle:boss circle:boss`, scale 2.8e-3) fails its
third op's auto-union with
`SelfIntersectingBooleanOutput { face_a: FaceId(28), face_b: FaceId(32), penetrations: 3 }`.

**The two faces are both CYLINDERS** (`KV2_SELFX_SITE_PROBE`): face 28 is
the op-2 boss lateral (axis point `(4.77e-4, 2.65e-4, 1.02e-3)`, radius
`7.1e-4`), face 32 the op-3 boss lateral (axis point
`(1.5e-3, −1.3e-3, 1.7e-3)`, radius `2e-3`).

**This is NOT P0007's family.** `KV2_OUT_CURVE_CENSUS` reads
`plane×curved chords: 0` on **every** op of both the minimized case and
the un-minimized lineage, and the output's half-edge kinds are
`{Arc, EllipseArc, Line}` with **zero `SurfacePair`** — the §4.4.2
carried-edge restoration has nothing left as a chord here. Nothing was
restored wrongly; the cylinder×cylinder intersection curve was never
derived at all. Face 28's loops carry only prism-plane ellipse arcs and
its own rim arcs; face 32's likewise. The union trimmed each boss against
the prism and **the two bosses against each other not at all**.

**The penetration is a TRUE B-Rep self-intersection, not a render-sampler
artefact.** The probe's face-28 triangle vertices lie exactly on that
cylinder's base rim (`t_a = ±2.7e-20`, radial deviation `0`), and one of
them sits `6.08e-6` INSIDE the op-3 cylinder while its neighbour sits
`4.84e-5` outside — face 28 crosses face 32's surface. The render mesh is
not the cause: it is only the resolution at which a real crossing becomes
visible (`validate::selfx`'s own calibration lineage, the C0116 class).

**Why the mesh missed it.** The two finite solids overlap in a razor lens:
of 893 040 samples of solid A only 11 lie inside B, and the deepest
interior point is **`6.162267e-6`** deep, at `s = 0` on A's axis and
`t_b = 6.8e-6` on B's — the lens is pinched between the two bosses'
**nearly coincident base cap planes**. The maximum radial clearance of A's
lateral inside B, within both faces' extents, is **`2.5714e-5`**. The
natural Stage-1 densities are `n_seg = 10` (A, `d_eps = 3.6457e-5`) and
`12` (B, `d_eps = 7.8473e-5`), whose combined chord sagitta is `9.2e-5` —
3.6× the clearance. The meshes are simply disjoint.

**The guard saw all of this and threw it away.** `YANG_SPLIT_PROBE` prints

```
[graze-guard] pair=(6,2) n=5 meshes_touch=false tris=(233,24)
```

`meshes_touch=false` is the guard's own exact (Cherchi tri-tri, exact
predicates) proof that the natural meshes MISS an intersection the
surfaces have — Yang's Case III by definition. And `n=5`: the demand it
derived is **5**, which the self-limiting natural-N gate
(`n > natural_rim_n(a) || n > natural_rim_n(b)`, i.e. `n > 10`) absorbs.
The evidence and the remedy were both in hand, and the remedy was dropped.

### The deviation (N75)

`cyl_pair_graze_demand`'s depth is `r_a + r_b − d_lines`, the penetration
of the two **INFINITE** cylinders measured at the common perpendicular of
their axis **LINES**. For P0021 that perpendicular's foot lies at
**`s* = −3.302106e-3`** on a cylinder whose own axial span is
**`[0, 1.68e-3]`** — 1.97 lengths off the *far* end, on the opposite side.
The depth it reports, `1.480362e-3`, is **59×** the clearance the finite
bands realize (`2.5197e-5`), and the N it derives is 5 where the realized
clearance demands 33.

The guard already owns the right idea and declines to apply it: its
`axial_span` helper and the SubSagitta arm's witness check exist precisely
to confirm that a graze "reaches both FACES' axial extents", and §3's
branch table says "the BOOST arm needs no extent check (a finer mesh is
always valid; mirror of Case-IV)". That reasoning is sound about a *false
boost* — which costs only triangles — but it is the wrong conclusion about
the *depth*: an out-of-extent witness does not merely risk a needless
boost, it corrupts the quantity that SIZES the needed boost, and the
error is in the dangerous direction (depth too large ⇒ N too small ⇒ the
miss survives).

The paper is unambiguous about which figure decides: Case III is "the
meshes miss intersections" (`:438`), and the mesh is of the finite, capped
operand. The deciding clearance is the finite bands'.

**Mutation-certified.** Ignoring A's own axial span in the new witness
makes it return exactly `1.4803612303380738e-3` — bit-for-bit the number
the existing guard uses. The deviation is that single substitution.

### Second observation (recorded, not this spec's owner)

Nothing downstream could have rescued it either. The §4.5.4 rim-graze
retry in `boolean.rs` fires only when `natural_broken` — measured by
yang-rs's own `output_improper_count` on the **boolean-resolution mesh**,
the very mesh whose coarseness is the defect. It reads `0` improper
contacts, `natural_broken == false`, and the retry never runs (the probe
trace shows no further `[stage1-nseg]` line after the gate). The
self-intersection becomes observable one crate later, at kernel-v2's
render-resolution gate, where no refinement path remains. This is the
two-layer disjointness `validate/selfx.rs`'s header already documents;
the trigger reads the layer that cannot see the class. Fixing the Stage-1
density decision makes the question moot for P0021, but the trigger
asymmetry is a real follow-up.

## 2. The fix

Two pieces, both in `crates/yang-rs/src/boolean/rim_junction.rs`.

### (a) `cyl_band_overlap_clearance` — the finite-band witness

The largest radial clearance `r_b − ρ_b(p)` over a point `p` on A's
lateral band with `s ∈ span_a` **and** `t_b(p) ∈ span_b`, the two spans
taken from the faces' own rim-circle centres (`axial_span`, already
written for the STOP arm).

For fixed θ the squared radial distance to B's axis is an exact quadratic
in `s` and `t_b(p)` is exact and affine in `s`, so the feasible
`s`-interval and the quadratic's clamped minimizer are **closed form** —
every reported sample is a REAL point of the overlap. The circumferential
walk (`BAND_WITNESS_SAMPLES = 512`) is therefore not a tolerance: the
value is a **lower bound** on the true maximum clearance, and a lower
bound is the safe direction (it can only derive a FINER N — always
chord-valid, governance A14.3). A coarser walk can only FAIL to find a
real overlap, in which case the caller keeps today's behaviour; it can
never invent one. Measured on P0021's pair: 512 samples → `2.2238e-5`
(the dense reference is `2.5714e-5`); 64 finds it, 32 does not.

**Self-limiting.** When the common perpendicular lies inside both spans
the witness recovers the closed-form infinite-surface depth exactly
(pinned on C0116's own pair), so the existing guard population is
untouched.

### (b) The escalation ladder

For a flagged pair whose closed-form demand WOULD be absorbed by the
natural-N gate but whose meshes are proven disjoint:

1. require an on-face witness from (a) — no witness ⇒ the infinite
   surfaces graze off-face (the adjacent-boss class), demand nothing;
2. otherwise walk rungs `base·2^k` (`base = max` of the two finite
   naturals) re-asking the **exact tri-tri predicate** at that rim
   density, and demand the first rung at which the meshes meet;
3. past 4096, demand nothing — a genuine sub-resolution graze no practical
   mesh observes. **No new STOP arm** (the #195 inc-2 disposition):
   kernel-v2's render selfx gate is already its loud tripwire, which is
   how P0021 was found.

The acceptance criterion is the exact predicate, so **no sagitta margin is
interposed** — and none is needed, because the predicate is a perfect
oracle for this case. Swept over 16 Stage-1 density floors (debug,
`YANG_NSEG_FLOOR`), `meshes_touch == true` ⟺ `SUPPORTED_CORRECT` at every
one of the 12 floors measured both ways:

| floor | 11 | 12 | 13 | 14 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 24 | 32 | 33 | 48 | 71 | 96 | 128 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `meshes_touch` | — | F | T | F | F | T | F | T | T | T | T | T | T | — | T | — | — | — |
| verdict | E | E | **C** | E | E | **C** | E | C | C | C | C | C | — | C | C | C | C | C |

(13 and 17 are phase flukes — green by chord luck, 14/16/18 red. The
ladder's rungs from `base = 12` are 24, 48, …, so it never lands in the
lottery band. Floors above 19 are reliably green; the realized clearance
`2.5714e-5` would derive N = 33, inside that zone.)

The ladder's warrant is the paper's own termination theorem: "The
algorithm is guaranteed to terminate since the mesh intersections converge
to the spline surface intersections under refinement"
(`refs/text/yang2025_hybrid_boolean.txt:668`).

**One implementation trap, measured:** a ladder rung must be tested with
`stage1_tessellate_with_rim_overrides(..., Some(n_seg))`. Rebuilding the
operand with `rebuilt_with_min_rim_segments(n)` and then calling plain
`stage1_tessellate` silently DROPS the boost (`min_n_seg` is a
tessellation-time parameter stored as `forced_rim_n`), and every rung
reported `touched=false` up to 3072 — a 135 s climb to the cap that
converted nothing. The same omission is why a `--release`
`YANG_NSEG_FLOOR` sweep is a no-op: that knob is
`cfg!(debug_assertions)`-gated.

## 3. Measurements

- **P0021**: gate off `ERROR … { face_a: FaceId(28), face_b: FaceId(32), penetrations: 3 }` (0.3 s);
  gate on **`SUPPORTED_CORRECT (2.0s) — all checks passed`**. The ladder
  takes one rung: `witness=Some(2.2238229344616594e-5) base=12`,
  `rung=24 touched=true`.
- **Un-minimized lineage** (`target/prospect/seed-2/candidates/X00000002-00184`,
  8 ops): gate off ERROR (`FaceId(56)` cylinder × `FaceId(62)` — the op-3
  boss's base cap PLANE; 5 bodies, χ 8); gate on **SUPPORTED_CORRECT**,
  4 bodies, χ 6, volume `4.7617355307854375e-5`. The minimizer minted no
  contact — its rounding only changed WHICH of the big boss's faces the
  leak surfaces through (lateral in the minimum, base cap in the lineage);
  the site, the operands and the mechanism are identical. A second flagged
  pair there (`pair=(14,2)`) gets no witness and the ladder declines it —
  the off-face declination working as designed.
- **Sharers, both gates, release, unchanged**: C0105 CORRECT 0.7 s /
  0.7 s; C0116 (the Case-III vehicle) CORRECT 136.5 s / 125.0 s; C0118
  (the designed `SubSagittaGrazeIntersection` STOP) ERROR / ERROR; C0057
  (the phase-filter fixture) CORRECT 0.8 s / 0.8 s; P0007 (the same output
  class) CORRECT 17.0 s / 16.7 s.
- **`cargo test -p yang-rs --release case_iii`**: 15 passed (11 pre-existing
  Case-III tests + 4 new), 0 failed.

## 4. Pins

- `crates/yang-rs/src/tests_unit/m5_case_iii.rs`:
  - `band_clearance_p0021_pair_is_orders_below_the_closed_form` — the
    measured pair: the closed form still says `Boost(5)`, the witness is
    `2.2238229344616594e-5`, and the two are 59× apart.
    **Mutation-checked**: ignoring `span_b` ⇒ witness `6.758677e-4` (RED);
    ignoring `span_a` ⇒ witness `1.4803612303380738e-3`, the closed form
    itself (RED).
  - `band_clearance_in_extent_recovers_the_closed_form` — the
    byte-identity claim on C0116's own pair.
  - `band_clearance_off_extent_declines` — the adjacent-boss declination
    and the degenerate-span guard. **RED** under the `span_b` mutation.
  - `extent_ladder_is_gated_off_by_default` — a careless default flip
    cannot land silently.
- `crates/kernel-v2/tests/p0021_case_iii_finite_extent.rs` — the BARE
  TWO-CYLINDER PAIR reproduces P0021 with no prism at all (a 2-operand
  reduction of the 3-op corpus case):
  - `p0021_pair_default_path_stops_loudly_on_the_untrimmed_laterals` —
    measured `SelfIntersectingBooleanOutput { face_a: FaceId(6), face_b: FaceId(9), penetrations: 2 }`;
    pins the honest wall, and refuses a silent unfused emission.
  - `p0021_pair_with_the_n75_ladder_derives_the_cyl_cyl_curve` — asserts
    the cylinder×cylinder `SurfacePair` curve SURVIVES into the output
    B-Rep (the P0007-shaped assertion: the curve TYPE, not just a
    plausible volume). **Stash-certified RED** with the gate forced off:
    `the N75 ladder must fuse the pair: SelfIntersectingBooleanOutput { face_a: FaceId(6), face_b: FaceId(9), penetrations: 2 }`.

## 5. Open items

- **The always-on flip** needs `full_corpus_categorized` in release
  (P10: zero CORRECT regressions, no new TIMEOUTs). The cost risk is
  real and unmeasured: a corpus case that is CORRECT today while its
  meshes miss a witnessed in-extent graze would be re-meshed by the
  ladder. The trigger conjunction is narrow (both operands analytic
  cylinders; a flagged shallow pair; the demand absorbed by the natural
  gate; the meshes exactly disjoint; an on-face witness), and the five
  sharers above are byte-identical, but that is not a corpus.
- **P0021's meta keeps `derived_meta: true`** and its
  `smoke_corpus_boundary_categories` pin stays `Error` — §8's rule,
  unchanged. The evidence the flip PR needs is recorded here, and it
  carries a question the flip PR MUST answer rather than inherit:

  Gated on, P0021 measures **1 body, χ 0, volume `3.922430276348064e-8`**
  (and the lineage 4 bodies, χ 6, `4.7617355307854375e-5`). **χ 0 is not
  the meta's `euler_target: 2`** — the run reads "all checks passed" only
  because `derived_meta: true` suppresses the χ comparison, so
  SUPPORTED_CORRECT here does NOT certify the genus. χ 0 means genus 1,
  and genus 1 is what this shape should have: the three solids form an
  overlap CYCLE (prism ∩ bossA, prism ∩ bossB and bossA ∩ bossB all
  non-empty with an empty triple intersection), which closes exactly one
  handle — and bossA ∩ bossB is the `6.162267e-6` lens this fix is about,
  so the handle exists only once the lens is resolved. That reasoning is
  a hand argument, not an oracle: the flip PR owes an independent reading
  (the Cherchi sidecar via `TOPO_SIDECAR=1 adjudicate_case`, or the
  exact-membership lattice at two rungs) before writing `euler_target: 0`
  and clearing the flag. Note the standing trap — the lattice's χ
  diverges on grazing operands, and this case is nothing but a graze.
- **The §4.5.4 retry trigger** (`natural_broken` from the coarse-mesh
  `output_improper_count`) cannot see this class; §1's second observation.
  Its own follow-up.
- **Non-cylinder pairs** remain out of scope exactly as
  `yang_172_case_iii_graze_guard.md` §3 leaves them (sphere/cone/torus
  grazes have no per-pair depth formula yet); the extent correction will
  need to travel with each one when they land.

*Created: 2026-10-03*
