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

`stage4_boundary_curve_relocate` (**always-on since inc-5, 2026-07-28**; was
gated behind `YANG_S4_RIM_SNAP_ENABLE`, default OFF — see §18).
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

---

## 9. inc-2 RESULT (2026-07-28) — wired, safe, fixes the reproduction; does NOT reach the corpus tail

**Census first (§8's arm (a) is REFUTED).** In-crate census of the I1 fixture's
output: cylinder-loop vertex sources are `{BRepVertex: 304, BRepEdge: 14}` with
the gate on, and **the defective `v6` is `BRepVertex(6)`** — not
`BRepEdge { edge, t }`. The OUTPUT `TessellationMap` is essentially
self-referential (output mesh vertex → output B-Rep vertex); it does NOT carry
provenance back to the input operand's edge parameter. So there is no recorded
`t` to re-evaluate and **arm (a) is unbuildable from the output map**. inc-2 is
arm (b) only: the guarded projection, with the curve derived in closed form from
the surface pair (`rim_circle_from_pair`: cylinder + PERPENDICULAR plane ⇒ an
exact `Circle`; oblique planes cut ellipses and are skipped, never approximated).

**A premise was refuted by a test, and the fix was to NARROW.** The first wiring
trusted the derived circle and STOPped when a vertex was beyond the chord bound.
That reddened `m8_nary_tessellated_overlay::flush_pocket_subtract_and_union_partition`:
**a same-input `Cylinder`+`Plane` patch adjacency does NOT imply the shared edge
lies on cylinder∩plane** — after a boolean a cylinder patch can be adjacent to a
plane patch along a trimming boundary nowhere near the rim. The derived circle is
therefore a CANDIDATE and membership is VERIFIED per edge: both endpoints must
be within `bound`, else the whole edge is abandoned (including its in-band
endpoint, which must not be snapped to a curve it may not belong to). "Beyond the
bound" now means "not this rim", not "defect" — so the loud STOP is reachable
only as a classification signal inside the pass, never as a boolean failure.

**Measured.**
- n2 `i1` with the #195 arm ON: **GREEN** — the pass moves exactly `v6`
  (`rim_edges=146 cross_excluded=20 bound=6.386929e-6 moved=1`).
- Full yang-rs suite: green in BOTH gate states.
- Full corpus, gate ON: **252C/0W/58E/0T — ZERO category deltas.** No
  regressions, and gate-OFF is unchanged by construction (every addition is
  inside `if rim_snap_enabled()`).
- **But no conversions.** F0083 and R0099 still fail `VertexOffSurface`.

**inc-3 question (measured lead, do not guess).** On F0083 the pass RUNS and
claims rim edges (`rim_edges=21`, then `15`) but `moved=0`, even though the
defective residual (2.305e-3) is INSIDE that op's bound (2.542e-3). And the
failure surfaces at a LATER op's import validation, i.e. the bad vertex is
already in that op's INPUT B-Rep — minted by an earlier op whose Stage 4 did not
claim it. So inc-3 must find out which of the three exclusions dropped it:
(a) the pair is not Cylinder+perpendicular-Plane (an ellipse rim, or
Cylinder+Cylinder), (b) membership verification abandoned the edge because the
other endpoint is out of band, or (c) it was excluded as a cross-input endpoint.
Probe `YANG_S4_RIM_SNAP_PROBE` per op on the PRODUCING op, not the failing one.

---

## 10. F0083 producing-op probe (2026-07-28) — a DIFFERENT class: the Fig-11 point q itself

`YANG_S4_RIM_SNAP_TARGET=x,y,z,r` (banked) reports, per mesh vertex near a target
point, every incident incidence edge and which filter dropped it. Aimed at
F0083's off-surface point in the PRODUCING op:

```
[rim-target] v=80 dist_to_target=0.000000e0 cross_excluded=true
[rim-target]   edge=(66,80) entries=["A:Plane", "A:Cylinder"] same_input=true diff_surf=true circle=true claimed=true
[rim-target]     endpoint v=66 resid=6.99e-18  in_band=true
[rim-target]     endpoint v=80 resid=2.304643692850343e-3  in_band=false
[rim-target]   edge=(79,80) entries=["A:Plane", "B:Plane"]    same_input=false circle=false claimed=false
[rim-target]   edge=(80,118) entries=["A:Cylinder", "B:Plane"] same_input=false circle=false claimed=false
```

**TWO independent exclusions fire, and both are correct as designed:**

1. `cross_excluded=true` — v80 IS an endpoint of a CROSS-input curve. Its other
   two edges are `A:Plane × B:Plane` and `A:Cylinder × B:Plane`.
2. Membership verification fails — its residual against A's own rim circle is
   2.3046e-3 against that op's bound 6.9298e-4, i.e. **3.3× outside**.

So v80 is simultaneously on A's OWN rim (`A:Plane ∩ A:Cylinder`, edge claimed,
whose other endpoint v66 is exact to 7e-18) AND on the A×B intersection curve.
**It is exactly Yang Fig. 11's point q — "an intersection point ON the boundary
curve" — and our pipeline seated it 2.3e-3 OFF the boundary curve it also
belongs to.** The cross-input relocation satisfied the A×B curve and ignored A's
rim.

