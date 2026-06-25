# M8 plan — generalize rim-crossing propagation to partial caps (curved coplanar neighbours)

**Status (updated 2026-06-25):** PARTIALLY EXECUTED. Step 0 (confirm-the-geometry)
**overturned the arc-rim premise** for R0015 — its shared edge is a STRAIGHT line
GENERATOR (not an arc rim), and the split points land exactly on the cylinder.
That case is now SHIPPED via a surface-agnostic edge split (`edge_split_curved_face`
+ `fan_split_tri` in `stage0.rs`; R0015 past `build-mesh-nonplanar`, full assay
74/0). **What REMAINS of this plan: the genuine arc-rim case** — a coplanar
overlap boundary subdividing a CURVED (Circle/arc) rim shared with a cylinder
lateral. `edge_split_curved_face` returns `None` for that (loud residue), so the
sections below (sample the arc rim conformally → per-rim-edge `collect_rim_crossings`)
still apply to it. Find a corpus case that hits it first (grep the `build-mesh-
nonplanar` residue with a `circle=true` split edge) — R0015 was NOT it.

**Scope:** one focused session. Touches `crates/yang-rs/src/stage0.rs` only.
**Goal:** un-block the `build-mesh-nonplanar` residue that the coplanar-overlay
sliver fix exposed (R0015 and the curved-coplanar class), so a coplanar overlap
boundary that subdivides a rim shared with a **cylinder lateral** propagates the
split into that lateral conformally — for **partial** planar caps (arc rim + line
edges, from revolves), not just clean full discs.

---

## 1. Problem

When solids A and B have a coplanar overlapping pair of planar faces, Stage-0
(`stage0_preprocess` → the general overlay path in `boolean()`) segments the
shared plane (exact 2D Boolean, `coplanar_overlay`) and re-tessellates both
solids so the overlap carries identical meshes (Yang §4.5.5). The overlap
**boundary** subdivides the coplanar faces' edges; those splits must propagate
to the adjacent faces so the result stays watertight + conformal.

For a planar neighbour, `build_stage0_mesh` re-triangulates it with the
subdivided ring. For a **curved** neighbour (a cylinder lateral that shares a
`Curve::Circle` rim with the coplanar cap), there is no handler — it hits:

```
stage0.rs (build_stage0_mesh): `let Surface::Plane { .. } = f.surface else { … build-mesh-nonplanar }`
```

`collect_rim_crossings` already does the right thing — it propagates rim
crossing points into the cylinder lateral + opposite rim via `rim_overrides`
(consumed by `stage1_tessellate_with_rim_overrides`, the base tessellation) — but
**only for a clean full disc** (`disc_circle_edge`). A partial cap is skipped.

---

## 2. The precise gap (R0015 diagnosis, instrumented 2026-06-25)

`R0015` = `revolve(rectangle,boss)+extrude(circle,boss)+revolve(circle,boss)`.
Its failing coplanar pair `(face_a=0, face_b=0)`:

```
[rimx] pair=(0,0) opposite=true rim_a_empty=true rim_b_empty=false
       rim_sub_a=true rim_sub_b=true disc_a=false disc_b=true
[bmnp] f=2 surf=cyl outer=4 holes=0 split_edges=[(11,2)] curves=["C","L","C","L"]
[bmnp] f=2 split_edges=[11] edges_in_rim_overrides=[] rim_ov_keys=[]   (solid A's mesh)
```

Reading:
- **Face B is a clean disc** (`disc_b=true`): `rim_cross_b` fires →
  `collect_rim_crossings(b, …)` runs → B's cylinder handled. ✔
- **Face A is a PARTIAL planar cap** (`disc_a=false`, `rim_a_empty=true`): an arc
  rim + line edges, produced by R0015's revolve. Because it is not a clean disc:
  - `face_polygon_2d_tessellated(A)` falls through to `face_polygon_2d` (raw loop
    **vertices**), so the arc rim becomes a single **chord** (start→end) and
    `rim_a` is empty.
  - `rim_cross_a = !rim_a.is_empty() && rim_subdivided(...)` = **false** →
    `collect_rim_crossings(A)` **never runs** → `rim_overrides_a` is empty.
  - `collect_edge_splits(A)` (unconditional) adds the rim split to `splits_a`.
  - `build_stage0_mesh(A)`: cylinder lateral `f=2` shares the rim edge `11`
    (present in `splits_a`) → planar re-triangulation path → **`build-mesh-nonplanar`**
    (reported as `build-mesh-a`, i.e. solid A's mesh).

So the cylinder lateral on the **partial-cap side** gets a rim split with no
`rim_override`, and the planar-only neighbour path rejects it.

---

## 3. The crux subtlety — arc-as-chord vs sampled arc

This is **not** a one-line relax of the `disc_circle_edge` gate. The partial
cap's arc rim is represented in `poly_a` as a **straight chord** (its two
endpoint vertices), so:

- An overlay vertex that subdivides that chord lies on the **chord**, which
  deviates from the true arc by the sagitta. Its 3D lift (`frame.lift`) is on the
  chord, in the cap plane — **not on the cylinder's circular rim**.
- `collect_rim_crossings` works for clean discs precisely because
  `face_polygon_2d_tessellated` **samples** the disc rim into `poly.outer` (the
  arc points), so crossings land between true-arc samples and the bit-exact 3D
  rim point comes from the sampled ring (`rim_a` map) — conformal with the
  cylinder lateral's own rim tessellation.

**Therefore the partial cap's arc rim must be SAMPLED too** (like a disc), so the
overlay sees true-arc points, crossings land on the arc = the cylinder rim, and
the shared 3D points are bit-identical to the lateral's rim samples
(watertightness). The conformality contract is the same one `disc_rim_ring`
satisfies for full discs: **read the rim samples from Stage-1's own output**, so
they match the cylinder lateral byte-for-byte.

> Before implementing, CONFIRM the deviation empirically: instrument the 3D
> position of the chord-crossing vertex (`coords[vi]`) vs the cylinder surface
> (`signed_distance_to_surface(Cylinder, …)`). If it is off the cylinder by ~the
> sagitta (expected), the sampling step is required. If it is already on the
> cylinder (e.g. the arc is tessellated upstream so the "chord" is already
> multi-segment), the fix is much smaller — see §4 step 0.

---

## 4. Implementation steps

**Step 0 — confirm the geometry (1 probe run).** Re-add the `YANG_RIMX/BMNP`
probes (see git history of this session, or re-derive) and additionally print, for
the partial cap's arc rim edge: is it ONE `Curve::Circle`/arc edge, or already
split into segments in the loop? And is `coords[vi]` for the crossing off the
cylinder by the sagitta? This decides between the full sampling fix (steps 1-3)
and a smaller "route the existing split to a rim_override" fix.

**Step 1 — sample partial-cap rim edges in `face_polygon_2d_tessellated`.**
Today (stage0.rs ~`face_polygon_2d_tessellated`):
- `disc_circle_edge(brep, fi).is_some()` → sample the full-disc rim into
  `poly.outer` + `rim_map`.
- `is_holed_disc` → (reverted swiss-cheese path — ignore).
- else → `face_polygon_2d` (raw vertices; arc rim becomes a chord).

Add: for a planar cap whose outer loop is a **mix of `LineSegment` + one-or-more
`Curve::Circle`/arc edges**, build the polygon by walking the loop and, for each
arc rim edge that `lateral_for_cap` resolves to a cylinder, **sample that edge's
arc into the polygon** (reading the arc's Stage-1 samples for bit-exact
conformality — generalize `disc_rim_ring` to read a SINGLE arc edge's samples
rather than the whole cap fan), inserting each sample into `rim_map`. Line edges
stay single segments. Result: `poly.outer` is the cap's true boundary (line
segments + sampled arcs), `rim_a` (the rim map) is non-empty.

Pitfall: the loop order/winding must be preserved (the polygon must remain a
simple ring). Sample the arc start→end in the loop's traversal direction.

**Step 2 — generalize the rim-cross detection + `collect_rim_crossings` to
per-rim-edge.** Today `rim_cross_a = !rim_a.is_empty() && rim_subdivided(...)`
and `collect_rim_crossings` keys on `disc_circle_edge(brep, fi)` (the single cap
edge). Generalize:
- detection: a face needs rim-crossing handling iff its loop has ≥1
  `Curve::Circle`/arc edge that (a) `lateral_for_cap` resolves to a cylinder and
  (b) is subdivided (an overlay vertex strictly interior to one of its sampled
  sub-chords). Reuse `rim_subdivided` but over the sampled arc's sub-chords (now
  in `poly.outer` after step 1).
