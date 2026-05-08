# PR-Y21-NMM-RENDER sub-phase 0a — canary-runner-8 anchor canary

**Author:** canary-runner-8
**Date:** 2026-05-06
**Scope:** Empirical verification that the 43 unpaired EDGES on R0092 b#7
(post-PR-Y20-MODE-A) are NMM-origin, justifying the F1 fix
(NMM-aware render mesh + watertight oracle exemption). Probes added at
`collect_loop_boundary` (every NMM HE that walks into render-mesh
emission) + `check_watertight_mesh` (every unpaired canonical key).
Probes REVERTED; `git status` clean.

**Verdict (§3): RED. ABORT condition triggered.**
On R0092 b#7: **0 of 43** unpaired triangle EDGES are NMM-canonical edges
(exact quantization match), **0 of 43** have BOTH endpoints in the NMM
endpoint set, and only **18 of 43** even share ONE endpoint with the NMM
set (consistent with these 18 being interior triangulation fan edges
that happen to spoke from an NMM vertex into a non-NMM hub). On F0044
and F0045 cohort siblings + F0030 control: **ZERO** `[render-nmm]`
probe-A entries fire despite 12 + 38 + 12 unpaired EDGES respectively —
either F0044/F0045/F0030 take a tessellation path that does not pass
through `collect_loop_boundary`, or their solids carry no
twin=None HEs at all post-PR-Y20-MODE-A. Either way F1's
NMM-flag-and-exempt machinery cannot exempt unpaired edges that have no
NMM-canonical-edge match to flag against. Per `feedback_anchor_before_fix.md`:
**ABORT** at canary stage 0a. Spec §3 = RED.

---

## §1 R0092 b#7 unpaired-EDGE breakdown

Command:
```
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0044 --ignored --nocapture --test-threads=1 \
  2>&1 | tee /tmp/canary8_f0044_v2.log
```

R0092 b#7 fail line:
```
R0092 Failed: watertight_mesh: 43 unpaired edges out of 281 total;
  consistent_normals: 81 of 173 triangles have reversed normals;
  no_degenerate_triangles: 4 of 173 triangles are degenerate;
  outward_normals: only 84 of 165 triangles (50.9%) have outward
  normals (need 95%); mesh_euler_characteristic: V(155) - E(281) +
  F(173) = 47 (expected 2)
```

Probe-A (`collect_loop_boundary` site, twin=None HEs only): **88 entries
total, 44 unique canonical edges** (each NMM HE printed once when
walked; the 88→44 doubling is each NMM canonical edge being walked twice
because both directions arrive in `collect_loop_boundary` from
adjacent face loops — actually only single-walked for NMM since by
definition twin=None means there IS no reverse HE; the 88 / 44 ratio is
because the spotlight runs the case twice through the LoadProject path,
once per oracle pass — confirmed by `[wt-meta] total_edges=281 unpaired=43`
firing once per b#7 invocation but probe-A runs at every tessellation
re-emission).

Probe-A2 (`render-nmm-seg`, discretization-segment granularity): **88
entries**, IDENTICAL set as `render-nmm-brep`. This means the NMM HEs
on R0092 b#7 are **all linear edges with `disc.edge_verts.len() == 2`**
— no interior discretization points. Granularity-mismatch hypothesis
(NMM HE disc-into-N-segments) is FALSIFIED: probe-A captures every
triangle-edge a render-mesh-emitting NMM HE produces.

Probe-B (`check_watertight_mesh`): **43 unpaired canonical keys**,
`grid_size=1.305296e-7, max_abs=1.305296e-2`.

