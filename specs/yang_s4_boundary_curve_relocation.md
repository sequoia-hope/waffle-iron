# SPEC — Stage-4 §4.4.1 boundary-curve relocation ("boundary curves map to boundary curves")

**Status:** inc-0 (design) — 2026-07-28.
**Crate:** `yang-rs` (Stage 4).
**Motivating measurements:** `memory/session_2026_07_28_195_inc5_flip_blocked.md`
(the n2 I1 fixture, vertex `v6`) and
`memory/session_2026_07_28_onsurface_ledger_blind_spot.md` (the corpus tail
F0083 / R0027 / R0099, now loud since commit 5b891ec2).

---

## 1. The paper requirement (this is the spec)

Yang 2025 §4.4.1 "Mesh updating", `refs/text/yang2025_hybrid_boolean.txt:545-565`,
Fig. 11. Two facts from the figure caption are load-bearing and were previously
read as out of scope:

> "The blue and yellow mesh patches are the discretizations of two adjacent
> surfaces **from one B-Rep model**. The red polyline is the inserted
> intersection curve, and point q is an intersection point **on the boundary
> curve**. (a) We locate the constrained edge containing q (the red edge) and
> split it using q. (b) If an endpoint p of the split edge is too close to q, we
> merge p with q."

and the invariant on the trimmed triangulation:

> "it **maps boundary curves to boundary curves**, and contains no flipping
> triangles"

So the paper explicitly covers the case where the two patches belong to **one
operand** and meet along that operand's **own boundary curve** (a rim). Any mesh
vertex representing a point of that rim must lie **on the rim curve**. A vertex
sitting at the Stage-1 CHORD position between two rim samples violates the
invariant: it is a boundary-curve point that was not mapped to the boundary
curve.

## 2. The defect, measured