- `collect_rim_crossings`: take the **specific rim edge** (not
  `disc_circle_edge`); everything downstream (`lateral_for_cap`, the opposite-rim
  azimuth projection, the `rim_overrides` population keyed by `cap_edge` +
  `opp_edge`) already works per-edge. The cap-side crossing points come from the
  sampled `poly.outer` sub-chords exactly as today.

**Step 3 — stop `collect_edge_splits` double-handling the rim edge.** Once a rim
edge is handled via `rim_overrides`, it must NOT also be in `splits` (else
`build_stage0_mesh` still routes the cylinder lateral to the planar path —
remember `face_split` keys on `splits`, the bug mechanism). Two options:
  (i) in `collect_edge_splits`, skip edges that are `Curve::Circle`/arc (they are
      a curved neighbour's concern — rim_overrides handles them); OR
  (ii) in `build_stage0_mesh`, when a non-planar face's split edges are ALL
      covered by `rim_overrides`, use the base tess (`continue`) instead of
      erroring. Prefer (i) — cleaner, removes the split at the source. VERIFY (i)
      does not regress the PLANAR neighbour cases that legitimately need a Circle
      edge split in `splits` (grep the disc-rim-crossing tests).

---

## 5. Verification (mandatory — the strengthened-harness lesson)

A coplanar change is **not** trusted on "builds without error" — R0082 built
clean yet silent-wrong. Gate on:
1. **Full assay** (`cargo test -p test-harness --test assay_kv2 -- --ignored`):
   **SUPPORTED_CORRECT ≥ 74, SUPPORTED_WRONG == 0**. The latter is the P9 gate.
2. **R0015 progresses**: it should leave `build-mesh-nonplanar`. It may flip to
   SUPPORTED_CORRECT, or reveal a further residue (e.g. R0098's
   `build-mesh-triangulate`) — either is acceptable progress; a new
   SUPPORTED_WRONG is not.
3. **Rewrite tier** green (`./scripts/test.sh rewrite`) — a stage0 change can
   break a downstream pin.
4. A focused **watertightness** assertion on R0015's result mesh
   (`no_self_intersection` + watertight oracle) if it reaches tessellation.
5. `cargo clippy -p yang-rs` + `rustfmt --check`.

Iterate single cases fast with the `ASSAY_ONLY="R0015"` filter (re-add the
`perl` one-liner from this session; it is reverted on `main`).

---

## 6. Risks / open questions

- **Conformality is the whole game.** If the sampled arc rim points are not
  bit-identical to the cylinder lateral's own rim tessellation, the result is
  non-watertight (a gap on the rim). `disc_rim_ring` solves this for full discs
  by reading Stage-1's output; replicate that exactly for the single arc edge.
- **Multiple arc rims per cap.** A washer cap (revolve(rectangle)) can have an
  inner AND outer arc rim, each shared with a different cylinder lateral. The
  per-rim-edge generalization (step 2) must loop over all qualifying edges.
- **Same-normal interaction.** The §369 scope gate walls same-normal rim
  crossings. R0015 is `opposite=true`, so it is in scope. Keep the same-normal
  gate as-is; this plan is the OPPOSITE-normal partial-cap case only.
- **Don't widen tolerances.** The `build-mesh-nonplanar` guard is correct (a
  curved face genuinely cannot go through the planar path). This plan adds the
  missing curved-neighbour handler; it does not relax the guard.

---

## 7. References

- `crates/yang-rs/src/stage0.rs`:
  - general overlay path + `rim_cross_a/b` decision (~line 360),
  - `collect_edge_splits` call (~463), `collect_rim_crossings` call (~488),
  - `face_polygon_2d_tessellated` (the disc-rim sampler), `disc_rim_ring`,
  - `collect_rim_crossings` (~943), `lateral_for_cap`,
  - `build_stage0_mesh` non-planar branch (the failure site).
- `refs/text/yang2025_hybrid_boolean.txt:717-760` — §4.5.5 + Fig. 16 (boundaries
  shared identically between the common surface and the two parts).
- Memory: `kernel_v2_m8_coplanar_landscape` (the full M8 map + this residue's
  diagnosis) and `sketch_gui_*` are unrelated.
- Sibling shipped fix this session: `coplanar_overlay` benign-needle drop
  (`rounded_tri_disposition`) — the layer below this one.
