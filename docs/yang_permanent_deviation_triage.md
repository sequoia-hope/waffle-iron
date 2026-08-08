# Triage of the 45 PERMANENT deviations

**Date:** 2026-08-08. **Method:** every entry in `docs/yang_deviations.md` with
`State: PERMANENT` read in full (not classified from the status-index line).
**Purpose:** separate divergences that are forced, borrowed, or derived from the
ones that are genuinely invented — the last group is the only one that carries
paper-compliance risk.

**Headline:** of 45, **8 are genuinely invented AND answer-affecting**, and they
collapse into **3 root causes**. The other 37 are scope consequences, Cherchi-port
decisions, derived metrics, re-identified paper operations, or bookkeeping.

---

## Bucket 1 — Forced by the analytic-primitive scope (6)

Yang's pipeline takes NURBS → rational Bézier sub-patches. Ours takes five
analytic surfaces. Where exactness makes the paper's machinery vacuous or where a
closed form beats the paper's iteration, the divergence is *downward-compatible*:
analytic surfaces are a strict subset of NURBS, so an analytic-only kernel can be
a faithful Yang implementation for that subset.

| id | divergence | why it is forced |
|---|---|---|
| D14 | no NURBS/Bézier; five analytic primitives | the scope decision itself; every other entry here is its shadow |
| N5 | Stage-1 planar 1:1, no `d_ε` iterate | a plane carries **zero** chord error, so `d_ε` densification is a no-op |
| N7 | closed-form algebraic SSI instead of §4.3 Newton | exact beats iterative for quadrics; strictly stronger than the paper |
| N9 | no-Steiner planar CDT (spade) | same exactness argument as N5; resolves the D1 "no ear-clipping" concern |
| N29 | §4.5.3 reversal test via **exact conic parameters**, not the paper's discrete tangent-angle proxy | the proxy exists because Bézier points have no closed-form parameter; ours do |
| N37 | sphere×cylinder / sphere×cone → procedural `SurfacePair` | degree-4 analytic pairs the Bézier framing never has to name |

**Risk: none.** Four of the six are the paper's method replaced by an exact one.

---

## Bucket 2 — Cherchi C++ port, not Yang at all (11)

These are divergences from the **Cherchi 2022 reference implementation** (Stage 2
only). They have no bearing on Yang-paper compliance. Two sub-kinds:

### 2a — reference-bug corrections (5): the C++ is wrong, we are right

| id | the C++ defect |
|---|---|
| N19 | `perturbRayAndFindIntersTri` early-`break` mixes hits gathered under **different** perturbed rays and sorts them with the last one; can also index an empty vector (UB) |
| N20 | keeps ray-parameter-**zero** hits, so a point-touching tetra pair labels the touching solid "inside" — a silent-wrong |
| N21 | `std::exit(EXIT_FAILURE)` on a fully-implicit patch ("requires exact rationals"); we implement the missing rational branch |
| N24 | trusts Shewchuk adaptive `0.0` as certified Zero — Shewchuk's guarantee **excludes underflow**; a true determinant ≈ 0.36·5e-324 read as a false Zero and silently discarded all of input B |
| N23 | our port had hardened a *debug* assert into a production error, stricter than the shipped NDEBUG reference; relaxed to match, verified by reference parity |

N20 and N24 are corrections of **silent-wrong** classes in the reference. Keeping
them would have been the bug.

### 2b — port structure / staged scope (6)

N13 (TPI deferred to AR2 — direct source reading contradicted the prompt),
N14 (readable `splitSingleTriangle` over the stack variant; **same output mesh**),
N15 (TPI routing via macro dispatch + blocking STOP), N16 (per-work-item
`source_tri` replaces global `seg2tris`), N17 (exact triage of coplanar deferrals
instead of deferring all), N18 (exact-coordinate canonicalization — the C++ keeps
one global vertex identity, our port reaches points per-pair and needs a
canonicalizing pass).

**Risk: low**, and orthogonal to the paper.

---

## Bucket 3 — Derived from the paper's own quantities (5)

Not invention: `d_ε` propagated into a metric the paper does not itself discuss,
with the derivation written down.

