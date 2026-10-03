# yang-rs / cherchi-rs — P0014: the arrangement's LPI pencil at an operand vertex is ONE point

**Status:** spec + landed increment (2026-10-03, night). **Change class:** bug
fix (modeling-related). **Crates:** `cherchi-rs` (Stage-2 emission — the
generator-identity record) + `yang-rs` (`boolean()` step (2b), after the I6
weld). Paper: §4.4.1 Fig-11,
`refs/text/yang2025_hybrid_boolean.txt:560-566`. Deviation **N70**.

## 1. Goal

Assay case **P0014** (`convex5:boss gear10:rev-cut`, prospector seed 2) STOPs
loud at the Yang §4.4.1(a) degenerate-triangle unzip:

```
YANG_LRR_STOP site=degenerate_no_longedge ndeg=2
YANG_LRR_SITE loc=crates/yang-rs/src/stage4_correct.rs:14388 reason=LocalRefinementRequired v=4294967295
```

Anchored 2026-10-03 (evening) in `docs/yang_tail_triage.md`: the site is a
TRIANGLE PAIR (the same vertex triple with opposite winding), the STOP carries
`under_resolution: None` (so the §4.5.2 certificate cannot speak for it and the
§4.5.2 ladder is measurably inert — both rungs re-tessellate to identical
counts, `b 15116 -> 15116`), and the root is a **2-ULP arrangement twin**
(`edge (139,141) len=2.730e-13 moved=(false,false)`, three more in the same
mesh). The anchor named the owner: **the producer** — the twin must not exist
by the time Stage 4 runs.

## 2. Measured mechanism — the twin is an LPI PENCIL (2026-10-03, night)

`CHERCHI_VERT_PROVENANCE=1e-12` on P0014's failing op, verbatim (the cluster
that becomes the compacted `(139,141)`):

```
CHERCHI_VERT_PROVENANCE pair out(143,145) d=2.730e-13 d_exact=2.822e-13 @(-650.066898747,251.102507487,94.659033900)
CHERCHI_VERT_PROVENANCE   out 143 soup 10217: LPI line[B#8119→B#8153] plane[A#1,A#0,A#7] sin_inc=2.552e-1
CHERCHI_VERT_PROVENANCE   out 145 soup 10219: LPI line[B#8187→B#8153] plane[A#1,A#0,A#7] sin_inc=6.220e-2
…
out 146 soup 10220: LPI line[B#8153→B#8188] plane[A#1,A#0,A#7] sin_inc=9.152e-2
out 147 soup 10221: LPI line[B#8153→B#8154] plane[A#1,A#0,A#7] sin_inc=9.615e-1
out 148 soup 10222: LPI line[B#8153→B#8120] plane[A#1,A#0,A#7] sin_inc=2.868e-1
out 8296 soup  8163: EXPLICIT B#8153
```

So the cluster is **one operand vertex plus the pencil of its own incident mesh
edges' pierce points against one plane**: B#8153 (a Stage-1 tessellation vertex
on the gear's cone flank) lies on the boss plane `[A#1,A#0,A#7]` to within
~2.7e-13 at coordinate scale 651 (relative 4.2e-16, ≈ 2 ULP) — but not
exactly. The exact arrangement therefore mints one LPI per incident B edge,
each at a slightly different point along its own edge, and emits the operand
vertex as well. Six output vertices, one geometric point.

All three further sub-floor twins in the same mesh have the identical
signature (shared line endpoint, one pierced plane triple):

| cluster | members | shared endpoint | pierced plane |
|---|---|---|---|
| `@(-235.97,51.92,630.64)` | out 114,116,119 + EXPLICIT 7644 | `B#7501` | `A#2,A#6,A#5` |
| `@(39.72,92.89,667.79)` | out 121,122,130 + EXPLICIT 7673 | `B#7530` | `A#2,A#6,A#5` |
| `@(-650.07,251.10,94.66)` | out 143,145,146,147,148 + EXPLICIT 8296 | `B#8153` | `A#1,A#0,A#7` |

