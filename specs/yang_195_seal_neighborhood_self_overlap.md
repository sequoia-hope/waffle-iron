# #195 — F0082 Extrude-12 residual: seal-neighborhood operand self-overlap

Status: CHARACTERIZATION (probe-first, plan-of-record discipline)
Predecessors: #194 (`specs/yang_194_subtauwork_edge_collapse.md`, retired the
sub-TAU_WORK twin layer), #188 (`specs/yang_188_f0082_j3_envelope_selection.md`,
§10.10 named this residual "secondary near-dups").

## 1. Problem statement

F0082's Extrude-12 union STOPs loud at `s4-shell-euler χ=3` off SIX
REAL-length double-cover edges in the seal neighborhood (the wall-plane
column at x≈0.30945, z≈2.0942). #194 proved these are NOT sub-resolution
twins: every edge is balanced (fwd=rev=2) but carries FOUR distinct
triangles — two sheets crossing through each other.

## 2. Measurements (2026-07-22, this session)

### 2a. Output-side attribution (`NONMANIFOLD_SITE_PROBE` s4-dc-attr arm)

All 24 double-cover-edge triangles attribute to operand **A** (the chained
seal-carrying body = Extrude-11's union output), across FOUR faces forming
THREE crossing seams:

| Seam | Faces | Double-cover edges |
|---|---|---|
| cap-disc × wall | A#362 (plane n≈(0.0506,−0.0178,0.9986)) × A#368 (wall plane n≈(0.9987,0.0009,−0.0506)) | (930,931), (930,934), (934,936) |
| wall × seal-plane | A#368 × A#370 (plane n≈(−0.0682,0.0516,−0.9963), d=2.1030) | (931,971) |
| seal-plane′ × tube | A#371 (SAME plane params as 370 — a distinct coplanar face) × A#373 (tube cylinder r=0.2123, axis through v935) | (932,994), (971,994) |

Each face covers BOTH directions of each shared edge — a 4-page book seam,
i.e. two kept sheets crossing, not a shared boundary. Face 362's fan
reaches the tube-axis vert v935; its far verts (e.g. v932 =
(0.31075, 0.09002, 2.09411)) measure **+1.25e-3 beyond the wall plane**
— the #188 masked-triple beyond-wall band (+1.29e-3) exactly.

Note: A#370 and A#371 are two distinct faces with IDENTICAL plane
parameters (same normal AND d, same orientation) — an intra-operand
same-plane face pair that `scan_near_coplanar`'s intra arm does not STOP.

### 2b. Input-side exact scan (`YANG_INPUT_SELFX_PROBE`, new)

`cherchi_rs::detect_improper_contacts` + double-cover scan on every
operand mesh handed to the arrangement, whole F0082 chain:

- **Every boolean in the chain is clean (improper=0) EXCEPT Extrude-12's
  operand A** (2012 tris): **5 improper pairs**, exactly the same four
  faces (1771 on 362 × 1891/1892 on 368; 1892 on 368 × 1897 on 370 /
  1994 on 373; 1917 on 371 × 1994 on 373).
- The self-overlap is therefore INHERITED by Extrude 12 from the
  producing op: **Extrude-11's union output B-Rep re-tessellates into a
  self-intersecting operand mesh.** The flagship union (#188) succeeds
  but emits a body whose cap-disc sheet penetrates its wall sheet by
  ~1.25e-3 in the seal neighborhood — a silent-wrong at op 11 caught
  loudly one op later.

### 2c. Producer attribution CONFIRMED — the submerged corner is B-Rep v925

The involved-face loop dump (same probe, loop arm) on Extrude-12's
operand A (= Extrude-11's output B-Rep):

- **The beyond-wall point IS a B-Rep boundary vertex: v925 =
  (0.31075, 0.09002, 2.09411)** — #188's antipodal ellipse↔rim triple
  point, to every digit. It appears in the loops of FOUR faces:
  **362** (cap disc: v925,v926,v927,v928,v929,v930,v931), **370** (seal
  plane: v925,v931,v930,v929,v942,…), **371** (coplanar twin:
  v951,v950,v949,v948,v925,v959,v960) and **373** (tube lateral, both
  loops). Wall face **368**'s loop does NOT contain it (it carries the
  on-wall seal cluster v926,v948,v949,v950,v951 instead).
- Faces 362 and 370 share the straight collinear chain
  v929–v930–v931–v925 (opposite traversal) — the intersection line of
  their two planes; **B-Rep v931 = the tube-axis point** is a boundary
  vertex of both.
- v925 measures **+1.25e-3 beyond wall face 368's plane** — i.e. a
  boundary vertex of the union output lies strictly INSIDE the union's
  material. The emitted envelope passes through the interior: the
  near-v925 regions of faces 362/370/371/373 overlap face 368's region.
  **Extrude-11's union emits a self-intersecting B-Rep** (the #188
  "wall-masked triple / submerged rim run" made boundary).

### 2d. Why op-11's own gates passed (hypotheses, to verify in inc-1)

- **Arrangement blindness (likely root)**: at op 11 the tool's cap-disc
  penetrates the chain body's wall by only ~1.25e-3 — plausibly
  SUB-SAGITTA in the coarse boolean mesh, so the arrangement never
  samples the cap×wall crossing; ray-cast labeling then classifies whole
  straddling triangles by majority and keeps the entire overhang. This
  is the C0116 / Yang §4.2.1 Case-III graze class, but at a
  plane-near-tangent seal corner rather than cyl×cyl lateral (the #172
  guard's scope).
- **#173 render-gate evasion**: grazing band = max_abs·TAU_WELD_MAX ≈
  2.2e-4 ≪ 1.25e-3 penetration, so the depth itself is over-band; the
  suspected miss is the PR-KV11 vertex-adjacency skip — 362/368 are
  ADJACENT (shared B-Rep edge v926–v927) and planar render CDT makes
  large corner triangles sharing those verts. Verify before relying on
  the gate as the regression fence (#177 family).

## 3. Fix directions (producer-side; pick after inc-1 verification)

The producing op (Extrude-11's union) must not emit the submerged
v925-corner overhang as boundary. Candidate vehicles, structural-first:

1. **Corner-junction trim (structural)**: mint the missing wall-crossing
   junctions (362/370/371/373 × wall-plane intersection at the seal
   corner) and terminate those faces AT the wall — the J3 osculation
   corner assembly #188 §10.10 deferred. The needed junction is the
   "old phantom to every digit" triple point (#188 inc-4d note).
2. **Graze-guard extension (#172 pattern)**: detect the sub-sagitta
   seal-corner penetration cross-operand at op 11 and rebuild at derived
   rim N so the arrangement samples the crossing and labeling trims the
   overhang naturally (paper-compliant §4.2.1 posture; scope lines
   derived as in `specs/yang_172_case_iii_graze_guard.md`).
3. **P10 net only (never in place of 1/2)**: producer-side loud STOP
   when an emitted boundary vertex measures beyond an adjacent face's
   surface by more than the derived band — converts the op-11
   silent-wrong into a loud producer STOP (fails F0082 one op earlier,
   honestly).

Consumer-side normalization (re-arranging inherited self-overlap at
Stage 1 of op 12) is REJECTED: it would silently launder invalid
producer output (P9).

## 4. Ledger

- 2026-07-22: task opened (#194 close-out). Output attribution + input
  selfx probes landed (`s4-dc-attr` arm in `stage4_correct.rs` (4b) gate;
  `YANG_INPUT_SELFX_PROBE` incl. double-cover + involved-face loop dump
  in `boolean.rs`). Measurements §2a–§2c: the class is a PRODUCER
  defect — Extrude-11's union output B-Rep is self-intersecting at the
  wall-masked seal corner v925 (+1.25e-3 beyond wall face 368, kept as
  a boundary vertex of faces 362/370/371/373). Next increment (inc-1):
  verify §2d hypotheses at op 11 (sub-sagitta measurement + render-gate
  adjacency-skip check), then pick the §3 vehicle.