**Consequence: F0083 is NOT the `v6` class and inc-2 structurally cannot fix
it — correctly.** Projecting v80 onto A's rim would break its A×B curve
membership; that is precisely why junction vertices are excluded. The two classes
are now cleanly separated:

| class | vertex | fix |
|---|---|---|
| **v6 / n2 I1** | on the operand's own rim ONLY, unclaimed, left at a chord position | inc-2 (SHIPPED): guarded projection |
| **v80 / F0083** | on the own rim AND the A×B curve — the Fig-11 q | inc-3: must satisfy BOTH |

**inc-3 is therefore a curve-CURVE intersection, not a projection.** q must be
placed at the intersection of the A×B intersection curve with the operand's own
rim circle — Fig. 11(a)'s "locate the constrained edge containing q and split it
using q", with q required to lie on the boundary curve. A projection onto either
curve alone is wrong by construction. Note the 3.3×-over-bound displacement also
says the junction is currently seated well off the rim, so this is a real
mis-seat, not rounding.

Open sub-question for inc-3: is v80's A×B curve itself exact (a conic), in which
case circle∩conic is closed-form, or a procedural surface-pair curve, which
needs the Newton pair-projection machinery (`relocate_onto_implicit_pair`)?

---

## 11. inc-3 fully specified (2026-07-28) — q is a TRIPLE POINT, and the guard is a certificate

Two measurements settled the design, and the first REFUTED the hypothesis inc-3
was about to be built on.

**(1) Not a chord artifact.** The claimed rim edge (66,80) spans 12.752766°, so
its OWN sagitta is **4.277442e-4** — yet v80's residual is **2.3046e-3, 5.4×
that** (and 3.3× the global bound 6.929823e-4). So "the global bound was simply
the wrong bound for this rim" is REFUTED: no Stage-1 chord error of this rim can
explain the displacement. v80 is not un-relocated, it is *mis*-relocated.

**(2) It lies exactly on two of its three surfaces.** Implicit values at v80:

| surface | value |
|---|---|
| `A:Plane` | −5.551115123125783e-17 (exact) |
| `B:Plane` | 0.0 (exact) |
| `A:Cylinder` | **−2.3046436928503417e-3** |

So v80 sits EXACTLY on the line `A:Plane ∩ B:Plane`, but at the wrong point
along it — it never reaches `A:Cylinder`. Its correct position is the **triple
point** `A:Plane ∩ B:Plane ∩ A:Cylinder`: equivalently, where that line pierces
the cylinder, or where A's own rim circle (`A:Plane ∩ A:Cylinder`) crosses
`B:Plane`. Both readings are the same point and both are CLOSED FORM.

### The inc-3 pass

Selection — a vertex that is ALL of:
- an endpoint of a CLAIMED own-rim edge (so its rim circle `C` is known);
- excluded from inc-2 as a cross-input endpoint (i.e. it is a Fig-11 q);
- incident to exactly ONE distinct other-operand surface, and that surface is a
  `Plane` (the measured instance; anything else returns `None` and is skipped,
  never approximated — the inc-2 discipline).

Solve — `C ∩ Plane` in closed form. With `C = (c, n̂, r)`, an orthonormal in-plane
basis `(û, v̂)`, and the plane `m̂·x + d = 0`:

```
A = r(m̂·û),  B = r(m̂·v̂),  K = m̂·c + d
A·cosθ + B·sinθ = −K      ⇒   Rr = hypot(A, B)
|K| > Rr ⇒ NO intersection (skip — the rim does not reach the plane)
θ = atan2(B, A) ± acos(−K / Rr)      (two roots)
```