### 2.1 This is NOT a port divergence (so no sidecar diff is owed)

Every pair's EXACT separation is **non-zero** (`d_exact` 2.822e-13, 2.002e-13,
… down to 7.410e-15 — computed in rationals from the soup's own
`VertexCoords`, the probe's documented discriminator: `d_exact = 0` is a dedup
gap, `d_exact > 0` is a true twin). A reference arrangement that de-duplicates
*exactly* coincident implicit points cannot fuse these: as exact points they
are distinct. The multiplicity is a faithful consequence of the input, not a
cherchi-rs port bug — which is why the fix is a RECONCILIATION at the
arrangement boundary and not a correction inside the arrangement.

### 2.2 Why all three existing welds skip it, each by design

| weld | why it skips the pencil |
|---|---|
| I6 weld, `boolean()` step (2) | mixed operands take the KV15 per-vertex path, and a vertex is eligible only when EVERY incident triangle descends from a `Surface::Plane` face — these are incident to gear-cone triangles. The curved fallback is BIT-EXACT, and the pair differs in 2 ULP. (The bit-exact weld does fuse the two members that round identically: `out(143,148) d=0.000e0` and `out(147,8296) d=0.000e0`.) |
| (3c) §4.4.1(b) sub-feature merge | its degeneracy DETECTOR is the triangle AREA (`area < floor²`). The twin triangle is a NEEDLE — base 2.73e-13, far vertex 192 away, `area_d=2.489e-11` ≥ `1e-12` — so it is never a candidate. |
| (3b′) N47 coincident-moved weld | restricted to `moved`×`moved` by design, "so it never touches un-relocated arrangement geometry `boolean()` kept for watertightness (cf. the §4.4.1(b) micro-scale R0091 revert)". Both twins are `moved=false`. Its own comment already names this population and its owner: *"the R0012/R0098 render twins are NOT reached by this §4.3 merge — they are un-relocated arrangement verts needing the Stage-0 fix."* |

The Stage-4 loop then MINTS the 4-incidence itself: the mesh enters it clean
(`[nm-edge before-3d] 390 tris, 0 open edge(s), 0 over-2 edge(s)`), the one
§4.4.1(a) simple action splits `N = [140, 141, 38]` at `b = 139`, and `b`
coincides with the long edge's own endpoint `141`, so the "half"
`[139, 141, 38]` is a second needle mirroring the first. Fig-11(a)'s
precondition holds on collinearity (`h/l = 1.35e-15`) but not on the
parameter.

## 3. Design — the producer's generator identity, welded under the existing band

Two pieces, each in the crate that owns the knowledge:

**(a) `cherchi-rs` records the identity (exact, no tolerance).**
`LabeledArrangement` gains

```rust
pub lpi_through_vertex: Vec<(u32, u32)>,
```

ascending, deduplicated `(lpi_out_vert, explicit_out_vert)` pairs: for every
emitted `VertexCoords::Lpi`, each of its generating line's two endpoints that
is ALSO emitted as a `VertexCoords::Explicit` output vertex. The match is
bit-exact on the scaled soup coordinates (the generators and the explicit
vertices are the same `Point3` values — the soup multiplier is a power of two,
so no rounding is involved). No band, no geometry test: this is pure
provenance, the per-VERTEX analog of the existing per-TRIANGLE `source` and
per-EDGE `intersection_edges`. Empty from a producer that does not track it
(the sidecar parity oracle, hand-built fixtures) — the established
`source`/`intersection_edges` contract.

