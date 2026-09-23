# B4 — `UnionAll`: balanced many-body union as a first-class feature

Status: **CHECKPOINT 1 (engine) + 2 (app, link, progress) — 2026-09-23.**
Roadmap: `specs/custom_features_and_modeling_roadmap.md` §B4.

## 1. Problem

A part assembled from many overlapping `NewBody` solids (the gravel bike's
frame tubes, a gearbox carrier built over the link) needs ONE solid at the
end: a multi-solid part exports as many STEP solids and cannot be measured
for mass. Today the only way is a chain of pairwise `BooleanCombine`
features — N−1 features, each a full Yang pipeline run whose accumulator
grows, so the chain costs O(N²·s) and the author has to write N−1 steps and
keep their target ids straight (the `Strict target on a CONSUMED output
duplicates the body` defect of `session_2026_09_17_planetary_gearbox_over_mcp`
was found doing exactly that).

## 2. The feature

```
Operation::UnionAll { params: UnionAllParams { targets: UnionTargets } }
UnionTargets = All                              // every live body before this feature
             | Selected { bodies: Vec<GeomRef> } // explicit Solid GeomRefs (≥ 1)
```

Serialized: `{"type":"UnionAll","params":{"targets":{"type":"All"}}}` or
`{"targets":{"type":"Selected","bodies":[GeomRef…]}}`. `targets` defaults
to `All`. New operation kind ⇒ no reader-floor bump (v4 Phase 1b: an
older build keeps it as `Operation::Unknown` and errors loudly).

### 2.1 Target resolution

- `All`: every live solid output (`Main` + `Body{..}`) of every ACTIVE,
  unsuppressed, not-already-consumed feature before this one, in tree
  order then output-key order. Consumes every such feature.
- `Selected`: each GeomRef must anchor a feature output. A body whose
  feature was already consumed is refused under `Strict` and dropped with
  a warning under `BestEffort` (the pattern's rule). Duplicates are
  refused. Consumes each surviving body's feature.
- Zero bodies ⇒ typed `ResolutionFailed` ("no live bodies"). One body ⇒
  passes through unchanged (custody transfer + warning), so a script that
  ends with `union_all()` is valid on a one-body part.

### 2.2 Execution — balanced tree with an AABB fast path

`kernel-v2` cannot know in advance which bodies touch. Every pairwise union
goes through `modeling_ops::execute_boolean` (the ordinary pipeline; a
union of disjoint operands returns TWO lumps via `boolean_union_multi` and
the operands are kept). The tree:

```
union_balanced(bodies):
  if |bodies| ≤ 1: return bodies
  L = union_balanced(bodies[..mid]);  R = union_balanced(bodies[mid..])
  for r in R: fold r into L                 // the merge step
fold(r, L):                                  // pattern.rs fold_union + AABB gate
  for each lump l in L (insertion order):
    if aabb(l) ∩ aabb(r) = ∅: skip           // never runs the kernel
    res = union(l, r)
    1 lump  ⇒ r := res, remove l, continue   // r may bridge later lumps
    ≥2 lumps ⇒ keep both (disjoint despite overlapping boxes)
  push r
```

Depth log N; total work O(N·s·log N) when everything connects. Every
union is an ordinary pipeline run — there is no new kernel code path and
no tolerance. Lump 0 is the first body's lump (the `Main` output); the
others are `Body{index}` in insertion order.

**AABB.** New `KernelIntrospect::solid_aabb(&handle) -> Option<([f64;3],[f64;3])>`:
a CONSERVATIVE box (vertex hull + every curved edge's `center ± radius`
and every sphere / torus `center ± (R + r)`; `None` when an edge carries
an unbounded-bulge curve — hyperbola / surface-pair — or the kernel does
not implement it). `None` on either side ⇒ treated as overlapping (the
kernel decides). The gate is exact: two bodies whose conservative boxes
are disjoint have no material in common, so their union IS the disjoint
sum, which the fold already models by keeping both.

The pattern's `fold_union` (`Add`, `Intersect`) gains the same gate — a
pure speed-up, byte-identical output: with overlapping boxes the same
unions run in the same order; with disjoint boxes the kernel's disjoint
passthrough already returned two lumps and the originals were kept.

### 2.3 Progress

`feature_engine::progress` — a thread-local sink (`install(Box<dyn Fn(&ProgressEvent)>)`,
`report(&ProgressEvent)`). `UnionAll` reports one event per pairwise
union it RUNS (`unions_done`, `bodies_remaining`, `label`), plus a final
event. The WASM worker installs a sink that posts a bare
`{type:"Progress", …}` frame; the bridge dispatches it as
`bridge.on('progress')`; the store shows it in the status bar. The agent
link forwards each frame to the relay as a `progress` frame for the
in-flight call (§2.3 of `specs/waffle_mcp_server.md`, reserved since Phase
0); the relay sends an MCP `notifications/progress` when the client passed
a `progressToken`, and logs it otherwise.

### 2.4 The consumed-operand defect (P10)

`BooleanCombine` and explicit combine targets resolved a body by feature
output alone, so a target whose feature an earlier feature had CONSUMED
still resolved — the boolean ran on the stale pre-consumption handle and
the part gained a duplicate (the 2026-09-17 gearbox finding). Now:
`BooleanCombine` refuses a consumed operand (typed `ResolutionFailed`,
either policy — a pair op cannot drop an operand); `resolve_combine_targets`'s
`Explicit` arm applies the pattern's rule (Strict ⇒ error, BestEffort ⇒
dropped with a warning). `ModelUpdated` carries `consumed_features` so the
Boolean dialog lists only live bodies.

## 3. Oracles

`crates/feature-engine/tests/union_all.rs` (MockKernel — structure, custody,
typed refusals, round-trip, script API) and
`crates/test-harness/tests/union_all_kv2.rs` (kernel-v2):

1. **Volume**: a chain of K overlapping boxes unions to ONE shell whose
   exact volume equals inclusion–exclusion (`solid_volume`, 1e-9), χ = 2,
   watertight at chord tol 1e-3.
2. **Disjoint clusters**: two far-apart clusters ⇒ two bodies, each the
   union of its cluster; the fast path never ran the kernel across clusters
   (the progress union count equals the number of connecting unions).
3. **Determinism vs the chain**: the `UnionAll` result of N bodies has the
   same exact volume and shell/χ census as the same bodies chained through
   N−1 `BooleanCombine` features, and two identical `UnionAll` builds
   tessellate identically.
4. **Custody**: every source feature is consumed; the result's `Main` is
   the first body's lump; a later feature can address `Body{1}`.
5. **Consumed operand is loud**: a `BooleanCombine` on a consumed feature
   is a typed error with no output; a `Selected` union with a consumed
   BestEffort body drops it with a warning.

## 4. Out of scope

- Re-ordering bodies for a better tree (e.g. by spatial sort) — the tree
  order is the tree order, so the result is deterministic and explainable.
- A many-body Cut / Intersect (the pattern's `Cut` already fans out).
- Parallel pairwise unions (WASM is single-threaded).
