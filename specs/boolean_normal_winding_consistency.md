# Boolean Fragment Winding Consistency

## Goal

Every `FacePoly` emitted from boolean clipping has vertices winding consistently
with its stored normal.

## Invariant

```
dot(newell_normal(frag.verts), frag.normal) >= 0
```

for all emitted fragments with `verts.len() >= 3`.

## Branch Table

| Condition | Action |
|-----------|--------|
| `dot >= 0` | Emit as-is (winding already consistent) |
| `dot < 0` | Reverse `verts` before emit |
| `verts.len() < 3` | Skip (degenerate) |

## Affected Locations

1. `collect_fragments()` — `emit` closure
2. `collect_union_fragments()` — `push_frag` closure

## Oracle

- `outward_normals` >= 95%
- `positive_signed_volume`

## Edge Cases

Near-degenerate fragments (area ≈ 0) have unreliable Newell normals. The dot
product could spuriously trigger reversal, but near-degenerate polygons are
visually negligible and the existing `len < 3` guard filters true degenerates.

## References

- [#33] Stroud — Newell normal is the standard polygon normal computation
