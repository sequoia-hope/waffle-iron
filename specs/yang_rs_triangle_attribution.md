# `yang-rs::boolean` per-triangle face attribution — PR-YR4

## Goal

After `boolean()` produces an output mesh + per-vertex `TessellationSource`
(PR-YR3), label each output triangle with the input B-Rep face it
descends from (if any) via **majority-vote** of its 3 vertices'
provenance.

This is a **sidecar-feasible substitute** for Yang Stage 5/6 patch
labels. Real Stage 5/6 (§4.4.2, lines 574-604) needs per-triangle
labels from Stage 2's arrangement; the C++ `mesh_booleans` binary
doesn't expose them. PR-YR4 substitutes a majority-vote heuristic over
PR-YR3 vertex provenance.

After PR-YR4:
- `boolean()` returns a `BRep` whose `triangle_attribution()` map
  identifies, for each output triangle, the input `(InputId, face_idx)`
  it descends from (or `None` if no majority).
- Downstream (eventually kernel-v2 / PR-YR5) can reason about which
  input B-Rep faces survived the boolean and which output triangles
  belong to fresh cut surfaces.
- Output `BRep` topology (`faces`, `edges`) remains empty —
  reconstruction by grouping same-attribution triangles is PR-YR5+.

## Architectural reality

Same constraint as PR-YR3: the C++ binary emits only the final result
mesh. No per-triangle labels. PR-YR4 cannot run real Stage 5/6 — only
heuristic attribution from existing vertex provenance is feasible.

## Public API

### New `InputId` enum

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InputId {
    /// Input A (first argument to boolean()).
    A,
    /// Input B (second argument to boolean()).
    B,
}
```

`A` is declared first so `InputId::A < InputId::B` by enum discriminant.
The lexicographic ordering on `(InputId, u32)` drives the tie-break.

### New `TriangleAttribution` struct

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TriangleAttribution {
    pub input: InputId,
    pub face: u32,
}
```

### New `TriangleAttributionMap`

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct TriangleAttributionMap {
    attributions: Vec<Option<TriangleAttribution>>,
}

impl TriangleAttributionMap {
    pub fn empty() -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    /// Returns `None` if the triangle has no majority face. Panics
    /// in debug if `mesh_tri` is out of range.
    pub fn lookup(&self, mesh_tri: u32) -> Option<TriangleAttribution>;
}
```

### `BRep::triangle_attribution()` accessor

```rust
impl BRep {
    pub fn triangle_attribution(&self) -> &TriangleAttributionMap;
}
```

`BRep::new` and `BRep::from_mesh` both produce
`TriangleAttributionMap::empty()`. Only `boolean()` populates it.

### `boolean()` signature: unchanged

```rust
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError>;
```

Internal behavior changes: after the PR-YR3 vertex-matching pass,
compute per-triangle attribution from vertex provenance + per-vertex
`InputId` tracked internally.

## Algorithm

```text
boolean(a, b, op, backend):
    output_mesh = backend.boolean(a.as_mesh(), b.as_mesh(), op)?

    // Vertex pass (extends PR-YR3 to also track InputId per vertex)
    sources: Vec<TessellationSource> = []
    inputs:  Vec<Option<InputId>>    = []     // internal only
    for v in output_mesh.verts:
        (input_id, src) = match_with_input(a, b, v)
        sources.push(src); inputs.push(input_id)

    // Triangle pass (PR-YR4)
    attribs: Vec<Option<TriangleAttribution>> = []
    for tri in output_mesh.tris:
        sets = [
            face_candidates(inputs[tri[0]], sources[tri[0]], a, b),
            face_candidates(inputs[tri[1]], sources[tri[1]], a, b),
            face_candidates(inputs[tri[2]], sources[tri[2]], a, b),
        ]
        attribs.push(majority_vote(&sets))

    Ok(BRep {
        vertices: [], edges: [], faces: [],
        mesh: output_mesh,
        tessellation: TessellationMap { sources },
        triangle_attribution: TriangleAttributionMap { attributions: attribs },
    })

match_with_input(a, b, target) -> (Option<InputId>, TessellationSource):
    if let Some(src) = match_against(a, target): return (Some(A), src)
    if let Some(src) = match_against(b, target): return (Some(B), src)
    (None, TessellationSource::Intersection)

face_candidates(input, source, a, b) -> Vec<(InputId, u32)>:
    let Some(input) = input else: return []
    let brep = match input { A => a, B => b }
    match source:
        BRepFace{face,..}  => [(input, face)]
        BRepEdge{edge,..}  => [(input, fi) for (fi, f) in brep.faces().enumerate() if f.outer_loop.contains(edge)]
        BRepVertex(v)      => [(input, fi) for (fi, f) in brep.faces().enumerate()
                               if f.outer_loop.iter().any(|&e|
                                   brep.edges()[e].start == v ||
                                   brep.edges()[e].end == v)]
        Intersection       => []
        Unknown            => []

majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution>:
    let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new()
    for set in sets:
        let mut unique = set.clone(); unique.sort(); unique.dedup()
        for c in unique: *counts.entry(c).or_insert(0) += 1
    // Pick the (input, face) with MAX count among those with count >= 2.
    // BTreeMap iterates ascending in (input, face) — on equal count, the
    // first seen (lowest key) wins because we only replace on strictly
    // greater count.
    let mut best: Option<((InputId, u32), u8)> = None
    for (key, count) in &counts:
        if *count < 2: continue
        match best:
            None                       => best = Some((*key, *count))
            Some((_, bc)) if *count > bc => best = Some((*key, *count))
            _                          => {} // equal count: keep first (lower key)
    best.map(|((input, face), _)| TriangleAttribution { input, face })
```

Complexity: per triangle O(F · E_per_face) for candidate generation +
O(k log k) for voting (k ≤ ~30). Whole pipeline
O((V + T) · F · E_per_face). For typical CAD (≤10k tris), sub-second.

## Invariants

1. `(BRep from new() or from_mesh()).triangle_attribution().is_empty() == true`
2. `(BRep from boolean()).triangle_attribution().len() == output_mesh.num_tris()`
3. If all 3 verts of triangle T are `BRepVertex(v_i)` of the same
   input I and they all share face F in I's topology, then
   `triangle_attribution.lookup(T) == Some(TriangleAttribution{I, F})`.
4. If ≥2 of 3 verts spatially match the same `(input, face)` candidate,
   attribution is `Some(that pair)`.
5. If no `(input, face)` reaches 2 votes, attribution is `None`.
6. PR-YR3 vertex-level invariants preserved
   (`tessellation_map().len() == output_mesh.num_verts()`;
   `Intersection` for unmatched verts).

## Edge cases

- **All 3 verts `Intersection`**: all candidate sets empty → `None`. ✓
- **Mixed `(A, F0)` + `(B, F1)` + `Intersection`**: candidates A:F0 (1
  vote), B:F1 (1 vote), Intersection contributes 0. No 2-of-3 majority
  → `None`. ✓
- **Tie at count 2 between `(A, 0)` and `(A, 1)`**: BTreeMap iterates
  ascending; `(A, 0)` returned first. Documented.
- **Tie at count 3** (impossible — only 3 verts; one (input, face) at
  count 3 means all 3 verts agree on it; another can have count ≤ 2):
  not actually possible. The "lowest-first" rule covers it harmlessly.
- **Input from_mesh** (no topology): all `BRepVertex` candidate lookups
  return empty (since `faces()` is empty). Attribution falls through
  to other input or `None`. Graceful degradation. ✓

## Error contract

No new error variants. Backend errors continue mapping to
`YangError::MeshBooleanFailed`.

## Limitations (banked)

1. **Output topology is still empty** — `faces`/`edges` accessors
   return empty slices. PR-YR5+ groups same-attribution triangles
   into reconstructed B-Rep face patches.
2. **No real Stage 5/6** — gated on labeled arrangement output
   (native cherchi-rs or upstream sidecar changes).
3. **O(F · E_per_face) candidate lookup** — naive scan. PR-YR4b can
   precompute vertex→edge / edge→face incidence indices for O(1)
   lookups when meshes exceed ~10k faces.
4. **Fixed majority threshold (2-of-3)** — `boolean()` does not expose
   a stricter (3-of-3) mode. Bank as future overload if needed.
5. **No per-vertex public InputId accessor** — downstream consumers
   only see triangle-level attribution. Revisit only if needed.

## Test plan (~13 tests, 4 groups)

### Group 1 — Types (3 tests)
- `InputId` derives + ordering (`A < B`) + Debug format
- `TriangleAttribution { input, face }` construction + equality + Copy
- `TriangleAttributionMap::empty()`, `len()`, `is_empty()`, `lookup()` OOR

### Group 2 — Algorithm via mock backend (6 tests)
- **Pure-A** (mock returns A verbatim) → all tris attribute to `(A, face)`
- **Pure-B** (mock returns B verbatim) → all tris attribute to `(B, face)`
- **All-new** (coords far from A or B) → all tris attribute to `None`
- **Mixed majority** (2 A-verts + 1 new) → `Some((A, F))`
- **No-majority** (1 A-vert + 1 B-vert + 1 new) → `None`
- **Tie-break determinism** (3 verts on shared edge F0/F1) → `Some((A, F0))`

### Group 3 — Empty-topology degradation (2 tests)
- Both inputs `from_mesh` → all `None`, length matches num_tris
- One `from_mesh` + one topology'd input, mock returns the topology'd
  one verbatim → attribution mirrors the topology'd input

### Group 4 — Sidecar regression (2 tests, in tests/end_to_end.rs)
- Two-cube intersection: `triangle_attribution().len() == num_tris()` AND
  ≥1 `Some` attribution
- Two-cube union: length invariant; ≥1 `None` (interior tris from new cut)

## Honest framing

PR-YR4 is **not** real Yang Stage 5/6. It's a sidecar-feasible
heuristic that gets one architectural level deeper than PR-YR3. The
output `BRep` topology is still empty after PR-YR4 — face
reconstruction is PR-YR5+. Full Stage 5/6 reassembly is deferred
indefinitely, gated on labeled arrangement output.

## References

- Yang 2025 §4.4.2 — `refs/text/yang2025_hybrid_boolean.txt:574-604`
- PR-YR3 spec — `specs/yang_rs_vertex_provenance.md`
- PR-CSR1 — sidecar OBJ-only output reality check