| id | derivation |
|---|---|
| N11 | a sphere vertex within `d_ε` **along its normal**, cut by a plane, projects to in-plane radial deviation up to `(R/r_circle)·d_ε`. Derived from `d/d(radial)√(h²+radial²)` |
| N39 | two chord half-spaces of half-width ρ meeting at gradient angle α intersect in a slab of half-width `ρ/sin α` ⇒ curve-membership band `d_ε/sin α` |
| N46 | exact cylinder∩plane generator band (Patrikalakis Ch.5 + `d_ε`), replacing a linear amplification factor |
| N38 | correctness fix: the cone chord bound must come from the edge's **own** cone band on a multi-band cone. Uses the paper's own single-source rule (A14.3), not a widening |
| N45 | a plane through a cone apex sections it into a **line pair** (Patrikalakis Ch.5); both are returned and the mesh edge lies on exactly one — position tie-break |

**Risk: low.** These are the same quantity in a different metric, and each shows
its work.

---

## Bucket 4 — Re-identified AS paper operations (3)

The 2026-07-16 N53 audit retired four "non-compliant tolerance welds". N55/N56
then **corrected that audit**: three of the four were genuine paper operations
with the wrong criterion.

| id | what it actually is |
|---|---|
| N55 | the `subfeature` weld is literally **Fig-11(b)**: "if a split-edge endpoint p is too close to q, merge p with q." Retightened criterion → compliant always-on merge; 228C→232C |
| N56 | `coincident` and `subres` are **§4.3**: *"During optimization, we remove a point if it is too close to another point on the same loop."* Reinstated; 232C→240C, 0 WRONG. Only the `f32` render weld was a real hack — it stays retired |
| N47 | the `coincident` weld's entry; reclassified by N56 as faithful §4.3 redundant-point removal |

**Risk: negative** — compliance went *up*. Worth noting the lesson the ledger
draws itself: *"the correct audit test is 'is it in the Yang paper?', not 'does it
use a tolerance.'"* An over-strict audit cost 12 correct cases.

---

## Bucket 5 — Generalization of a stated paper mechanism, or kernel-v2 constructor scope (8)

The paper states the mechanism; we widen the input class it accepts. Several are
not yang-pipeline stages at all.

| id | generalization | basis |
|---|---|---|
| N25 | §4.5.5 from one coplanar A×B pair → n-ary plane groups + tessellated faces | §4.5.5 |
| N44 | mixed-orientation side-A faces in an n-ary group | §4.5.5 Fig 16 — the A/B/overlap segmentation is a partition **of the plane**, orientation-independent |
| N28 | torus-profile rim crossings (CapLateral torus arm, poloidal opposite-rim projection) | our primitive set; a torus band's two rims must keep matched sample counts |
| N30 | circle × plane∩plane-line junction closed form | §4.4.1 relocation onto both incident curves |
| N31 | cone-ellipse / cone-hyperbola same-type junction routing | fixes a map **overwrite** that hid a triple junction |
| N33 | disjoint-union passthrough | explicitly *outside* Yang's interacting-solid scope — A∪B with A∩B=∅ is the disjoint sum |
| N34 | full-turn revolve alternation gate narrowed | **kernel-v2 constructor**, not a yang stage |
| N35 | closed-torus / on-axis-sphere full-turn revolve (+ a Stage-4 containment guard) | **kernel-v2 constructor**, not a yang stage |

**Risk: low-moderate.** Each widens an input class rather than changing an
algorithm. N34/N35 are primitive construction, outside Yang entirely.

---

## Bucket 6 — Not a divergence at all (1)

**N36** — tolerance-vocabulary consolidation (`TAU_EVAL`). Its own entry says:
*"Class: refactor, value-identical — no behavior change; every replaced literal
keeps its exact prior value."* It is filed as a PERMANENT deviation and inflates
the count. **Recommend: reclassify to HISTORICAL.**

---

## Bucket 7 — GENUINELY INVENTED (10)

No paper mechanism. Split by whether the invention can ever produce an answer.

### 7a — refuses only; cannot make the kernel lie (2)

| id | what it does |
|---|---|
| N42 | Stage-6 planar-face **gross** non-planarity self-check — a producer-contract STOP for the #146 off-plane emission class |
| N57 | sub-resolution coplanar-gap STOP: two genuinely DISTINCT parallel planes inside the detection band reject loudly instead of the overlay silently dissolving the interposed feature (C0111/C0113) |

Both convert a silent wrong into a typed error. That is the P10 posture the
Constitution *prescribes*; the worst case is a false STOP, never a wrong solid.
**Risk: acceptable by design.**

