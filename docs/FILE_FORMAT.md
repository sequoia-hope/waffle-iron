# The `.waffle` File Format — Specification

**Format version: 4** (`FORMAT_VERSION`, `crates/file-format/src/save.rs`)
**Spec written:** 2026-08-28 (v3), from the code as it exists on `main`; **v4
section added 2026-09-07.** This document is *descriptive of the current
implementation*, not aspirational: every claim below was verified against the
source (file:line references throughout) and against real files
(`sketch.waffle`, `err.waffle`, `minihexa.waffle`, and the 312-case assay corpus in
`app/tests/cases/assay/`, all of which are version 3 and load through the
v3→v4 migration — pinned by `crates/file-format/tests/corpus_backcompat.rs`).

> **v4 (2026-09-07) in one paragraph — design: `specs/waffle_v4_document_model.md`.**
> The envelope gains `document.id` (stable document identity) and a
> `sources` table of external content with **git-aware locators** (`Git`
> remote + path + `Commit`-pinned or `Branch`/`Tag`-floating ref, with the
> `resolved` commit and a `git-blob-sha1` `content_hash`; also `Relative`,
> `Url`, `Local`, `Embedded`). Imported STEP payloads move out of the
> feature into `sources[].embed` (deduplicated by hash); the feature names
> its source by `source_id`. Unknown **tab kinds, source kinds and locator
> kinds** are preserved opaquely and re-emitted verbatim (so adding
> `Assembly`/`Drawing` later is NOT a reader-floor bump); unknown keys are
> preserved at the envelope, `document`, `Tab`, `SourceEntry` and
> `FeatureTree` levels. `FeatureTree.provenance` records who/what created a
> feature. Every production save now goes through **one writer**, the Rust
> `save_document_verified`, via the bridge's `SaveDocument` message; the JS
> side no longer composes an envelope. Timestamps are written in
> JavaScript's `toISOString()` form so a `created` round-trips byte-exact,
> and float parsing is exact (`serde_json/float_roundtrip`) so coordinates no
> longer drift by an ULP per load. Migration v3→v4: mint `document.id`,
> rewrite non-UUID tab ids (the historical `"default"`) to UUIDs, lift STEP
> blobs into sources. `MIN_READER_VERSION` is 4.

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
| IDs — features/sketches/tabs/datums/document | UUIDs, serialized as lowercase hyphenated strings. `Tab.id`/`active_tab` stay typed as free-form strings so legacy documents (the historical `"default"`) keep loading, but v4 writers always emit UUIDs and the v3→v4 migration rewrites non-UUID ids (`crates/file-format/src/metadata.rs`, `migrate.rs`). |
| IDs — sketch entities | `u32`, unique *within one sketch*. |
| Built-in datum planes | Fixed well-known UUIDs (`app/src/lib/engine/planes.js`): Front `00000000-0000-0000-0000-000000000001`, Top `…0002`, Right `…0003`. |
| Timestamps | RFC 3339 / ISO-8601 UTC strings, written in JavaScript's `toISOString()` form (always ≥ 3 fractional digits: `"2026-07-05T01:21:04.049Z"`, `"2020-01-02T03:04:05.000Z"`; nanosecond values keep full precision) — `metadata::rfc3339_js`, since v4, so a `created` that came from the app round-trips byte-exact. Any RFC 3339 string parses. |
| Floats | IEEE-754 doubles, shortest-representation printing. Parsing is **exact** since v4 (`serde_json/float_roundtrip`, enabled in `crates/file-format/Cargo.toml` and unified across the build); before, the default best-effort parser drifted 17-digit values by one ULP per load→save (measured: C0028). **Hazard:** a non-finite value (NaN/∞) serializes as `null` and then **fails to load**. Guarded since 2026-08-28: the bridge save path self-verifies (`save_document_verified`) and errors loudly instead of emitting an unloadable file (§14.10). |
| Tuples | Rust `(f64, f64)` serializes as a 2-element array `[x, y]`. Fixed arrays `[f64; 3]` as 3-element arrays. |
| Maps with u32 keys | JSON objects with **stringified** keys (`"12": [x, y]`) via the `u32_key_map` helper (`crates/waffle-types/src/sketch.rs:10`). |
| Enums | All persisted enums are **internally tagged**: `#[serde(tag = "type")]` (one exception: `RegionEdge` uses `tag = "kind"`, and `PlaneDefinition` uses `tag = "method"` with renamed variants). An unknown tag value is a **hard parse error** — see §13 — **except** `TabKind`, `SourceKind` and `Locator` (v4), whose unknown tags are preserved opaquely (§5.3, §5.5). |
| Unknown fields | v4: **preserved** across load → save at the envelope, `document`, `Tab`, `SourceEntry` and `FeatureTree` levels (flattened `extra` maps; convention: prefix tool-added keys with `x-`). Elsewhere (`Feature`, `GeomRef`, sketch entities, operation params) still silently ignored on load and dropped on save — do not stash data there. |
| Optional fields | A field is optional iff it is `Option<T>` (serde derives treat missing `Option` as `None`) or carries `#[serde(default…)]`. All other fields are **required**; omitting them is a parse error. The tables below mark optionality. |

---

## 3. Top-level envelope

### 3.1 Current (v4)

