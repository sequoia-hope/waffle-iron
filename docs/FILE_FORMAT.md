# The `.waffle` File Format — Specification

**Format version: 3** (`FORMAT_VERSION`, `crates/file-format/src/save.rs`)
**Spec written:** 2026-08-28, from the code as it exists on `main`. This document is
*descriptive of the current implementation*, not aspirational: every claim below was
verified against the source (file:line references throughout) and against real files
(`sketch.waffle`, `err.waffle`, `minihexa.waffle`, and the 312-case assay corpus in
`app/tests/cases/assay/`, all of which are version 3).

**Supersedes** `projects/09-file-format/ARCHITECTURE.md`, which describes format v1
and contains claims that are no longer (or never were) true — see §14.

---

## 1. Overview

A `.waffle` file is a UTF-8 JSON document that stores the **parametric recipe** of a
Waffle Iron model: an ordered feature tree of operations (sketches, extrudes,
revolves, booleans, datum planes, imported bodies) plus the persistent geometry
references (`GeomRef`) that tie them together. On load, the model is **rebuilt from
the recipe** by replaying the feature tree through the kernel.

The design intent (recipe, not geometry) is mostly upheld, with deliberate and
accidental exceptions that this spec documents honestly:

- **Stored:** features, operation parameters, sketch entities and constraints,
  GeomRefs, document/tab structure, body-name overrides, display unit, metadata,
  and (since 2026-08-31) design parameters — the tree-level `parameters` table of
  named variables plus optional `expression` strings on dimension constraints and
  on extrude `depth_expr` / revolve `angle_expr` / datum `distance_expr` (all
  serde-defaulted, purely additive; the driven numeric field always carries the
  last evaluated value, so pre-parameters readers still see correct geometry).
  Expressions evaluate in mm-space: bare numbers are millimeters for lengths and
  degrees for angles, independent of the display unit
  (`specs/parameterized_designs.md`).
- **Also stored (derived data — see §10):** sketch `solve_status`,
  `solved_positions`, `solved_profiles`, region boundary tessellations inside
  extrude params, and optional per-tab preview meshes. These are performance/
  bridge conveniences that leak into the file; loaders must treat them as
  *hints that may be stale* (the engine recomputes them only when they are absent).
- **Also stored (payloads):** imported STEP bodies embed the full source STEP text,
  deflate-compressed and base64-encoded (§11), so files are self-contained.
- **Never stored:** B-Rep solids, kernel handles/ids, tessellated render meshes
  (other than the optional ≤500-triangle preview), undo history.

One file = one **document** = one or more **tabs**, each tab holding an independent
part (its own feature tree). File extension `.waffle`; also accepted with `.json`
by the app's file picker.

---

## 2. Encoding and conventions