**Cross-reference table (R0092 b#7):**

| metric | count |
|---|---|
| Total unpaired EDGES | 43 |
| Total NMM canonical edges (unique) | 44 |
| Unpaired keys EXACTLY matching an NMM canonical edge (f64 quantization) | **0 / 43** |
| Unpaired keys EXACTLY matching an NMM canonical edge (f32→f64 quantization) | **0 / 43** |
| Unpaired keys matching with ±1 quantization tolerance per axis | **0 / 43** |
| Unpaired keys with BOTH endpoints in the NMM endpoint set | **0 / 43** |
| Unpaired keys with AT LEAST ONE endpoint in the NMM endpoint set | 18 / 43 |
| Unpaired keys with NEITHER endpoint in the NMM endpoint set | 25 / 43 |

**Geometric character of unpaired edges (load-bearing observation):**
- length distribution: shortest 1.6e-5 (16 µm), median 4.05e-3 (4 mm),
  longest 8.1e-3 (8.1 mm)
- length distribution of NMM HEs: shortest 1.2e-4, median 8.8e-4 (0.88
  mm), longest 4.05e-3 (4 mm). NMM HEs are SHORTER than the median
  unpaired edge.
- ONE vertex `(69883, 354, -31047)` ≈ `(0.00912, 4.6e-5, -0.00405)`
  appears as an endpoint of MANY unpaired edges — classic earcut /
  fan-triangulation hub signature. This vertex is a **non-NMM
  endpoint**.
- The 18 unpaired edges with one NMM endpoint pattern: each has the
  NMM endpoint as v0, the hub `(69883, 354, -31047)` as v1 (or
  symmetric). These are not NMM HEs themselves — they are interior
  triangulation diagonals that happen to spoke from an NMM-vertex
  outward to the fan-hub.

**Interpretation:** the 43 unpaired edges are predominantly **interior
triangulation defects** in `tessellate_planar_face_bounded` (or
similar earcut fan path), not NMM-boundary defects. F1's
`nmm_edges: BTreeSet<PosEdge>` set would be populated with the 44 NMM
canonical edges, but the watertight oracle's lookup against that set
would exempt **ZERO** of the 43 unpaired edges because none of those
keys appear in the NMM canonical set.

---

## §2 Cohort comparison

| Case | unpaired EDGES | render-nmm probe entries | NMM canonical edges | exact match | ≥1-NMM-endpoint match |
|---|---|---|---|---|---|
| F0044 (b#1+b#2+b#3 batch) | 12 | **0** | 0 | 0/12 | 0/12 |
| F0045 | 38 | **0** | 0 | 0/38 | 0/38 |
| R0092 b#7 | 43 | 88 (44 unique) | 44 | **0/43** | 18/43 |
| F0030 (control; PR-Y19-MODE-B-resolved) | 12 | **0** | 0 | 0/12 | 0/12 |

**Aggregate:** 105 unpaired EDGES across 4 cases; **0** exact matches.

**F0044/F0045/F0030 ZERO probe-A entries observation (CRITICAL):**
Per PR-Y20 canary §3 cohort table, F0044 booleans #5/#6/#7 had 30/36/36
Mode A NMM canary cases at the `topology_extract.rs` Step 7 layer.
Yet probe-A here, planted in `collect_loop_boundary`, fires zero times
on F0044's 3 booleans. Two possibilities (this canary CANNOT
discriminate; flagging both as banked questions for spec-writer-u and
adversary-21):

(a) **Path-divergence**: F0044's tessellation path bypasses
`collect_loop_boundary` entirely. Per `tessellation/mod.rs` the
top-level `tessellate_solid_ext` dispatcher routes to
`tessellate_solid_bounded` only when no analytic primitives + no arcs
are present. F0044 has arc-edge boolean results
(`needs_fan_welding=true` path at line 237). The fan-welding /
analytic / cone / sphere / torus / revolve paths use their own
boundary collection, not `collect_loop_boundary`. **If true, F1's
populate-during-tessellation step must be replicated across multiple
tessellation paths**, not just the one named in spec §3. Spec scope
inflates significantly.

(b) **Topology-loss**: PR-Y20-MODE-A's `Option<HalfEdgeIdx>` does set
`twin=None` at the topology layer, but downstream B-Rep assembly /
retessellation re-pairs the HEs (perhaps via the HEDS rebuild step) and
produces a final solid where every HE has `twin=Some(_)`. The Mode A
NMM cases the topology-extract probe saw never make it through to the
final solid passed to tessellation. **If true, the validator's
`twin=None` is not load-bearing for tessellation at all** and F1 has
nothing to plumb.

(Banked diagnostic for sub-phase 0b spec-writer-u or adversary-21:
add a trace-level probe at the validator-pass site
(`yang_integration.rs::validate_yang_result_topology`) and
post-validator point counting `.twin.is_none()` HEs in the final
solid. If the count is 0 across F0044, hypothesis (b) is true; if non-0
but probe-A still 0, hypothesis (a) is true.)

**F0030 control:** 12 unpaired EDGES, 0 probe-A entries. Per PR-Y20
canary §3 + plan note "F0030 has 0 Mode A (PR-Y19-MODE-B fully resolved
its twin defect; remaining failure is downstream)", consistent: F0030's
final solid has no NMM HEs and its 12 unpaired EDGES are unrelated to
the NMM mechanism. F1 cannot help F0030.

