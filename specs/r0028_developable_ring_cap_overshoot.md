# R0028 — the rejected ring is a DEVELOPABLE patch whose boundary overshoots its own cap

**Status: ANCHORED 2026-08-04. Fix NOT built.** Anchor only, per the standing
posture: name the mechanism, do not improvise a repair.

Prompted by the 2026-08-03 loop-simplicity census
(`specs/f0067_chord_crossing_ring_self_intersection.md` §5a), which refuted
R0028's membership in F0067's class: R0028 reports the IDENTICAL wall string
yet every one of its PLANAR producer loops is simple. This spec establishes
what its ring actually is.

## 1. The wall, and why its text says nothing

```
Extrude 3: Auto-union failed: kernel-v2 boolean_union failed:
TessellationFailed { face: FaceId(32),
                     reason: "ring rejected by CDT (degenerate/self-intersecting)" }
```

That string is a blanket message. Two independent reasons it cannot identify a
class:

1. **It erases the CdtError.** `triangulate_ring`
   (`crates/kernel-v2/src/tessellate/mod.rs:685`) maps EVERY
   `cdt_polygon_with_holes_floodfill` failure to this one string; the typed
   variant is printed only under `KV2_RING_REJECT_PROBE`. R0028's is
   `TriangulationFailed`.
2. **It is shared by both tessellation cores.** The planar core
   (`sampled_loop_points`) and the developable core
   (`tessellate_developable_patch` → `triangulate_with_pinch_split` →
   `triangulate_ring`, mod.rs:779) funnel through the same function. The
   message names neither the core nor the failure mode.

R0028's ring came from the **developable** core. F0067's came from the planar
one. Same string, different producers.

## 2. The ring — measured

`KV2_RING_REJECT_PROBE`: `cdt_err=TriangulationFailed outer_len=146 holes=0`.
`KV2_PATCH_PROV` confirms `face=FaceId(32)`, 146 entries: **25 boundary
half-edge origins + 121 interpolated samples**.

Exact self-intersection test (same predicate as `YANG_S6_LOOP_SIMPLICITY`,
rational orientations) on the unrolled 2D ring: **2 proper crossings**,
`seg1 × seg142` and `seg4 × seg138`. No touches, no spikes, no degenerate
segments, no duplicate positions.

**Both crossings pair a BOUNDARY segment against a CONSTANT-v sample run** —
which is the whole mechanism, once the ring's structure is read out:

| ring idx | kind | v (unrolled axial) |
|---|---|---|
| 0..14 | boundary chain | 0.000000 … 0.010030 |
| 15..66 | bottom rim row | **constant v = 0** |
| 67..72 | boundary chain | 0.000000 … 0.009071 |
| 73..145 | top rim row | **constant v = 0.009669786** |

The patch is a cylinder lateral bounded below by v = 0, above by
v = 0.009669786, and on the sides by two trimmed chains. **Three vertices of
the first side chain sit ABOVE the top rim row:**

| idx | half-edge | v | overshoot |
|---|---|---|---|
| 2 | `HalfEdgeId(300)` | 0.010030169 | **+3.6038e-4** |
| 3 | `HalfEdgeId(301)` | 0.009873018 | +2.0323e-4 |
| 4 | `HalfEdgeId(302)` | 0.009862532 | +1.9275e-4 |

So the chain rises through the rim row between idx 1 and 2 and drops back
through it between idx 4 and 5 — **exactly the two crossings**, and no others.

## 3. The overshoot IS the distance outside B's own cap plane

The adjacent planar cap face (`YANG_S6_LOOP_PROV`: `face=1 input=B`, a 12-gon,
every vertex `inc=[B:Cylinder,B:Plane]`) carries the plane
`n = (-0.148913640, -0.967203029, 0.205774218)`, `d = -0.003764079603`.
Evaluating `n·p + d` at the ring vertices:

```
 idx=  1   -2.572355e-04
 idx=  2   +3.603829e-04   <== outside
 idx=  3   +2.032321e-04   <== outside
 idx=  4   +1.927458e-04   <== outside
 idx=  5   -2.061060e-04
 idx=139   +5.311775e-12   (a rim vertex — on the plane)
```

The signed distances **equal the unrolled v-overshoots to every printed
digit**. The unrolled axial coordinate and the cap-plane residual are the same
measurement, so this is not a parametrization artifact: three real B-Rep
vertices of B's lateral face lie **beyond B's own trimming cap**, where B's
lateral surface does not exist.

Scale check — this is a defect, not noise. Patch u-span 0.0540 (circumference,
so radius ≈ 0.00859), height 0.00967. The 3.60e-4 overshoot is **3.7% of the
patch height, 12.6× the ring's minimum segment (2.87e-5), and 3600× TAU_MODEL**.

Neither is it a seam artifact: the crossings sit at u ≈ 0.0016…0.0053, far from
the unroll seam at u = 0 / u = 0.0539 (node 0 appears at both, as expected).

