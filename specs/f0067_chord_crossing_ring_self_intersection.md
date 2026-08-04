# F0067 ANCHOR — a Stage-0 chord-depth crossing mint, refined by Stage 4 WITHOUT its loop neighbours, makes the output ring self-intersect

**Status:** ANCHORED 2026-08-02, fix NOT built.
**Crate:** `yang-rs` (Stage 0 rim-chord crossing mints; Stage 4 §4.4.1 refinement)
**Wall:** `TessellationFailed { face: FaceId(3994), reason: "ring rejected by CDT (degenerate/self-intersecting)" }`
**Predecessor:** `yang_s4_boundary_curve_relocation.md` §19–§21 — inc-6 cleared
F0067's `s6-planar-loop-nonplanar` wall and advanced the case to THIS one.

## 1. The wall, and the ring behind it

F0067's op-10 union fails in kernel-v2's exact planar CDT, not in yang. The
rejected ring (`KV2_RING_REJECT_PROBE`) is 32 points, no holes, all at
`z = 1.7518978673859233` — the FLUSH plane where the op-10 cylinder's base
sits on the op-9 gear's top cap. It is yang output face 328 (input A, the gear
top cap) → kernel-v2 `FaceId(3994)`.

Measured: the ring has **4 self-intersections**, all between the tooth-notch
outline and a run that departs from it. Classified against the op-10 rim circle
`R = 0.20884629067185412`: ring indices 0–10 and 23–31 are OUTSIDE it, 11–14 and
19–22 INSIDE it, and 15–18 lie exactly ON it. The loop therefore visits both
sides of the trim curve and crosses itself.

## 2. What moved — measured, per vertex

New probe `YANG_S6_LOOP_PROV=x,y,z,r` (below) dumps the emitted loop with each
vertex's Stage-4 provenance. Face 328's loop is **77 vertices**, of which
**exactly six moved**, all by a pure radial projection onto the exact cylinder:

| k | v | pre `r−R` | displacement | `inc` | `curve` |
|---|---|---|---|---|---|
| 31 | 841 | −3.7048e-3 | **3.7048e-3** | A:Plane,A:Plane,B:Cylinder,B:Plane | Circle,LineSegment |
| 32 | 834 | −3.6064e-3 | **3.6064e-3** | A:Plane,B:Cylinder | Circle |
| 33 | 827 | −3.1259e-3 | **3.1259e-3** | A:Plane,B:Cylinder | Circle |
| 34 | 810 | −1.9856e-3 | **1.9856e-3** | A:Plane,B:Cylinder | Circle |
| 45–47 | 814, 835, 842 | mirror | mirror | mirror | mirror |

Every displacement EQUALS that vertex's pre-move radial deficit: Stage 4 put
each one exactly on the circle, which is exactly what §4.4.1 asks of a vertex
carrying an A×B `Circle` key.

Their loop NEIGHBOURS did not move, and correctly so: v833 / v836 (the notch
corners, `r−R = −3.55e-3`) report `inc=[A:Plane,A:Plane] curve=[-] disp=0` —
A's own profile vertices, never relocation candidates. The un-moved and the
moved sit on the SAME side of the ring, 3.7e-3 apart after refinement.

**The scale that makes this fatal:** the notch-bottom segment v833→v834 is
**6.4e-4** long. The refinement displaces its endpoint by **3.7e-3 — 5.8× the
length of the loop segment it belongs to.** A displacement that large cannot
stay on its own side of a feature that small, so the run sweeps across the
outline and the ring crosses itself 4×.

## 3. Where the off-circle vertices come from — MINTED by Stage 0, measured pre/post

`YANG_STAGE0_DUMP_DIR` writes both the mesh handed to the backend and the
pre-Stage-0 Stage-1 mesh, which settles minted-vs-inherited per case rather
than by inference (the R0038 rule):

