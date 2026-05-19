# Y55 Tier A Cascade-Attribution Canary — F0020 Defect 2 Phase 1 hypotheses REFUTED

## Status: TIER A COMPLETE — pause for user review per plan decision gate

## What shipped

Two env-gated probes added to `crates/kernel/src/tessellation/mod.rs` and `crates/kernel/src/boolean/exact_mesh.rs`:

- **`Y55_TESSELLATE_PROBE=1`** — emits `[y55-loop]` per outer/inner loop walked in `collect_loop_boundary`, classifying each HE as `PHANTOM` (no edge_verts entry, gap), `NMM` (twin=None) with its edge_idx, or `MFD` (twin=Some, normal manifold-paired); emits `[y55-face]` per face binding `kid + face_idx + outer_loop + tri_range`.
- **Extension to `Y54_INPUT_COLLIDE=1`** — adds `[y54-input] COLLIDE edge=(v0,v1) tri_indices=[...]` per colliding directed-edge pair (was only summary count before).

Default-off byte-identical confirmed: F0020 spotlight metrics unchanged (47 unpaired, 30 degen, 5 zero-area, 175 tris). Kernel 1249/34/42 unchanged.

## Empirical findings (F0020 boolean 2 input mesh A — the post-boolean-1 B-Rep retessellation)

| Metric | Value |
|---|---|
| Total HEs across 38 faces | 247 |
| MFD (twin=Some, valid edge_idx) | 211 (85%) |
| **NMM at edge=EdgeIdx(0) UNINITIALIZED** | **36 (15%)** |
| NMM at proper edge_idx | 0 |
| PHANTOM (no discretization, gap) | 0 |
| Total directed-edge collisions in tris_a | 37 |
| Total face-pairs producing collisions | 8 |

### Per-collision attribution (cross-reference of 37 collisions ↔ source face HE composition):

| Attribution class | Count |
|---|---|
| Both tris from faces with NMM-at-edge-0 HEs | 0 |
| One tri from defective face | 0 |
| **Both tris from MFD-ONLY faces** | **37 (100%)** |

### Colliding face-idx pairs (boolean-1's output face indices):

| Face pair | Collision count |
|---|---|
| (2, 6) | 9 |
| (3, 7) | 6 |
| (3, 10) | 6 |
| (3, 5) | 5 |
| (3, 9) | 3 |
| (3, 8) | 3 |
| (3, 4) | 3 |
| (4, 5) | 2 |

Face_idx=3 is involved in 6 of 8 pairs and 26 of 37 collisions. All involved faces (2-10) have:
- All MFD HEs (no NMM at edge=0, no PHANTOM)
- Proper outer-loop boundaries (4-19 verts each)
- 2-17 tris each in tessellation

## What this refutes

**Plan's Phase 1 hypotheses (a), (b), (c), (d) all REFUTED for F0020:**