---

## §3 Verdict

**RED.** ABORT condition triggered per spec §3 ABORT criterion (<20 of
43 are NMM-origin).

Concrete numbers:
- R0092 b#7 (the load-bearing case): **0 / 43** exact NMM-canonical
  match
- F0044 batch: **0 / 12** (F1 not applicable; cases use a
  tessellation path probe-A doesn't cover, or have no NMM HEs in the
  final solid)
- F0045: **0 / 38** (same F1 not-applicable signature)
- F0030 control: **0 / 12** (no NMM, downstream defect class)

The dominant defect class is **interior triangulation fan-hub failures**
(the `(0.00912, 4.6e-5, -0.00405)` hub vertex on R0092 spoking many
unpaired diagonals), NOT non-manifold boundary edges. F1's
`nmm_edges: BTreeSet<PosEdge>` set will be populated correctly with 44
NMM canonical edges on R0092, but the watertight oracle's lookup
against that set will exempt **zero** of the 43 unpaired triangle
edges, because none of the 43 keys are NMM-canonical edges. F1 ships a
no-op for these cases.

---

## §4 Quantization-coupling probe (CRITICAL for any future F1)

Although F1 is RED-aborted, the quantization-coupling concern (spec §3
risk #2) is independently answered for the record:

**Quantization-coupling: BYTE-IDENTICAL between f64 and f32→f64 paths.**

For 6 sampled coordinates from R0092 NMM HEs (3 HEs × 2 endpoints):

| HE | endpoint | f64 source | f32→f64 round-trip | q_f64 | q_f32 | delta | match |
|---|---|---|---|---|---|---|---|
| 32 | v0 | 5.16527800e-3 | 5.16527798e-3 | (39572, 248, -37831) | (39572, 248, -37831) | (0,0,0) | ✓ |
| 32 | v1 | 6.00993700e-3 | 6.00993680e-3 | (46043, -17594, -13385) | (46043, -17594, -13385) | (0,0,0) | ✓ |
| 38 | v0 | 8.48459550e-3 | 8.48459546e-3 | (65001, -7488, -22066) | (65001, -7488, -22066) | (0,0,0) | ✓ |
| 38 | v1 | 5.16527800e-3 | 5.16527798e-3 | (39572, 248, -37831) | (39572, 248, -37831) | (0,0,0) | ✓ |
| 43 | v0 | 6.00993700e-3 | 6.00993680e-3 | (46043, -17594, -13385) | (46043, -17594, -13385) | (0,0,0) | ✓ |
| 43 | v1 | 6.06551200e-3 | 6.06551208e-3 | (46468, -16763, -14358) | (46468, -16763, -14358) | (0,0,0) | ✓ |

`grid_size = 1.305296e-7` for R0092 b#7 (`max_abs * TAU_TESS_GRID_FACTOR
= 1.305296e-2 * 1e-5`). f32 representation noise at this scale is
~1.2e-9 — **130× smaller than the grid spacing** — so quantization is
robust to f32 round-trip. **No f32/f64 coupling fix needed**: any
future F1-style implementation can quantize either f64 or f32 vertices
through `(v as f64 * inv_grid).round() as i64` and obtain identical
keys. The spec §3 mitigation `quantize_for_oracle()` extraction is
still architecturally wise (single source of truth for the math), but
NOT load-bearing — there is no silent-mismatch landmine here. The
load-bearing concern is the EDGE-IDENTITY mismatch documented in §1,
not the quantization mechanic.

---

## §5 Self-canaried recommendation

Per `feedback_adversary_recommendations_need_canary.md`: every
recommendation below is grounded in this canary's empirical
observation, not inference.

**Primary recommendation (load-bearing):** ABORT PR-Y21-NMM-RENDER as
designed. Do NOT proceed to sub-phase 0b spec-writer-u with the F1
shape. Empirical basis: §1 + §2 cross-reference shows F1's NMM-flag
exemption catches **0 / 105** unpaired EDGES across all 4 probed cases.
Per `feedback_anchor_before_fix.md` and the spec's own ABORT condition
(§3), the canary's job is to halt PRs whose hypothesis isn't supported
by data.

**Banked discoveries for next-PR planning** (each grounded in §1/§2
data, NOT in inferred next-anchor speculation):

1. **R0092's dominant defect is fan-hub interior triangulation, not
   NMM.** Anchor: vertex `(0.00912, 4.6e-5, -0.00405)` appears as an
   endpoint of MANY unpaired edges in R0092's wt-unpaired output. This
   is the signature of `tessellate_planar_face_bounded`'s earcut /
   fan-triangulation producing diagonals that don't match across
   adjacent faces. The next PR should probe **which face** R0092
   tessellates with this hub-vertex pattern and whether that face's
   planar triangulation has the bug.

2. **F0044/F0045/F0030 take a tessellation path probe-A doesn't
   cover.** Spec for any future PR (NMM or otherwise) targeting these
   cases must first establish, by canary, **which tessellation
   function actually emits the unpaired edges** — `tessellate_polygon_face`,
   `tessellate_revolve_lateral`, `tessellate_cylindrical_face_bounded`,
   `tessellate_arc_bounded_cap`, or `tessellate_solid_ext`'s analytic
   sphere/cone/torus paths. Probe-A planted in `collect_loop_boundary`
   doesn't fire on F0044 → either the path doesn't go there, or there
   are no NMM HEs in the final solid post-validator.

3. **The hypothesis "44 NMM HEs ≈ 22 NMM canonical pairs ≈ 43 unpaired
   edges" from the plan + adversary-20 §4 is FALSIFIED.** The 44 NMM
   HEs and 43 unpaired edges are **disjoint sets** by canonical-edge
   identity. The off-by-one (44→43) suggested by the plan-narrative
   was a coincidence of cardinality, not a causal link. Future
   spec-writers should read `feedback_external_coherence.md` and
   resist the temptation to treat a card-count match as evidence of
   the same mechanism.

4. **PR-Y20-MODE-A type-system change is structurally CORRECT but may
   not BE the wedge that unblocks any user-visible spotlight test.**
   PR-Y20 enabled validator-pass on R0092 (panic eliminated; case
   reaches watertight oracle). But the watertight oracle now reports
   defects from a different mechanism layer — fan-triangulation, not
   NMM. PR-Y20's "structural unlock" is real but the next user-visible
   wedge is downstream of NMM, not adjacent to it. The plan's premise
   (PR-Y21 is the cleanest next step after PR-Y20) is empirically
   wrong; the cleanest next step is fan-hub triangulation
   investigation, banked for PR-Y22 or rotation thereof.

5. **Quantization-coupling concern (spec §3 risk #2) is NOT
   load-bearing.** §4 byte-identity match across all 6 sampled f64↔f32
   paths confirms the grid is coarse enough that any rounding mode
   converges. Future PRs should still extract `quantize_for_oracle()`
   for code-quality reasons (single source of truth for the math), but
   the silent-mismatch landmine the plan worried about does not
   exist at TAU_TESS_GRID_FACTOR=1e-5.

**NO recommendation made about implementer-y / adversary-21 actions:**
since spec-writer-u doesn't proceed, those rotations don't happen this
PR.

---

## Verification

- `git status --short` clean (probes reverted from
  `crates/kernel/src/tessellation/mod.rs` +
  `crates/test-harness/src/oracle.rs`; results.json `git checkout`'d
  back). Only file in `git diff` is this memo.
- §1 has R0092 b#7 empirical probe data: 43 unpaired ↔ 44 NMM canonical
  ↔ 0 exact matches ↔ 0 BOTH-endpoint matches ↔ 18 ONE-endpoint matches.
- §2 cohort table covers F0044, F0045, R0092 (the 3 cases in
  spotlight_f0044) + F0030 control. Aggregate: 0/105 exact.
- §3 picks ONE verdict: **RED**. ABORT condition triggered.
- §4 explicitly answers quantization-coupling YES (byte-identical).
- §5 self-canaried per `feedback_adversary_recommendations_need_canary.md`:
  every recommendation cites §1/§2 directly. Banked discoveries are
  signposts for next PR's canary, NOT next-anchor directives.

**Sub-phase 0a complete. Verdict RED. Routing to team-lead for ABORT
decision and re-plan. Do NOT mark sub-phase 0b in_progress.**