| B's mesh (op-10, the cylinder) | verts | wall bottom row | max off-cylinder |
|---|---|---|---|
| `009_union_b_pre.obj` (Stage-1 out) | 28 | 13 | **2.8e-17** |
| `009_union_b.obj` (into the backend) | 1509 | 535 | **6.0602e-3**, on 78 vertices |

The pre-Stage-0 cylinder is a **13-gon** (13 rim samples), whose theoretical max
sagitta is **6.0687e-3** — the measured 6.0602e-3 maximum is that sagitta. And
the four moved vertices lie **ON the 13-gon's chords** to 5e-13:

```
v841 |r−R|=3.7048e-3  dist_to_13gon=4.9e-13
v834 |r−R|=3.6064e-3  dist_to_13gon=8.3e-14
v827 |r−R|=3.1259e-3  dist_to_13gon=2.2e-13
v810 |r−R|=1.9856e-3  dist_to_13gon=4.1e-13
v833 (A's own notch corner) |r−R|=3.5502e-3  dist_to_13gon=1.5e-4   ← NOT on it
```

So: Stage 0's flush overlap segmentation (pair `face_a=328 × face_b=0`,
`opposite=true`, `d=-1.7518978673859236`) mints its crossing vertices on the
CHORDS of B's 13-gon rim, up to the full Stage-1 sagitta off the exact cylinder.
Stage 1 minted nothing off-cylinder; Stage 4 refined, it did not mint.

## 4. Why the existing design permits this — and the premise that breaks

`m8_exact_opposite_rim_projection.md` records the design premise verbatim:

> cap-side chord-deep crossings are legitimate ring members (the stage1 override
> band explicitly admits up to sagitta depth) **because they are
> intersection-curve points that Stage 4 refines onto exact geometry**

That premise is sound per VERTEX and false per LOOP. Stage-4 refinement is a
per-vertex operation selected by curve key; ring simplicity is a property of the
whole cycle. When a chord-deep crossing's loop neighbours are the OTHER
operand's own profile vertices — which are not relocation candidates and must
not move — refining the crossing moves it up to a full sagitta RELATIVE to
them. Nothing in the chain checks that the resulting cycle is still simple:

- Stage 4 relocates each candidate correctly and in isolation;
- Stage 6's planarity gate passes (every vertex is exactly ON the inherited
  plane — the loop is planar and self-intersecting at the same time);
- the first thing that notices is kernel-v2's exact CDT, one crate away from
  the producer.

**There is no producer-side gate for a self-intersecting emitted loop.** That is
the structural hole this anchor names.

## 5. Corpus reach (measured, but the sibling MECHANISM is NOT censused)

`ring rejected by CDT` is the wall of **8 of the 47 corpus ERRORs**: F0045,
F0067, R0004, R0011, R0028, R0049, R0074, R0085 (F0067 joined the other seven
when inc-6 cleared its earlier wall). That is the largest single named class in
the ERROR tail — 17%.

**Do NOT inherit this anchor to the other seven.** `s453_line_run_reversal.rs`
already attributes F0045 (with R0072) to a DIFFERENT mechanism — a §4.5.3
straight-run seam reversal. Per the R0038 rule, each case's mechanism must be
censused on that case. What this section establishes is only that the class is
worth the structural fix, not that it has one cause.

### 5a. CENSUSED 2026-08-03 — the list above was wrong in three places

The `YANG_S6_LOOP_SIMPLICITY` scan (§7) measured all 312 cases directly. The
wall-text membership does not survive contact with the measurement:

- **R0028 is NOT a member.** It reports the identical `ring rejected by CDT
  (degenerate/self-intersecting)` string, yet all 10 of its planar producer
  loops are simple. **Anchored separately 2026-08-04 —
  `specs/r0028_developable_ring_cap_overshoot.md`:** its ring comes from the
  DEVELOPABLE core (an unrolled cylinder lateral), not the planar one, and
  self-intersects because three real B-Rep boundary vertices sit 3.60e-4
  BEYOND the face's own cap plane. Stage 4 relocated nothing in that boolean
  (`n_relocations=0`, both ops), so it is not this mechanism at all. Note both
  cores funnel through `triangulate_ring`, which is why the wall string cannot
  separate them.
