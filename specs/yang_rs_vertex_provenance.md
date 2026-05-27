# `yang-rs::boolean` output vertex provenance — Spike PR-YR3

## Goal

After the mesh boolean backend runs, walk each output mesh vertex and
identify its spatial source: a B-Rep vertex on input A, on input B,
or a new intersection point.

This is a **sidecar-feasible substitute** for Yang Stages 5/6 (§4.4.2).
Real Stages 5/6 require per-triangle inside/outside labels from
Stage 2's arrangement; the C++ `mesh_booleans` binary doesn't expose
those. PR-YR3 reconstructs vertex-level provenance via spatial
matching against inputs.

After PR-YR3:
- `boolean()` returns a `BRep` with a meaningful (non-`Unknown`)
  `TessellationMap` even via the sidecar backend.
- Downstream (eventually kernel-v2) can reason about which output
  vertices descend from which input B-Rep feature.
- Output `BRep` topology remains empty — face/edge reconstruction
  is PR-YR4+; full Yang Stages 5/6 reassembly is gated on native
  arrangement output with labels.

## Architectural reality

Yang §4.4.2 (lines 574-604) requires:
- Per-triangle labels from Stage 2 (Cherchi arrangement)
- Intersection curves from Stages 3/4

The C++ binary consumes these internally and emits only the final
result mesh. PR-YR3 cannot run real Stages 5/6 — only vertex
matching against inputs is feasible.

## Public API

### New `TessellationSource` variant

```rust
pub enum TessellationSource {
    BRepVertex(u32),
    BRepEdge { edge: u32, t: f64 },
    BRepFace { face: u32, u: f64, v: f64 },
    /// Output vertex created by the boolean operation; doesn't
    /// spatially match any input vertex. New in PR-YR3.
    Intersection,
    /// Source genuinely unknown (BRep::from_mesh degenerate path).
    Unknown,
}
```

### New constant

```rust
/// Spatial tolerance for matching output mesh vertices to input
/// mesh vertices. Tight enough to avoid false positives on
/// genuine intersection points; loose enough to absorb the
/// sidecar's internal coordinate-normalization rounding.
pub const MATCH_TOLERANCE: f64 = 1e-9;
```

### `boolean()` signature: unchanged

```rust
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError>;
```

Internal behavior changes: after backend produces output_mesh, run
the spatial matching loop to populate the output `TessellationMap`.

## Algorithm

```text
1. output_mesh = backend.boolean(a.as_mesh(), b.as_mesh(), op)
2. sources = Vec::with_capacity(output_mesh.num_verts())
3. for v in output_mesh.verts:
     src = try_match(a, v)
         .or_else(|| try_match(b, v))
         .unwrap_or(TessellationSource::Intersection)
     sources.push(src)
4. return BRep {
       vertices: [], edges: [], faces: [],
       mesh: output_mesh,
       tessellation: TessellationMap { sources },
   }

fn try_match(brep: &BRep, target: Point3) -> Option<TessellationSource>:
   for (i, v) in brep.as_mesh().verts.iter().enumerate():
       if sqr_dist(v, target) <= MATCH_TOLERANCE^2:
           return Some(brep.tessellation_map().lookup(i))
   None
```

O(output_n * (input_a_n + input_b_n)) per boolean call. For typical
CAD meshes (<10k verts each), <100M comparisons → sub-second.
Spatial indexing is a future PR.

## Invariants

1. `output_map.len() == output_mesh.num_verts()`
2. Every map entry is one of `BRepVertex / BRepEdge / BRepFace / Intersection / Unknown`
3. If `output_mesh.verts[i]` spatially matches `a.mesh.verts[j]` within tolerance, `output_map.lookup(i) == a.tessellation.lookup(j)`
4. If no match in either input, `output_map.lookup(i) == TessellationSource::Intersection`
5. PR-YR2 invariants for input BReps are preserved (no mutation)

## Error contract

No new error variants. Backend errors continue mapping to `YangError::MeshBooleanFailed`.

## Limitations (banked for future PRs)

1. **Output topology is empty** — no faces, edges. PR-YR4+ adds face/edge reconstruction.
2. **No triangle-level attribution** — we know vertex provenance, but not which input B-Rep face each output triangle descends from. PR-YR4 candidate.
3. **No real Stage 5/6 reassembly** — gated on labeled arrangement output (native cherchi-rs or upstream sidecar changes).
4. **O(n*m) spatial matching** — fine for v1; future spatial index for large meshes.
5. **Fixed tolerance** — `MATCH_TOLERANCE` is a constant; future overload may expose it as a parameter.

## Test plan (~10 tests, 4 groups)

### Group 1 — `Intersection` variant (2 tests)
- Construction + match round-trip
- `Intersection != Unknown` (distinct)

### Group 2 — `MATCH_TOLERANCE` (1 test)
- Constant value is 1e-9

### Group 3 — Spatial matching via mock backend (4 tests)
- Mock returns input A verbatim → output map matches A's map entries
- Mock returns input B verbatim → output map matches B's map entries
- Mock returns mesh with all-new coords (offset 100) → output map all-`Intersection`
- Mock returns mixed (half from A, half new) → mixed output map

### Group 4 — Sidecar integration regression (2 tests)
- Two-cube intersection still produces non-empty BRep + non-empty TessellationMap
- The two-cube output map contains AT LEAST ONE non-Unknown entry when both inputs were constructed via `BRep::new`

## Honest framing

PR-YR3 is **not** real Yang Stages 5/6. It's a sidecar-feasible
substitute that populates vertex-level provenance via spatial
matching. The crate doc + memo explicitly call this out. Full
reassembly is deferred.

## References

- Yang 2025 §4.4.2 — `refs/text/yang2025_hybrid_boolean.txt:574-604`
- PR-CSR1 sidecar recon — output format limitations
- PR-YR2 spec — `TessellationMap` structure