**(b) `yang-rs` applies the band it already owns.** In `boolean()` step (2b),
after the bit-exact weld (and after KV15), union each recorded pair whose two
output positions are within the SAME per-pair KV10 band already used by the
all-planar weld, `TAU_WORK·(1 + max|coord|)`. **Survivor = the EXPLICIT
vertex**, not the minimum index: the explicit vertex is the operand's own
model geometry and the point the arrangement would have emitted had the vertex
been exactly on the plane, and keeping it preserves whatever Stage-1 ring /
rim sample identity it carries (an LPI's coordinates carry none).

### 3.1 Why this is safe where the blanket near-weld was not

The KV10 comment's counter-example for curved operands is "near-coincident but
structurally distinct vertices at ruling-line / tangency junctions (one copy
per incident surface's chord ring)". Those clusters are generated by
*different* surfaces; no member is an LPI whose own generating line ENDS at
another member. The pencil rule is therefore strictly narrower than KV15's
blanket near-weld: membership requires an exact generator incidence, and the
band only confirms that the pierce landed on its own line's endpoint. A
genuine model feature is ≥ `MIN_FEATURE_SIZE` away — six orders beyond the
band — so a transversal pierce of a real edge against a real face is never in
scope.

It is also not the reverted hazard: the M8 holed-disc increment-3 revert
(R0091, Euler → −4, SUPPORTED_WRONG) was an ABSOLUTE `MIN_FEATURE_SIZE`
criterion applied globally in Stage 4. This is scale-relative, provenance-
gated, and at the producer — the two properties that revert recorded as its
own exit condition ("a twin merge must be scale-aware or live at Stage-0").

### 3.2 Branch table

| case | behaviour |
|---|---|
| producer with empty `lpi_through_vertex` (sidecar, fixtures) | byte-identical to today |
| all-planar operands | byte-identical: the KV10 near-weld already fuses the cluster by position; the pencil pass finds its pairs already welded (idempotent) |
| mixed/curved operands, no pencil in band | byte-identical |
| mixed/curved operands with a pencil in band | pencil members weld to the explicit operand vertex; the kept-mesh compaction drops the triangles that become degenerate, exactly as for every other weld |

## 4. Pins

- `crates/cherchi-rs/src/labeling/native.rs` unit test
  `lpi_pencil_through_an_on_plane_vertex_is_recorded` — a cube-corner-on-plane
  fixture: the record names the pencil, and a transversal pierce whose LPI is
  NOT near its line's endpoint is recorded too (the record is tolerance-free)
  while the yang-side band refuses it.
- `crates/yang-rs/src/tests_unit/p0014_lpi_pencil_weld.rs` —
  `pencil_in_band_welds_to_the_explicit_vertex` and
  `pencil_out_of_band_is_left_alone`, driving the weld helper directly on the
  P0014 numbers (coordinate scale 651, separation 2.730e-13, band 6.5e-10).

## 5. Open, recorded rather than changed

- **TPI pencils.** The same degeneracy is possible for a `VertexCoords::Tpi`
  (three planes meeting within the band of an operand vertex). No corpus case
  exhibits it — every sub-band pair in P0014's mesh is an LPI — so the record
  covers LPI only. A TPI pencil would surface as the same loud
  `degenerate_no_longedge` STOP, with `CHERCHI_VERT_PROVENANCE` naming `TPI`
  instead of `LPI`.
- **The §4.4.1(a) `t ≈ 0 | 1` refusal.** Fig-11(a)'s unzip splits a long edge
  at an INTERIOR point; P0014's action split at `t ≈ 1` (the off-vertex
  coincides with an endpoint), which Fig-11(b)/(c) says to MERGE instead. With
  the pencil welded at the producer the situation no longer arises in the
  corpus, so the arm is left as the loud STOP it is rather than being given a
  merge path on zero customers (the "build the general feature, not the
  special case" rule cuts the other way here: a second merge site competing
  with (3b′)/(3c) needs its own customer).
- **The §4.5.2 ladder's inertness on this operand pair** (`b 15116 -> 15116`
  at both rungs) is real and unexplained by this spec: either a legitimate
  per-face segment FLOOR on a 10-tooth gear or a chord-bound plumbing gap in
  `retessellated_at_current_d_eps`. Measured, recorded, NOT changed here — it
  moved zero cases for P0014 either way.
