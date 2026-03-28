# Swiss Cheese Disc (F0086-F0090)

## Purpose

Test mass boolean cuts and genus calculation. A circular disc with many randomly-placed
through and blind holes exercises the kernel's ability to handle high-genus topology.

## Algorithm

### Base disc

Circle boss extrude with R ∈ [1.0, 2.0], depth ∈ [0.2, 0.5].

### Hole placement (rejection sampling)

1. Generate candidate position in polar coordinates: r ∈ [0, R_disc - r_hole), θ ∈ [0, 2π)
2. Convert to Cartesian: (x, y) = (r·cos(θ), r·sin(θ))
3. Check non-overlap: `dist(c_i, c_j) > 2·r_hole` for all previously placed holes
4. Reject and retry if overlap detected
5. Max attempts = `n_holes × 1000` to avoid infinite loops

For high hole counts (20+), scale hole radius down: `r_hole / sqrt(n_holes / 10)`.

### Hole types

- **Through-holes**: depth > disc_depth (cylinder penetrates entirely)
- **Blind holes**: depth < disc_depth (pocket doesn't exit bottom)

### Euler characteristic

Each through-hole adds genus 1 to the solid:
```
euler_target = 2 - 2·n_through_holes
```

Blind holes (pockets) don't change genus — they create an internal face but the
topology remains genus-0 per pocket.

## Cases

| ID    | Total holes | Through | Blind | Seed  |
|-------|-------------|---------|-------|-------|
| F0086 | 5           | 3       | 2     | 30001 |
| F0087 | 10          | 5       | 5     | 30002 |
| F0088 | 15          | 8       | 7     | 30003 |
| F0089 | 20          | 10      | 10    | 30004 |
| F0090 | 30          | 15      | 15    | 30005 |

## Oracle expectations

- `euler_target`: `2 - 2·n_through`
- `expect_watertight`: true
- `max_bbox_extent`: `2·disc_radius + 1.0`
- `volume_monotonicity`: `["increase", "decrease", ..., "decrease"]`
- `expect_rebuild_error`: false