```json
{
  "format": "waffle-iron",
  "version": 4,
  "min_reader_version": 4,
  "document": {
    "id": "6f1c2a4e-9b0e-4c1d-8d7a-2f3b4c5d6e7f",
    "name": "Untitled",
    "created": "2026-07-05T01:21:04.049Z",
    "modified": "2026-07-05T01:21:04.049Z",
    "display_unit": "mm"
  },
  "sources": [],
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
| `version` | u32 | ✔* | Format version (6 since 2026-09-24). `> 6` ⇒ `LoadError::FutureVersion` (refuse, don't guess). *The Rust loader defaults a missing/non-numeric version to `0`, which then fails migration (`no migration path from v0`). |
| `min_reader_version` | u32 | opt (default 0) | Since 2026-08-28: the oldest reader (by its `FORMAT_VERSION`) that can parse this file. Readers refuse `max(version, min_reader_version) > FORMAT_VERSION` with `FutureVersion`. Writers set it to `MIN_READER_VERSION` (currently 6); bump it together with `version` whenever a change lands that old readers cannot parse — new constraint/selector/`PlaneDefinition` variants included, and any new field a reader must not silently ignore (v5: `GeomRef.scope`, §8 — a v4 reader would drop it and resolve the reference against the wrong part; v6: `Sketch.plane_x_axis`, §9.1 — a v5 reader would derive the basis from the normal and draw the sketch rotated). Since v4, new **tab kinds, source kinds and locator kinds do not** require a bump (§5.3), and since Phase 1b (2026-09-08) **new operation kinds do not either** (§7: unknown `Operation` kinds are preserved opaquely). Absent in pre-2026-08-28 files ⇒ no requirement. |
| `document` | DocumentMetadata | ✔ | §5.1. `document.id` since v4 (writers always emit; a reader minting one for a hand-written file warns). |
| `sources` | SourceEntry[] | opt (default `[]`) | v4 §5.5: external content the document depends on. |
| `tabs` | Tab[] | ✔ | At least one tab expected; `load_document` rejects an `active_tab` that names no tab; `load_project` falls back to the first tab. |
| `active_tab` | string | ✔ | Id of the tab open when saved. |
| *any other key* | | | v4: preserved on load and re-emitted on save (`WaffleDocument.extra`). |

The loader's v4 branch triggers on `version >= 4` **and** the presence of a
`tabs` key, the v3 branch on `version == 3` + `tabs` (then
`migrate::migrate_v3_to_v4`); otherwise it falls through to the legacy flat
shape (`crates/file-format/src/load.rs`). `load_document` returns a
`LoadedDocument { document: WaffleDocument, warnings }` — warnings carry
non-fatal findings (legacy tab id rewritten, missing `document.id`,
unresolvable locator, embed hash mismatch, unknown tab kind).

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
| 4 | 2026-09-07 | `document.id`; `sources` table (git-aware locators, content hash, optional embed); opaque unknown tab/source/locator kinds; unknown-key preservation; `FeatureTree.provenance`; `ImportedBody.source_id` replaces the in-feature blob; JS-form timestamps; exact float parsing | `migrate_v3_to_v4`: mint `document.id` (serde default), rewrite non-UUID tab ids to fresh UUIDs (`active_tab` follows, warning emitted), lift every `ImportedBody.blob` into a `sources` entry (`Embedded`, `pack: true`, `content_hash: git-blob-sha1(text)`, byte-identical payloads share one entry) and set `source_id`. |
| 5 | 2026-09-08 | `GeomRef.scope` (§8): a reference into another tab's instance — the assembly tab and the instance path that owns the anchor feature — for in-context editing (v4 spec §2.8, Phase 3d-4). The only change; additive, but a v4 reader would drop the field and resolve the anchor locally, so the reader floor moved with it. | none (a v4 file parses as-is; absent `scope` ⇒ local). |
| 6 | 2026-09-24 | `Sketch.plane_x_axis` (§9.1): the sketch's own in-plane +x direction, so a caller can orient a sketch instead of reproducing the engine's derivation (`docs/notes/eiffel/FEATURE_NOTES.md` §3). The only change; additive, but a v5 reader would drop it and derive the basis from the normal, drawing the sketch and everything built on it ROTATED, so the reader floor moved with it. | none (a v5 file parses as-is; absent `plane_x_axis` ⇒ derived). |

Migrations run **sequentially** (v1→v2→v3→v4). They live only in the Rust loader;
the JS `initDocumentState` applies the same tab-id rewrite so its tab list agrees
with the engine's migrated view.
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

Two known variants (Phase 3, 2026-09-08):

```json
{ "type": "Part", "features": { …FeatureTree… }, "preview_mesh": null }
{ "type": "Assembly", "assembly": { …AssemblyTree… }, "preview_mesh": null }
```

| Field | Type | Req | Notes |
|---|---|---|---|
| `features` | FeatureTree | ✔ (Part) | §6. |
| `assembly` | AssemblyTree | ✔ (Assembly) | §5.6. |
| `preview_mesh` | PreviewMesh \| null | opt | Thumbnail mesh for the document browser. Omitted when `None`; an explicit `null` also loads. |

**Unknown kinds (v4).** A well-formed `{"type": …}` the reader does not know
(`Assembly`, `Drawing`, …) loads as `TabKind::Unknown(Value)`: the tab is kept,
reported in the load warnings, not editable, and re-emitted **verbatim** on
save (`crates/file-format/src/metadata.rs`, pinned by
`tests/v4_document_tests.rs::unknown_tab_kind_is_preserved_verbatim_and_reported`).
A *malformed* known kind (`{"type":"Part","features":42}`) or an object without a
string `type` is still a hard parse error. Consequence: adding a tab kind is not a
`MIN_READER_VERSION` bump. The bridge refuses to open an unknown-kind tab as the
active part (`NotImplemented`) and refuses to hold the live tree in one.
`Tab` also carries a flattened `extra` map for unknown keys.

### 5.6 `AssemblyTree` (Phase 3, `crates/feature-engine/src/assembly.rs`)

The content of an `Assembly` tab: placed **instances** of parts related by
**mate connectors** and **mates**. All lengths meters; rotations as unit
quaternions `[x, y, z, w]` (three.js order). Unknown keys on the tree,
instances, connectors and mates are preserved (flattened `extra`).

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `instances` | Instance[] | default `[]` | `{id (UUID), name, source: {source_id?, tab_id}, transform: {translation_m: [3], rotation_quat: [4]} (default identity), fixed (default false, omitted), suppressed (default false, omitted), external_key?, parameter_overrides? ({name → number}, reserved)}`. `source` names a Part **or Assembly** tab (a sub-assembly, 3d-2) of this document (`source_id` absent) or of a linked `.waffle` source (§5.5). `fixed` grounds the instance; with none marked, the first non-suppressed instance is grounded. |
| `connectors` | MateConnector[] | default `[]`, omitted when empty | `{id, name, instance_path: [UUID, …] (the top-level instance, then members through sub-assemblies — 3d-2), geom_ref? (a face or an EDGE of the PART, §8), frame: {origin, z_axis (default +z), x_axis (default: chosen deterministically)}}`. With `geom_ref`, the frame is derived from the current geometry at evaluation by `feature_engine::connector` (`specs/assembly_connector_frame_resolver.md`): a planar face gives its centroid + outward normal; a cylindrical, conical or toroidal face gives its axis, at the middle of that face's axial extent (so a bore's connector sits at mid-depth); a spherical face gives its centre; a circular or elliptical edge gives its centre, z out of its one planar neighbour face when it has one; a straight edge gives its midpoint along the edge. `frame.x_axis` is the secondary direction when set. Anything else is a loud error (never a substituted frame). Without `geom_ref`, `frame` is the frame. Coordinates are the part's own. **Adjustments** (`specs/assembly_connector_adjustments.md`, additive, each omitted at its default): `anchor` (`"middle"` \| `"positive_end"` \| `"negative_end"` — where on a cylindrical/conical/toroidal face's axis the frame sits: the middle of the face's extent or the end its z points toward/away from, named by the FINAL z; ignored by other picks), `flip_z` (reverse z: a 180° turn about x), `rotation_deg` (turn about z, after the flip), `offset_m` (`[x, y, z]` along the connector's OWN axes, after the turn). **`part_connector`** (UUID, additive, omitted when absent; `specs/part_mate_connectors.md`): the id of a `MateConnector` feature of the instance's part (§7.8) — the frame is that connector's as the part evaluates it, taking precedence over `geom_ref` and `frame`; this connector's adjustments still apply on top. A part without that working connector is a loud evaluation error (the explicit `frame` is used meanwhile). |
| `mates` | Mate[] | default `[]`, omitted when empty | `{id, name, kind, connectors: [a, b], suppressed}`. `kind` is tagged `type`; `flip` (default false) on every kind makes b's z axis oppose a's instead of aligning (two outward face normals "facing"). `Fastened {flip, rotation_deg}` — frames coincident after `rotation_deg` about z, solved exactly by composition. Solved numerically (Phase 3d, `feature_engine::assembly_solver`: damped Gauss-Newton over the free instances' poses from their current placements, so the free degrees of freedom keep their current values): `Revolute {flip}` (origins coincide, z axes parallel; rotation about z free), `Slider {flip}` (frames aligned, b's origin on a's z axis; travel along z free), `Cylindrical {flip}` (z axes parallel, b's origin on a's z axis), `Planar {flip}` (z axes parallel, b's origin in a's xy plane), `Ball` (origins coincide). A mate the solver cannot satisfy within tolerance is a loud error (over-constrained or conflicting). Any other `type` is preserved opaquely and reported. |
| `placements` | {UUID → Transform} | default `{}`, omitted when empty | **Derived hints**: the solved placement of every non-suppressed instance, recomputed on every evaluation by `feature_engine::assembly::solve_fastened` (rigid-transform composition from the grounded instances through the mates; an over-constrained mate is a loud error; an instance no mate reaches keeps its own `transform`, with a warning). Persisted so a reader without the engine can position instances; never authoritative. |

Loader warnings (`WaffleDocument::validate`): duplicate ids, dangling
instance/connector references, a connector on a missing instance, an
instance whose tab or source the document does not have, an unknown mate
kind. The single-tree API (`load_project`) refuses to open an Assembly tab
as a part.

### 5.5 `SourceEntry` (v4)

(`crates/file-format/src/sources.rs`; design and semantics:
`specs/waffle_v4_document_model.md` §2.3–2.4, §7.)

```json
{
  "id": "3b9e…", "name": "bracket.waffle", "kind": { "type": "Waffle" },
  "locator": { "type": "Git", "remote": "https://github.com/acme/parts",
               "path": "brackets/bracket.waffle", "ref": { "type": "Branch", "name": "main" } },
  "resolved": { "commit": "9fceb02a…", "at": "2026-09-07T18:00:00.000Z" },
  "content_hash": "git-blob-sha1:2aae6c35…", "pack": false, "embed": null,
  "fetched_at": "2026-09-07T18:00:00.000Z"
}
```

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `id` | UUID | ✔ | Referenced by `ImportedBody.source_id` (and by assembly instances / drawing views / `scope.source_id` in later phases). Duplicate ids ⇒ `ParseError`. |
| `name` | string | ✔ | Display. |
| `kind` | `{"type": "Waffle"\|"Step"\|"KicadPcb"\|"Mesh"\|"Script"}` | ✔ | Unknown types preserved opaquely + warned. `Script` (2026-09-19): a custom feature script's Rhai text (§7.10), embedded like a STEP import; added by the editor / `script_source_add` (A-M4), replaced in place by a save / `script_source_update` — neither is an undo step (sources are assets). |
| `locator` | `Git{remote,path,ref,host?}` \| `Relative{path}` \| `Url{url}` \| `Local{provider,doc_id}` \| `Embedded` | ✔ | `ref` ∈ `Commit{sha}` (pinned) \| `Branch{name}` \| `Tag{name}` (floating). `remote` normalized without `.git`; `host` ∈ `github`\|`gitlab`\|`gitea`\|`generic`, inferred from the hostname when absent. Structural problems (non-https, absolute/`..` paths, bad ref names, non-hex sha) are load **warnings**; the entry stays, unresolvable. Unknown types preserved opaquely. |
| `resolved` | `{commit, at}` \| null | opt | Commit actually loaded last (git locators). |
| `content_hash` | string \| null | opt | `git-blob-sha1:<40 hex>` of the exact bytes (= `git hash-object`); unknown prefixes are "no hash", never a mismatch. |
| `pack` | bool \| null | opt | Writer policy for `embed`; absent ⇒ `true` for `Embedded`, else `false`. |
| `embed` | `{encoding: "deflate-base64", blob}` \| null | opt | Cached content. Same codec + 256 MiB inflation cap as the old STEP blob (`step_import::decode_step_blob`; over-cap ⇒ `EmbedTooLarge`). An embed whose hash mismatches `content_hash` is ignored with an `EmbedHashMismatch` warning. |
| `fetched_at` | timestamp \| null | opt | |
| *any other key* | | | preserved. |

**Content resolution at rebuild** (engine `SourceStore`, `crates/feature-engine/src/sources.rs`):
the store (embeds registered at load + host-provided content via the bridge
`ProvideSource` message) first, then the legacy inline blob; neither ⇒ the
dependent feature fails `SourceUnavailable` loudly and the document still loads.
The single-tree API keeps storeless consumers working: `load_project` inlines a
hash-verified embed back into the feature, `save_project` lifts blobs out.

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
`Fillet`, `Chamfer`, `Shell`, `BooleanCombine`, `DatumPlane`, `ImportedBody`,
`MateConnector`, `PatternCircular`, `PatternLinear`, `Pipe`, `Script`,
`UnionAll`.
Parameter payloads sit under `sketch` (for `Sketch`) or `params` (all others).

**Unknown kinds (v4 Phase 1b, 2026-09-08).** A well-formed `{"type": …}`
operation whose tag this build does not know (one from a newer build) loads
as `Operation::Unknown(Value)` (`crates/feature-engine/src/types.rs`,
`opaque.rs`): the feature stays in the tree, the load warnings name it
(`feature … unknown operation kind …`), its rebuild fails with a loud
per-feature `UnsupportedOperation` error while every other feature builds,
and the writer re-emits the operation object **verbatim** (pinned by
`crates/file-format/tests/v4_document_tests.rs::unknown_operation_is_preserved_verbatim_reported_and_fails_its_rebuild_loudly`).
A *malformed* KNOWN kind (`{"type":"Extrude","params":42}`, or a known kind
missing a field) and an object without a string `type` are still hard parse
errors that name the tag. Consequence: adding an operation kind is no longer
a `MIN_READER_VERSION` bump; the `Feature` envelope around it (`id`, `name`,
`suppressed`, `references`) is still a dense struct, so a `GeomRef` selector
or anchor kind the reader does not know inside `references` still fails to
parse. Same contract as `TabKind` (§5.3) and `SourceEntry.kind`/`locator`
(§5.5).

> **Deferred operations:** `Fillet`, `Chamfer`, `Shell` are serializable and
> loadable but the operations themselves are deferred indefinitely (root
> `CLAUDE.md`); the UI keeps their dialogs disabled. Their formats are frozen as
> below and files containing them still parse.

### 7.1 `Sketch`

`{ "type": "Sketch", "sketch": { …Sketch… } }` — see §9.

### 7.2 `Extrude` — `ExtrudeParams` (`crates/feature-engine/src/types.rs:208`)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `sketch_id` | UUID | ✔ | The id of the **sketch feature** (`Feature.id`), not the `Sketch.id` inside it — `find_sketch_in_tree` matches on the feature id (rebuild.rs). |
| `profile_index` | usize | ✔ | Index into the sketch's solved profiles. Ignored when `region` or `profile_entity_ids` is set (still range-checked). |
| `profile_entity_ids` | u32[] \| null | opt (omitted when absent; **v4 §2.9**) | Agent-friendly profile addressing: the profile is the solved loop whose entity-id set equals this set (order-insensitive), so a writer that never ran the solver can name "the loop bounded by lines 10–13" — the same identity `Region.profile_entity_ids` carries (§10.3). Takes precedence over `profile_index`. No such loop, or two loops with the same set, is a loud per-feature rebuild error (`ProfileNotFound` / `ProfileAmbiguous`), never a silent fallback to the index. The app's own writers address by index and omit it; a UI re-pick of the profile drops it. |
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
| `sketch_id` | UUID | ✔ | The sketch **feature's** id, as for extrude. |
| `profile_index` | usize | ✔ | |
| `profile_entity_ids` | u32[] \| null | opt (v4 §2.9) | As for extrude. |
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

### 7.7 `ImportedBody` — `ImportedBodyParams` (types.rs)

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `file_name` | string | ✔ | Display/diagnostics, e.g. `"minihexa.step"`. |
| `source_id` | UUID \| absent | v4 | The `sources[]` entry holding the STEP content (§5.5). v4 writers emit this and **no blob**. |
| `blob_encoding` | string \| absent | legacy (v3) | Was required; must equal `"deflate-base64"` (`step_import::STEP_BLOB_ENCODING`) when present. |
| `blob` | string \| absent | legacy (v3) | The **entire source STEP text**, raw-deflate-compressed then base64. Still accepted (`load_project` also *produces* it for storeless consumers); decoded by `step_import::decode_step_blob` with a 256 MiB inflation cap (`EmbedTooLarge`). |
| `translation_m` | [f64;3] | default `[0,0,0]` | Placement translation, meters, applied after rotation. |
| `rotation_deg` | [f64;3] | default `[0,0,0]` | Intrinsic X→Y→Z Euler angles, degrees, about the imported model's origin. |
| `scale` | f64 | default `1.0` | Extra uniform scale on top of the STEP file's own unit conversion. |

The import replays on every rebuild (a process-wide parse cache makes transform
edits cheap). This is the one place the format deliberately embeds bulk payload
data; observed cost ≈ 430 KB blob for a small STEP part.

### 7.8 `MateConnector` — `MateConnectorParams` (types.rs, 2026-09-14)

A named frame on the part that an assembly's connectors reference
(`part_connector`, §5.6; `specs/part_mate_connectors.md`). No geometry of its
own; the rebuild derives the frame and fails the feature loudly when it
cannot. The connector's name is the FEATURE's name. A new operation kind, so
no reader-floor bump (older readers keep it as `Unknown`).

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `name` | string | default `""`, omitted when empty | The feature's name at creation (empty ⇒ "Mate connector"). |
| `geom_ref` | GeomRef \| absent | opt | A face or an edge of this part; derived exactly as an assembly connector's `geom_ref` (§5.6). |
| `frame` | `{origin, z_axis, x_axis}` | default origin, +z | The frame when there is no `geom_ref` (meters, part coordinates); with one, a non-zero `x_axis` is the secondary direction. |
| `anchor`, `flip_z`, `rotation_deg`, `offset_m` | as §5.6 | defaults omitted | The same adjustments, same order. |

### 7.9 `PatternCircular` / `PatternLinear` / `PatternMirror` (types.rs, 2026-09-18; mirror 2026-09-24)

Rigid copies of seed BODIES (`specs/custom_features_and_modeling_roadmap.md`
§B1). A pattern instances bodies, not features: the seed's body is copied
exactly (`Kernel::transform_body`), never re-executed. The pattern takes
custody of its seeds (their features are consumed) and emits every instance
as its own output — `Main` is instance 0 (the seed body itself), then
`Body:{i}` instance-major. New operation kinds, so no reader-floor bump
(older readers keep them as `Unknown`).

`AxisRef` (tagged `method`) names a line: `explicit` `{origin, direction}`
(meters / any non-zero vector, normalized at rebuild) or `entity`
`{geom_ref}` — a rotational face's axis, a circular edge's axis, a straight
edge's line, or a planar face's normal, derived exactly as a mate
connector's frame (§5.6). A pick with no derivable axis fails the feature.

`PatternCircularParams`:

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `seeds` | GeomRef[] \| `{"type": "All"}` | default `[]` | Solid references to feature outputs; each must resolve and not be already consumed. Written as a bare ARRAY (the only form before 2026-09-24, and still what naming bodies produces), or as `{"type": "All"}` — every live solid body at this point in the tree, the same set `UnionAll`'s `All` folds, all of them consumed. Any other shape is a parse error. |
| `axis` | AxisRef | ✔ | Rotation axis. |
| `count` | u32 | ✔ (≥ 2) | Instances INCLUDING the seed. |
| `angle_deg` | f64 | default `360` | TOTAL sweep. A full turn spaces `count` instances `360/count` apart; any other sweep puts the last instance exactly at `angle_deg`. |
| `angle_expr` | string | opt | Driving expression (degrees), like `Revolve.angle_expr`. |
| `skip` | u32[] | default `[]` | Instance indices (≥ 1) to omit. |
| `combine` | CombineMode | default NewBody | `Add` folds targets + instances into connected lumps; `Cut` subtracts every instance from every target; `Intersect` keeps target ∩ (∪ instances). |
| `targets` | GeomRef[] | opt | Explicit targets only; a pattern never auto-targets by position. `Cut`/`Intersect` need ≥ 1. |

`PatternLinearParams`: `seeds`, `direction: AxisRef` (direction only),
`count` (≥ 2), `spacing` (meters, negative reverses), `spacing_expr`,
optional `second: {direction, count, spacing, spacing_expr}` (a grid;
instance index `i + j·count`; a second direction parallel to the first is
refused), `skip`, `combine`, `targets` as above.

`PatternMirrorParams` (2026-09-24): `seeds`, `plane: AxisRef` whose
`direction` is the plane NORMAL (an `entity` pick uses its frame's z axis, so
a planar face or a datum plane names its own plane), `combine`, `targets`.
One copy — the seed's mirror image — so there is no `count` and no `skip`;
`Main` is still the seed body and `Body:{i}` the reflections. A reflection is
improper, so the copy goes through `Kernel::mirror_body` rather than
`transform_body`, which reverses the copy's loops to keep its faces outward.

### 7.10 `Script` — `ScriptParams` (types.rs, 2026-09-19)

A custom feature script (`specs/custom_features_and_modeling_roadmap.md`
Part A): a Rhai script the document carries as a `Script` source
(§5.5 `SourceEntry.kind`, embedded text like a STEP source), run INSIDE the
engine over the same operations the tree has, appearing as ONE node. The
node's outputs are the bodies its script's child operations leave (`Main`
first); the private sub-tree is re-derived on every rebuild and never
persisted. New operation kind and new source kind, so no reader-floor bump.

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `source_id` | Uuid | ✔ | The `Script` source holding the text. |
| `entry` | string | default `"feature"` | The function called as `entry(ctx, p)`. |
| `args` | object | default `{}`, omitted when empty | Values of the script's `@param`s in MODEL units (meters / degrees / plain numbers / bools / strings); a `plane` param takes `{origin, normal}` or a datum plane id string. Unknown names are refused; missing ones take the header default or fail. |
| `arg_exprs` | object | default `{}` | `name → expression` over the design parameters (mm-space / degrees), converted by the param's declared type at rebuild. |
| `arg_values` | object | default `{}` | Last evaluated raw value of each `arg_exprs` entry (the change-detection cache, like `depth`/`depth_expr`). |

The script header (`// @feature name="…" version=N`, `// @param name: type
[= default] [min=…] [max=…]`, `// @output name: kind`) is parsed before
evaluation; parameter types are `int`, `number`, `length`, `angle`, `bool`,
`string`, `plane`, and (A-M3, 2026-09-23) `body` / `face` / `edge` — a
`GeomRef` of that kind (§8) naming geometry OUTSIDE the script, which the
script may target (`combine: "Cut", targets: [p.target]`); the node then
consumes that feature exactly as a boolean would. Output kinds are `main`,
`body`, `face`, `edge`, `connector`: the script's return value
(`#{ main: …, hub: …, top: face_query }`) keys the node's outputs —
`Main`, `OutputKey::Named { name: "hub" }`, and `Role::Named { name: "top" }`
on the resolved face — so a later feature references them by name (§8)
without knowing the private sub-tree; `ctx.mate_connector(#{ name, on })`
places a part mate connector the node exposes under `name`. Any failure —
header, parse, runtime, `ctx.fail`, a sandbox limit, an argument, a child
operation, a broken output contract — is a typed `Script` feature error and
the node has no outputs.

