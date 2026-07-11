# Spec: Chain select + chain offset (sketch), project-with-offset (model → sketch)

Driver: import a KiCad STEP board, project its outline into a sketch, offset
it by 0.5 mm, and extrude a 3D-printed housing. Generalizes to any connected
run of sketch geometry (drawn or projected).

## Goal

1. **Chain select** — one gesture selects an entire connected run of sketch
   entities (lines/arcs/splines sharing endpoints), instead of shift-clicking
   each segment.
2. **Offset tool** — create a parallel copy of a selected chain (open or
   closed) or circle at an exact signed distance, as regular (extrudable)
   sketch geometry.
3. **Project-with-offset** — with the Offset tool active, picking model
   geometry (body face/edge, e.g. an imported STEP body) projects it into the
   sketch (existing `projectRef` path) and immediately seeds the offset from
   the projected chain.
4. **`projectFace` curved edges** (SI3 slice) — face projection currently
   skips curved in-plane boundary edges, leaving gaps at e.g. rounded board
   corners. Project them as static construction polylines that SHARE the
   bound corner points, so the projected boundary is one connected loop.

## Concepts & Data Model

No Rust/engine changes. All new geometry is plain sketch entities
(`Point`/`Line`/`Arc`/`Circle`) added via `addLocalEntity`, so solver,
persistence, profiles, and extrude pick them up for free.

### Chain connectivity (`app/src/lib/sketch/chain.js`, pure)

- Connector endpoints: Line `start_id`/`end_id`; Arc `start_id`/`end_id`
  (center is NOT a connector); Spline first/last of `point_ids`. Circle,
  Point, Gear members: not chainable (singleton chains).
- Two entities connect iff they share an endpoint id OR their endpoint
  positions coincide within `CHAIN_WELD_TOL = 1e-6` m (projected loops mix
  bound corner ids and static polyline points; proximity welding joins them).
- `findConnectedChain(id, entities, positions)` → entity ids (BFS).
- `orderChain(ids, entities, positions)` → `{ items: [{id, reversed}],
  closed }` or `{ error: 'branching' | 'disconnected' }`. A weld node with
  more than 2 chain edges is branching → offset refuses (toast); chain
  SELECT still selects the whole connected component.

### Offset math (`app/src/lib/sketch/offset.js`, pure)

Traversal segments: `{type:'line', p0, p1}` (traversal direction),
`{type:'arc', center, r, a0, a1, ccw}` (traversal from a0 to a1 in `ccw`
sense), `{type:'circle', center, r}`.

- Sign convention: `d > 0` offsets to the LEFT of traversal. UI derives the
  sign from the cursor side, so users never see the convention. Select-first
  + typed value: closed chains are normalized CCW and positive means
  OUTWARD; open chains: positive = left of the first entity's direction.
- Line → parallel line at `d`. Arc (CCW traversal) → same center/angles,
  `r' = r − d`; CW → `r' = r + d`. Circle → `r' = r + d` (outward positive).
  Any `r' ≤ RADIUS_EPS` → typed failure `radius-collapse` (toast, no
  geometry) — no silent dropping (P9).
- Joints between consecutive offset segments (incl. last→first when closed),
  with `E` = end of offset seg i, `S` = start of offset seg i+1, `J` = the
  source joint position, `turn = cross(dir_out_i, dir_in_i+1)`:
  - `|E−S| < JOINT_WELD_TOL` → weld (tangent joints: fillets, slots).
  - Outside corner (`turn·d < 0`): true-offset corner arc centered at `J`,
    radius `|d|` (E and S always lie on that circle), sweep ≤ π. Exception:
    line-line with turn angle < `MITER_MAX = 30°` → miter intersection
    (keeps projected polylines at 1 entity/segment instead of 2).
  - Inside corner (`turn·d > 0`): trim/extend to the analytic intersection
    (line-line / line-circle / circle-circle) nearest `J`; if none exists
    (rare degenerate), weld at midpoint(E, S).
- Global self-intersection removal (offset > local feature size) is OUT OF
  SCOPE v1 — documented failure mode, same as SolveSpace.

## Branch Table

| # | State | Gesture | Outcome |
|---|---|---|---|
| 1 | Select tool | double-click entity | selection = connected chain of that entity |
| 2 | Select tool | shift+double-click entity | selection ∪= chain |
| 3 | Select tool | double-click gear entity | unchanged: gear edit dialog (checked BEFORE chain) |
| 4 | Offset tool, no chain | hover entity | chain ghost highlight (line-segments preview) |
| 5 | Offset tool, no chain | click entity | capture chain → live offset preview follows cursor |
| 6 | Offset tool, no chain | click hovered body Edge/Face | projectRef → captured projected entities become the chain (branch 5) |
| 7 | Offset tool, chain armed | move | preview at cursor's side/distance |
| 8 | Offset tool, chain armed | click | popup pre-filled with |current distance|; Enter commits at typed value on the clicked side |
| 9 | Offset tool, chain armed | Escape / tool switch | resetTool clears chain, no geometry |
| 10 | Offset tool | click branching chain | toast "Offset needs a simple chain (no branches)", stays unarmed |
| 11 | Offset tool | click circle | branch 5 with circle semantics (cursor outside → grow) |
| 12 | Select tool, chain selected | press `o` / Offset button | offset tool seeded from selection → branch 7 |
| 13 | any | commit with `r' ≤ 0` arc | toast radius-collapse, chain stays armed |