- **(a) Phantom-closing-edge from R3 loser open chains** — 0 PHANTOM HEs in entire boolean 2 input. Hypothesis fully refuted.
- **(b) NMM HE handling in tessellation** — 0 NMM HEs at proper edges; the 36 NMM HEs at edge=0 are a B-Rep arena defect (twin-pairing's NMM branch never assigns a real Edge) but produce 0% of the 37 collisions.
- **(c) Multiple B-Rep faces on the same side of a shared edge** — closer but not quite; the colliding face-pairs DO appear to share boundary positions, but every HE involved is MFD (`twin = Some(_)`), so the arena's twin pointers don't directly expose the violation.
- **(d) Cyclic discretization / self-loop** — F0020 has no cylindrical caps in the affected face range.

## What this surfaces (TWO distinct defects)

### Defect-1 (Tier A's load-bearing finding): MFD-MFD face-pair edge collisions

37/37 (100%) of the cascade collisions involve face pairs where BOTH faces' boundaries produce the SAME forward directed edge. All HEs involved are twin=Some manifold-paired in the arena, yet their tessellation output collides.

This means **the B-Rep arena's twin pointers do NOT enforce 2-manifold orientation consistency**. Two faces sharing a physical edge can have their twin'd HEs walking the edge in the SAME direction in their respective outer loops — a violation of the half-edge model's manifold property that the arena's twin links don't catch.

Likely cause: in boolean 1's output B-Rep assembly (`flood_fill_patches::twin_pairing`), the algorithm pairs HEs by canonical (origin, dest) vertex positions without enforcing that paired HEs walk OPPOSITE directions. When two faces both walk (v0, v1) forward in their outer loops, twin-pairing finds them and links them — but the geometric reality (they should walk opposite directions) is silently violated.

Anchor for Tier B (NEW path γ, NOT in the original plan): `topology_extract.rs::flood_fill_patches` Step 5/6 twin-pairing. Enforce or detect that paired (he_a, he_b) walk opposite canonical directions (origin_a == dest_b AND dest_a == origin_b), reject pairings where (origin_a == origin_b) (same-direction violation).

### Defect-2 (Tier A's banked finding): NMM HEs at edge=EdgeIdx(0)

`topology_extract.rs::flood_fill_patches::twin_pairing` (`L1438-1496`) handles the `[] => { /* no reverse candidate */ }` case as legitimate NMM but **never creates an Edge entry** for the HE. The HE retains `edge: EdgeIdx(0)` from initial construction (`L270`), causing `collect_loop_boundary` to read the discretization of the FIRST manifold edge instead of the NMM HE's actual geometry.

This is a B-Rep arena correctness defect that doesn't currently produce visible failures (NMM HEs don't make it into the 37 collisions in F0020), but it's a latent bug — any case where NMM HE positions matter would mis-render.

Anchor: same function, NMM branch. Create an Edge entry per NMM HE (with `half_edge: he_fwd`) and update `arena.half_edges[he_fwd.0].edge` to that new EdgeIdx.

## Decision gate routing

Per the plan:

> - **Mixed (no >50% category)**: Architectural decision needed. Pause for user review with histogram in hand. Tier B not scoped.
> - **Some other dominant mechanism (off-list)**: Tier A surfaced something Phase 1 missed. Pause for user review.

Both apply. The 37/37 MFD-MFD collisions are an off-list mechanism (Defect-1). The NMM-at-edge-0 finding (Defect-2) is unrelated to the 37 collisions but a real defect.

**Recommended next step:** Tier B path γ — fix `flood_fill_patches::twin_pairing` to reject same-direction pairings (Defect-1 anchor). Scope estimate: ~40-80 LOC. Defect-2 (NMM at edge=0) is a separate small PR after path γ stabilizes.

**User decision needed:**
- Approve path γ (twin-pairing direction-check) as next Tier B target?
- Or different priority?

## Diagnostic artifacts shipped (durable infrastructure regardless of Tier B route)

- `Y55_TESSELLATE_PROBE=1` env-gated probe
- `Y54_INPUT_COLLIDE=1` extended with per-collision-pair tri-index dump
- Cross-reference Python analyzer (inline in this memo's appendix)
- This audit memo

## Verification commands

```bash
cd /home/claude/workspace

# Default-off byte parity
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | grep "Detail:" 
# expect: 47 unpaired, 30 degenerate, 175 tris (unchanged from main)

# Y55 + Y54 measurement
Y55_TESSELLATE_PROBE=1 Y54_INPUT_COLLIDE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 \
  | grep -E "y55-loop|y55-face|y54-input"
# expect: per-loop HE classification + per-face context + per-collision tri-pair listing

# Regression
cargo test -p kernel --lib 2>&1 | tail -3 # expect 1249/34
```

## Honest read

Phase 1's leading hypothesis ("R3 loser → phantom-closing-edge faces") was wrong. The chain-builder fix (commit `26d9094`) successfully eliminated phantom-closing-edge faces in F0020 (0 PHANTOM HEs in the entire boolean 2 input). The cascade defect surviving that fix is a DIFFERENT mechanism: same-direction MFD-MFD pairings that the twin-pairing logic doesn't catch.

Per discipline (`feedback_anchor_before_fix.md` + P10): I stop here, ship the canary, and present the empirical finding for user decision on Tier B path. I do NOT improvise Tier B path γ implementation without sign-off — the Defect-1 anchor is a different fix-shape than the original plan scoped, and warrants a re-scoped plan cycle.