A-M4 (2026-09-23): the header is what generates the Script dialog's fields
and the `script_run_check` tool's `interface` (`{name, version, params:
[{name, type, default?, min?, max?}], outputs: [{name, kind}]}`); a node
added from the dialog or `script_feature_add` takes the header's `name`.
User/agent reference: `docs/CUSTOM_FEATURE_SCRIPTS.md`.

---

### 7.11 `Pipe` — `PipeParams` (types.rs, 2026-09-21)

A circle (optionally hollow) swept along an OPEN, tangent-continuous chain
of sketch lines and arcs, built by the kernel as ONE solid whose laterals
share their rim circles (`specs/b2_pipe_sweep.md`). The chain is
re-extracted from the current sketch at every rebuild
(`waffle_types::path::extract_open_chain`): a branching, disconnected,
closed or non-tangent selection is a loud per-feature error. New operation
kind, no reader-floor bump.

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `sketch_id` | UUID | ✔ | The sketch **feature's** id. |
| `entity_ids` | u32[] | ✔ | Path entities (lines / arcs, construction allowed), order-insensitive; the chain starts at the free end holding the first listed entity. |
| `radius` | f64 (m) | ✔ | Tube (outer) radius. |
| `radius_expr` | string \| null | opt | Driving expression (mm-space), as `depth_expr`. |
| `inner_radius` | f64 (m) \| null | opt | Bore radius of a hollow pipe, `0 < inner < radius`. |
| `inner_radius_expr` | string \| null | opt | Driving expression for `inner_radius`. |
| `combine` | CombineMode \| null | opt | `null` ⇒ NewBody. |
| `targets` | GeomRef[] \| null | opt | Explicit targets; a combine with none falls back to the most recent solid body (a pipe has no profile to share a face with). |

### 7.12 `UnionAll` — `UnionAllParams` (types.rs, 2026-09-23)

Many-body union as one feature (`specs/b4_balanced_union.md`): the target
bodies are folded into connected lumps by a balanced tree of ordinary
pairwise unions, skipping pairs whose conservative bounding boxes are
disjoint. Output `Main` is the first body's lump, `Body{index}` the rest.
Every source body's feature is consumed. New operation kind, no reader-floor
bump.

| Field | Type | Req/default | Notes |
|---|---|---|---|
| `targets` | `{"type":"All"}` \| `{"type":"Selected","bodies":[GeomRef…]}` | `All` | `All`: every live solid output of every active, unsuppressed, not-yet-consumed feature before this one, in tree order. `Selected`: `TopoKind::Solid` feature-output refs; a consumed body is refused (`Strict`) or dropped with a warning (`BestEffort`); duplicates are refused. |

Zero live bodies is a per-feature error; one body passes through unchanged
(with a warning). Since the same date a `BooleanCombine` (§7.5) whose
operand's feature an earlier feature consumed is a per-feature error rather
than a silent duplicate of the stale body.

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
| `scope` | `{ source_id?: UUID, tab_id?: string, instance_path: UUID[] }` (**v5**) | opt; omitted when local |

`OutputKey`: `Main` \| `Body {index}` \| `Profile {index}` \| `Datum {name}`
\| `Named {name}` (2026-09-23, additive: a body a custom feature script named
in its return value, §7.10; tag `Named:{name}` in body ids).

Additive on the same date, both for scripts (§7.10): `Role::Named {name}`
(a face or edge a script named — selected as `Role { role: {type: "Named",
name}, index: 0 }`) and `TieBreak::FarthestAlong {direction}` (the query
entity whose centroid lies farthest along `direction`; ties keep the first).
Per §4/§13 an older reader given a file that uses them fails with a raw
serde parse error rather than a clean message.

**`scope` (v5, in-context editing — v4 spec §2.8, `feature_engine::context`).**
Absent ⇒ the reference is local to the tab that holds it. Present ⇒ the
anchor feature belongs to ANOTHER part: `tab_id` names the Assembly tab (of
this document; `source_id` is reserved for a linked document's assembly and
is refused today) and `instance_path` is the chain of instance ids from that
assembly down to the part instance that owns the anchor. Written by the app
when a Part is edited *in the context* of an assembly and the user sketches
on (or projects from, or extrudes up to) a face of one of the other
instances. The engine resolves such a reference only through its edit
context — the other instances' built geometry with their placement relative
to the edited instance — and expresses the result in the edited part's own
frame; a sketch's `plane_origin`/`plane_normal` are re-derived from it on
every rebuild in context (face centroid + normal). Opened WITHOUT the context
(the part on its own, or the assembly's instance gone), a scoped reference is
loud and inert: the sketch keeps its last derived plane and warns what it
depends on; an up-to depth fails its feature; it is never resolved against
the local part (`resolve::refuse_scoped`). A connector's `instance_path` in
an `AssemblyTree` (§5.6) predates this field and stays where it is.

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
| `plane` | GeomRef | ✔ | See reality note in §8. **Scoped** (v5, `scope` present): a face of another instance of the assembly the part is edited in — the engine re-derives `plane_origin`/`plane_normal` from it on every rebuild in context (§8). |
| `plane_origin` | [f64;3] (m) | default `[0,0,0]` | 3D snapshot of the sketch plane. |
| `plane_normal` | [f64;3] | default `[0,0,1]` | |
| `plane_x_axis` | [f64;3] \| absent | default absent; omitted when absent | 2026-09-24: the world direction the sketch's **+x** points along, so `(x, y)` in this sketch is `plane_origin + x·x̂ + y·ŷ`. Absent (every file written before) ⇒ the engine derives the in-plane axes from the normal alone (`SketchPlaneBasis`), which is why a caller that cared had to reproduce that derivation. Orthogonalized against the normal at use; zero-length, non-finite or parallel to the normal fails the sketch feature loudly. New optional field an older reader may ignore only at the cost of drawing the sketch rotated, so it is **not** written unless the sketch has one. |
| `entities` | SketchEntity[] | ✔ | §9.2. |
| `constraints` | SketchConstraint[] | ✔ | §9.3. |
| `solve_status` | SolveStatus | default `{"type":"Unsolved"}` (**v4 §2.10**) | `Unsolved` \| `FullyConstrained` \| `UnderConstrained {dof}` \| `OverConstrained {conflicts: u32[] — indices into the constraint list}` \| `SolveFailed {reason}`. A sketch written without the field (or as `Unsolved`) is solved by the engine's next rebuild, which writes the solution into the entities and replaces the status — a tool never has to run the solver; a first solve that fails records the failed status and a per-feature error. Writers emit the solved status. |
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
| `Gear` | `params: GearParams` | Parametric involute gear, stored compactly and expanded to primitives on load (`expand_gears`). `GearParams` is **camelCase on the wire** (`#[serde(rename_all = "camelCase")]`; corrected 2026-09-07 from the generated schema): `toothCount: u32` (req), `module: f64` (m, req), `pressureAngleDeg` (default 20), `backlash` (default 0), `centerX`/`centerY` (default 0), `rotationOffset` (default 0), `internal: bool` (default false — ring gear teeth point inward). |
| `Sprocket` | `params: SprocketParams` | Parametric ISO 606 roller-chain sprocket (added 2026-09-19, §B3 of `specs/custom_features_and_modeling_roadmap.md`), stored compactly and expanded to points + arcs on rebuild (`expand_generators`; its primitives take the id range `generated_entity_id_base(id)`). `SprocketParams` is **camelCase on the wire**: `toothCount: u32` (req, ≥ 5), `pitch: f64` (m, req), `rollerDiameter: f64` (m, req), `centerX`/`centerY` (default 0), `rotationOffset` (default 0, radians; the first roller seat is centred on +u), `standard` (default `"Iso606"`, the only form), and optional overrides of the standard's mid-range defaults: `seatingRadius`, `flankRadius`, `tipDiameter` (m), `seatingAngleDeg`. Parameters the generator cannot expand are refused at `sketch_create` / `sk.sprocket` and are a typed `InvalidParameter` on the consuming extrude/revolve. |

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
| wasm-bridge | `crates/wasm-bridge/src/dispatch.rs` | **`SaveDocument`** (v4; NO payload since S2 C3c) ⇒ the session composes the file (`DocumentSession::to_document`): it already holds the metadata, every tab with its tree and thumbnail, and which tab is active, and it stashes the live tree into the active tab on the way. The engine attaches its `sources` table (embeds from the source store per `pack`), re-attaches unknown `document.*` keys captured at load, lifts any legacy inline payloads, and returns `SaveReady{json_data}` from `save_document_verified` (self round-trip; a corrupt document is a loud bridge error, not a dead file). An active tab of a kind this build cannot open keeps its content verbatim rather than being refused — the live tree is empty while such a tab is open, so there is nothing to stamp onto it. `SetDocumentMeta{name?, display_unit?, id?, created?}` is how the host keeps the session's metadata current: `id` and `created` are the HOST's to mint and latch (the storage record is keyed by the document's identity, v4 §4 inv. 1), and only `modified` is stamped by the engine at save time. `SaveProject` (legacy, tests) ⇒ the live tree as a one-tab v4 document with the sources table. `LoadProject{data}` ⇒ `load_document` (all migrations) — adopts `sources`, registers usable embeds into the engine store, rebuilds the active tab's tree, appends the loader's warnings to the rebuild warnings. `ProvideSource{source_id, data, resolved_commit?}` ⇒ host-fetched content registered (hash, `fetched_at`, and — for git — `resolved`) + rebuild. `ListSources` ⇒ `SourcesListed{sources: [{id, name, kind, locator, content_hash?, resolved?, pack, available}]}` — the host's worklist for content resolution (Phase 2 P2-3: cache by hash, else fetch at the RECORDED commit, then `ProvideSource`). `ImportStepFromLocator{file_name, locator, data, resolved_commit?}` ⇒ a LINKED `Step` source (no embed, hashed, resolved) + a feature naming it; `Local` locators refused. `OpenAssembly{tab_id}` (Phase 3b/3d-2; since S2 C3b the message is ONLY the tab's name — the session holds that tab's assembly and every same-document Part/Assembly tree its instances reference, which removed a re-send of every part tree on every evaluation. `tab_id` since S2 C3a: opening an assembly IS a tab switch, and the session makes that tab active. `EditAssembly{tab_id, assembly}` is how the panel's edits reach the session — it re-evaluates when that tab is the open one) ⇒ evaluates an Assembly tab: one engine per distinct part (same-document Part and Assembly trees supplied by the UI; linked-source tabs loaded from the source store), sub-assemblies recursed (cycle/depth guards), connector frames from geometry (a member connector's frame composed with the member's relative placement), placements solved per level; `ModelUpdated.assembly{placements, errors, warnings, parts, connectors: [{id, kind, origin, x_axis, y_axis, z_axis}] (every connector's evaluated frame in WORLD coordinates, and what it was derived from — the viewport's triads)}`; the per-body accessors then enumerate every leaf body (`instanceId` = top-level instance, `instancePath`, `instanceName`, `partTabId`, `transform` = world placement, body id `"{path…}/{feature}/{key}"`). `ListSourceTabs{source_id}` ⇒ `SourceTabsListed{tabs: [{id, name, kind}]}` for a linked `.waffle` source. `ProbeConnectorRef{instance_path, geom_ref}` ⇒ `ConnectorRefProbed{ok, kind?, reason?}` — judged against the ALREADY-evaluated assembly (no rebuild) so the app refuses a pick that derives no connector frame at creation instead of minting one that falls back to a default frame. `OpenPartInContext{tab_id, assembly_tab_id, instance_path}` (Phase 3d-4, in-context editing; three names and nothing else since S2 C3b — the part's tree is the session's copy of `tab_id`, which the switch makes live, and the assembly and its trees come from the session too) ⇒ evaluates the assembly as `OpenAssembly` does, makes `features` the live tree, snapshots every OTHER leaf (kernel handles + placement relative to the edited instance) as the engine's edit context, renders those leaves as ghost bodies baked into the edited part's frame (`context: true`, no `transform`; face/edge refs carry `scope`, planar faces a `plane {origin, normal}`), and reports `ModelUpdated.context{assembly_tab_id, instance_path, instance_name, placement, instances, errors, warnings}`; refused (loud) for an instance that is not a rendered same-document part. Any `SwitchTab`/`OpenAssembly`/`LoadProject`/`NewDocument` drops the context. `UpdateSourceEntry{source_id, pack?, git_ref?}` (P2-4) ⇒ writer policy (`pack:false` refused on an `Embedded` source) and retargeting: pinning to the commit already resolved keeps the content, any other ref drops content/hash/`resolved` and rebuilds (loud until the host re-resolves) — content from one commit is never labelled with another. Every `ModelUpdated` now carries `sources` (the `SourcesListed` rows) so the Sources panel is reactive. `ImportStep` ⇒ a packed `Embedded` `Step` source + a feature naming it (Import provenance). `RebaseSources{base, commit}` (Phase 2, fork of a linked document) ⇒ every `Relative` source becomes an absolute `Git` locator in `base`'s repository pinned at `commit`, `resolved` recorded, ids kept (`file_format::rebase_relative_sources`). `SwitchTab{tab_id}` (S2 C3; it carried the tree until then) makes that tab active: the session stashes the live tree and the undo history into the tab being left and loads the incoming tab's, so no tree crosses the wire and undo does not leak across tabs. `AddTab{kind, name?}` / `CloseTab{tab_id}` / `RenameTab{tab_id, name}` / `MoveTab{tab_id, index}` edit the tab bar in the session (the engine mints tab ids; closing the active tab hands over to its successor and rebuilds), and `SetDocumentMeta{name?, display_unit?}` (which replaced `SetDisplayUnit`) writes the document's metadata. Sources are document-scoped and survive a switch. `NewDocument` clears them. |
| JS document composer | `buildDocumentJson`, `app/src/lib/engine/store.svelte.js` | **No longer a writer.** Sends the UI-owned document metadata (`id`, `name`, latched `created`, `modified: now`, `display_unit`) and tab list (inactive tabs with their trees; unknown-kind tabs verbatim) to `SaveDocument` and returns the engine's bytes. Used by autosave (3 s debounce), Ctrl+S, provider sync and the download path. `initDocumentState` latches `document.id` (minting one for legacy files) and rewrites non-UUID tab ids exactly as the Rust migration does — but since S2 C3 the ENGINE's ids win: the store adopts them positionally from `ModelUpdated.document` (`adoptSessionTabIds`), because `SwitchTab` now names a tab and two independently minted id sets would name different ones. `format.js` mirrors the version constants for the engine-less read paths (home page, file-picker refusal). |
| JS new-doc template | `app/src/routes/home/+page.svelte` | Hand-writes a minimal empty v4 document (`document.id`, `sources: []`) from the `format.js` constants; the loader normalizes anything it lacks. |
| JS loader/bookkeeper | `initDocumentState` / `loadPendingDocument`, store.svelte.js:5395/5508 | Parses the document in JS for tab structure, then feeds the whole JSON to the Rust loader for the engine model. |
| Storage envelope — IndexedDB | `app/src/lib/storage/indexeddb.js` | DB `waffle-iron`, store `documents`, records `{id, json: <the .waffle text>, created, modified, link?}` (epoch ms). The `.waffle` JSON travels as an opaque string. **Record key (P2-5, 2026-09-08):** a record created since then is keyed by the document's own `document.id` (new documents, File→Open of a file that has one, forks); older records keep their 8-char base62 key and are NOT re-keyed. `get(id)` resolves a key first and then, for a UUID, the `document.id` inside any record's JSON (a linked read-only copy only when no record of the user's own carries it), so `/doc/<document.id>` opens every record and File→Open of an export re-homes to the stored document with that identity. `link` (Phase 2, `open-link.js` `DocumentLink`: `{locator, resolved: {commit, at}, contentHash, readOnly: true, name}`) marks a record created from a share link: the app treats it as **read-only** (no autosave, Ctrl+S refused) until "Fork to edit" copies it into an ordinary record with a new `document.id`. A separate DB `waffle-iron-cache` (`git/cache.js`) caches fetched source bytes by `content_hash`. |
| Storage envelope — git repositories | `app/src/lib/storage/git-provider.js` (`GitHubStore` in `github.js` is the same provider on `github.com/<login>/<repo>` with repo auto-creation) | One file per document (`<folder><slug>.waffle`) in a repository on GitHub, GitLab or Gitea/Forgejo, plus an index file `.waffle-index.json` (`[{id, name, filename, created, modified, displayUnit, tabCount}]`; `id` = the storage id, i.e. `document.id` for entries created since P2-5). Reads/writes go through the host adapters' write-back API (`git/hosts.js`); tokens per host (`git/tokens.js`); saved configs in localStorage `waffle-git-providers` (`providers.js`), registered at startup by `git-init.js`. A document stored here has a git location (`getLocator`), so its share link is the `/open` locator and its `Relative` sources resolve. Same opaque JSON. Documents can be **shared** — files must be treated as potentially untrusted input (§14.8). |
| sessionStorage handoff | keys `waffle-active-doc` / `waffle-active-json` / `waffle-active-link` | Route → editor transfer of the full JSON (and, for a linked record, its `DocumentLink`). |
| Open-from-link | `app/src/routes/open/+page.svelte`, `app/src/lib/storage/open-link.js`, `app/src/lib/storage/git/*` | `/open?remote=&path=&ref=` (also `?url=`; the legacy `/?src=<raw url>` redirects here) is the share link — a **locator**, not a copy (spec §7.4). Host adapters (`git/hosts.js`: GitHub, GitLab, Gitea/Forgejo, generic) resolve the ref to a commit and fetch the file **at that commit**; the git blob sha becomes `content_hash`; a private repository prompts for a per-host token (`git/tokens.js`, localStorage `waffle-host-tokens`; the legacy GitHub token still honored). The document opens linked and read-only; the GitHub provider's `getShareUrl` emits this link. Pinned by `app/tests/gui/git-links.spec.js` and `open-from-link.spec.js` against mocked APIs. |
| Assay corpus | `app/tests/cases/assay/*.waffle` (312 v3 files) + `crates/test-harness/src/assay/gen.rs:4874` | De-facto backward-compat pin: every kernel assay run loads the corpus through `file_format::load_project`. A change that breaks old files breaks the assay loudly. |
| JSON Schema | `docs/schema/waffle-v5.schema.json` (draft 2020-12, 55 `$defs`), generated by `crates/file-format/src/schema.rs` behind the `json-schema` cargo feature (schemars derives on every persisted type in waffle-types/feature-engine/file-format; hand-written schemas for the three opaque enums `TabKind`/`SourceKind`/`Locator`). Pinned by `tests/schema_golden.rs`: the committed file must equal the generated one (regenerate with `UPDATE_SCHEMA=1 cargo test -p file-format --features json-schema --test schema_golden`), every repo `.waffle` file must validate after migration, a malformed Part tab and a locator-less source must NOT validate, an unknown tab kind MUST. CI runs it as its own step (`rust-test.yml`). Tooling that writes `.waffle` files should validate against it before submitting. |
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
   variants — unavoidable retroactively. Since v4, unknown *keys* at the
   envelope/document/tab/source/feature-tree levels survive a resave (§3.1,
   §5.2, §5.5, §6.1), and unknown tab kinds, source kinds, locator kinds and
   (Phase 1b) operation kinds are preserved opaquely (§5.3, §5.5, §7). Unknown
   keys *inside* a known operation's params, a `GeomRef`, a sketch entity or a
   constraint are still dropped on resave.
3. **Version-bump rule (now explicit):** bump `version` for value-reinterpreting
   or structural changes (v1→v2 units, v2→v3 tabs, v3→v4 identity/sources).
   Bump `MIN_READER_VERSION` (Rust `save.rs` + JS `format.js`, together with
   `version`) for **any** change old readers cannot parse — which includes new
   constraint/selector/`PlaneDefinition` *variants*, not just structural
   changes, and a new field that a reader must not silently ignore (v5:
   `GeomRef.scope`). Purely additive defaulted fields need no bump. **Since v4, new tab
   kinds, source kinds and locator kinds need no bump, and since Phase 1b
   neither do new operation kinds**: v4 readers preserve unknown ones opaquely
   (§5.3, §5.5, §7).
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
   is deleted; since 2026-09-08 kernel-v2 writes STEP itself —
   `kernel_v2::step_export`, analytic AP214 — and a kernel without it surfaces
   `NotSupported` as `ExportError::StepExportFailed`,
   `crates/file-format/src/step_export.rs`).
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
   **FIXED 2026-09-07:** one writer — `buildDocumentJson` routes through
   the bridge `SaveDocument` message and the Rust `save_document_verified`
   (§12) — and a JSON Schema generated from the Rust types is committed at
   `docs/schema/waffle-v5.schema.json` (§12) and pinned in CI.
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
2. **Single writer — DONE 2026-09-07.** `buildDocumentJson` routes through the
   bridge `SaveDocument{document, tabs, active_tab}` message and the Rust
   `save_document_verified`; `document-format-seam.spec.js` pins the v4 envelope
   as seen from the app.
3. **Forward-compat policy — DONE (v4).** `min_reader_version` is written and
   enforced (§13.2-3). Unknown-field preservation landed at the envelope,
   `document`, `Tab`, `SourceEntry` and `FeatureTree` levels; unknown tab,
   source and locator kinds are preserved opaquely (§5.3, §5.5). Not preserved
   inside `Feature`/`GeomRef`/sketch entities/operation params (dense structs
   with ~70–120 literal sites each; documented in the v4 spec §2.6).
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