Take the root nearest the current seat. **Guard is a CERTIFICATE, not a
tolerance:** accept only if the chosen q satisfies all three surfaces to ~f64
noise (each implicit value ≤ `TAU_WORK`-scaled), and it is the nearer root. A
displacement band is explicitly WRONG here — measurement (1) proves the
displacement is not chord-bounded, so any band would either refuse the real fix
or admit anything.

Do NOT reuse inc-2's `boundary_relocation_for_vertex` for this class: its band
guard would refuse q (2.3e-3 > 4.28e-4). inc-2 and inc-3 are different classes
with different acceptance rules, which is why inc-2 excludes these vertices.

### Status

NOT BUILT. Design is complete and measured; the remaining work is the solver,
its tests (root selection, the no-intersection skip, the certificate refusal),
the gated wiring, and a corpus run.

---

## 12. inc-3 BUILT (2026-07-28) — works, peels one layer, corpus-neutral; F0083 has MORE than one q

`circle_plane_nearest_root` (closed form, root nearest the current seat),
`satisfies_all_surfaces` (the §11 certificate), `plan_triple_point_reseats`
(selection per §11), wired behind its OWN gate `YANG_S4_TRIPLE_POINT_ENABLE` so
the two classes measure independently. 13 unit tests total in the group.

**It works.** On F0083's producing op: `candidates=2 reseated=2`, and v80 moves
from `(-1.49702e-2, -5.47636e-2, 3.30980e-1)` to
`(-1.54787e-2, -5.70093e-2, 3.31077e-1)` — a displacement of ≈2.31e-3, exactly
the measured residual, i.e. it lands on the cylinder it was missing.

**But F0083 still ERRORs, on a DIFFERENT vertex.** The tripwire now names
`p=(-2.49052e-2, -5.21491e-2, 3.29829e-1)` at residual **1.914e-3** (was 2.305e-3
at another point). So the fix is correct and the case simply has MORE THAN ONE
Fig-11 q; inc-3's selection caught two of them in that op and the next one fails
one of the three selection tests.

**Corpus, BOTH gates ON: 252C/0W/58E/0T — ZERO deltas.** No regressions, no
conversions. yang-rs suite green in all gate combinations; gate-OFF unchanged by
construction.

### inc-4 (the next layer)

Probe the NEW vertex with `YANG_S4_RIM_SNAP_TARGET` exactly as §10 did and find
which of the three selection tests drops it:
- not an endpoint of a *claimed* rim edge (its rim pair may be
  Cylinder+Cylinder, or an oblique plane ⇒ ellipse, both of which
  `rim_circle_from_pair` skips by design);
