# SI5 C4b — the arc-patch tier (arcs and ellipse arcs)

Status: **design**, 2026-10-01. Checkpoint C4b of
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

## 5. Measurements to take BEFORE writing code (checkpoint C4b-M)

Numbers this design needs and does not have. Each is a probe over ABC chunk
0000, in the shape §2/§5.1 established, and each can change the plan:

1. **How much reach does the cone-section-ellipse wall cost?** Of the
   in-vocabulary arc-patch models, how many have an `ELLIPSE` edge on a
   `CONICAL_SURFACE` face (walled) versus on a cylinder or a plane
   (supported)? If that number is large, C4b's ellipse scope is gated on
   KV16b rather than on ingestion, and the increment splits again.
2. **Do multi-loop arc patches actually exist in the corpus?** §3.3 is the
   only genuinely new logic in C4b; if every arc patch has exactly one
   boundary loop, the unrolled ranking has no customer yet and should be
   written as the one-loop statement plus a typed refusal for the rest, not as
   general machinery nobody can test. (The C4a′ increment is the cautionary
   tale: a gate that cannot fire is not coverage.)
3. **Per-model reach of the whole tier**, so the +25.8-point figure is
   confirmed on the same 400 models the other checkpoints report.
4. **Half-turn and near-half-turn arcs** — already measured at 11 %; re-check
   it survives the `interior`-based mapping, since this is the one place C4b
   can silently build the complementary arc.

---

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