`build_intersection_curves` (`stage3_ssi.rs:521`) builds the analytic `Curve` for
each output boundary edge, and Stage 4 relocates that edge's vertices onto it.
It handles **only cross-input edges** — `if input0 == input1 { continue }`
(`stage3_ssi.rs:534`). That scope is stated in its own doc comment ("EXACTLY TWO
entries with DIFFERENT `InputId`"); it is PR-YR9's remit, not an error. But the
consequence is that **an operand's own rim never enters the curve map, so Stage 4
never claims its vertices**, and nothing else in the pipeline enforces the
Fig-11 invariant on them.

Two independent instances, same fingerprint:

| | n2 I1 `v6` | R0063 face 636 |
|---|---|---|
| site | cylinder-patch loop vertex | same |
| neighbours | both EXACTLY on the circle (~1e-19) | same (~1e-19/1e-20) |
| axial height | equal to both neighbours | `h` bit-identical to one, 1 ulp from the other |
| azimuth | strictly between them | strictly between them (20.2244° + 22.4416° = 42.6660° = span) |
| residual | 6.840109e-7, ≈ the 360/37 chord sagitta (7.69e-7) | 3.1257e-5 ≈ the chord's own sagitta at t=0.4752 (3.1016e-5) |
| on the chord? | EXACTLY (perp 7.14e-22) | 2.409e-7 off it (0.77 % of the sagitta) |
| span | 9.7297° (N=37) | 42.666° (N_equiv 8.44) |

**Fingerprint (the detection rule):** an intermediate loop vertex, at the same
axial height as its two azimuthal neighbours, both of which are exactly on the
analytic curve, whose radial residual is ≈ the chord sagitta of the neighbours'
span. R0063 shows the vertex may be *perturbed off* the chord, so the fix must
NOT assume a pristine chord position — it must project onto the curve from
wherever the vertex is, under a band guard.

## 3. Scope decision — a narrow projection pass, NOT re-plumbing the curve map

The tempting one-line change is to drop the `input0 == input1` skip and let the
existing machinery derive the rim curve. **Rejected for this increment:** it
routes EVERY rim edge of EVERY operand in EVERY case into the full Stage-4
relocation machinery (per-curve-type arms, junction logic, LRR STOPs). That is a
corpus-wide behavioural change to satisfy a defect measured on a handful of
vertices — the opposite of a de-risked increment. Revisit only if the narrow pass
proves insufficient.

**This spec builds a dedicated pass** that enforces the Fig-11 invariant
pointwise on vertices that are already off their own model's boundary curve.

## 4. The pass

`stage4_boundary_curve_relocate` (gate `YANG_S4_RIM_SNAP_ENABLE`, default OFF).
Runs in Stage 4 AFTER the existing cross-input relocation (so junction vertices
are already seated and can be excluded).

For each undirected output boundary edge with **exactly two** incidence entries
where `input0 == input1` **and `surf0 != surf1`** (the operand's own rim — two
adjacent surfaces of one solid; equal surfaces are patch-interior and skipped):

1. **Derive the boundary curve — NO SSI NEEDED (revised, see §8).** An operand's
   own rim already HAS its analytic curve in the input B-Rep
   (`BRepEdge.curve`). Prefer the Stage-1 bijection over re-deriving anything:
   `TessellationSource::BRepEdge { edge, t }` (`brep.rs:151`) records, per mesh
   vertex, the owning B-Rep edge AND its parameter — and for a `Curve::Circle`
   "`t` is an **angle in radians**" (PR-YR7). Where that tag survives, the
   vertex's canonical position is simply the curve evaluated at `t`: exact by
   construction, no projection and no selection tolerance. Fall back to §4's
   projection only for vertices with no such tag (see §8).
2. **Select vertices.** Both endpoints, EXCEPT any vertex that:
   - is an endpoint of a cross-input curve (a key of `curves0`) — those are
     A×B junctions already relocated and required to lie on BOTH curves;
     moving them would break that (measured: the I1 fixture's exact junction
     `v5` at +7.0361° must not move, while `v6` must);
   - is already on the curve within `TAU_WORK` (no-op).
3. **Project** onto the derived curve — closest point, exact for `Circle`
   (project into the circle's plane, rescale radially to the radius).
4. **Band guard (P9/P10).** Let `d` be the displacement and `bound` the owner's
   Stage-1 chord bound (`chord_tol_for_curved_owner`):
   - `d <= bound` ⇒ relocate. This is exactly the chord-position artifact class;
     the vertex was never further from its curve than Stage 1's own guarantee.
   - `d > bound` ⇒ **LOUD STOP** (`Stage4RegionInvalid` /
     `LocalRefinementRequired`). A vertex further from the rim than the chord
     bound is NOT this class and must not be snapped — that would be tolerance
     widening producing a right answer for a wrong reason.

**Not a tolerance band.** The guard does not admit anything; it *refuses*
anything outside the bound Stage 1 already guarantees. The relocation itself is
an exact projection onto an exact curve.

## 5. Why this is safe

- It moves vertices only ONTO an analytic curve they are already within the
  Stage-1 chord bound of — it introduces no new surface, no new topology, no new
  vertex, and no edge split.
- Junction vertices (the only ones with a competing constraint) are excluded by
  construction.
- Any case with no off-curve rim vertex sees zero displacement ⇒ gate-OFF and
  gate-ON must be byte-identical on the clean majority of the corpus.

Residual risk to measure, not to argue: a relocated rim vertex changes the mesh
handed to downstream ops in a chained case, so a chain could move. That is what
inc-2's corpus run is for.

## 6. Increments

- **inc-0 — this spec.** DONE.
- **inc-1 — the primitive, gated OFF / unwired.** Curve derivation + projection +
  guards as a pure function over `(mesh, incidence, curves0, a, b)`. Unit tests
  on hand-built fixtures: (i) a pristine chord-position vertex relocates exactly
  onto the circle; (ii) a vertex perturbed off the chord (the R0063 sub-class)
  still relocates; (iii) a junction vertex is NOT moved; (iv) a vertex beyond the
  chord bound raises the loud STOP; (v) an already-exact vertex is a bit-exact
  no-op.
- **inc-2 — wire, gated.** Gate-OFF corpus **byte-identical** to 252C/0W/58E/0T.
  Gate-ON: the n2 `i1` test green with the #195 arm on, and the corpus measured.
  Target: F0083 / R0027 / R0099 convert; nothing regresses.
- **inc-3 — flip**, if inc-2 is clean, then re-run the #195 gate-ON assay (it
  should then be worth its honest +2 with `i1` green, unblocking that flip too).

## 7. Corpus-tail triage (MEASURED 2026-07-28, `KV2_OFFSURF_PROBE`) — R0027 is OUT of scope

The spec's own pre-build check, run before writing any code:

| case | site | residual | band | relative | class |
|---|---|---|---|---|---|
| F0083 | `cylpatch-vertex` | 2.305e-3 | 1.331e-9 | **3.3 % of r**=6.914e-2 | IN — chord family |
| R0099 | `cylpatch-vertex` | 8.651e-2 | 9.033e-9 | **2.8 % of r**=3.125 | IN — chord family |
| R0063 | `cylpatch-vertex` | 3.126e-5 | 1.002e-9 | **6.9 % of r**=4.538e-4 | IN — chord family (§2) |
| **R0027** | **`torus-vertex`** | 3.725e-9 | 2.138e-9 | **1.74e-12 of minor_r**=2137.7 | **OUT** |

F0083 and R0099 carry percent-of-radius residuals consistent with a chord
sagitta — the same family as R0063, so the pass should reach them.

**R0027 is a different beast and must not be swept in here.** It is a TORUS
vertex, it fails inside `revolve` (a kernel-v2 constructor, so neither this pass
nor yang-rs is even on the path), and its residual is only **1.74×** its band —
that band being `minor_r · CURVED_SURFACE_DEBUG_TOLERANCE` (1e-12 relative), far
tighter than `import_band`'s 1e-9. So it is a ~1.7e-12 RELATIVE miss on an
exact-construction check at coordinates ~7.6e3: a question about whether the
revolve constructor is exact at that scale, not a chord-position artifact.

Per the "structural fixes first / do not tune bands" posture, R0027 gets its own
increment in `kernel-v2` and is explicitly NOT a success criterion for inc-2.
Expected inc-2 conversions are therefore **F0083 and R0099** (plus the n2 `i1`
test and R0063 under the #195 arm) — not the full tail.

---

## 8. inc-2 design revision (2026-07-28) — use the Stage-1 bijection, not SSI

Found while closing inc-1. `TessellationSource::BRepEdge { edge: u32, t: f64 }`
(`crates/yang-rs/src/brep.rs:151`) already carries exactly what this pass needs:
the owning B-Rep edge and the parameter along it, with `t` an **angle in
radians** for a `Curve::Circle`. This is the Stage-1 bijection the crate's own
rules call first-class ("TessellationMap is first-class... Stages 5/6 consult
it").

So inc-2 has TWO arms, and the exact one is preferred:

- **(a) Tagged vertex — exact re-evaluation.** A vertex still carrying
  `BRepEdge { edge, t }` has a canonical position: its owning edge's curve
  evaluated at `t`. Re-seat it there. This is "boundary curves map to boundary
  curves" implemented literally and exactly — no projection, no selection
  tolerance, no SSI call, and no ambiguity about WHICH curve (the tag names the
  edge, so `matched != 1` cannot arise).
- **(b) Untagged vertex — inc-1's projection.** A vertex MINTED after Stage 1
  (the Stage-0 overlay insertions — the measured `v6` class — arrives as
  `Intersection`/`Unknown`) has no parameter to re-evaluate, so it needs the
  closest-point projection with the chord-bound guard that inc-1 shipped.

Both arms keep the §4 guards: junction vertices excluded, displacement beyond
the owner's Stage-1 chord bound is a LOUD STOP. On arm (a) the guard becomes a
pure P10 safety net — a tagged vertex further from its OWN recorded parameter
than the chord bound means something upstream corrupted the bijection, which
must be loud rather than silently re-seated.

**Open question for inc-2 (measure first, do not assume):** how many of the
defective vertices still carry a `BRepEdge` tag in the OUTPUT mesh? `boolean()`
spatially re-matches output vertices to input ones (`brep.rs:161`), so some rim
vertices should retain it — but `v6` is overlay-minted and probably will not.
Measure the tag census on the I1 fixture and on R0063/F0083/R0099 BEFORE
building, the same way §7's triage was run: if arm (a) covers nothing, build
only (b); if it covers most, (b) stays a narrow fallback.