- not `cross_excluded` (then it is the inc-2 class, and inc-2's band refused it);
- more than one distinct other-operand surface (a 4-surface corner, which needs a
  genuine multi-surface solve rather than circle∩plane).

The third is the interesting possibility and would be a real capability step, not
a selection tweak. Measure before building — the last two increments were both
re-scoped by exactly this kind of probe.

---

## 13. inc-4 probe (2026-07-28) — the next layer is NOT a §4.4.1 class; it is an UNBUILT cross-input curve

Probing F0083's new off-surface vertex:

```
[rim-target] v=118  dist=0  cross_excluded=FALSE
  SURFACE A:Cylinder implicit_value=-1.9143943665599628e-3
  SURFACE B:Plane    implicit_value=-5.551115123125783e-17   (exact)
  edge=(80,118)  ["A:Cylinder","B:Plane"]  same_input=false  circle=false claimed=false
  edge=(116,118) ["A:Cylinder","B:Plane"]  same_input=false  circle=false claimed=false
```

Three facts, and none of them matches the §4.4.1 classes:

1. **Only TWO surfaces**, not three — so it is not a triple point, and §12's
   "4-surface corner" guess is REFUTED.
2. **It has NO own-rim edge at all.** Both incident edges are CROSS-input, so no
   rim circle is claimed and inc-2/inc-3 correctly never look at it.
3. **It lies EXACTLY on `B:Plane` (−5.55e-17) but 1.9e-3 OFF `A:Cylinder`** — and
   `cross_excluded=false`, meaning neither of its edges is a key of `curves0`.

(3) is the diagnosis. A vertex on the `A:Cylinder ∩ B:Plane` intersection edge
must lie on BOTH surfaces. It does not, AND its edges are absent from the
cross-input curve map — so **`build_intersection_curves` never produced a curve
for them, and Stage 4 therefore never relocated their vertices onto it.** Had the
curve been built, the existing cross-input machinery would have seated this
vertex correctly.

**So F0083's remaining defect is upstream of this spec entirely: an A×B
intersection edge whose analytic curve was never built.** That is PR-YR9 /
Stage-3 territory (`build_intersection_curves` selection), not §4.4.1 mesh
updating. It should be tracked as its own task, and this spec's remaining scope
(inc-2 + inc-3) is COMPLETE as designed.

**Next step for that task, not this one:** find why `build_intersection_curves`
skipped `A:Cylinder × B:Plane` on this edge — a cylinder∩plane is a circle or
ellipse, so the likely candidates are the unique-selection test failing
(`matched != 1`) or the incidence entry count not being exactly 2.

## 14. ROOT CAUSE of the F0083 residual (2026-07-28) — the `on_both` gate is CIRCULAR

Confirmed with the existing `YANG_V_PROBE`. `build_intersection_curves`'
`on_both` gate (`stage3_ssi.rs:615`) skipped BOTH of v118's edges:

```
on-both gate SKIP edge (80,118)   tol=6.930e-4  d_s=(2.305e-3, 0.000e0)   d_e=(1.914e-3, 5.551e-17)
on-both gate SKIP edge (116,118)  tol=6.930e-4  d_s=(0.000e0, 5.551e-17)  d_e=(1.914e-3, 5.551e-17)
```

The gate requires BOTH endpoints to be on BOTH surfaces within the Stage-1 chord
band `tol`, and treats a failure as "this is a single-surface internal edge, not a
true intersection edge" → `continue` → `LineSegment` fallback.

**On edge (116,118) that classification is provably wrong:** endpoint v116 is
EXACTLY on both surfaces (0.0 and 5.551e-17). An edge with one endpoint exactly
on both surfaces is a true `A:Cylinder ∩ B:Plane` intersection edge; only its
other endpoint, v118, is off (1.914e-3 off the cylinder, exactly on the plane).

**So the gate is CIRCULAR:** the vertex is off the surface ⇒ the gate concludes
the edge is not an intersection edge ⇒ no analytic curve is built ⇒ Stage 4 never
relocates that vertex ⇒ it stays off the surface. The gate's own precondition is
the very thing the relocation exists to establish.

Its design comment says it "can only reclassify edges that today raise
`AmbiguousCurve` with an endpoint off a surface beyond `tol`" — i.e. it was
introduced to convert a LOUD error into a silent skip. That trade is what
produces this silently-wrong off-surface vertex, and it was invisible until
`kernel-v2/strict-validation` (5b891ec2) made the on-surface tripwire visible in
release.

**Fix direction (a separate Stage-3 task, NOT §4.4.1):** the gate must not
require the endpoint it is supposed to fix to already be correct. Candidates,
in order of preference:
1. **Asymmetric acceptance** — accept the edge when at least one endpoint is
   exactly on both surfaces (a witness that the edge IS the intersection), then
   let Stage 4 relocate the other. This uses the exact witness, not a band.
2. Restore the loud `AmbiguousCurve` for this shape rather than a silent skip, so
   the case fails visibly instead of emitting an off-surface vertex.

Do NOT widen `tol` — that is the tolerance-escalation pattern P9/P10 forbids, and
it would admit genuinely-unrelated edges.

## 15. Asymmetric acceptance ATTEMPTED and REVERTED (2026-07-28) — the exact witness does NOT discriminate

§14's preferred fix was built behind `YANG_S3_ASYM_ONBOTH_ENABLE`: admit an edge
when at least one endpoint is EXACTLY on both surfaces (`TAU_WORK`-scaled), on
the reasoning that such a witness proves the edge IS the intersection. Measured,
then **REVERTED**.

- **F0083:** `VertexOffSurface` → a LOUD Stage-3 SSI refinement error. That is
  the P10 direction (silent-wrong → loud) but NOT a conversion: with the edge
  admitted, the unique-curve selection then rejects it because the far endpoint
  is 1.9e-3 off, beyond `tol`.
- **Corpus, gate ON: 252C/0W/58E/0T, ZERO deltas.**
- **But it REDS two permanent PR-YR18 oracles:**
  `oracle1_misclassified_edge_does_not_raise_ambiguous_matched_zero` and
  `oracle2_no_ambiguous_from_off_cylinder_endpoint` (`tests/yr18_attribution.rs`).

Those oracles exist precisely to pin that a misclassified edge / an off-cylinder
endpoint must NOT raise `AmbiguousCurve` — which is what the `on_both` gate was
introduced to guarantee. My gate admitted their fixtures, so **those fixtures
also carry an exactly-on-both endpoint**.

**Conclusion: the exact witness does NOT discriminate** between "a true
intersection edge with one mis-seated endpoint" (F0083) and "a misclassified
single-surface edge" (PR-YR18). Both present a witness. §14's option 1 is
therefore REFUTED as specified, and the change is reverted rather than shipped
against two permanent oracles.

**What a real fix needs (open):** a discriminator the witness alone does not
provide. Candidates, unmeasured:
- the number of DISTINCT surfaces at the non-witness endpoint (a true
  intersection edge's far endpoint should still be on ONE of the two surfaces —
  F0083's v118 is exactly on `B:Plane`, off only the cylinder; is that also true
  of the PR-YR18 fixtures?);
- whether the candidate curve exists at all (run the SSI, and accept only if a
  curve passes through the witness exactly — using the curve as the arbiter
  rather than the endpoints);
- §14's option 2 (restore the loud error) scoped to the shape F0083 presents.

Start by measuring the PR-YR18 fixtures' non-witness endpoints against BOTH
surfaces — that single measurement decides between the first two.

## 16. §15 measurement TAKEN (2026-07-28) — both proposed discriminators are REFUTED

`YANG_V_PROBE` over the PR-YR18 oracle fixtures, versus F0083:

| | non-witness endpoint | witness endpoint | off / `tol` |
|---|---|---|---|
| PR-YR18 oracle1+2, edges (0,1) and (0,46) | `d=(1.005e-1, 0.000e0)` — off surf0 ONLY | `(0, 0)` | 1.005e-1 / 3.464e-2 = **2.90×** |
| F0083 edge (116,118) | `d=(1.914e-3, 5.551e-17)` — off cylinder ONLY | `(0, 5.551e-17)` | 1.914e-3 / 6.930e-4 = **2.76×** |

**Both §15 candidates die on this table:**

1. *"the far endpoint is still on ONE of the two surfaces"* — TRUE of both. The
   PR-YR18 fixtures' bad endpoint is exactly on surf1 and off surf0, precisely
   like F0083's v118. **REFUTED.**
2. *A magnitude/ratio threshold* — the two are 2.90× and 2.76× of their
   respective `tol`. Indistinguishable. **REFUTED.**

So at the level of information the `on_both` gate has — two surfaces, two
endpoints, their four distances — a true intersection edge with one mis-seated
endpoint and a misclassified single-surface edge are **structurally identical**.
No endpoint-distance predicate can separate them, which retrospectively explains
why the symmetric gate was written the way it was: it is not over-conservative by
oversight, it is at the limit of its own inputs.

**The discriminator must therefore come from the CURVE, not the endpoints.**
Surviving lead (unmeasured): run the SSI, then test whether the EDGE DIRECTION
aligns with the candidate curve's TANGENT at the witness. A true intersection
edge is a chord of the curve, so its direction matches the tangent to within the
chord's own turning angle; a misclassified internal edge runs away from the
curve and will not. That is a geometric test with a self-derived bound (the
chord's turning angle), not a tuned band.

Measure that on both fixtures BEFORE building — this is the fifth successive
hypothesis on this thread, and the previous four were all refuted by exactly this
kind of check.

## 17. Tangent test MEASURED and REFUTED (2026-07-28) — and it explains all the others

At a point on BOTH surfaces the intersection curve's tangent is exactly
`n₀ × n₁` (no SSI needed), so the test is cheap and exact. Measured at each
fixture's witness endpoint:

| fixture | angle(edge, n₀×n₁) | edge length |
|---|---|---|
| PR-YR18 edges (0,1) and (0,46) — MISCLASSIFIED | **12.465233°** | 2.917e-1 |
| F0083 edge (116,118) — TRUE intersection | **14.968750°** | 1.023e-2 |

**REFUTED, and it points the WRONG WAY**: the true intersection edge is *less*
aligned with the true tangent than the misclassified one. No threshold in this
direction can work.

**Worse, F0083 fails its own self-derived bound.** Its edge is 1.023e-2 long on a
cylinder of radius 6.914e-2, so the chord subtends ≈8.5° and the edge direction
should sit ≈4.2° off the tangent. It measures **14.97°**.

**That discrepancy is the general explanation.** v118 is mis-seated by 1.914e-3,
which on a 1.023e-2 edge is ~19% of its length — a large angular perturbation. So
**the mis-seating corrupts the edge direction itself**: any predicate computed
from the edge's own endpoints is contaminated by the very defect it is trying to
detect. That is the same circularity as the original gate, one level up, and it
retrospectively explains every refutation on this thread — endpoint distances
(§16), the exact witness (§15), and now the tangent.

**Conclusion: the `on_both` gate cannot be fixed from edge-local data.** The
discriminating information must come from OUTSIDE the edge. The natural
candidate, unmeasured: the CHAIN — a run of edges sharing the same surface pair,
whose *witness* endpoints are all exact and collectively define the curve
independently of any single mis-seated vertex. Fit/verify against the chain's
exact vertices, then classify each edge against that, rather than asking each
edge about itself.

The probe (`YANG_V_PROBE` tangent-test line) is retained — it is the tool that
produced this and costs nothing when unset.

## 18. inc-2 FLIPPED ALWAYS-ON (2026-07-28) — as the dependency of #195 inc-5

The rim-snap pass shipped corpus-NEUTRAL (§ above: 252C/0W, zero deltas), which
on its own is not a flip case — a corpus-neutral pass has no measured value to
weigh against always-on cost. What changed is that a SECOND arm turned out to
depend on it.

#195's §4.5.4 rim×plane graze refinement (spec
`yang_195_seal_neighborhood_self_overlap.md`) boosts the rim sampling on a
detected graze. That boost EXPOSES exactly the latent Stage-4 blind spot this
pass repairs — an operand's own rim vertex left at its Stage-1 chord position,
because `build_intersection_curves` skips same-input edges (`stage3_ssi.rs:534`).
Measured on the `n2_junction_cluster::i1` oracle:

| gates | i1 |
|---|---|
| both off | GREEN (0 off-band) |
| `YANG_RIM_PLANE_GRAZE_ENABLE` alone | **RED** — 1 vertex 6.840109e-7 off the cylinder vs band 1.000213e-9 |
| + `YANG_S4_RIM_SNAP_ENABLE` | GREEN |
| + `YANG_S4_TRIPLE_POINT_ENABLE` too | GREEN |

So the graze arm's +2 CORRECT (R0072, R0095) is only available WITH this pass.
Both gates were removed in one commit; the combined flip re-measured on the
honest baseline is **252C → 254C, 0W, zero CORRECT→ERROR** over all 312 cases.

**`YANG_S4_TRIPLE_POINT_ENABLE` (inc-3) stays GATED.** It was not part of the
measured combination and has no conversion of its own yet; flipping it is a
separate increment with its own measurement.

This is the general shape worth remembering: **a corpus-neutral repair can be
the enabling dependency of a converting one.** Neutral is not the same as
valueless — it can be the floor another arm needs in order to stand on.

## 19. F0067 ANCHORED (2026-08-01) — inc-2 drops a constraint at a QUADRUPLE point, and inc-3's uniqueness guard cannot pick it up

F0067's post-amendment-19 wall is `s6-planar-loop-nonplanar`: face 888 (an A
gear-tooth flank, inherited plane `n=(0.856701,-0.515813,0)`, `d=-0.0069008`),
loop vertex 1049 **4.096e-5** off that plane against band 2.752e-7 — 149×. The
failing op is the 10th of 10 stacked extrudes: a circle `r=0.20884629` whose
BASE plane `z=1.7518978673859` is FLUSH with the op-9 gear's top cap.

