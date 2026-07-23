# yang #199 — F0072 stacked-coplanar arrangement pair explosion: characterization

**Status:** characterization (2026-07-23, task #199). **The task's premise —
that F0072's arrangement cost is the *cross-solid* coplanar contact
(O(face_tris²) between the two operands) that Yang Stage-0 §4.5.5 would
segment away — is REFUTED by measurement.** The dominant cost (77% of the
pairs on the heaviest op) is **`selfA` — near-coplanar self-crossings of the
accumulated chained *tower* operand at the contact plane** — while the genuine
cross-solid contact is only **7%**. Stage-0 §4.5.5 operates on cross (A×B)
pairs and cannot reduce the tower-self cost, so **Lever A as scoped in #199 is
the wrong lever.** Drilling the `selfA` pairs to their coordinates (§4) shows
the true driver: at each coplanar contact the overlay subdivides the shared rim
**finely** but re-triangulates the cap **coarsely**, so a few giant cap
triangles each contact many fine rim/sliver triangles → **O(cap_tris ×
rim_subdivisions)** benign shared-boundary contacts that the exact arrangement
counts+classifies. It is a contact-region **mesh conformality/quality** problem
— NOT an off-plane cap fold (caps arrive exactly planar) and NOT a cross-solid
segmentation gap.

**Crate boundary:** the measured hotspot is `cherchi-rs` Stage-2
(`detect_intersecting_pairs` + `classify_all`); the tractable reroute target is
`yang-rs` Stage-0/1 contact-cap ⇄ rim conformality, with a `cherchi-rs`
dense-slab short-circuit as the pure-perf alternative — NOT a `yang-rs` Stage-0
cross-pair handler and NOT a coordinate snap.

---

## 1. The case

F0072 = 20 chained extrudes (`gear`/`polygon`/`circle`) stacked in Z, all
unions (no cuts), each starting exactly where the previous ends — a clean
abutting tower (verified from the meta: op k+1's `plane_origin.z` == op k's top
z, every op). The only cross-solid contact per union op is one coplanar cap
(top of the accumulated tower ∩ bottom of the new extrude). F0072's verdict is
an honest **ERROR** at Extrude 11 (`azimuth-merge rims mismatched 229 vs 230`,
unrelated to this perf work). #199 is purely about the ~132s runtime, which
matters for the app on complex models and heavier corpus cases.

## 2. Stage-0 DOES detect and overlay every contact (no walls)

`YANG_COPLANAR_PROBE=1` on the full run: **19 `cross-pair` detections (one per
union op), ZERO walls** — no `face-unsupported`, `overlay-failed`,
`intra-solid`, or `subres` stops. Each op's `scan_near_coplanar` finds the
`(tower_top, extrude_bottom)` planar pair (gap ≤ 2.8e-16, band 1e-7) and routes
it through the §4.5.5 overlay, which resolves the Overlap region to ONE shared
3D point per vertex (bit-identical tris on both meshes) that
`remove_degenerate_and_duplicated_triangles` then dedups **before** the pair
detector. **So the cross-contact machinery Lever A would build already exists
and already fires on this case.**

## 3. Measured pair breakdown (the refutation)

`CHERCHI_PAIR_PROBE` in `mesh_arrangement` (after `classify_all`) reports, per
call: post-dedup tri count, pair count, and each pair bucketed by classification
(Transversal / Coplanar) × label-pattern, plus a Z-histogram of pair centroids.
Label patterns: **selfA** = both tris a single-operand {A} label (the tower);
**selfB** = both {B} (the fresh extrude); **merged** = ≥1 tri carries the
`{A,B}` OR-merged label (the collapsed contact); **cross** = single {A} ×
single {B} (genuine, un-dedup'd A×B).

Heaviest arrangement (matches the perf-spec op-7 figure of 666,731 pairs):

```
tris=14342  pairs=666731   T=462647  C=203279
  selfA (t342849, c173270) = 516119   77%   <-- tower self, transversal-heavy
  selfB (t 28018, c  3009) =  31027    5%
  merged(t 46524, c 26959) =  73483   11%   (the collapsed contact region)
  cross (t 45256, c    41) =  45297    7%   <-- the ONLY thing Lever A touches
  top-z  z≈1.52:328310  z≈1.48:158237  z≈1.56:67974   (one thin slab at the
         op-8 contact plane z=1.5412; ±1 bucket)
```

Every heavy op shows the same shape (selfA or selfB dominant, cross small):
e.g. `pairs=177245 → selfA=100524, cross=3263`; `pairs=185291 → selfA=57408,
selfB=38676, merged=48285, cross=31659`. **The pair explosion is
predominantly SINGLE-OPERAND self-crossing at the contact plane, transversal-
dominated, and it GROWS with tower height** (later ops → taller tower → denser
top-cap region → more pairs).

## 4. Operands arrive CLEAN and EXACTLY PLANAR — the cost is contact-slab
##    mesh NON-CONFORMALITY (giant cap tris × fine rim slivers)

Two more probes rule out the obvious "dirty operand" theories:

- `YANG_OPERAND_SELFX_PROBE` (`detect_improper_contacts` per operand before the
  arrangement): **every operand, all 20 ops, `improper_pairs=0`,
  `unresolved_pairs=0`** — each operand mesh is a clean single-cover surface.
- `YANG_FACE_PLANARITY_PROBE` (max off-plane residual over each planar face's
  loop verts): **worst ~8.9e-16 (machine ε), `planar_faces_off(>1e-12)=0`** on
  every operand, every op. **The caps are EXACTLY planar** — the "3e-8 off-plane
  emission" of task #153 is NOT present on F0072's operands, so an off-plane
  cap FOLD is refuted (coplanar cap tris would classify Coplanar, not
  Transversal, anyway).

`CHERCHI_SELFA_DUMP` (dump the actual selfA-transversal pair geometry in the
z≈1.52 contact slab) shows what the pairs really are:

- **`ta`** = a **giant cap triangle** at the contact plane (all 3 verts at
  z=1.5412), spanning the full cap width (e.g. y −0.28 → +0.28, a 0.56-wide
  triangle with two long rim edges).
- **`tb`** = **wall / thin-sliver triangles** at the rim — some with two top
  vertices only **~4e-4 apart** in x (slivers minted by the overlay inserting
  the two profiles' edge-crossing points as fine rim samples).

**Mechanism:** at each stacked coplanar contact, Stage-0's overlay subdivides
the shared rim **finely** (many small wall/sliver triangles from the
profile-edge crossings) but re-triangulates the coplanar cap **coarsely** (a few
large triangles whose long edges run the length of the finely-subdivided rim).
One big cap triangle then geometrically contacts **many** of the fine
rim/wall triangles along that edge → **O(cap_tris × rim_subdivisions)**
non-Disjoint pairs, all in the thin contact slab, all run through the expensive
exact `classify_all`. This is a contact-region **mesh conformality / quality**
problem (coarse cap vs fine rim), NOT an off-plane fold and NOT a cross-solid
segmentation gap. `detect_improper_contacts` reports 0 because these are
shared-BOUNDARY contacts (benign topologically), but the arrangement's
`detect_intersecting_pairs` counts every one and classifies it. It grows with
profile complexity (gear/polygon rim density) and with tower height.

## 5. Where this actually routes

- **NOT Lever A (Stage-0 §4.5.5 cross handler).** Already fires on every
  contact; only touches the 7% `cross` bucket.
- **NOT "snap the off-plane cap" (the first Lever-1 guess).** REFUTED — caps
  arrive exactly planar (§4).
- **Real lever — contact-cap ⇄ rim conformality (yang-rs Stage-0/1).** Make the
  coplanar cap re-triangulation CONFORMAL with the rim subdivision: insert the
  rim/sliver sample points as vertices along the cap triangles' boundary edges
  (the §4.5.5 `collect_edge_splits` shared-boundary sampling, evidently not
  reaching this cap↔wall interface), so each cap triangle touches ONE wall
  triangle per shared segment → O(subdiv), not O(cap×subdiv). Also: avoid
  minting near-duplicate sliver rim samples (~4e-4 twins) in the first place.
  Correctness-adjacent (touches the mesh handed to the boolean) → spec-first,
  gated, corpus category-identical.
- **Real lever — near-coplanar / dense-slab cluster short-circuit
  (cherchi-rs Stage-2).** When `detect_intersecting_pairs` finds a large cluster
  of benign shared-boundary contacts in one thin coplanar slab, resolve/skip
  them without the full O(M²) exact tri-tri classify. Pure Stage-2 speedup,
  orthogonal to correctness, but a substantial port.
- **Lever B (rayon parallel, shipped #198)** cuts wall-clock only.

**Recommendation:** do NOT ship any coordinate-snap fix (premise refuted). The
tractable structural win is the **contact-cap ⇄ rim conformality** fix in
yang-rs Stage-0/1; the pure-perf alternative is the **cherchi dense-slab
short-circuit**. Both are real efforts; pick per appetite for correctness-risk
(yang) vs port size (cherchi).

## 6. Diagnostic instrumentation (reusable; reverted to keep the tree clean)

- cherchi-rs `arrangements/soup.rs`, after `classify_all`: `CHERCHI_PAIR_PROBE`
  — post-dedup tris, pairs, Transversal/Coplanar × selfA/selfB/merged/cross
  buckets, top-z histogram. (~15 lines; trivial to re-add.)
- yang-rs `boolean.rs`, before the arrangement: `YANG_OPERAND_SELFX_PROBE` —
  per-operand `detect_improper_contacts` count (the arrives-clean test).
- yang-rs `boolean.rs`, before the arrangement: `YANG_FACE_PLANARITY_PROBE` —
  per-operand max off-plane residual over planar faces (the exactly-planar test).
- cherchi-rs `arrangements/soup.rs`, after `classify_all`: `CHERCHI_SELFA_DUMP`
  — dump selfA-transversal pair coordinates (optionally filtered to a z-slab).
- Existing, un-reverted: `YANG_COPLANAR_PROBE` (Stage-0 cross/wall tags).

All measured 2026-07-23 on `cargo test -p test-harness --test assay_kv2 --release
single_case` with `ASSAY_CASE=F0072`.