## Invariants

- **O1 — Offset output is ordinary geometry**: non-construction
  Points/Lines/Arcs/Circles, fresh point ids (never shared with source),
  one undo action.
- **O2 — Arc entity convention preserved**: created arcs are CCW
  `start_id → end_id` (traversal-CW segments swap endpoints on creation).
- **O3 — Chain select never mutates**: selection only.
- **O4 — Projected loops stay connected**: after `projectFace` on a face
  with curved boundary edges, the projected boundary is ONE chain
  (curved runs share the bound corner points with straight neighbors).
- **O5 — Tangent chains offset watertight**: a rounded-rectangle chain
  (4 lines + 4 tangent arcs) offsets to exactly 8 entities, all joints
  welded (no corner arcs).
- **I4 (cycle 2) still holds**: select-first and tool-first offset produce
  identical geometry for the same chain and distance.

## Oracles (GUI, real pointer events for tool flows)

- Pure branch coverage via `__waffle.findConnectedChain` /
  `__waffle.computeChainOffset` over live drawn geometry (pattern:
  dimension-heuristic.spec.js).
- E2E: draw rectangle → double-click chain-selects 4 lines; offset outward
  0.5 → 4 new lines + 4 corner arcs, vertices at expected coords; extrude
  of the offset profile succeeds.
- E2E: slot (tangent chain) offset → welded joints, entity count 8, arc
  radii r±d.
- E2E: circle offset grows/shrinks by cursor side.
- Regression canary: sketch-drawing-regression.spec.js before commit.

## Failure Modes

- Offset larger than local feature size self-intersects (v1 known limit).
- Static projected polylines (curved edges) go stale if the source body is
  repositioned after projection — same limitation as existing curved-edge
  projection (documented in projected_sketch_geometry.md).
- Splines: chainable for SELECT, refused by OFFSET v1 (toast).

## Implementation Plan (each its own increment)

1. `chain.js` + `offset.js` pure modules + `__waffle` exposure.
2. Chain select in `handleSelectTool` double-click path (after gear check).
3. Offset tool state machine + commit + Toolbar/shortcut/preview rendering.
4. Select-first seeding (`seedOffsetFromSelection`) + body-ref capture
   (branch 6).
5. `projectFace` curved-edge polylines (O4).
6. GUI specs `sketch-chain-select.spec.js`, `sketch-offset.spec.js`.

# Cycle 2 (2026-07-11, task #140): explicit body-geometry chains

User asks: offset must apply to any projectable geometry; chain selection
must be explicit for Project and Offset; Project must work on faces
tool-first.

## Behavior

- **Chain by default, explicitly previewed.** Hovering body geometry with
  Project or Offset ghosts EXACTLY what a click will act on and states it in
  the status bar (`setToolHint`): a body edge expands to its connected
  coplanar edge chain (`bodyChain.js`: endpoint-welded BFS gated to edges
  whose points lie within CHAIN_PLANE_TOL of the seed edge's plane parallel
  to the sketch — a board outline chains, a box wireframe does not); a face
  ghosts its boundary (`faceBoundaryPreview`). **Alt-click limits an edge
  pick to the single hovered edge.** Hovering a sketch entity with Offset
  ghosts the sketch chain as before (Alt: single entity).
- **Offset on body geometry**: clicking a hovered body edge chain (or face)
  with Offset projects it (construction, bound corners) and immediately arms
  the offset on the projected chain. Root enabler: `isBodyPickingEnabled`
  previously allowed body hover only for select/project in sketch mode — the
  offset tool never saw a body ref at all.
- **Tool-first faces**: face refs only resolve reliably in
  `CadModel.handleClick` (Threlte raycast at click time; the tools'
  window-pointerdown handlers see a stale/absent face hover). handleClick now
  delegates to `handleBodyFaceClick(ref)` in tools.js — Project projects the
  face boundary, Offset projects-and-arms — before any selection happens.
  The tools' own pointerdown paths deliberately ignore Face refs.
- **Multi-edge chains project connected**: `projectEdgeChain` (store) shares
  bound corner points across the ranges (same corner allocator as
  projectFace, now factored into `projectEdgeRange`). Single edges keep the
  classic `projectRef` path so bindings stay byte-identical to the
  select-first Proj button (invariant I4).

## Oracles

- `project-offset-body-chain.spec.js`: edge-chain ghost + hint text, chain
  click (4 lines, one connected chain), Alt-click single edge, tool-first
  face projection (click consumed, nothing selected), offset arm on body
  edge chain + commit (4 lines + 4 arcs real), offset arm on face click.
- `projection-select-first.spec.js` O6 updated to the chain-by-default
  contract (F2 straight-on hover oracle unchanged).
