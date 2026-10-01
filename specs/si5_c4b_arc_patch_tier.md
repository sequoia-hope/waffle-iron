# SI5 C4b — the arc-patch tier (arcs and ellipse arcs)

Status: **LANDED**, 2026-10-01 (C4b-M measured first, §5; outcome in §7). Checkpoint C4b of
`specs/step_import_si5_exact_analytic_ingestion.md` §7, following C4a (full
bands) and C4a′ (the geometry-provenance tier,
`specs/si5_geometry_provenance_tier.md`).

**Ellipse edges are IN SCOPE** (user decision, 2026-10-01), which §7 had left
as a C4b scoping call.

---

## 1. What C4b is

C4a ingests a curved face only when every loop is one **closed** circle — the
full band. C4b takes the other 58.4 % of the corpus's cylindrical/conical
faces: the **arc patch**, bounded by open circular arcs, ellipse arcs and
straight rulings — a fillet, a half-round, a rounded corner, an oblique cut.

It also takes what comes with them: a **planar** face whose boundary contains an
arc or an ellipse arc, which until now has been refused with the curve.

Reach at stake, per model (§5.1's per-model table): the models that are in
vocabulary and carry arc patches are **17.8 % both forms + 8.0 % arc-only =
25.8 points**, on top of C4a′'s 18.8 %.

### 1.1 These models gain geometry, not booleans — and that is the decision

`to_yang_brep` requires a cylinder to present exactly two full-circle rims, so
an arc patch bounces off it with a typed `UnsupportedCurvedBoolean`. SI5 §5.1
already decided this: **ingest anyway and let the typed wall speak.** A tier
that renders, measures, exports STEP and round-trips exactly — but refuses
booleans, loudly — is strictly better than today's mesh-backed body, which
does none of those things exactly. C4b must not pretend otherwise: the wall is
part of the deliverable, and the C6 fallback must report it as a feature
warning, never silently.

---

## 2. What the arena already provides (read, not assumed)

Checked against the live tree at `366a3179`+C4a′:

| need | state | where |
|---|---|---|
| `Curve::Arc` (centre, normal, radius) | exists | `arena.rs` |
| `Curve::EllipseArc` (centre, normal, major axis, both radii) | exists | `arena.rs` |
| cylinder patch: `Arc` + `LineSegment` loops, unrolled-winding law | **ready** | `validate_cylinder_patch`, `faces.rs:906+` |
| cylinder patch: `EllipseArc` (oblique section) azimuth advance | **ready** (PR-KV9) | `faces.rs:1049+` |
| cone patch: `Arc` + `LineSegment` in the (θ, τ) development | **ready** (KV6c) | `faces/cone.rs:186+` |
| cone patch: `EllipseArc` / `HyperbolaArc` | **typed wall** (KV16b) | `faces/cone.rs:337-346` |
| planar face with `Arc` / `EllipseArc` edges, exact signed area | **ready** | `validate_planar_face`; `geom::planar_loop_signed_area` + `LoopEdgeCurve` |
| arc patch → boolean | refused by design | `boolean/mod.rs:23-28` |

So C4b is mostly an **ingestion** increment, not a kernel-capability one. The
one capability gap it will meet is the cone-section ellipse: a conical face
bounded by an ellipse arc must be refused, named, because
`validate_cone_patch` has no rule for it ("a cone-section ellipse has no
constant-radius axis-⊥ projection, unlike the cylinder-section ellipse"). That
is KV16b's work, not SI5's, and §5 below measures how much reach it costs.

---

## 3. The three pieces of work

### 3.1 Curve mapping (`ingest.rs` 1b)

Today an open `CIRCLE` is `AnalyticIngestUnsupportedCurve { curve: "circular
arc (C4b)" }` (`ingest.rs:487`) and an `ELLIPSE` is `"ellipse (C4b)"`
(`:516`). Both become arena curves:

- open `CIRCLE` → `Curve::Arc { center, normal, radius }`
- `ELLIPSE` → `Curve::EllipseArc { center, normal, major_axis, major_radius,
  minor_radius }`

**The traversal side is read, never re-derived.** `AnalyticCurve` carries an
`interior` point taken from the middle of the file's own parameter range
(C2's design note), so the arc is pinned by data. This is why C4b may take the
**11 %** of corpus arcs that are exact or near half-turns
(539 exact + 207 within 179–180°, of ~6 456) that `from_yang_brep` must reject
as ambiguous: that rejection exists because the boolean path *derives* the
normal from two endpoints, where a half turn is genuinely undecidable. Reading
`interior` is not the same operation and carries no such limit
(`geom::ccw_sweep` is well defined on all of (0, 2π] once the normal is given).
A C4b implementation that re-derives the side from endpoints would import the
ambiguity it does not have — the single most likely wrong turn here.

Consistency, not replay: the file's declared axis is checked against the
`interior` point's implied side, and a disagreement is a refusal.

### 3.2 Mixed loop shapes (`ingest.rs` 1c)

`LoopShape` today is `Rim` (one closed circle) or `Polygon` (all straight,
continuity and closure verified in the file's declared direction). C4b adds the
mixed chain: arcs, ellipse arcs and lines in one loop. The continuity and
closure verification is unchanged — it is about vertex indices, not curve type
— so this is a widening of what 1c admits plus a per-edge curve descriptor,
not new logic.

### 3.3 Outer-loop determination for a curved patch (`ingest.rs` 1h) — the new part

1h currently short-circuits: *"A full band has exactly one loop, already
Outer"*. An arc patch can have more than one loop (a window in a fillet), and
for a curved face the 3-D signed area is meaningless — the loop does not lie in
a plane.

The law it must satisfy is `validate_cylinder_patch`'s unrolled winding: in the
unrolled `(θ·r, h)` frame (mirrored for `reversed`), either **exactly one
non-wrapping loop is CCW with every other loop CW**, or **exactly two loops
wrap the axis (±1)** with the `+1` wrap at the lower axial height and every
non-wrapping loop CW. The cone's analogue replaces `h` with
`τ = (p − apex)·axis_dir`.

So the determination is: unroll every loop with the validator's own frame and
azimuth-advance rules (`Arc` → signed `ccw_sweep`; `EllipseArc` → the PR-KV9
signed parametric sweep; `LineSegment` → `wrap_to_pi` of the azimuth
difference), sum the advance per loop, and rank by that. **Reuse the
validator's measurement rather than re-deriving it** — two implementations of
one law is how the C3 `FACE_OUTER_BOUND` class got its chance. The likely shape
is to lift the per-loop unrolled measure out of `validate_cylinder_patch` into
a `pub(crate)` helper that both the validator and ingest call, which also keeps
the ingest-side refusal and the validator-side refusal from drifting apart.

---

## 4. Refusals C4b keeps (each named, none guessed)

- A conical face bounded by an `EllipseArc` or `HyperbolaArc` — KV16b's
  vocabulary gap, surfaced as a typed wall naming it.
- A face mixing a closed circle with open edges (172 faces, 2.7 %) — the arena
  has no such face; C4a already refuses it.
- Holed / unclosed bands with 1, 3, 4, 6, 10 rim loops (38 faces, 0.6 %).
- Anything C2 refuses file-wide (b-splines, `VERTEX_LOOP`).

---

## 5. C4b-M — the measurements, taken first (2026-10-01)

`ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 cargo test -p test-harness --test
si5_analytic --release -- --ignored --nocapture c4b_arc_patch_census`:

```
  in C4b surface/curve vocabulary   166  (41.5 %)
    of those, walled by a CONE-section ellipse (KV16b)  0  (0.0 % of vocab)
  PROJECTED C4b reach               166  (41.5 % of scanned)
  arc-patch faces                   1113
    loops per arc patch: {1: 1111, 2: 2}
  ellipse edge uses by face kind: {"cylindrical": 60, "planar": 20}
  open arcs 2305: exactly a half turn 261, within 1° of one 126
```

Two of the four change the plan.

### 5.1 The cone-section ellipse wall costs NOTHING — ellipses are free

**Zero** of the 166 in-vocabulary models puts an ellipse on a conical face. All
80 ellipse edge uses in the sample are on **cylinders (60)** and **planes
(20)** — precisely the two forms whose validators already handle `EllipseArc`
(PR-KV9 and the planar exact area). So C4b's ellipse scope is not gated on
KV16b at all; the cone-ellipse refusal stays in the code as a named wall, but
it is a wall with no customer in this sample rather than a reach cost.

### 5.2 The unrolled outer-loop ranking has almost no customer — do NOT build it

**1 111 of 1 113** arc patches have exactly one boundary loop; **2** have two.
So §3.3's general machinery — ranking loops by unrolled winding in the (θ, h)
domain — would be written for 0.18 % of faces, against a law that only the
validator can currently check.

C4a′ is the precedent for what to do instead, and it cuts the other way: there,
a gate that could not fire was removed; here, a form that DOES occur (twice)
gets a **typed refusal** naming it, and the 1-loop case is stated as what it
is — a single boundary loop is the outer loop, no ranking needed. That is
honest, reachable, measurable, and it removes the hardest part of C4b. When a
corpus case demands the ranking, the refusal will say so by name and the work
will have a customer to validate against.

**Revised §3.3**: a curved patch with exactly one `Edges` loop takes that loop
as Outer; two or more is `AnalyticIngestUnsupported("a curved patch with more
than one boundary loop")`.

### 5.3 Reach, and the half-turn share

- **Projected reach 166/400 (41.5 %)** against C4a′'s 75 (18.8 %) — about
  **+23 points**, in line with §1's +25.8 estimate. The census gate is coarser
  than ingestion (it checks surfaces, curves and loop kinds, not every form
  rule), so 41.5 % is an upper bound and the ingestion run is the number of
  record.
- **Half turns are 16.8 % of open arcs** (261 exactly + 126 within 1°, of
  2 305) — higher than the 11 % §3.1 quoted from a different denominator. One
  arc in six is a half turn, so reading `interior` rather than deriving the
  side from endpoints is load-bearing for the tier, not a nicety.

## 6. Oracles

The same differential pattern that C3/C4a used, which costs nothing and cannot
be fooled by a self-consistent mistake:

- **Export → extract → ingest is a fixed point** on every new fixture.
- **The solid that WROTE the fixture and the solid ingested back from it agree**
  on `(V, E, F, R, S, G)` exactly and on volume to the exporter's decimals.
- New first-party fixtures (written by `kernel_v2::step_export`): a half-round
  (cylinder cut through its axis — two arcs + two rulings, the `CCLL` form the
  corpus actually writes), an oblique cut (an `EllipseArc` boundary on both the
  cylinder patch and its planar face), and a cone patch from a partial revolve.
- The corpus census, with in-vocabulary success **asserted** at 100 % as
  C4a′ left it — so a C4b miss is a red test.

---

## 7. Outcome (2026-10-01)

| | C4a′ | C4b |
|---|---|---|
| reach, 400 models | 75 (18.8 %) | **148 (37.0 %)** |
| solids / faces | 273 / 5 708 | **518 / 10 233** |

**+18.2 points**, against the §5.3 upper bound of 41.5 %; the gap is entirely
named refusals (18 unclosed/holed bands, 2 closed ellipses, 1 mixed-form loop).

### 7.1 What it took beyond the plan: one gap in `signed_volume`

The first corpus run after the ingestion work converted nothing like the
projection — **29 models** stopped at
`CurvedGeometryMismatch { reason: "signed_volume: loop mixes full circles with
arcs" }`. The cause is architectural rather than geometric, and it was invisible
until C4b produced the shape: `signed_volume` has an **exact-rational** path for
faces whose loops are circles and segments, and an **f64** path
(`planar_arc_face_flux`) for faces with arcs. A face that mixes a full circle
with arc chains — a disc whose ring is a chain of arcs, an arc patch's cap
beside one — fits neither: the rational path refuses any arc, and the f64 path
had no circle term. No boolean output had ever produced the combination, so the
wall had never been reachable.

It needed no new closed form. **A closed circle is the Δθ = 2π case of the arc
formula already there**: the chord term vanishes (`p0 == p1`) and the segment
correction is `±r²(2π − sin 2π) = ±2πr²`, exactly twice the circle's area. Those
faces' volume term is f64 rather than exact-rational, which is not a precision
regression — before C4b they were refused outright.

Reach went 114 → 148 with that one term.

### 7.2 The one finding left standing, anchored

`00000062_767e4372b5f94a88a7a17d90_step_003`, face 35:
`TessellationFailed { reason: "ring rejected by CDT (degenerate/self-intersecting)" }`.
One model of 400, reached through the self-intersection gate (which tessellates),
so it is a **render-tier** finding on real imported geometry rather than an
ingestion one. Recorded here with its model and face so the follow-up has an
anchor rather than a class name; not fixed in this increment, because a CDT ring
rejection is kernel-v2 M1/KV2 work with its own oracles.

The corpus census now also reports **every validation-tier refusal** as a
finding, in or out of vocabulary: once ingest has assembled a solid, a refusal is
about geometry, not capability, and the model's name is what a follow-up needs.

### 7.3 Tests

In `crates/kernel-v2/src/ingest/tests.rs`, built on a new `half_round` fixture —
a cylinder cut through its axis, which is the corpus's own `CCLL` arc-patch form
(two open arcs, two distinct rulings, no seam) with half-disc caps whose loops
have just two edges:

- `a_half_round_ingests_as_an_arc_patch_with_its_exact_volume` — (V, E, F, R, S,
  G) = (4, 6, 4, 0, 1, 0) with nothing minted, and volume π r² h / 2 exactly.
- `an_arcs_side_comes_from_the_files_interior_point_not_its_endpoints` — the
  same four vertices, circles and edges with each `interior` moved to the far
  side and every loop reversed: the other half. Both halves have identical
  volume, so the oracle is the tessellated body's own y-extent. A reading that
  derived an arc's side from its endpoints could not tell the two files apart —
  and these arcs are half turns, the case `from_yang_brep` must refuse.
- `a_closed_ellipse_edge_is_a_typed_refusal_naming_the_tier`,
  `a_windowed_curved_patch_is_refused_by_name`,
  `an_unclosed_band_is_still_a_typed_refusal` — the three walls, each named.
- `an_arc_that_does_not_bound_its_face_is_refused_by_the_winding` — the C4a-era
  arc-vocabulary test, rewritten: the refusal moved one step later (to the exact
  winding measurement) rather than weakening.

### 7.4 What C4b did NOT do, and why

- The unrolled-domain outer-loop ranking (§5.2): 2 of 1 113 patches, refused by
  name instead.
- Closed `ELLIPSE` edges: rim-like, and no interior point can settle the sense
  of a curve that passes through all its own points both ways. 2 models.
- Unclosed and holed bands: 18 models, the largest named bucket — a band with a
  ring is a shape the arena has no face for.
- The in-vocabulary assertion still uses C4a's form predicate, so the corpus
  test's 100 % claim does not yet cover the C4b forms. Widening it belongs with
  **C7** (the corpus gate), where the predicate becomes the gate's definition.