## 4. NOT the F0067 mechanism — measured, not inferred

F0067's class is a Stage-4 refinement displacing a vertex further than its own
local segment. R0028 has **no Stage-4 displacement at all**:

- `YANG_S5_RELOC_SET n_relocations=0` for BOTH ops of the failing boolean.
- The adjacent cap loop reports `disp=0.0000e0` on all 12 vertices via the
  `S4_PRE_POS` oracle (the correct one — `relocations` alone is blind on torus
  models, but this model is cylinder+plane and the two oracles agree).
- The three offenders are half-edge ORIGINS, not interpolated `sample` points,
  so the render re-sampling did not mint them either.

The defect is a **containment violation in the emitted B-Rep**: the A×B
intersection curve on B's lateral surface was not trimmed at B's own cap rim,
so the lateral patch's loop claims boundary that belongs past the cap. This is
a Stage-5 patch-segmentation / curve-trimming question, upstream of anything
§4.5.2 local refinement addresses.

## 5. Why the census could not see it

`YANG_S6_LOOP_SIMPLICITY` covers PLANAR emitted loops only and counts curved
faces as `curved_faces_not_scanned` (6,870 corpus-wide). R0028's defect is on a
cylinder lateral, inside that declared gap. The declared gap was load-bearing:
had it been reported as "simple" instead of "not scanned", this anchor would
have started from a false negative.

**The gap is closable for this class.** A cylinder or cone lateral unrolls
exactly the way `tessellate_developable_patch` already unrolls it, so the same
exact scan applies to developable faces — the projection is a local isometry
and a 2D crossing away from the seam IS a 3D crossing on the surface.

## 6. The class — SWEPT 2026-08-04, and this mechanism is a SINGLETON

Corpus sweep, all 312 cases, `KV2_RING_REJECT_PROBE` then `KV2_RING_PROVENANCE`
on the hits (the `idx=0` provenance line preceding each reject names the face
AND its core). The provenance face equals the wall face in all 8 hits, so the
classification is unambiguous.

**Exactly 8 cases reject a CDT ring corpus-wide** — precisely the 8 the
2026-08-02 anchor named. All 8 rings are `TriangulationFailed` and all 8
SELF-CROSS. None is rejected for degeneracy, and none is simple-but-rejected:
the "degenerate/" half of the message never fires anywhere in the corpus.

| producer | cases |
|---|---|
| **planar** (`sampled_loop_points`) | F0045, F0067, R0011, R0074, R0085 |
| **developable** (`tessellate_developable_patch`) | R0004, **R0028**, R0049 |

**Within the developable trio, this spec's mechanism is R0028's alone.** All
three rings have the expected constant-v rim rows, but measured against their
own rows (exact f64 values, no rounding):

| case | max vertex overshoot beyond the top rim row | crossings involve a rim-row segment? |
|---|---|---|
| R0004 | 3.3307e-16 (ulp noise — zero) | no: seg244 × seg246, both off-row |
| R0049 | 2.7756e-17 (ulp noise — zero) | no: seg44/45 × seg46/55, all off-row |
| **R0028** | **3.6038e-04** | **yes: both crossings pair a chain segment against the rim row** |

Twelve orders of magnitude separate R0028 from the other two. R0004 and R0049
self-cross between side/interior chains, which is a different mechanism and is
NOT anchored — per the R0038 rule they get their own census, not this one's
conclusion.

## 7. Consequence for §4.5.2 scoping

R0004 and R0049 ALSO carry self-crossing PLANAR producer loops (`cross=7` and
`cross=1` in the 2026-08-03 census) — yet their REPORTED wall is a developable
ring. They hold two defects, and the developable one fires first, so **planar
loop-coherence alone cannot convert them.** The realistic §4.5.2 candidate set
is F0067, R0011, R0074, R0085 (+F0045, which `s453_line_run_reversal` already
attributes elsewhere); R0051 and R0100 have crossing planar loops but fail at
other walls entirely; R0004/R0049 are blocked behind a developable defect; and
R0028 was never a member.

## 8. Still open

**Which stage drops the trim** — Stage 2 (curve not cut at the rim), Stage 5
(patch segmentation putting rim-crossing vertices in the lateral patch), or the
operand's own pre-boolean B-Rep — is not established. Stage 4 is excluded (§4);
the rest is open.

## 9. Recommended, not done

`triangulate_ring`'s reason string should name the core and carry the typed
`CdtError`. Two anchors have now spent effort establishing which producer a
"ring rejected by CDT" came from, and both times the answer was one env-gated
probe away from being in the error itself. The sweep makes the case concrete:
the message's "degenerate/" half fires on ZERO of the corpus's 8 rejections
(all are `TriangulationFailed`, all self-crossing), so it is not merely
imprecise — half of it is dead text that misdirects every reader.