### 7b — answer-affecting, and this is the group that matters (8)

They do not cluster by symptom. They cluster by **three root causes**:

**Root A — §4.2.3 exact per-triangle provenance is not implemented (3 entries).**
The paper resolves which B-Rep face a kept mesh triangle came from via the
labeled arrangement's exact barycentric implicit map. We use geometric proximity
plus two disambiguation heuristics stacked on top.

- **N10** — intersection-edge classification gated by an on-both-surfaces
  predicate, because `compute_phase_a` pushes a patch's inherited surface onto
  *every* boundary edge and mis-tags single-surface edges. The entry names its own
  compliant replacements: re-tag by local incident-triangle surface, or consume
  true two-surface provenance from the `LabeledArrangement` producer — *"the
  paper's intent."*
- **N12** — face resolution ranks ties by an **exact-vs-band tier** invented here.
- **N43** — a `Plane` face is measured by its triangle's **worst** vertex rather
  than the centroid, because a straddling triangle fools the centroid test.

*Closing §4.2.3 retires all three.*

**Root B — exact → f64 emission rounding (2 entries).** The paper's arrangement is
exact throughout and never specifies an f64 output stage; ours must land on f64.

- **N26** — a triangle of three distinct **exact** vertices whose f64 image is
  collinear gets a constrained edge-collapse repair instead of failing.
- **N27** — a rim-override point that coincides with a uniform rim sample is
  merged (the direct consumer of N26).

**Root C — cleanup of defects minted by our own upstream stages or by chaining
(3 entries).** Yang never feeds a boolean output back in as an operand; we do.

- **N22** — fold-sliver exclusion + loop T-subdivision. The entry states it
  plainly: *"The paper does not treat these degenerate children."*
- **N40** — backtrack-spike normalization of **chained-boolean-drift** operand
  loops: deleting a spurious degree-2 needle vertex our own previous output made.
- **N41** — doubled-membrane removal: deleting coincident opposite-winding
  triangle pairs the mesh boolean minted, which read as the topologically
  impossible χ=3.

**Plus one that fits none of the three:**

- **N32** — output arc orientation obeys the CCW-minor convention, so a yang
  boolean OUTPUT is a valid yang boolean INPUT. Yang has no directed-arc-with-
  stored-normal representation, so there is no paper mechanism to diverge from;
  this is producer-contract enforcement in a representation the paper lacks.
  **Risk: low.**

---

## Summary

| bucket | n | risk |
|---|---:|---|
| 1 — forced by analytic scope | 6 | none |
| 2 — Cherchi port (5 reference-bug fixes + 6 port scope) | 11 | low, orthogonal to Yang |
| 3 — derived from the paper's own quantities | 5 | low |
| 4 — re-identified AS paper operations | 3 | negative (compliance rose) |
| 5 — generalization / kernel-v2 constructor scope | 8 | low-moderate |
| 6 — not a divergence (mis-filed refactor) | 1 | none — reclassify |
| **7a — invented, refuses only** | **2** | **acceptable by design (P10)** |
| **7b — invented, answer-affecting** | **9** | **the real surface area** |

Of 7b, **8 reduce to 3 root causes** (A: §4.2.3 provenance ×3, B: exact→f64
emission ×2, C: our own upstream mints / chaining ×3) and one (N32) is a
representation the paper does not have.

## Recommendations

1. **Root A is the single highest-value compliance target in the ledger.** One
   unimplemented paper mechanism (§4.2.3 barycentric implicit provenance) is
   generating three stacked heuristics that decide face attribution — and face
   attribution decides which faces survive a boolean, i.e. exactly the class the
   review of 2026-08-06 found is geometrically unverified on 159 of 261 passing
   cases. These two findings point at the same place from opposite ends.
2. **Root C is the mesh-updating epic's actual customer list**, and it is not the
   self-crossing-loop cases the N2-3 campaign has been building against. N22/N40/N41
   are downstream cleanup standing in for upstream exactness.
3. **Reclassify N36** (value-identical refactor) out of the divergence ledger.
4. **Nothing here is a tolerance hack of the N53 kind.** The one real hack the
   audit found — the `f32` render weld — is retired and stays retired. The other
   three the audit accused were paper operations, and reinstating them correctly
   was worth 12 corpus cases.
