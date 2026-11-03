# Board Shape Feature (`Operation::BoardShape`)

Status: SPECIFIED 2026-11-03 (Task `board-shape`, Deliverable B1).

## 1. Goal & Scope

Waffle Iron and Boardroom (the companion PCB design suite) share a printed circuit board's mechanical definition: its outer perimeter, interior cutouts, mounting/drilled holes, and component height keepouts. Either side can drive. When Waffle Iron drives, a mechanical author sketches the board geometry on a planar `Sketch` feature and promotes it to a board definition via `Operation::BoardShape`.

This specification covers the **authoring operation, validation, evaluation, document-format plumbing, and bridge exposure** — the self-contained mechanical foundation that does not depend on the shared `board-interface` DXF crate:

- `Operation::BoardShape { params: BoardShapeParams }` in `feature-engine`.
- Strict, analytical 2D validation of entity role assignments (`outline`, `cutout`, `hole`, `keepout`) with a dedicated typed error variant per failure class (`BoardShapeError`).
- Exact 3D solid evaluation (outline extruded by `thickness`, minus cutouts and holes) and deterministic top-face mate connector emission (one connector per hole, keyed by its stable element ID).
- Document persistence in `file-format` (`.waffle` schema golden, migration arm, opaque-operation compatibility) and `wasm-bridge` plumbing.

Publishing to DXF, pulling from `board-interface`, the provenance lock, take-over, MCP agent tools, and the Svelte dialog UI are deliberately out of scope for this slice (see §7 and §8).

---

## 2. Parameters & Wire Format

### 2.1 Operation Variant

`Operation` gains a new tagged variant:

```rust
Operation::BoardShape {
    params: BoardShapeParams,
}
```

On the `.waffle` JSON wire:

```json
{
  "type": "BoardShape",
  "params": {
    "sketch_id": "2c8d55b8-7e07-4b88-981a-9a51bc1091f0",
    "board": "easy2350",
    "thickness": 0.0016,
    "thickness_expr": "pcb_t",
    "roles": {
      "1": { "type": "outline" },
      "2": { "type": "outline" },
      "3": { "type": "outline" },
      "4": { "type": "outline" },
      "5": { "type": "hole", "plated": false, "clearance": 0.0003 },
      "9": { "type": "keepout", "side": "front", "max_height": 0.0025 }
    }
  }
}
```

### 2.2 Parameter Fields (`BoardShapeParams`)

All length quantities are stored in **metres** (`f64`), matching the v2+ unit invariant across `feature-engine` (`waffle_types::kernel::units`). Expression sidecars evaluate in working space (**mm**) through `Dimension::Length` and write metres into their numeric twin (`params.rs`).

| Field | Rust Type | Units / Constraints | Default / Wire |
|---|---|---|---|
| `sketch_id` | `Uuid` | `Feature.id` of an upstream planar `Operation::Sketch` in the same `FeatureTree` (same convention as `ExtrudeParams::sketch_id`). | Required |
| `board` | `String` | Board identifier (e.g. `"easy2350"`). Trimmed text must be non-empty. | Required |
| `thickness` | `f64` | Board substrate thickness in metres. Must be finite and `>= MIN_FEATURE_SIZE` (`1e-6` m). | Required |
| `thickness_expr` | `Option<String>` | Optional driving expression (`Dimension::Length`, mm → metres). Re-evaluated by `params::apply_parameters_scoped` and enumerated in `params::expression_sites` under `"thickness"`. | `None` (omitted when `None`) |
| `roles` | `BTreeMap<u32, BoardEntityRole>` | Map from sketch entity ID (`SketchEntity::id()`) to its board role. Unassigned entities in the sketch (endpoint `Point`s, arc/circle centres, construction centrelines, reference curves) carry no board role and are ignored. Serialized as a JSON object with decimal `u32` string keys; non-`u32` or duplicate keys fail deserialization loudly. | Required |