| Aspect | Rule |
|---|---|
| Container | JSON, UTF-8. The Rust writer pretty-prints; the JS writer (`buildDocumentJson`) emits compact JSON. Both are valid — **whitespace is not significant** and consumers must not rely on it. |
| Units | **All lengths in meters** (since v2). All angles in **degrees** (`angle`, `value_degrees`, `rotation_deg`, `pressure_angle_deg`). Direction vectors are unitless. |
| Display unit | `display_unit` is a UI preference only (`"mm"`, `"cm"`, `"m"`, `"in"`, `"ft"`); it never changes stored values, which stay meters. |
| IDs — features/sketches/tabs/datums | UUIDs, serialized as lowercase hyphenated strings. **Exception:** `Tab.id` and `active_tab` are free-form strings (the UI has historically emitted `"default"`); they must only be equal-comparable, not parseable (`crates/file-format/src/metadata.rs`). |
| IDs — sketch entities | `u32`, unique *within one sketch*. |
| Built-in datum planes | Fixed well-known UUIDs (`app/src/lib/engine/planes.js`): Front `00000000-0000-0000-0000-000000000001`, Top `…0002`, Right `…0003`. |
| Timestamps | RFC 3339 / ISO-8601 UTC strings (chrono `DateTime<Utc>` serde), e.g. `"2026-07-05T01:21:04.049Z"`. |
| Floats | IEEE-754 doubles. serde_json and `JSON.stringify` both round-trip f64 exactly (shortest-representation printing). **Hazard:** a non-finite value (NaN/∞) serializes as `null` in both writers and then **fails to load** (`null` is not a valid f64 for serde). Guarded since 2026-08-28: the bridge save path self-verifies (`save_project_verified`) and errors loudly instead of emitting an unloadable file (§14.10). |
| Tuples | Rust `(f64, f64)` serializes as a 2-element array `[x, y]`. Fixed arrays `[f64; 3]` as 3-element arrays. |
| Maps with u32 keys | JSON objects with **stringified** keys (`"12": [x, y]`) via the `u32_key_map` helper (`crates/waffle-types/src/sketch.rs:10`). |
| Enums | All persisted enums are **internally tagged**: `#[serde(tag = "type")]` (one exception: `RegionEdge` uses `tag = "kind"`, and `PlaneDefinition` uses `tag = "method"` with renamed variants). An unknown tag value is a **hard parse error** — see §13. |
| Unknown fields | Silently **ignored on load and dropped on the next save**. There is no unknown-field preservation anywhere (the old dossier's claim of a `#[serde(flatten)]` catch-all is false — no persisted type has one). |
| Optional fields | A field is optional iff it is `Option<T>` (serde derives treat missing `Option` as `None`) or carries `#[serde(default…)]`. All other fields are **required**; omitting them is a parse error. The tables below mark optionality. |

---

## 3. Top-level envelope

### 3.1 Current (v3)

```json
{
  "format": "waffle-iron",
  "version": 3,
  "min_reader_version": 3,
  "document": {
    "name": "Untitled",
    "created": "2026-07-05T01:21:04.049Z",
    "modified": "2026-07-05T01:21:04.049Z",
    "display_unit": "mm"
  },
  "tabs": [
    {
      "id": "9068ef01-8734-4955-95d4-2e78f0878fcb",
      "name": "Part 1",
      "kind": {
        "type": "Part",
        "features": { "features": [ … ], "active_index": null },
        "preview_mesh": null
      }
    }
  ],
  "active_tab": "9068ef01-8734-4955-95d4-2e78f0878fcb"
}
```

| Field | Type | Req | Meaning |
|---|---|---|---|
| `format` | string | ✔ | Must be exactly `"waffle-iron"`; anything else ⇒ `LoadError::UnknownFormat`. |
| `version` | u32 | ✔* | Format version. `> 3` ⇒ `LoadError::FutureVersion` (refuse, don't guess). *The Rust loader defaults a missing/non-numeric version to `0`, which then fails migration (`no migration path from v0`). |
| `min_reader_version` | u32 | opt (default 0) | Since 2026-08-28: the oldest reader (by its `FORMAT_VERSION`) that can parse this file. Readers refuse `max(version, min_reader_version) > FORMAT_VERSION` with `FutureVersion`. Writers set it to `MIN_READER_VERSION` (currently 3); bump it together with `version` whenever a change lands that old readers cannot parse — **including new enum variants**. Absent in pre-2026-08-28 files ⇒ no requirement. |
| `document` | DocumentMetadata | ✔ | §5.1. |
| `tabs` | Tab[] | ✔ | At least one tab expected; `load_document` rejects an `active_tab` that names no tab; `load_project` falls back to the first tab. |
| `active_tab` | string | ✔ | Id of the tab open when saved. |

The loader's v3 branch triggers on `version >= 3` **and** the presence of a `tabs`
key (`crates/file-format/src/load.rs`); otherwise it falls through to the legacy
flat shape.

### 3.2 Legacy v2 (and v1) flat shape

```json
{
  "format": "waffle-iron",
  "version": 2,
  "project": {
    "name": "My Part", "created": "…", "modified": "…", "display_unit": "mm"
  },
  "features": { "features": [ … ], "active_index": null }
}
```

Same `format`/`version` rules; `project` is `ProjectMetadata` (identical fields to
`DocumentMetadata`); `features` is the `FeatureTree` directly. Loaders wrap this in
a synthetic single tab named `"Part 1"` with a freshly generated tab id.

v1 files have the same shape as v2 but with **millimeter-scale** length values;
they are converted on load (§4).

---

## 4. Version history and migrations

| Version | Introduced | Change | Migration on load |
|---|---|---|---|
| 1 | initial | Flat `project` + `features`; coordinates in mm-scale scene units | `migrate_v1_to_v2` (`crates/file-format/src/migrate.rs`): multiply every **length-valued** field by 0.001 — sketch plane origins, Point x/y, Circle radius, Distance/Radius/Diameter constraint values, solved positions, profile circles and spline control points, extrude depths (both directions), revolve axis origin, fillet radius / chamfer distance / shell thickness, datum-plane origins and offsets. Angles, unit direction vectors, and ratios are **not** scaled. |
| 2 | true-meters | Same shape as v1, values in meters | — |
| 3 | multi-tab | Envelope restructured: `document` + `tabs[]` + `active_tab`; feature-tree content unchanged (v2→v3 is a no-op content migration) | Structural: legacy files wrapped into one tab. |

Migrations run **sequentially** (v1→v2→v3). They live only in the Rust loader.
The app's file-open and document-open paths do route through the Rust loader
(`UiToEngine::LoadProject` → `file_format::load_project`,
`crates/wasm-bridge/src/dispatch.rs:196`), so mm→m conversion is applied in
practice; but the pure-JS tab bookkeeping (`initDocumentState`,
`app/src/lib/engine/store.svelte.js:5508`) does **not** migrate — see §14.4.

**Version-bump policy as actually practiced:** the number has stayed at 3 while the
format grew additively (ImportedBody, `combine`/`targets`, `regions`, `projected`,
`body_names`, point-pair H/V constraints, `OffsetFromFace`, …). Additive =
new optional fields (defaulted) or new enum variants. Consequence: **backward
compatibility is real** (old files load in new builds — enforced de facto by the
312-case assay corpus, loaded via `file_format::load_project` in
`crates/test-harness/src/assay/gen.rs:4874`), while **forward compatibility is
absent**: an older build given a newer file fails with a raw serde
`ParseError` (unknown variant / missing struct), *not* a clean
`FutureVersion` message. See §13.

---

## 5. Document layer

### 5.1 `DocumentMetadata` (v3) / `ProjectMetadata` (v1–v2)

| Field | Type | Req | Notes |
|---|---|---|---|
| `name` | string | ✔ | Document display name. |
| `created` | timestamp | ✔ | **Currently unreliable:** the production JS writer stamps `created: now` on every save, destroying the original creation time (§14.2). |
| `modified` | timestamp | ✔ | Last save time. |
| `display_unit` | string | opt (omitted when absent) | UI unit preference; see §2. Absent in legacy v1 files. |

### 5.2 `Tab`

| Field | Type | Req | Notes |
|---|---|---|---|
| `id` | string | ✔ | Free-form; unique within the document; matched by `active_tab`. |
| `name` | string | ✔ | e.g. `"Part 1"`. |
| `kind` | TabKind | ✔ | Tagged enum, below. |

### 5.3 `TabKind`

Single variant today:

```json
{ "type": "Part", "features": { …FeatureTree… }, "preview_mesh": null }
```

| Field | Type | Req | Notes |
|---|---|---|---|
| `features` | FeatureTree | ✔ | §6. |
| `preview_mesh` | PreviewMesh \| null | opt | Thumbnail mesh for the document browser. The Rust writer omits the key when `None`; the JS writer emits an explicit `null`. Both load fine. |

Future tab kinds (assembly, drawing) would be new `type` tags — which, per §13,
old builds will reject with a parse error, not skip.

### 5.4 `PreviewMesh`

| Field | Type | Notes |
|---|---|---|
| `vertices` | f32[] | Flat xyz triples. |
| `normals` | f32[] | Flat xyz triples, parallel to `vertices`. |
| `indices` | u32[] | Triangle list. |

Produced by decimating the last body's render mesh to ≤500 triangles
(`model_updated_response`, `crates/wasm-bridge/src/dispatch.rs:372`). Note the type
is defined **twice** with identical shape: `file_format::metadata::PreviewMesh`
(the file contract) and `feature_engine::preview_mesh::PreviewMesh` (what the
bridge actually sends and JS actually stores into the file) — a drift hazard
(§14.6).

---

## 6. Feature-tree layer

### 6.1 `FeatureTree` (`crates/feature-engine/src/types.rs:14`)

| Field | Type | Req | Notes |
|---|---|---|---|
| `features` | Feature[] | ✔ | Ordered; index 0 rebuilds first. |
| `active_index` | usize \| null | ✔ (nullable) | Rollback bar: features **after** this index are skipped during rebuild. `null` = all active. |
| `body_names` | object {string: string} | opt (omitted when empty) | User body-name overrides. Key is the persistent body identity `"{feature_uuid}/{output_tag}"` where the tag is `Main`, `Body:N`, `Profile:N`, or `Datum:name` (`OutputKey::tag()`). Value is the display name. |

### 6.2 `Feature`

| Field | Type | Req | Notes |
|---|---|---|---|
| `id` | UUID | ✔ | Stable across edits; anchors GeomRefs. |
| `name` | string | ✔ | User-visible. |
| `operation` | Operation | ✔ | Tagged enum, §7. |
| `suppressed` | bool | ✔ | Suppressed features are skipped during rebuild but retained. |
| `references` | GeomRef[] | ✔ (may be `[]`) | Declared upstream dependencies. In practice frequently empty; operations also embed GeomRefs directly in their params, and those are authoritative. |

---

## 7. Operations

`operation` is internally tagged with `type` ∈ `Sketch`, `Extrude`, `Revolve`,
`Fillet`, `Chamfer`, `Shell`, `BooleanCombine`, `DatumPlane`, `ImportedBody`.
Parameter payloads sit under `sketch` (for `Sketch`) or `params` (all others).

> **Deferred operations:** `Fillet`, `Chamfer`, `Shell` are serializable and
> loadable but the operations themselves are deferred indefinitely (root
> `CLAUDE.md`); the UI keeps their dialogs disabled. Their formats are frozen as
> below and files containing them still parse.

### 7.1 `Sketch`

`{ "type": "Sketch", "sketch": { …Sketch… } }` — see §9.

### 7.2 `Extrude` — `ExtrudeParams` (`crates/feature-engine/src/types.rs:208`)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `sketch_id` | UUID | ✔ | The sketch feature's *sketch* id (not the feature id). |
| `profile_index` | usize | ✔ | Index into the sketch's solved profiles. Ignored when `region` is set. |
| `depth` | f64 (m) | ✔ | Primary blind depth. |
| `direction` | [f64;3] \| null | opt | Override direction; `null` = sketch-plane normal. |
| `symmetric` | bool | ✔ | Symmetric about the sketch plane. |
| `cut` | bool | ✔ | **Legacy** boolean flag (see `combine`). |
| `merge` | bool | default `true` | **Legacy** auto-union flag. |
| `target_body` | GeomRef \| null | opt | **Legacy** explicit boolean target (historically never written by the UI). |
| `depth_mode` | DepthMode | default `{"type":"Blind"}` | `Blind` \| `ThroughAll` \| `UpTo {reference: GeomRef}`. |
| `second_direction` | SecondDirection \| null | opt | `Symmetric` \| `Blind {depth}` \| `ThroughAll` \| `UpTo {reference}`. |
| `region` | Region \| null | opt (omitted when absent) | Explicit sub-region boundary (annulus/lens/…) that no whole-loop `profile_index` denotes; §10.3. **This is the format's largest field in practice** — a sampled polygon plus curve-recovered edge list (374 KB in one observed file). |
| `regions` | Region[] | opt (omitted when empty) | ≥2 sub-regions extruded as one body (2D-unioned before extrude). |
| `combine` | CombineMode \| null | opt | **Current** boolean model: `NewBody` \| `Add` \| `Cut` \| `Intersect` (tagged). `null` ⇒ legacy file: mode derived from `cut`/`merge`/`target_body` by `normalize_extrude_combine` (types.rs:301) — `cut:true`⇒Cut, else `merge:true`⇒Add, else NewBody, targeting the most-recent solid. New features always write `Some`. |
| `targets` | GeomRef[] \| null | opt (omitted when absent) | Combine targets. `null` ⇒ auto ("share a face" with the sketch geometry); `[]` ⇒ forced new body; else exactly those bodies. Meaningful only with `combine` ∈ Add/Cut/Intersect. |

### 7.3 `Revolve` — `RevolveParams` (types.rs:367)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `sketch_id` | UUID | ✔ | |
| `profile_index` | usize | ✔ | |
| `axis_origin` | [f64;3] (m) | ✔ | |
| `axis_direction` | [f64;3] | ✔ | Unit direction; not scaled by migration. |
| `angle` | f64 (deg) | ✔ | 360 = full revolution. |
| `cut` | bool | default `false` | Legacy flag. |
| `merge` | bool | default `true` | Legacy flag. |
| `combine` | CombineMode \| null | opt | Same semantics as extrude. |
| `targets` | GeomRef[] \| null | opt | Same semantics as extrude (no legacy `target_body` here). |

### 7.4 `Fillet` / `Chamfer` / `Shell` (deferred ops)

- `FilletParams`: `edges: GeomRef[]`, `radius: f64` (m).
- `ChamferParams`: `edges: GeomRef[]`, `distance: f64` (m).
- `ShellParams`: `faces_to_remove: GeomRef[]`, `thickness: f64` (m).

### 7.5 `BooleanCombine` — `BooleanParams`

| Field | Type | Notes |
|---|---|---|
| `body_a` | GeomRef | Target body. |
| `body_b` | GeomRef | Tool body. |
| `operation` | `{"type": "Union" \| "Subtract" \| "Intersect"}` | |

### 7.6 `DatumPlane` — `DatumPlaneParams`

`{ "name": string, "definition": PlaneDefinition }` where `PlaneDefinition` is
tagged with **`method`** (not `type`) and uses kebab-case tags:

| Variant | Fields | Notes |
|---|---|---|
| `"method":"point-normal"` | `origin: [f64;3]` (m), `normal: [f64;3]` | |
| `"method":"offset"` | `basePlaneId: UUID` (note **camelCase** rename), `distance: f64` (m) | Offset from another datum plane (including the three built-ins, §2). |
| `"method":"offset-face"` | `base: GeomRef`, `distance: f64` (m) | Offset from a planar face, re-resolved each rebuild; negative distance flips sides. |

(The JS plane model also has a `three-points` definition; it is **not** part of the
Rust persisted enum and never appears in files.)

### 7.7 `ImportedBody` — `ImportedBodyParams` (types.rs:141)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `file_name` | string | ✔ | Display/diagnostics, e.g. `"minihexa.step"`. |
| `blob_encoding` | string | ✔ | Must equal `"deflate-base64"` (`step_import::STEP_BLOB_ENCODING`); anything else is a loud decode error. |
| `blob` | string | ✔ | The **entire source STEP text**, raw-deflate-compressed then base64 (standard alphabet). Decoded by `step_import::decode_step_blob` (`crates/step-import/src/blob.rs`). No size cap on inflation (§14.8). |
| `translation_m` | [f64;3] | default `[0,0,0]` | Placement translation, meters, applied after rotation. |
| `rotation_deg` | [f64;3] | default `[0,0,0]` | Intrinsic X→Y→Z Euler angles, degrees, about the imported model's origin. |
| `scale` | f64 | default `1.0` | Extra uniform scale on top of the STEP file's own unit conversion. |

The import replays on every rebuild (a process-wide parse cache makes transform
edits cheap). This is the one place the format deliberately embeds bulk payload
data; observed cost ≈ 430 KB blob for a small STEP part.

---

## 8. Persistent geometry references — `GeomRef`

(`crates/waffle-types/src/geom_ref.rs`; background: `docs/PERSISTENT-NAMING.md`.)

```json
{
  "kind":     { "type": "Face" },
  "anchor":   { "type": "FeatureOutput", "feature_id": "…uuid…", "output_key": { "type": "Main" } },
  "selector": { "type": "Role", "role": { "type": "EndCapPositive" }, "index": 0 },
  "policy":   { "type": "BestEffort" }
}
```

| Field | Values | Req/default |
|---|---|---|
| `kind` | `Vertex` \| `Edge` \| `Face` \| `Shell` \| `Solid` (tagged) | ✔ |
| `anchor` | `FeatureOutput { feature_id: UUID, output_key: OutputKey }` \| `Datum { datum_id: UUID }` | ✔ |
| `selector` | see below | ✔ |
| `policy` | `Strict` (fail rebuild on ambiguity) \| `BestEffort` (closest match + warning) | default `BestEffort` |

`OutputKey`: `Main` \| `Body {index}` \| `Profile {index}` \| `Datum {name}`.

`Selector` variants:

- `Role { role, index }` — semantic role assigned by the producing operation.
  `Role` values: `EndCapPositive`, `EndCapNegative`, `SideFace{index}`,
  `RevStartFace`, `RevEndFace`, `FilletFace{index}`, `ChamferFace{index}`,
  `ShellInnerFace{index}`, `ProfileFace`, `PatternInstance{index}`,
  `BooleanBodyAFace{index}`, `BooleanBodyBFace{index}`.
- `Signature { signature: TopoSignature }` — geometric fingerprint matching; all
  fields optional: `surface_type` (string), `area`, `centroid [f64;3]`,
  `normal [f64;3]`, `bbox [f64;6]`, `adjacency_hash u64`, `length`.
- `Query { query: TopoQuery }` — `filters:
  [SurfaceType{surface_type} | NormalDirection{direction, tolerance} |
  NearPoint{point, distance} | AreaRange{min,max}]` plus optional `tie_break:
  LargestArea | NearestTo{point} | SmallestIndex`.
- `Position { x, y, z }` — nearest entity to a 3D point.

**Reality note:** files in the wild overwhelmingly use `Role` selectors, and a
sketch-on-face is persisted with a *random* `Datum` UUID anchor plus a
`Role` selector while the actually-used plane geometry is snapshotted into the
sketch's `plane_origin`/`plane_normal` (see §9.1) — i.e. plane resolution from the
GeomRef is partially vestigial in current files. Treat `plane_origin`/`plane_normal`
as authoritative when present.

---

## 9. Sketch layer

(`crates/waffle-types/src/sketch.rs`.)

### 9.1 `Sketch`

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `id` | UUID | ✔ | Referenced by extrude/revolve `sketch_id`. |
| `plane` | GeomRef | ✔ | See reality note in §8. |
| `plane_origin` | [f64;3] (m) | default `[0,0,0]` | 3D snapshot of the sketch plane. |
| `plane_normal` | [f64;3] | default `[0,0,1]` | |
| `entities` | SketchEntity[] | ✔ | §9.2. |
| `constraints` | SketchConstraint[] | ✔ | §9.3. |
| `solve_status` | SolveStatus | ✔ | **Required.** `FullyConstrained` \| `UnderConstrained {dof}` \| `OverConstrained {conflicts: u32[] — indices into the constraint list}` \| `SolveFailed {reason}`. A hand-written sketch JSON without this field will not parse. |
| `solved_positions` | {string→[f64,f64]} | default; omitted when empty | Derived (§10.1). |
| `solved_profiles` | ClosedProfile[] | default; omitted when empty | Derived (§10.2). |
| `projected` | ProjectedEntity[] | default; omitted when empty | External-geometry-driven points: `{point_id: u32, source: {geom_ref: GeomRef, kind: {"type":"Vertex"} | {"type":"EdgeSample","t":f64}}}`. Re-projected on rebuild. |

### 9.2 `SketchEntity` (tagged `type`)

All variants carry `id: u32` and `construction: bool` (default `false`).

| Variant | Extra fields | Notes |
|---|---|---|
| `Point` | `x: f64, y: f64` (m, sketch UV) | The only entity carrying coordinates; all curves reference point ids. |
| `Line` | `start_id, end_id: u32` | |
| `Circle` | `center_id: u32, radius: f64` (m) | |
| `Arc` | `center_id, start_id, end_id: u32` | Radius implicit (center→start distance). |
| `Spline` | `point_ids: u32[]` | Control/through points by id. |
| `Gear` | `params: GearParams` | Parametric involute gear, stored compactly and expanded to primitives on load (`expand_gears`). `GearParams`: `tooth_count: u32` (req), `module: f64` (m, req), `pressure_angle_deg` (default 20), `backlash` (default 0), `center_x`/`center_y` (default 0), `rotation_offset` (default 0), `internal: bool` (default false — ring gear teeth point inward). |

### 9.3 `SketchConstraint` (tagged `type`)

Entity references are `u32` entity ids. Lengths in meters, angles in degrees.

| Variant | Fields |
|---|---|
| `Coincident` | `point_a, point_b` |
| `Horizontal` / `Vertical` | `entity` (a line) |
| `HorizontalPoints` / `VerticalPoints` | `point_a, point_b` |
| `Parallel` / `Perpendicular` | `line_a, line_b` |
| `Tangent` | `line, curve` |
| `Equal` | `entity_a, entity_b` |
| `Symmetric` | `entity_a, entity_b, symmetry_line` |
| `SymmetricH` / `SymmetricV` | `point_a, point_b` |
| `Midpoint` | `point, line` |
| `Distance` | `entity_a, entity_b, value` |
| `PointLineDistance` | `point, entity, value` |
| `HDistance` / `VDistance` | `point_a, point_b, value` (constrains |Δx| / |Δy|) |
| `Angle` | `line_a, line_b, value_degrees` |
| `Radius` / `Diameter` | `entity, value` |
| `OnEntity` | `point, entity` |
| `Dragged` | `point` (soft interaction hint, weight 1/20) |
| `Pinned` | `point, x, y` (hard position lock) |
| `EqualAngle` | `line_a, line_b, line_c, line_d` |
| `Ratio` | `entity_a, entity_b, value` |
| `EqualPointToLine` | `point_a, point_b, line` |
| `SameOrientation` | `entity_a, entity_b` |

Reference (driven) dimensions are a UI-side flag and are **not persisted as a
distinct constraint kind** — the UI filters them out of the driving set before
solving.

---

## 10. Derived-but-persisted data

These fields exist because the same Rust types serve as both the **bridge wire
format** (engine⇄UI worker messages) and the **file format**. They are populated
in live state, so they get written into files. Loaders must apply the following
contract:

**Contract:** on load, derived fields are recomputed **only when empty**
(`Sketch::recompute_derived*`, sketch.rs:116/172 — "only populate if empty",
"if profiles already exist, preserve them"). Persisted values therefore *win* over
recomputation. They were correct at save time for the saving build; after solver
or profile-extraction changes they may not match what the current build would
compute. The v1→v2 migration deliberately scales them so this precedence stays
consistent for legacy files.

### 10.1 `solved_positions`

`{ "<point_id>": [x, y], … }` — solver output per point, sketch UV, meters.
Redundant with `Point.x/y` for plain points; load reconstructs from entities when
absent. Also carries positions of gear-expansion points.

### 10.2 `solved_profiles` — `ClosedProfile` (sketch.rs:453)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `entity_ids` | u32[] | ✔ | Ordered loop entity ids. |
| `is_outer` | bool | ✔ | CCW outer vs CW hole. |
| `vertex_ids` | u32[] | default; omitted when empty | Point ids in winding order (kernel polygon construction). |
| `circle` | CircleProfile \| null | opt | `{center_u, center_v, radius}` (m) — standalone circle ⇒ true NURBS wire. |
| `spline_segments` | SplineSegment[] | default | `{start_point_index, end_point_index, control_points: [[u,v],…]}` — indices into `entity_ids`. |
| `arc_segments` | ArcSegment[] | default | `{start_vertex_index, end_vertex_index, center_u, center_v, radius}` — indices into `vertex_ids`; drives cylindrical side-face geometry on extrude. |

### 10.3 `Region` / `RegionEdge` (`crates/waffle-types/src/regions.rs:42`)

Persisted inside `ExtrudeParams.region` / `.regions` when a sub-region (not a
whole profile loop) is extruded.

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `outer` | [[f64,f64],…] | ✔ | Outer boundary, CCW, **tessellated** (chord tolerance 1e-3 relative). |
| `holes` | [[[f64,f64],…],…] | default | Hole loops, CW. |
| `area` | f64 | default 0 | UV area (UI pick ranking). |
| `profile_entity_ids` | u32[] \| null | opt | When set: this region equals one whole profile (UI resolves to `profile_index`; analytical path). |
| `outer_edges` | RegionEdge[] | default | Curve-aware boundary: `{"kind":"Line", a, b}` or `{"kind":"Arc", a, b, center, radius, ccw}` — vertices exactly on the source circle; the kernel builds true cylinder walls from these. |
| `hole_edges` | RegionEdge[][] | default | Parallel to `holes`. |

**Cost note:** `outer` and `outer_edges` encode the same boundary twice (tessellated
+ curve-recovered). Measured in `err.waffle`: one extrude's region = **374 KB**
(103 KB `outer` + 266 KB `outer_edges`) of a 1.2 MB file. This is the format's
main size pathology (§15).

---

## 11. STEP payload encoding

`ImportedBodyParams.blob` = `base64(deflate_raw(step_text))`, tagged by
`blob_encoding: "deflate-base64"`. Encode/decode in
`crates/step-import/src/blob.rs`. Decoder rejects unknown encoding tags loudly.
Round-trips byte-exact. **Bump the tag only with a decoder that accepts both.**

---

## 12. Readers, writers, and storage envelopes

The format has **one Rust implementation and two-and-a-half JavaScript writers**.
Anyone changing the format must touch all of them:

| Component | Location | Role |
|---|---|---|
| `file-format` crate | `crates/file-format/` | Reference implementation. `save_project`/`load_project` (single-tree, v3-wrapped), `save_document`/`load_document` (full multi-tab), `migrate`, `export_step`. **`save_document`/`load_document` currently have no production callers** — only tests; the bridge exposes only the single-tree pair. |
| wasm-bridge | `crates/wasm-bridge/src/dispatch.rs:189-216` | `SaveProject` ⇒ Rust-serialized v3 (one tab, live tree), **verified** via `save_project_verified` (self round-trip; a corrupt tree is a loud bridge error, not a dead file). `LoadProject{data}` ⇒ Rust load (with migrations) of **the active tab only**, then full rebuild. `SwitchTab{features}` swaps trees without touching the file format. |
| JS document writer | `buildDocumentJson`, `app/src/lib/engine/store.svelte.js` | **The production writer.** Pulls the live tree via `SaveProject`, splices it into the active tab, re-assembles the full v3 envelope in JS (all tabs, metadata, preview meshes, `min_reader_version`, preserved `created`). Used by autosave (3 s debounce), Ctrl+S, provider sync, and (since 2026-08-28) the file-download path, which previously dropped every non-active tab. Version constants live in `app/src/lib/engine/format.js` (must mirror the Rust constants). |
| JS new-doc template | `app/src/routes/home/+page.svelte` | Hand-writes a minimal empty v3 document from the `format.js` constants (note: no `display_unit`, no `preview_mesh` keys — legal per §2 optionality). |
| JS loader/bookkeeper | `initDocumentState` / `loadPendingDocument`, store.svelte.js:5395/5508 | Parses the document in JS for tab structure, then feeds the whole JSON to the Rust loader for the engine model. |
| Storage envelope — IndexedDB | `app/src/lib/storage/indexeddb.js` | DB `waffle-iron`, store `documents`, records `{id, json: <the .waffle text>, created, modified}` (epoch ms). The `.waffle` JSON travels as an opaque string. |
| Storage envelope — GitHub | `app/src/lib/storage/github.js` | One file per document in a user repo (default `waffle-iron-documents`) plus an index file `.waffle-index.json`. Same opaque JSON. Documents can be **shared** — files must be treated as potentially untrusted input (§14.8). |
| sessionStorage handoff | keys `waffle-active-doc` / `waffle-active-json` | Route → editor transfer of the full JSON. |
| Assay corpus | `app/tests/cases/assay/*.waffle` (312 v3 files) + `crates/test-harness/src/assay/gen.rs:4874` | De-facto backward-compat pin: every kernel assay run loads the corpus through `file_format::load_project`. A change that breaks old files breaks the assay loudly. |
| file-format tests | `crates/file-format/tests/format_tests.rs` (43 tests) | Round-trips (incl. rebuild + topology compare), v1→v3 chains, tab validity, non-UUID tab ids, constraint round-trips, back-compat for pre-`combine`/pre-`body_names`/pre-`min_reader_version` files, `FutureVersion` refusal, verified-save NaN rejection. |
| JS-writer regression spec | `app/tests/gui/document-format-seam.spec.js` | Pins the production (JS) writer's envelope: `created` preservation, `display_unit` round-trip, `min_reader_version`, multi-tab File→Open adoption + storage-doc re-homing, clean refusal of too-new files. |

---

## 13. Compatibility contract (as it actually is)

1. **Backward (old file, new build): supported.** Mechanisms: sequential
   migrations (v1→v2 value scaling; v2→v3 wrapping), `#[serde(default)]` on every
   post-v3 additive field, legacy-flag normalization (`combine == null` ⇒ derive
   from `cut`/`merge` — `normalize_extrude_combine`), tolerant tab-id strings.
   Pinned by the assay corpus and back-compat unit tests.
2. **Forward (new file, old build): not supported, but now fails cleanly going
   forward.** Since 2026-08-28 every writer emits `min_reader_version` (§3.1)
   and every reader (Rust loaders + the JS open paths via
   `format.js/fileTooNew`) refuses files that demand a newer reader with a
   clean `FutureVersion` / "saved by a newer version" error. Builds older than
   2026-08-28 ignore the field and still fail with parse noise on future
   variants — unavoidable retroactively. Unknown *fields* are still silently
   dropped on resave (round-tripping a newer file through an older build strips
   data without warning); there is still no "ignore-unknown-variant" mechanism.
3. **Version-bump rule (now explicit):** bump `version` for value-reinterpreting
   or structural changes (v1→v2 units, v2→v3 tabs). Bump `MIN_READER_VERSION`
   (Rust `save.rs` + JS `format.js`, together with `version`) for **any** change
   old readers cannot parse — which includes new enum *variants*
   (`Operation`/`TabKind`/constraint/selector tags), not just structural
   changes. Purely additive defaulted fields need no bump.
4. **Writer duties:** never emit NaN/∞ (serializes as `null`, poisons the file —
   §2): the bridge save path enforces this via `save_project_verified`, which
   round-trips its own output through the loader and errors loudly instead of
   emitting an unloadable file. Preserve `created` (the JS writer latches it at
   open / first save). Preserve fields you don't understand — impossible today,
   which is why non-Rust tooling should modify files only field-wise, never
   load-modify-save through partial models.
5. **Reader duties:** validate `format`; refuse
   `max(version, min_reader_version) > FORMAT_VERSION`; run migrations *before*
   interpreting values; treat derived fields per the §10 contract; validate
   `active_tab` (fall back to first tab); treat blob decode failures and
   unknown `blob_encoding` as loud per-feature errors, not file rejection.

---

## 14. Known defects and divergences (verified 2026-08-28)

Numbered for reference. Items 2, 3, 4, 10, and 11 were **FIXED on 2026-08-28**
(the seam-fix change set; regression-pinned by
`app/tests/gui/document-format-seam.spec.js` and the new format_tests) — their
original text is kept for the record with a status line.

1. **Stale dossier.** `projects/09-file-format/ARCHITECTURE.md`/`PLAN.md`/
   `INTERFACES.md` describe v1, claim `#[serde(flatten)]` unknown-field
   preservation (never implemented), claim solved positions "are NOT stored"
   (they are — §10), claim STEP export works via ruststep/truck (the truck kernel
   is deleted; kernel-v2's `export_step` returns `NotSupported`, surfaced as
   `ExportError::StepExportFailed` — `crates/file-format/src/step_export.rs`).
   Last substantive update: initial commit `c2b6cb9d`.
2. **`created` is destroyed on every save.** `buildDocumentJson` stamped
   `created: now` on every save. The storage envelope kept its own honest
   `created`, but the file's was wrong.
   **FIXED 2026-08-28:** `initDocumentState` adopts the stored `created`
   (v3 `document.*` or legacy `project.*`); the writer emits the latched value
   and only stamps "now" on a document's first-ever save.
3. **`display_unit` lost on file-open of v3 files.** `extractDisplayUnit`
   read `parsed?.project?.display_unit` — the **v2** path only. Opening a v3
   file reset the JS-side unit to `mm`; the next autosave persisted the reset.
   **FIXED 2026-08-28:** both `extractDisplayUnit` and `initDocumentState` read
   `document.display_unit ?? project.display_unit` (the latter also covers
   empty documents, which never reach the engine-load path).
4. **Multi-tab loss through the single-tree path.** `load_project` returns only
   the active tab's tree by design, and the bridge `LoadProject` uses it — so
   File→Open of a multi-tab document loaded one tab into the engine while the
   JS `documentTabs` were **not** reinitialized; a subsequent autosave merged
   live features into whatever tab state JS happened to hold (potentially
   overwriting the previously open storage document), and the file-download
   path emitted only the active tab.
   **FIXED 2026-08-28:** the File→Open picker branch cancels any pending
   autosave, then on successful engine load adopts the file's full tab
   structure via `initDocumentState` under a **fresh storage doc id** (autosave
   keeps working for the opened file; the previously open storage doc is
   untouchable), and the download path (`saveProject`) now writes the full
   document via `buildDocumentJson` — including a doc-less editor session,
   where the live tree is wrapped in an implicit tab. Programmatic
   `loadProject(json)` callers still own their document state by design.
5. **JS legacy branch drops v2 features from tab bookkeeping.**
   `initDocumentState`'s legacy fallback creates an *empty* implicit tab; the
   engine separately loads the real tree via the Rust path, and the next
   autosave heals the file from live state. Works by accident for single-tab
   legacy docs; fragile (an autosave firing between the two steps, or a failed
   engine load, would persist an empty tree over the document).
6. **Duplicate `PreviewMesh` definitions** (`file_format::metadata` vs
   `feature_engine::preview_mesh`). Same shape today; nothing enforces it.
7. **Two writers, no shared schema.** The Rust crate and `buildDocumentJson`
   both compose the envelope; divergences (2) and (3) are the existing proof of
   drift. There is no JSON Schema, no golden-file diff test between the writers.
8. **Untrusted-input hardening is absent.** GitHub-shared documents make
   `.waffle` files an exchange format. `decode_step_blob` has no inflation size
   cap (deflate bomb ⇒ memory abort, and wasm32 alloc-abort is a known hard
   crash — see `session_2026_07_28_octree_duplication_oom`); `tooth_count`,
   entity counts, and array lengths are unvalidated; `active_index` is not
   bounds-checked at parse time (the accessor clamps at use —
   `active_features`, types.rs:83).
9. **Region duplication bloat.** §10.3: the same boundary stored tessellated
   *and* curve-recovered; dominates file size when sub-region extrudes exist.
10. **NaN poisoning.** §2 floats hazard: both writers emit `null` for
    non-finite floats; every reader then rejects the file. Save succeeded, load
    never did — silent data loss of the only copy if it was the autosave.
    **FIXED (guarded) 2026-08-28:** the bridge save path uses
    `save_project_verified` (serialize, then self-load); a poisoned tree is a
    loud save-time error (toast on Ctrl+S, console warning from autosave, which
    then leaves the last good stored copy untouched). Residual: NaN that
    entered a *JS-held inactive tab* via an earlier `ModelUpdated` was already
    null-ed by that serialization and is not caught — the guard covers the live
    tree, where NaN originates.
11. **`version` is not honest about content.** Post-v3 additive changes
    (ImportedBody et al.) shipped without a bump or a `min_reader_version`
    field, so old builds fail with parse noise instead of a clean
    "file is newer than this app" message (§13.2).
    **FIXED (forward) 2026-08-28:** all writers emit `min_reader_version`
    (§3.1); all readers refuse too-new files cleanly. Builds older than this
    change still fail with parse noise on future files — unavoidable
    retroactively.

---

## 15. Assessment and recommendations

### Verdict

**The format core is sound; the documentation was badly wrong; the seams need
work.** Recipe-based JSON with tagged enums, defaulted additive fields, real
migration precedent (v1→v2 units), and a 312-file compat corpus is a solid
foundation — there is no need to redesign the format or switch containers.
The problems are (a) documentation that actively misleads, now addressed by this
spec, and (b) a small set of correctness bugs and policy gaps at the
JS/Rust seam (§14.2-5, 7, 11), of which two silently corrupt user-visible
metadata today (`created`, `display_unit`).

### Recommended, in priority order (status as of 2026-08-28, post seam fixes)

1. **DONE.** The stale v1 dossier is superseded by this spec; keep this spec
   updated in the same PR as any format change.
2. **Single writer — PARTIALLY DONE.** The two metadata drift bugs are fixed
   (`created` latched, `extractDisplayUnit` reads the v3 path), the JS version
   literals are consolidated into `app/src/lib/engine/format.js`, and
   `document-format-seam.spec.js` pins the JS writer's envelope. The structural
   consolidation (routing `buildDocumentJson` through the Rust `save_document`
   via a bridge `SaveDocument{tabs…}` message) remains OPEN — it is what would
   prevent the next drift class outright.
3. **Forward-compat policy — DONE.** `min_reader_version` is written by all
   writers and enforced by all readers (§13.2-3). Unknown-field preservation
   remains deliberately absent (documented in §2); designing it in (serde
   `flatten` catch-alls on every struct) is OPEN and unscheduled.
4. **Multi-tab load correctness — DONE.** File→Open adopts the file's tab
   structure under a fresh storage doc id and the download path writes the
   full document (§14.4).
5. **Guardrails — PARTIALLY DONE.** Non-finite floats are rejected loudly at
   save time via the verified bridge save (§14.10). Load-time hardening for
   shared files (STEP blob inflation cap, count bounds-checking) remains OPEN.
6. **Defer (unchanged):** region size optimization (drop the redundant
   tessellated `outer` when `outer_edges` is present, behind a version bump),
   binary/compressed container, JSON Schema generation, `PreviewMesh` type
   dedup (§14.6). Real but not urgent at current file sizes.

### Priority context

Per the project's standing priorities (root `CLAUDE.md`), the Yang kernel
pipeline outranks file-format work. The 2026-08-28 seam-fix session closed the
user-facing correctness bugs (items 2-5 above, minus the flagged OPEN parts);
what remains is hardening and consolidation, suitable for a change-of-pace
slot. Nothing here blocks kernel work; conversely, the assay corpus means
kernel work already exercises this format's load path on every run.