- **R0051 and R0100 ARE members**, and neither reports this wall — they fail at
  `SelfIntersectingBooleanOutput` (face_a 8 / face_b 10, 88 penetrations) and
  `patch triangulation folded (inverted tri)`. The class is larger than the
  wall that named it.

Confirmed set (9 of 47): **F0045 F0067 R0004 R0011 R0049 R0051 R0074 R0085
R0100**. F0045 stays in on its own measurement (`cross=1`) — its §4.5.3
attribution and this one may both be live.

### 5b. RING-REJECT SWEEP 2026-08-04 — the producer split, and who can convert

All 312 cases, `KV2_RING_REJECT_PROBE` then `KV2_RING_PROVENANCE` on the hits.
**Exactly 8 cases reject a CDT ring, and all 8 rings SELF-CROSS** (every one
`TriangulationFailed`; the message's "degenerate/" half fires nowhere in the
corpus, and no rejected ring is simple). Producer split by the `idx=0`
provenance line preceding each reject — the face matches the wall face in all 8:

| producer | cases |
|---|---|
| **planar** — this class | F0045, F0067, R0011, R0074, R0085 |
| **developable** — NOT this class | R0004, R0028, R0049 |

**R0004 and R0049 carry BOTH defects.** They have self-crossing planar producer
loops (`cross=7`, `cross=1` above) AND their reported wall is a developable
ring, which fires first. **Planar loop-coherence alone therefore cannot convert
them** — a prediction to check rather than a surprise to discover later.

Realistic §4.5.2 conversion candidates: **F0067, R0011, R0074, R0085** (plus
F0045, whose §4.5.3 seam attribution may dominate). R0051/R0100 have crossing
planar loops but fail at unrelated walls (`SelfIntersectingBooleanOutput`,
`patch triangulation folded`), so they are separate questions.
R0028: `specs/r0028_developable_ring_cap_overshoot.md` (its own mechanism, a
singleton even within the developable trio — R0004 and R0049 show 3.3e-16 /
2.8e-17 rim overshoot, i.e. zero, and neither of their crossings touches a rim
row).

**The class is defined by a proper CROSSING, not by non-simplicity.**
`cross > 0` holds for 0 of 261 SUPPORTED_CORRECT, 0 of 3 UNSUPPORTED, 0 of 1
EXPECTED_ERROR and 9 of 47 ERROR — a perfect split. `touch` / `spike` do not
split anything: F0055 (SUPPORTED_CORRECT, 33-point loop, `touch=4`) and F0064
both survive them. The §6 P10 net, if ever built, must therefore gate on
`cross > 0`; a gate on non-simplicity regresses F0055.

F0067 itself is larger than §1–§4 record: **17 faces / 150 crossings in its
final op**, with `max_s4_disp / min_seg` up to 5.2e4 — the 5.8× of §4 is the
mild end of its own distribution. Faces 357 and 359 carry `touch=5 spike=2`
with ZERO crossings, a pinch/backtrack sub-mechanism the ring-reject probe
could not separate from a crossing.

**Coverage:** 186,234 planar loops scanned, 0 unmeasurable; **6,870 curved
faces NOT scanned** (no exact 2D projection — R0044 is 448 curved vs 2 planar,
i.e. effectively unmeasured by this instrument) and **47 cases where Stage-6
emission never ran**. Canonical 261C/0W/47E/0T reproduced verbatim with the
probe ON.

## 6. What a fix must do (NOT built)

Two candidate directions, both structural, neither a band:

1. **Mint the crossing on the exact circle, not on the chord** (Stage 0). The
   exact-rim machinery already exists for neighbouring cases
   (`rim_chord_ctxs`' exact `Curve::Circle`, the annular arm in
   `m8_holed_disc_coplanar_overlay.md` increment 6). Then the vertex needs no
   Stage-4 refinement at all and the ring never moves. The open question this
   raises — and the measurement to take first — is what the OTHER side of that
   crossing does: A's notch corners are legitimately off the circle, so moving
   the shared vertex onto the circle at mint time relocates a point of A's
   profile, which is the same loop-coherence problem one stage earlier.
2. **Make the refinement loop-coherent** (Stage 4): a relocation that moves one
   vertex of a cycle by more than the local edge length must either carry its
   neighbours (Yang §4.5.2 local refinement — re-cut the outline against the
   refined curve) or refuse. This is the mesh-updating capability epic #169
   already names; F0067 is a new customer for it.

Direction 2 is the paper-compliant one: §4.5.2 exists precisely because moving a
vertex onto exact geometry invalidates the local triangulation, and the answer
is to update the mesh, not to move the vertex alone.

**A P10 safety net is available and is NOT a substitute:** a producer-side
simplicity check on each emitted planar loop would convert this silent-wrong
(an invalid ring that only a downstream CDT catches) into a loud STOP naming
Stage 6. That is the sanctioned use of a gate — it names the defect at its
producer — but it converts 8 ERRORs into 8 differently-worded ERRORs and fixes
nothing, so it must not be shipped as a fix.

## 7. Instruments banked (read-only, env-gated, production byte-identical)

- **`YANG_S6_LOOP_SIMPLICITY`** (2026-08-03, `stage5_loop_simplicity.rs`) — the
  census instrument for this class. Scans every emitted PLANAR loop for
  self-contact with EXACT predicates (`dashu` orientation + on-segment over a
  dominant-axis projection, which copies the surviving f64 coordinates
  verbatim), reporting four separated columns: `cross` (proper transversal),
  `touch` (pinch / collinear overlap), `spike` (adjacent-pair backtrack),
  `degen` (zero-length segment), plus `min_seg` / `max_s4_disp` /
  `disp_over_min_seg` — the ratio §4 identifies as the fatal quantity. Set to
  any value to report only non-simple loops; `=all` for every loop, so "no
  findings" is distinguishable from "emission never ran". Runs BEFORE the
  non-planarity gate deliberately (this class passes every per-vertex gate),
  and emits a SUMMARY line counting curved faces it could not scan. Pair with
  `YANG_S5_FOLD_PROBE=1` for the displacement columns. ~5% overhead; off by
  default. Sweep via subprocess-per-case `single_case` — the `ASSAY_JOBS`
  driver nulls child stderr.
- **`YANG_S6_LOOP_PROV=x,y,z,r`** (new, `stage5_topology.rs`) — dumps every
  emitted face loop passing within `r` of the target, each vertex with its
  Stage-4 provenance (`pre`, `disp`, `inc`, `curve`) via the shared
  `probe_vertex_prov` helper. Deliberately NOT gated on a wall firing: the
  nonplanar probe's columns only appear when the planarity gate rejects, and
  this defect class is perfectly planar. Requires `YANG_S5_FOLD_PROBE=1` to
  populate the provenance columns.
- Existing, used here: `KV2_RING_REJECT_PROBE` (the rejected ring's 2D/3D
  points), `KV2_RING_PROVENANCE` (ring index → half-edge/curve kind),
  `YANG_S6_PATCH_PROBE` + `YANG_S6_CYCLE_DUMP=all` (pre-Stage-4 cycles),
  `YANG_S6_VERT_PROBE` (triangles and cycle membership at a vertex),
  `YANG_S4_RIM_SNAP_TARGET` (per-vertex incidence + implicit residuals),
  `YANG_STAGE0_DUMP_DIR` (the pre/post Stage-0 operand meshes — the
  minted-vs-inherited oracle).