### 2.3 Entity Roles (`BoardEntityRole` and `BoardSide`)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardEntityRole {
    /// Segment of the single closed outer board perimeter (`Line`, `Arc`, or lone `Circle`).
    Outline,
    /// Segment of an interior closed board cutout (`Line`, `Arc`, or `Circle`).
    Cutout,
    /// Circular drilled hole (`Circle` entity only).
    Hole {
        #[serde(default)]
        plated: bool,
        /// Copper-to-hole-edge clearance in metres (finite, `>= 0.0`).
        #[serde(default)]
        clearance: f64,
    },
    /// Segment of a closed component placement keepout (`Line`, `Arc`, or `Circle`).
    Keepout {
        side: BoardSide,
        /// Maximum component height allowed inside the keepout in metres
        /// (finite, `>= 0.0`; `0.0` forbids components of any height).
        max_height: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardSide {
    Front,
    Back,
    Both,
}
```

**Circle & Hole Diameter Note (`SUITE-SPEC-AGREED.md`):** In Waffle Iron sketches, `SketchEntity::Circle` stores `radius` in metres. When evaluated into `EvaluatedBoardShape` (and when later serialized to `board-interface`), each hole records both `radius_m: f64` and `diameter_m: f64 = 2.0 * radius_m` (exact power-of-two scaling in IEEE-754 floating point, zero rounding error).

---

## 3. Stable Element IDs

Every board element authored in Waffle Iron carries a deterministic, human-readable stable ID derived from the **sketch feature's UUID** (`Feature.id`, i.e. `params.sketch_id` — not `Sketch.id`) and the **sketch entity's `u32` ID**:

```text
waffle:<sketch_feature_uuid>/<entity_id>
```

Example: `waffle:2c8d55b8-7e07-4b88-981a-9a51bc1091f0/5`.

Why `<sketch_feature_uuid>` rather than `Sketch.id`:
- Every cross-feature reference in `feature-engine` (`ExtrudeParams::sketch_id`, `RevolveParams::sketch_id`, `PipeParams::sketch_id`, `SweepParams::sketch_id`, `FeatureTree` lookups, MCP `feature_get`) addresses a sketch by its `Feature.id`.
- Within a sketch, `SketchEntity` IDs (`u32`) are monotonic and never reused on edit, so editing a dimension, dragging a corner, or adding a new hole leaves existing `waffle:<sketch_feature_uuid>/<entity_id>` keys unchanged.

*Future take-over compatibility:* When `board-interface` pull and take-over land in a later task, foreign elements brought into a Waffle sketch (`composer:<uuid>`, `kicad:<uuid>`) will retain their arrival IDs via an optional side-table (`provenance_ids: BTreeMap<u32, String>`, defaulting to empty), falling back to `waffle:<sketch_feature_uuid>/<entity_id>` for any newly drawn entity. No dead field is added prematurely in this slice.

---

## 4. Validation Rules & Branch Table

Validation runs over the current solved/reprojected `Sketch` before any kernel call. Endpoints of open curves (`Line`, `Arc`) weld when they share a `Point` ID, are linked by `SketchConstraint::Coincident` within `MIN_FEATURE_SIZE` (`1e-6` m), or lie within `TAU_MODEL` (`1e-7` m, matching `kicad_pcb::OUTLINE_WELD_M`). Gaps larger than `TAU_MODEL` are reported with their exact size and never silently bridged (Constitution P9/P10).

### 4.1 Branch Table

| # | Condition | Outcome / Error |
|---|---|---|
| B1 | `sketch_id` missing from `FeatureTree`, not an `Operation::Sketch`, or suppressed / rolled back / failed upstream | `Err(EngineError::SketchNotFound { id })` |
| B2 | Referenced sketch has `SolveStatus::OverConstrained` or `SolveStatus::SolveFailed` | `Err(BoardShapeError::UnsolvedSketch { sketch_id, reason })` |
| B3 | Sketch plane normal is zero/non-finite, or `plane_x_axis` is zero/non-finite/parallel to normal | `Err(BoardShapeError::InvalidSketchPlane { reason })` |
| B4 | `board.trim().is_empty()` | `Err(BoardShapeError::EmptyBoardName)` |
| B5 | `!thickness.is_finite() \|\| thickness < MIN_FEATURE_SIZE` (`1e-6` m) | `Err(BoardShapeError::InvalidThickness { thickness })` |
| B6 | `roles` contains an `entity_id` not present in `sketch.entities` | `Err(BoardShapeError::UnknownEntity { entity_id })` |
| B7 | `roles` assigns a role to a construction entity (`construction == true`) | `Err(BoardShapeError::ConstructionEntity { entity_id, role })` |
| B8 | `roles` assigns a role to `Point`, `Spline`, `Gear`, or `Sprocket` (board interface curves are strictly lines, circular arcs, and circles; splines are never chord-approximated) | `Err(BoardShapeError::UnsupportedEntityKind { entity_id, kind, role })` |
| B9 | `Hole` assigned to a `Line` or `Arc` (not a `Circle`) | `Err(BoardShapeError::HoleNotCircle { entity_id, kind })` |
| B10 | `Hole.clearance` is negative or non-finite | `Err(BoardShapeError::InvalidHoleClearance { entity_id, clearance })` |
| B11 | `Keepout.max_height` is negative or non-finite | `Err(BoardShapeError::InvalidKeepoutHeight { entity_id, max_height })` |
| B12 | Assigned `Line`, `Arc`, or `Circle` has missing/non-finite point coordinates, line length `<= TAU_MODEL`, circle/arc radius `< MIN_FEATURE_SIZE / 2.0`, coincident arc endpoints (`<= TAU_MODEL`), or non-concentric arc endpoints (`\|r_start - r_end\| > MIN_FEATURE_SIZE`) | `Err(BoardShapeError::DegenerateEntity { entity_id, reason })` |
| B13 | Zero entities carry `BoardEntityRole::Outline` | `Err(BoardShapeError::MissingOutline)` |
| B14 | `Outline` entities have a degree-1 welded endpoint (open contour) | `Err(BoardShapeError::OutlineNotClosed { gap_m, at })` |
| B15 | `Outline` entities have a welded vertex with `degree > 2` (T-junction / figure-eight shared vertex) | `Err(BoardShapeError::OutlineBranching { at, degree })` |
| B16 | `Outline` entities form `count >= 2` closed loops (disjoint panel or nested loop marked `Outline`) | `Err(BoardShapeError::MultipleOutlines { count })` |
| B17 | Any `Outline`, `Cutout`, or `Keepout` loop self-intersects (non-adjacent segments cross/touch, adjacent segments backtrack/overlap, or net area `<= TAU_MODEL²`) | `Err(BoardShapeError::SelfIntersectingLoop { role, entity_ids })` |
| B18 | `Cutout` entities have a degree-1 welded endpoint | `Err(BoardShapeError::CutoutNotClosed { gap_m, at })` |
| B19 | `Cutout` entities have a welded vertex with `degree > 2` | `Err(BoardShapeError::CutoutBranching { at, degree })` |
| B20 | A `Cutout` loop lies outside `Outline` or touches/crosses `Outline` (`dist <= TAU_MODEL`) | `Err(BoardShapeError::CutoutOutsideOutline { entity_ids })` |
| B21 | Two `Cutout` loops cross/touch each other (`dist <= TAU_MODEL`) or one nests inside the other | `Err(BoardShapeError::CutoutsOverlapOrNested { first_entity_ids, second_entity_ids })` |
| B22 | A `Hole` circle's centre is not strictly inside `Outline` or its boundary touches/crosses `Outline` (`dist(center, outline) <= radius + TAU_MODEL`) | `Err(BoardShapeError::HoleOutsideOutline { entity_id })` |
| B23 | A `Hole` circle lies inside, encloses, or touches/crosses a `Cutout` loop (`dist(center, cutout) <= radius + TAU_MODEL`) | `Err(BoardShapeError::HoleOverlapsCutout { hole_entity_id, cutout_entity_ids })` |
| B24 | Two `Hole` circles touch or overlap (`dist(c_a, c_b) <= r_a + r_b + TAU_MODEL`) | `Err(BoardShapeError::HolesOverlap { first_entity_id, second_entity_id })` |
| B25 | `Keepout` entities (grouped by `(side, max_height)`) have a degree-1 welded endpoint | `Err(BoardShapeError::KeepoutNotClosed { gap_m, at })` |
| B26 | `Keepout` entities (grouped by `(side, max_height)`) have a welded vertex with `degree > 2` | `Err(BoardShapeError::KeepoutBranching { at, degree })` |
| B27 | Valid board with 1 closed `Outline`, 0..N interior `Cutout`s, 0..M interior `Hole`s, 0..K closed `Keepout`s | Evaluates 3D board solid (`OutputKey::Main`), `EvaluatedBoardShape`, and `M` top-face `PartConnector`s |

*Note on Keepout placement:* Unlike cutouts and drilled holes (which remove substrate material and must lie strictly inside the board perimeter), keepouts are closed 2D height-restriction zones and are deliberately allowed to overlap or overhang the board perimeter (e.g. edge-launch connectors or enclosure wall keepouts), matching real ECAD/MCAD workflows and the brief's rule ("one closed outer outline loop; cutouts closed and inside it; holes are circles; keepouts closed"). Grouping `Keepout` entities by `(side, max_height)` before chaining allows front and back keepouts or distinct height zones in the same sketch to share an edge without false branching refusals.

### 4.2 Typed Error Plumbing

`BoardShapeError` derives `Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error` with `#[serde(tag = "type", rename_all = "snake_case")]`.
- `EngineError` gains `BoardShape(#[from] BoardShapeError)`.
- `ErrorKind` gains `BoardShape { error: BoardShapeError }`, mapped in `From<&EngineError> for ErrorKind`.
- Both Rust callers and JSON/MCP/UI hosts inspect the structured `BoardShapeError` variant directly and never parse human error strings.

---

## 5. Evaluation Semantics

### 5.1 Board Solid (`OutputKey::Main`)

`Operation::BoardShape` is a standalone solid-producing feature (`CombineMode::NewBody` semantics: it consumes no upstream solids and participates normally as a body for downstream extrudes, fillets, patterns, booleans, and `UnionAll`).

Given sketch plane `origin`, unit `normal`, and unit in-plane `x_axis` (`y_axis = normal × x_axis`):

1. **Outer substrate:** The validated `Outline` loop is oriented counter-clockwise (`is_outer = true`) and converted to a kernel-ready `ClosedProfile` — a `CircleProfile` when the outline is a lone `Circle`, or an ordered vertex loop with `ArcSegment` metadata (16 interior synthetic arc samples per arc, identical to `waffle_types::profiles::build_finish_profiles`) so `kernel-v2` constructs exact analytical cylindrical faces for arc corners. The face at `origin` is extruded along `+normal` by `thickness`, spanning `[0, +thickness]`.
2. **Cutouts and holes:** Every `Cutout` loop (ordered deterministically by minimum `entity_id`) and every `Hole` circle (ordered by ascending `entity_id`) is converted to its own kernel-ready `ClosedProfile` (`CircleProfile` for circles/holes; CCW loop + `ArcSegment`s for line/arc cutouts) and subtracted sequentially via `modeling_ops::execute_boolean(..., BooleanKind::Subtract)`.
   - **Coplanar-cap avoidance (`kicad.rs:451-457` invariant):** Cutting a `[0, +t]` slab with a `[0, +t]` tool makes both cutter caps coplanar with the board's top and bottom faces, triggering Yang Stage-0's loud coplanar refusal on line+arc cutters. Each cutter is therefore placed at `origin - thickness * normal` and extruded along `+normal` by `3.0 * thickness`, spanning `[-t, +2t]` (symmetric clearance `t` outside both board caps).
   - **Deterministic per-element face PIDs:** Each sub-extrude sets a deterministic `ConstructSeed` hashed from `(feature.id, element_stable_ids)` so adding or removing a hole does not shift persistent face IDs of other holes or the outer board.
3. **Keepouts:** Keepouts validate and record into `EvaluatedBoardShape::keepouts`; they do not cut the 3D solid.
4. **Uniform semantic face roles:** On the final solid, planar faces with outward normal `+normal` receive `Role::EndCapPositive`, planar faces with `-normal` receive `Role::EndCapNegative`, and lateral/hole/cutout walls receive `Role::SideFace { index }`, keeping `EndCapPositive` / `EndCapNegative` invariant whether the board has zero holes or many.

### 5.2 Hole Mate Connectors & `EvaluatedBoardShape`

For each `Hole` entity (in ascending `entity_id` order), evaluation produces a top-face connector matching KiCad's mounting-hole convention (`crates/feature-engine/src/kicad.rs:545-566`):

- **Stable key:** `stable_id = "waffle:<sketch_id>/<entity_id>"`.
- **3D Frame (in Part coordinates):**
  - `origin = plane_origin + center_u * x_axis + center_v * y_axis + thickness * normal` (centre of the hole on the board's top `+normal` face).
  - `z_axis = normal`
  - `x_axis = x_axis`
- **Connector identity (`PartConnector`):**
  - `PartConnector` gains `pub key: Option<String>` (`None` for ordinary `MateConnector` and `Script` features; `Some(stable_id)` for board-shape hole connectors), plumbed to `PartConnectorInfo::key` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
  - Because `assembly_view` and MCP `connector_add` address a part's `PartConnector` by `PartConnector::feature_id: Uuid`, giving multiple holes on one `BoardShape` the same `feature_id` would collide. Each hole connector therefore gets a deterministic 128-bit FNV-1a UUID derived from `(board_shape_feature.id, stable_id)` (same pattern as `assembly_view::step_product_tree`), making every hole individually addressable in assemblies across rebuilds.
  - `name`: `format!("{} hole {entity_id}", params.board.trim())`.
  - `geometry`: `Some(ConnectorGeometry::CircularEdge(AxisKind::Cylindrical))`.

**Engine storage & incremental rebuild:**
- `RebuildState` and `Engine` gain `pub board_shapes: HashMap<Uuid, EvaluatedBoardShape>`, carried forward across partial rebuilds when an unchanged `BoardShape` feature is not re-executed (mirroring `Engine::sketch3d` and `Engine::script_connectors`).
- `EvaluatedBoardShape` exposes:
  - `board: String`, `sketch_id: Uuid`, `thickness_m: f64`,
  - `outline: EvaluatedBoardLoop` (`entity_ids`, `stable_ids`, `segments`, `area_m2`),
  - `cutouts: Vec<EvaluatedBoardLoop>`,
  - `holes: Vec<EvaluatedBoardHole>` (`entity_id`, `stable_id`, `connector_id`, `center_uv`, `radius_m`, `diameter_m`, `plated`, `clearance_m`, `frame`),
  - `keepouts: Vec<EvaluatedBoardKeepout>` (`entity_ids`, `stable_ids`, `loop_2d`, `side`, `max_height_m`),
  - `connectors: BTreeMap<String, PartConnector>` (keyed directly by `stable_id`).
- At the end of `Engine::rebuild_once`, every active, unsuppressed, successfully built `BoardShape` feature appends its hole `PartConnector`s (in `BTreeMap` / ascending `entity_id` order) to `Engine::connectors`, so `ModelUpdated.connectors` and `AssemblyStatus.part_connectors` surface them automatically.
- `depends_on_tree_position` returns `true` for `Operation::BoardShape` when its referenced `Sketch` carries `projected` geometry, matching `Extrude` / `Revolve` / `Pipe` / `Sweep`.

---

## 6. Document Format Impact (`crates/file-format`)

### 6.1 Does `Operation::BoardShape` Require a `FORMAT_VERSION` or `MIN_READER_VERSION` Bump?

**No.** Both `FORMAT_VERSION` and `MIN_READER_VERSION` in `crates/file-format/src/save.rs` stay at **`15`**.

**Why:** Just as `crates/file-format/src/sources.rs` preserves unknown `SourceKind`s (`:182-205`) and `Locator`s (`:368-381`) verbatim via `feature_engine::opaque::known_or_unknown`, `Operation` itself has used `crate::opaque::known_or_unknown` since v4 Phase 1b (`crates/feature-engine/src/types.rs:684-692, 804-817`; `specs/waffle_v4_document_model.md` §2.5; pinned by `crates/feature-engine/tests/opaque_operation.rs`):

- An older reader whose `KnownOperation` enum lacks `BoardShape` deserializes `{"type": "BoardShape", "params": ...}` into `Operation::Unknown(serde_json::Value)`.
- On re-save, `Operation::Unknown` re-emits the original JSON object byte-for-byte.
- On rebuild, only that feature fails loudly with `EngineError::UnsupportedOperation { type_tag: "BoardShape" }`; every other feature and tab in the document loads and rebuilds normally.
- Conversely, a malformed `BoardShape` payload on a reader that *does* know `"BoardShape"` (`OPERATION_TAGS` contains `"BoardShape"`) is rejected as a loud parse error rather than silently falling through to `Operation::Unknown`.

This follows the exact precedent set by `Operation::Sketch3d` (`10f7f847`) and `Operation::Sweep` (`9c00a0b9`).

### 6.2 Required Format & Plumbing Updates

1. `crates/feature-engine/src/types.rs`: add `BoardShape` to `Operation`, `KnownOperation`, `OPERATION_TAGS` (`18` → `19`), `From<KnownOperation> for Operation`, and `Operation::type_tag()`.
2. `crates/feature-engine/tests/opaque_operation.rs`: update the `OPERATION_TAGS.len()` pin (`18` → `19`).
3. `crates/file-format/src/migrate.rs`: add no-op arm `Operation::BoardShape { .. } => {}` in `migrate_feature_v1_to_v2` (`BoardShape` postdates v15; lengths are already metres).
4. `docs/schema/waffle-v5.schema.json`: regenerate via `UPDATE_SCHEMA=1 cargo test -p file-format --features json-schema --test schema_golden` so the committed JSON Schema golden includes `BoardShape`, `BoardShapeParams`, `BoardEntityRole`, and `BoardSide` (purely additive diff).
5. `crates/wasm-bridge/src/dispatch.rs`: add `Operation::BoardShape` arm to `default_feature_name`; plumb `PartConnector::key` into `PartConnectorInfo` in `crates/wasm-bridge/src/messages.rs`.
6. `crates/test-harness/src/{assertions.rs,report.rs}`: add `Operation::BoardShape` arms to exhaustive operation-name / report formatters.

---

## 7. Invariants, Oracles & Analytical Method Justification

### 7.1 Invariants

1. **Exact Net Volume Invariant:** For any valid `BoardShape` evaluated on `KernelV2Adapter`, `KernelIntrospect::solid_volume(&handle)` equals:
   $$V_{\text{exact}} = \left(A_{\text{outline}} - \sum_{i} A_{\text{cutout}, i} - \sum_{j} \pi r_j^2\right) \cdot t$$
   within relative tolerance `1e-9`, where $A_{\text{outline}}$ and $A_{\text{cutout}, i}$ are the exact analytical planar areas (polygon shoelace plus circular-segment $\frac{1}{2} r^2 (\theta - \sin\theta)$ per arc, $\pi R^2$ for a circle).
2. **Topological Genus Invariant:** A valid board with $C$ interior cutouts and $H$ drilled through-holes is a single watertight manifold solid with Euler characteristic $\chi = 2 - 2(C + H)$.
3. **Stable Connector Identity Invariant:** Evaluating a board with $H$ holes produces $H$ `PartConnector`s in deterministic order, each keyed by `waffle:<sketch_id>/<entity_id>`, with deterministic `feature_id` UUIDs that survive full and incremental rebuilds and save/load round-trips.
4. **Loud Refusal Invariant:** Every invalid role assignment in §4.1 (B1–B26) returns `Err` with its specific typed variant and emits zero partial solids and zero connectors.

### 7.2 §7a Analytical vs. Approximate Method Justification (FIP §3.2)

All 2D validation and area calculations in `board_shape.rs` are **100% analytical** over `{Line, Arc, Circle}` — no chord-polyline approximation is used for containment, clearance/overlap, or area:

- **Curve–curve intersection** (self-intersection, cutout-vs-outline, cutout-vs-cutout): solved analytically in closed form via 2×2 linear intersection (line–line), quadratic projection onto circle with exact arc-sweep membership (line–arc / line–circle), and radical-axis intersection with exact angular interval overlap (arc–arc / arc–circle / circle–circle).
- **Point-to-curve distance** (hole-vs-outline, hole-vs-cutout, boundary contact): exact segment projection for `Line`, exact $|\|p - c\| - R|$ for `Circle`, and exact angular-sector dispatch ($|\|p - c\| - R|$ inside sweep else endpoint minimum) for `Arc`.
- **Point-in-loop containment**: exact analytical winding number $\frac{1}{2\pi} \sum \Delta\theta = \pm 1$ vs $0$. Each `Line` contributes $\operatorname{atan2}((a-p)\times(b-p), (a-p)\cdot(b-p))$; each `Arc` is split at its midpoint into sub-arcs of sweep $< \pi$, combining the chord angle with the exact $\pm 2\pi$ circular-segment lune test ($\|p - c\| < r$ on the bulge side of the chord, and $\pm\pi$ when $p$ lies on the open chord). This eliminates both polygon-chord error near curved boundaries and horizontal ray-grazing singularities at vertices/tangents.
- **Loop area**: exact shoelace sum over segment endpoints plus exact circular-segment correction $\pm\frac{1}{2} r^2 (\theta - \sin\theta)$ per `Arc` (and $\pi r^2$ for `Circle`).
- **3D B-Rep construction**: circles and holes pass `CircleProfile` and arcs pass `ArcSegment` metadata to `make_faces_from_profiles`, so `kernel-v2` builds analytical `SurfaceGeom::Cylindrical` faces rather than faceted prisms.

---

## 8. Deliberately Deferred (Out of Scope for Task `board-shape`)

1. **`board-interface` crate & DXF I/O:** Reading/writing the R2000 board-interface `.dxf` (`BOARD`..`KEEP_BOTH` layers, `1001 = BOARD_INTERFACE` XDATA, `00000000` header, `content_hash`) belongs to the shared `board-interface` crate being built in Boardroom's repo.
2. **Document source & publishing (`SourceKind::BoardInterface`, `board_publish`):** Exporting `EvaluatedBoardShape` to `<board>.board.dxf` and recording provenance/hashes waits for `board-interface`.
3. **Pulling, locking, and taking over (`ProvenanceOrigin`, lock enforcement, take-over):** Generating a read-only locked `Sketch` + `BoardShape` from a pulled `.board.dxf`, refusing edits while locked, and severing provenance on take-over while preserving foreign `composer:<uuid>` / `kicad:<uuid>` element IDs are deferred to the integration slice once `board-interface` lands.
4. **MCP agent tool & Svelte UI (Brief B3):** Adding `BoardShape` to `wasm-bridge` agent authoring tools, `agent-tools.manifest.json`, and the Svelte feature dialog / entity-role picker is out of scope for this task and catalogued in `docs/agy/board_shape_summary.md`.