### The measurement

Extended `YANG_S6_NONPLANAR_PROBE` with the pre-Stage-4 provenance columns
(`pre`/`disp`/`inc`/`curve`, populated under `YANG_S5_FOLD_PROBE`). The loop's
two top-corner vertices report:

| v | `curve` | `reloc` | `disp` | post | verdict |
|---|---|---|---|---|---|
| 1050 | `Circle,LineSegment` | `t=-2.0797041` | 1.2329e-3 | r=0.2088466 @ the exact root angle | ON plane (1.4e-17) |
| 1049 | **none** | none | 1.2322e-3 | r=0.2088465 @ its ORIGINAL angle | **4.096e-5 OFF plane** |

Both carry the SAME pre-Stage-4 position and the same incidence
`[A:Plane, B:Cylinder, B:Plane]`. `YANG_S4_RIM_SNAP_PROBE` names the mover
outright: `[s4-rim-snap] v=1050 -> [-0.10171908174514499, -0.18240066210493067,
1.7518978673859231]` — exactly final v1049's off-plane position.

### The mechanism

The junction point is represented by a **femto pair**: two vertices
**1.347289e-15** apart (`YANG_S4_COINCIDENT_PROBE`) — three orders BELOW
`TAU_WORK`, so it is invisible to a bit-exact duplicate census and to every
identification the pipeline runs. Stage 4 then relocates the two members by
two DIFFERENT authorities:

- the member whose edges are keyed into `intersection_curves` is excluded from
  inc-2 as a `cross_curve_endpoint` and is relocated onto the exact
  `Circle ∩ Plane` root — correct;
- the member with no curve key falls to inc-2, whose projection is
  nearest-point-on-the-rim-circle. That projection **preserves the tessellated
  angle and silently drops the A-flank-plane constraint**, landing 4.1e-5 off it.

Both land in face 888's loop, so the flank face is non-planar and Stage 6
rejects it. Note what inc-2 does here is not a tolerance slip: it is an exact
projection onto the RIGHT curve, having discarded a constraint it never knew
about.

inc-3 exists for precisely this class — but it cannot pick this vertex up, for
TWO independent reasons, both measured:

1. its selection requires `cross_curve_endpoints.contains(&v)`, i.e. only
   vertices inc-2 EXCLUDED. This vertex is one inc-2 CLAIMED. (Removing the
   requirement alone changes nothing — reason 2 still bites.)
2. with the requirement removed, the candidate probe reports `v=1050 owner=B
   n_others=2`. At a FLUSH junction the other operand contributes the gear's
   TOP cap plane `d=-1.7518978673859236` **coplanar with the rim's own bottom
   cap** (`d=-1.7518978673859231`, 5e-16 apart) ALONGSIDE the real flank plane.
   That redundant plane adds no constraint but makes `others.len() == 2`, and
   `let [other] = others[..] else { continue }` refuses.

So the point is really a QUADRUPLE point whose fourth surface is a duplicate of
its first. Every guard in the chain is individually defensible; the vertex falls
between them.

### Anchor CONFIRMED causally, not by inference

Behind a temporary gate (widened inc-3 selection + `others` de-duplicated
against the rim's own planes by coplanarity), F0067's face-888 wall
**disappears** and the case advances to a deeper, unrelated wall
(`TessellationFailed FaceId(3994)`, "ring rejected by CDT"). The experiment was
REVERTED — it is a proof of mechanism, not a fix: the same de-duplication also
dropped 6 previously-reseated inc-3 candidates from 7 to 1, which a shipped
version must account for.

### MASKED, not MINTED (censused per case, not inherited)

Per the R0038 lesson, the sibling verdict was NOT assumed. Re-adding a
temporary `YANG_A19_OFF` and censusing at Stage-4 entry: the femto pair is
present with amendment-19 both ON and OFF, at the same position and the same
multiplicity. Stage 0 did not mint it; amendment-19 only made the Stage-6 gate
REACHABLE. inc-2 (always-on since §18) is the mover, and predates both of this
week's amendments.

### What a fix must do (NOT built)

The structural repair is an IDENTIFICATION at each of the two boundaries this
defect slipped through, not a band:

1. identify the sub-`TAU_WORK` twins as ONE junction so a single authority
   relocates them, and/or
2. identify the coplanar duplicate plane against the rim's own surfaces, so a
   flush junction presents its true constraint count — then let the 3-surface
   certificate seat it, with inc-2 refusing any vertex that carries a further
   other-operand surface rather than projecting onto the rim alone.

Scoping either one needs the corpus-wide census of how often inc-2 snaps a
vertex that carries an unconsumed other-operand surface (the "census the arm's
firings BEFORE fixing it" rule), which this session did not run.

**LESSON (5th application).** *When a femto pair survives every identification,
enumerate which protection each member hides behind.* Here the answer is
symmetric and is what makes the case new: one member hides behind
`cross_curve_endpoints`, the other behind inc-2's own claim — and the guard
built to catch the survivor is itself defeated by a SECOND sub-`TAU_WORK`
duplicate, this time of a SURFACE rather than a vertex. A uniqueness guard
(`let [other] = ...`) is an identification decision in disguise.
