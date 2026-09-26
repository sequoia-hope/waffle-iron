# KiCad board link — `KicadPcb` sources, derived board, footprint instances, board data on hover

> Phase 3b of `specs/waffle_v4_document_model.md` §9. Assessment that led
> here: `docs/kicad_board_link_assessment.md` (2026-09-26). Governance:
> `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3.2 — sections 1–7a below
> are the required ones; §8 is the landing order.
>
> Scope discipline: the `.kicad_pcb` is a **source**, the board and its
> components are **Derived** content, and everything the user authors on top
> (enclosure, mates, connectors) is ordinary content that a re-sync must
> not touch. This spec introduces no surface-surface intersection (§7a).

## 1. Goal

A user pastes the URL of a `.kicad_pcb` in a git repository (GitHub, GitLab,
Gitea/Forgejo; a share-link `/open?remote=…&path=…&ref=…` form is
equivalent) or links one through the Sources panel. The app then:

1. Adds one `KicadPcb` source entry (linked, pinned to the resolved commit,
   hashed, cached) and, when a board STEP sits beside it, one linked `Step`
   source for it.
2. Creates a **Board** Part tab holding an exact board solid: the `Edge.Cuts`
   outline as a sketch on the XY plane and an extrude by the stackup
   thickness, both with `Derived` provenance. Users sketch on it, build
   offset planes from it, and boolean an enclosure against it exactly as
   with any native body.
3. Creates a **Board assembly** tab: the board instance grounded at the
   origin, one instance per footprint (keyed by the footprint UUID in
   `Instance.external_key`) placed from the footprint's position, side and
   3D-model offset, and a mate connector on every mounting hole.
4. Shows board data in the viewport: hovering a component shows a card with
   reference designator, value and footprint; clicking it opens a detail
   panel with the full record, the nets on its pads, its datasheet link and
   an "open at this commit on the host" link. Hovering the board shows the
   title block, revision, layer count and thickness.
5. Keeps the link live: "Update to tip" re-fetches the board at the new
   commit and regenerates every Derived feature and instance while
   preserving the user's own features, connectors, mates and instance
   renames, reconciling components by footprint UUID.

Non-goals (this spec): copper, silkscreen, courtyard or drawing geometry;
`.kicad_sch`; writing anything back to KiCad; models fetched from the KiCad
3D library through a path-variable table (§8 C6 sketches it, a later spec
lands it).

## 2. Parameters

### 2.1 Inputs

| Input | Type | Default | Units | Valid | Error |
|---|---|---|---|---|---|
| `locator` | `Locator` (`Git` \| `Relative` \| `Url`; `Embedded` for a file picker) | — | — | shareable, `path` ends in `.kicad_pcb` | `Local` ⇒ `InvalidRequest` (same rule as `ImportStepFromLocator`) |
| `data` | UTF-8 text of the `.kicad_pcb` | — | — | S-expression whose head is `kicad_pcb` with `(version N)`, `N ≥ 20211014` (KiCad 6 format) | older or non-`kicad_pcb` head ⇒ `KicadParse::UnsupportedVersion` / `NotAKicadPcb` |
| `resolved_commit` | 40/64-hex | `None` for non-git | — | — | — |
| `board_step` | `Option<{locator, data, resolved_commit}>` | `None` | — | ISO 10303-21 text | parse failure ⇒ the board and instances still land; the STEP source reports `SourceUnavailable`-style warning `BoardStepUnusable{reason}` |
| `components` | enum `Placeholders` \| `BoardStep` | `BoardStep` when `board_step` is given, else `Placeholders` | — | — | `BoardStep` without a `board_step` ⇒ `InvalidRequest` |
| `sides` | set of `{Front, Back}` | both | — | — | — |
| `mounting_holes` | bool | `true` | — | — | — |

### 2.2 Values read from the `.kicad_pcb`

Coordinates in the file are **millimetres, Y down, angles in degrees
counter-clockwise**. The engine world is metres, Z up. Every value below is
converted at parse time; nothing downstream sees a millimetre.

| Field | S-expression | Required | Default when absent |
|---|---|---|---|
| format version | `(version N)` | ✔ | — |
| board thickness | `(general (thickness T))`; overridden by the sum of `(setup (stackup (layer … (thickness t))))` when a stackup is present | ✔ (one of the two) | `Missing thickness` is a loud failure — a board of unknown thickness is not extruded |
| title block | `(title_block (title …) (rev …) (date …) (company …) (comment N …))` | — | empty strings |
| copper layer count | `(layers …)` entries whose name ends in `.Cu` | — | 0 |
| outline | every `gr_line`, `gr_arc`, `gr_circle`, `gr_rect`, `gr_poly` whose `(layer "Edge.Cuts")`, plus footprint-level `fp_*` graphics on `Edge.Cuts` (a footprint that cuts the board, e.g. a slot) | ✔ (≥ 1 closed loop) | no closed loop ⇒ `OutlineNotClosed` |
| footprints | `(footprint "Lib:Name" (layer "F.Cu"\|"B.Cu") (uuid U) (at x y [rot]) (property "Reference" "R1" …) (property "Value" …) (property "Footprint" …) (property "Datasheet" …) (attr …) (pad …)* (model "path" (offset (xyz …)) (scale (xyz …)) (rotate (xyz …)))*)` | — | a board with no footprints yields an assembly with the board alone |
| pads | `(pad "1" thru_hole\|smd\|np_thru_hole circle\|oval\|rect\|roundrect (at x y [rot]) (size w h) (drill d) (net N "NAME"))` | — | — |
| nets | `(net N "NAME")` at top level | — | — |

Older aliases accepted: `(tstamp U)` for `(uuid U)` (KiCad 6/7 files);
`(fp_text reference "R1" …)` for the `Reference` property (KiCad 6). Every
other token is skipped without error; a **count** of skipped top-level
forms is reported as one warning, never one warning per form.

### 2.3 Derived content, exactly

**Board sketch** (`Derived{source_id, rule: "kicad.board_outline"}`): plane
XY, origin `(0,0,0)`, normal `+Z`, `plane_x_axis = +X`. Entities: one `Point`
per distinct outline vertex (welded at `TAU_MODEL`), one `Line` per
`gr_line`/`gr_rect` edge/`gr_poly` edge, one `Arc` per `gr_arc`, one
`Circle` per `gr_circle`. File `(x, y)` maps to sketch `(u, v) = (x·1e-3,
−y·1e-3)`: the Y flip mirrors orientation, so a file arc `start → mid →
end` (KiCad stores arcs as three points, direction implied by the mid point)
is emitted with start/end **swapped when the flip reverses its sweep** so
that it still sweeps counter-clockwise from `start_id` to `end_id` about
`center_id` in sketch coordinates; the centre is recomputed from the three
points exactly (circumcentre), never read from any cached `(center …)`.

**Board extrude** (`Derived{…, rule: "kicad.board_extrude"}`): `sketch_id`
= the board sketch, `profile_entity_ids` = the outer loop's entity ids
(§2.9 addressing, so the solver's index order is irrelevant), `depth =
thickness_m`, `direction = +Z`, `symmetric: false`, `cut: false`, `merge:
false`. Inner closed loops (cutouts, slots) become one Derived cut extrude
each (`rule: "kicad.board_cutout"`, `cut: true`, same depth) — a hole in
the outline is a hole in the board, not a second solid. The board's top
face (copper side `F.Cu`) is at `z = thickness_m`, its bottom at `z = 0`.

**Board assembly** (tab kind `Assembly`, name `"<board name> assembly"`):

- instance `board`: `PartRef{tab_id: <Board tab>}`, `fixed: true`,
  identity transform, `external_key: "board"`.
- per footprint `F` with `sides ∋ side(F)`: instance `name = Reference`,
  `external_key = uuid(F)`, `source` per §3 row C1–C3, `transform` =
  `T_board_side(F) ∘ T_place(F) ∘ T_model(F)` where
  - `T_place = translate(x·1e-3, −y·1e-3, 0) ∘ rotZ(rot)` (front) —
    rotation is counter-clockwise in the file's own frame; after the Y
    flip the same counter-clockwise angle applies about `+Z`.
  - `T_board_side = translate(0,0,thickness_m)` for `F.Cu`; for `B.Cu`
    it is `rotX(π)` about the board's bottom plane (`z = 0`), so a back
    component's `+Z` points to `−Z` and its footprint origin sits on the
    bottom face. KiCad stores a back footprint's `at` already mirrored in
    X; the composition here is the one `kicad-cli pcb export step` applies
    and is pinned by oracle O5.
  - `T_model = translate(offset·1e-3) ∘ rotZ(rz) ∘ rotY(ry) ∘ rotX(rx)`
    from the first `model` of `F` (`rotate` is applied as `Rz·Ry·Rx` with
    KiCad's sign convention, which is **negated** relative to right-hand
    rotation: the file's `(rotate (xyz 0 0 90))` is a −90° turn about Z;
    pinned by O5). `scale` must be `(1 1 1)`; anything else is a per-instance
    warning and the scale is ignored (an instance has no scale).
- per mounting hole `H` (a pad of kind `np_thru_hole`, or a `thru_hole`
  pad with no `net`, in a footprint whose `Footprint` property starts with
  `MountingHole`): a `MateConnector` on the board instance,
  `instance_path = [board]`, `frame = { origin: (hole centre on the top
  face), z_axis: +Z, x_axis: +X }`, `name = "<Reference> hole"`.
  Provenance for instances and connectors is carried in
  `Instance.extra["x-derived"] = {source_id, rule}` / the same key on the
  connector, because those structs have `extra` maps and no provenance
  table (§2.6 of the v4 spec allows `x-` keys at structural levels).

**Metadata** (engine-held, not in features): `BoardMeta{source_id, title,
rev, date, company, comments, copper_layers, thickness_m, net_count,
footprint_count}` and `ComponentMeta{footprint_uuid, reference, value,
footprint, datasheet, side, attrs, pads: [{number, net_name}]}`.
Persisted? **No** — regenerated from the source at load (the `.kicad_pcb`
is either embedded or fetched, and the meta is a pure function of it).

### 2.4 Bridge surface

- `UiToEngine::LinkKicadFromLocator{file_name, locator, data,
  resolved_commit?, board_step?: {file_name, locator, data,
  resolved_commit?}, components?, sides?, mounting_holes?}` ⇒ two new tabs
  and up to two sources; reply `ModelUpdated` with the Board tab active.
- `UiToEngine::ImportKicad{file_name, data, board_step_data?}` ⇒ the
  `Embedded` twin (file picker / paste).
- `UiToEngine::QueryEntityMeta{body_id?: String, instance_path?: [Uuid]}`
  ⇒ `EngineToUi::EntityMeta{board?: BoardMeta, component?: ComponentMeta,
  source?: {source_id, locator, resolved_commit}}`; an id that derives
  from no KiCad source answers with all fields `None` (not an error — the
  hover card simply does not show).
- `ProvideSource` on a `KicadPcb` source (P2-4 "Update to tip") ⇒ §3 rows
  R1–R4.
- MCP (`specs/waffle_mcp_server.md`): `kicad_link` (the locator form),
  `entity_meta` (the query). Derived features already refuse `feature_edit`
  with `DerivedFeatureReadOnly`; instances and connectors carrying
  `x-derived` get the same refusal in `instance_edit` / `connector_edit`
  for every field except `name` and `suppressed` (what a re-sync preserves,
  §3 R3).

## 3. Branch table

### Link

| # | `components` | `board_step` | Side of footprint | Result |
|---|---|---|---|---|
| C1 | `Placeholders` | — | any in `sides` | instance with `PartRef{tab_id: <generated "Component placeholder" Part>}`: one shared Part tab holding a Derived box `courtyard × 1 mm` centred on the footprint origin (the footprint's `F.CrtYd`/`B.CrtYd` bounding box; the pad bounding box when no courtyard). Same tab for every footprint; the instance transform does the placing. |
| C2 | `BoardStep` | present, product tree carries one product per footprint whose name equals the footprint's `Reference` | any in `sides` | instance with `PartRef{source_id: <Step source>, tab_id: <product name>}`: the linked STEP's per-product sub-body (§5 O6 pins the name match); the instance transform is **identity** because the product is already placed by the STEP's own `ITEM_DEFINED_TRANSFORMATION` (a board STEP is world-placed), and the footprint transform of §2.3 is used only for the oracle O5 comparison. |
| C3 | `BoardStep` | present, no product named like the footprint | any | as C1 for that footprint, plus warning `ComponentModelMissing{reference}`; no position matching (a silent wrong model is worse than a box). |
| C4 | `BoardStep` | present, product tree has the board itself (product named `board`/`<file stem>` or the largest planar product) | — | ignored: the exact Derived board is the board; warning only if no such product is found (nothing to ignore is fine). |
| C5 | any | — | side ∉ `sides` | no instance; the footprint still counts in `BoardMeta.footprint_count`. |
| C6 | any | — | footprint with `(attr board_only)` or `(attr exclude_from_bom)` and no `model` | no instance, no warning (KiCad's own STEP export skips these). |
| C7 | any | — | `mounting_holes: true` and the footprint qualifies (§2.3) | connector per qualifying pad. |

### Outline

| # | Outline content | Result |
|---|---|---|
| O1 | one closed loop | board = extrude of that loop |
| O2 | one outer + n inner closed loops (inner ⊂ outer) | board = extrude(outer) then n Derived cuts |
| O3 | ≥ 2 closed loops, none containing the others (a panel of boards) | **loud** `MultipleBoardOutlines{count}`: the user chooses (future: one Board tab per outline) |
| O4 | an open chain (gap > `TAU_MODEL` between consecutive primitive endpoints after welding) | **loud** `OutlineNotClosed{gap_m, at}`; no gap-closing, no tolerance escalation (P9/P10) |
| O5 | a `gr_circle` alone on `Edge.Cuts` | closed loop of one `Circle` entity (a round board) |
| O6 | self-intersecting loop | **loud** `OutlineSelfIntersecting{at}` (detected by the sketch solver's region extraction: zero or > 1 outer regions) |
| O7 | `gr_arc` with collinear start/mid/end | treated as a `Line` (radius is infinite); warning |

### Re-sync (`ProvideSource` with a new `content_hash` on a `KicadPcb` source)

| # | Change upstream | Result |
|---|---|---|
| R1 | outline or thickness | board sketch + extrudes regenerated in place (same feature ids ⇒ the user's sketches on the board's faces re-resolve through persistent naming, `docs/PERSISTENT-NAMING.md`; a face that no longer exists fails loudly, as any face reference does) |
| R2 | footprint added | new instance (and connector), `external_key` = its uuid |
| R3 | footprint moved / renamed / value changed | the instance with that uuid keeps its id, its user `name` override (if the user renamed it) and `suppressed`; `transform`, `source` and meta are regenerated; a user mate that references it survives |
| R4 | footprint deleted | its instance and connectors are removed; a user mate that referenced them is reported `MateTargetGone{instance}` and left in place (loud, not deleted) |
| R5 | board STEP changed as well | the `Step` source updates in the same action; C2/C3 re-evaluated per footprint |
| R6 | the same hash | no-op |

## 4. Invariants

1. **Exactness of the board.** Every outline vertex `p_file` maps to a
   sketch point `(x·1e-3, −y·1e-3)` with no other rounding; arc centres are
   the exact circumcentre of the three file points. The board is a
   kernel-v2 arena solid that passes `validate_solid` (manifold, closed,
   Euler `V−E+F = 2(S−H)` with `H` = number of cutouts).
2. **Volume.** `V_board = A_outer − Σ A_inner` times `thickness_m`, with
   `A` the exact signed area of the sketch loops (`planar_loop_signed_area`
   for line+arc loops), to `1e-9` relative.
3. **Placement.** For every footprint `F` on the front, the instance
   transform maps the origin to `(x·1e-3, −y·1e-3, thickness_m)`; on the
   back to `(x·1e-3, −y·1e-3, 0)` with `z_axis · (0,0,−1) = 1`.
4. **Identity.** `Instance.external_key` is the footprint uuid; two
   footprints never share one; a re-sync never mints a new instance id for
   a uuid that already has one.
5. **Derived is read-only.** No `feature_edit` / `instance_edit` (beyond
   `name`, `suppressed`) succeeds on Derived content; every regeneration is
   a whole-rule replacement, never a partial patch.
6. **The source is the truth.** `BoardMeta`/`ComponentMeta` are pure
   functions of the `.kicad_pcb` bytes at `content_hash`; loading the
   document offline from its embed reproduces them byte-identically.
7. **No network on rebuild.** Link and re-sync fetch; rebuild never does
   (v4 §2.4 pin/update semantics).
8. **Wire impact: none.** No new field on `Feature`, `Instance`,
   `MateConnector` or `SourceEntry`; `x-derived` lives in `extra` maps; no
   `MIN_READER_VERSION` bump.

## 5. Oracles

| # | Oracle | Branches |
|---|---|---|
| O1 | Parser golden: `tests/fixtures/*.kicad_pcb` (authored, ≤ 20 KB each) → canonical JSON of §2.2 fields, pinned; one fixture per KiCad major (6, 7, 8, 9 syntax variants: `tstamp`/`uuid`, `fp_text`/`property`, `(rotate …)` presence) | all |
| O2 | Board volume = exact area × thickness (`Kernel::volume`, rel. `1e-9`) on: rectangle (`50×30×1.6 mm` ⇒ `2.4e-6 m³`), rectangle with two `Ø3.2 mm` holes, rounded-rect from four `gr_arc`s, `gr_circle` disc | O1, O2, O5 |
| O3 | Face count of the rounded-rect board = 2 caps + 4 planar + 4 cylindrical; each cylindrical face reports the arc radius exactly | O1 |
| O4 | Open-outline fixture with a `0.01 mm` gap ⇒ `OutlineNotClosed{gap_m ≈ 1e-5}`; panel fixture ⇒ `MultipleBoardOutlines{2}`; bow-tie ⇒ `OutlineSelfIntersecting` | O3, O4, O6 |
| O5 | **Placement parity against KiCad's own STEP export.** Fixture `two_sided.kicad_pcb` (one front and one back footprint, rotated 37° and 123°, model offset `(1, 2, 0.5) mm`, rotate `(0 0 90)`) and its `kicad-cli pcb export step` output, generated once and committed as a fixture (our own board, our own dummy models ⇒ our licence). For each product the STEP's folded transform must equal §2.3's `T_board_side ∘ T_place ∘ T_model` to `1e-9 m` / `1e-9 rad`. This pins the two sign conventions the spec asserts (back-side flip, `rotate` negation). | C2, invariant 3 |
| O6 | Product naming: in the O5 STEP, every product name equals a `Reference` of the fixture; the C2 match rate on the fixture is 100 %, on a fixture with one un-modelled footprint it is n−1 with exactly one `ComponentModelMissing` | C2, C3 |
| O7 | Re-sync: link fixture A, user adds a sketch on the top face, an enclosure extrude, one Fastened mate to `R1`, renames `C1` to `bulk cap`; provide fixture A′ (R1 moved, C1 value changed, R2 added, U1 deleted). Assert: enclosure and sketch features unchanged (ids, params); `R1` instance id unchanged, transform new; `C1` name still `bulk cap`, meta value new; `R2` present; `U1` gone with `MateTargetGone` only if it had a mate; the document round-trips and re-loads identically | R1–R6 |
| O8 | Metadata query: every body id enumerated by `getBodies()` for the assembly answers `QueryEntityMeta` consistently — `component.reference == instance.name` (unless renamed) and `board.footprint_count == fixture count` | §2.4 |
| O9 | Offline determinism: save with `pack: true`, clear the content cache, reload with the network adapter stubbed to throw; the board, instances and meta are identical | invariant 6, 7 |
| O10 | GUI (Playwright): paste the fixture URL (served by the test relay), see the board and instances; hover `R1` ⇒ card contains `R1`, its value and footprint within `500 ms`; click ⇒ panel shows pads with net names and a link whose href contains the resolved commit | goal 4 |

## 6. Failure modes

| Condition | Behaviour |
|---|---|
| not a `kicad_pcb` / unsupported version | `KicadParse::NotAKicadPcb` / `UnsupportedVersion{found, min}`; no tab created; source entry **not** added (nothing to hang it on) |
| malformed S-expression (unbalanced parens, bad number) | `KicadParse::Syntax{line, col, expected}`; as above |
| missing thickness | `MissingThickness`; as above |
| outline open / multiple / self-intersecting | §3 O3, O4, O6 — the Board tab is created with the sketch only (so the user can see the gap) and the extrude reports the loud error; instances still land |
| `Local` locator | refused, as `ImportStepFromLocator` |
| board STEP unparseable | board + placeholder instances land; `Step` source entry present with warning; C2 falls to C1 for all |
| model `scale ≠ 1` | per-instance warning; ignored |
| footprint with no `model` and `components: BoardStep` | C3 |
| duplicate footprint uuid in the file | `DuplicateFootprintUuid{uuid}`; loud; no instances created (the key would be ambiguous) |
| re-sync removes a footprint that a user mate references | `MateTargetGone`; mate kept |
| `QueryEntityMeta` for an unknown id | all-`None` answer (not an error) |
| STEP over the host's raw-content limit | already handled by the adapter's raw fallback; the embed inflation cap (v4 §6) applies unchanged |

Nothing in this table is silent. No tolerance is widened to make an outline
close; `TAU_MODEL` is the weld distance and the only band.

## 7. Research basis

- **KiCad board file format** (S-expression, `kicad_pcb` node, version
  stamps, footprint/pad/graphic grammar, arc as start/mid/end since v6):
  REFERENCES.md #59. We implement a tolerant reader of the documented
  grammar; the only interpretation choices (back-side composition, `rotate`
  sign) are pinned by oracle O5 against KiCad's own STEP export rather than
  by reading its source.
- **`kicad-cli pcb export step`** (product per footprint named by
  reference, board product, world placement): REFERENCES.md #60. Used as an
  oracle and as the component-model supply (C2); never as a dependency.
- **Derived / linked content and re-sync**: the v4 document model's own
  precedents (FreeCAD `App::Link`, Onshape derived features, Cargo lockfile
  semantics) — v4 spec §8. Reconciling by a stable external key rather
  than by name is the standard ECAD↔MCAD practice (IDF/IDX "reference
  designator + unique id" pairing, REFERENCES.md #61).
- **Outline → sketch**: no algorithm beyond endpoint welding, loop chaining
  and the sketch solver's existing region extraction; the exact
  circumcentre and exact signed area reuse kernel-v2's `exact2d` helpers.

### 7a. Analytical vs approximate

No surface-surface intersection is introduced. The board is a planar
extrude of lines and arcs (planar and cylindrical faces, both exact). The
cutouts are extrude-cuts of a planar profile through a planar-topped
solid, which the existing boolean pipeline resolves exactly (plane×plane
and plane×cylinder pairs, already covered). Component models arriving
through STEP are mesh-backed imported bodies exactly as today's STEP import
produces and participate in no boolean (roadmap SI1 wall unchanged).

## 8. Increments (each atomic, tests first)

| # | Landing | Proves |
|---|---|---|
| C1 | `crates/kicad-pcb/`: tokenizer + reader for §2.2, unit conversion, outline chaining; fixtures + golden (O1, O4). **LANDED 2026-09-26**: `sexpr` / `read` / `model` / `outline` modules, `parse_kicad_pcb` + `Pcb::outline_loops`; 8 authored fixtures (KiCad 6/8/9 syntax, `gr_poly` with an embedded arc, a footprint-level `fp_*` slot placed by the footprint's rotation, disc, gap, panel, collinear arc, legacy v5, duplicate uuid, missing thickness); 33 tests incl. O2's exact areas (rounded rect `2300 + 25π mm²`, holes, slot `8 + π`) and O4's `gap_m = 1e-5`. Decisions the code records: the `Pcb` stays in the file's Y-down frame (the flip is §2.3's, in C2); a back-side footprint's local pad/graphic coordinates are placed by the same `RotatePoint + at` rule as the front (the file stores them already mirrored — to be pinned by O5 in C3); stackup thickness = copper + core + prepreg (mask excluded), used over `general.thickness` with a warning when they differ. `BoardMeta`/`ComponentMeta` are derived views the engine builds in C2, not parser output. | parser |
| C2 | `feature-engine`: `derive_board(source_id, &Pcb) -> (Vec<Feature>, AssemblyTree, Vec<MateConnector>, meta)` with Derived provenance; `wasm-bridge` `ImportKicad` + `LinkKicadFromLocator` with `components: Placeholders`; O2, O3, O9. **LANDED 2026-09-26**: `feature_engine::kicad::{derive_board, DerivedBoard::assembly, BoardMeta, ComponentMeta}`, bridge `ImportKicad` / `LinkKicadFromLocator`, `EngineState.kicad_boards` (the metadata record C4 will query); 9 real-kernel tests (`wasm-bridge/tests/kicad_link_tests.rs`): O2 exact on the rectangle, the two-hole board and the rounded board with slot and poly cutout; Derived provenance on every feature; instances keyed by footprint uuid; the H1 connector; `OpenAssembly` evaluates. **Four things the implementation settled differently from the text above, each for a reason found on the real kernel:** (1) cutouts live in their OWN Derived sketch (`kicad.board_cutouts`) so the outer profile is exactly the outer loop and each hole is a standalone profile; (2) each cut is chained onto the PREVIOUS feature's body (a cut consumes its target — the second cut naming the board extrude was a loud `already consumed`); (3) a cut is a **symmetric extrude three thicknesses tall** (−1.5t…+1.5t), not a `depth = thickness` slab: a slab shares both cap planes with the board and hits the kernel's loud coplanar Stage-0 wall on line+arc cutters (the circular holes happened to pass) — a through-hole is the mechanical intent anyway; (4) placeholders are one Part tab **per distinct footprint name** (pad bounding box × 1 mm in the footprint's own frame), not one shared box — a shared box has no size. Also: engine-authored sketches must go through `script::host::derive_sketch` (chord-only profiles otherwise: measured 50 mm² short on the rounded board); `T_model` is NOT applied to placeholders (it belongs to the STEP model, C3); `sides` / `mounting_holes` are `DeriveOptions` in the engine but not yet on the wire (defaults: both sides, holes on). O3 (face count) and O9 (offline reload) are still open for C4/C5. | exact board, instances, connectors |
| C3 | `step-import` per-product split (`ImportedBodyData.products: Vec<{name, shells}>`, roadmap SI4, composite unchanged for `ImportStep`); `Step` source sub-body `PartRef{source_id, tab_id: product}`; C2/C3 rows; O5 fixture generated and committed, O5 + O6 | component models, sign conventions |
| C4 | `QueryEntityMeta`; app hover card + click panel (the first tooltip component: positioned from the existing hover pointer coordinates, `aria-live`, dismiss on leave/esc); Sources panel "Link KiCad board…"; MCP `kicad_link`, `entity_meta`; O8, O10. **LANDED 2026-09-26**: bridge `QueryEntityMeta` → `EntityMeta{board, component, source}` (an assembly body id resolves through its first segment, a live-part body through the open tab); `EngineState.kicad_boards[].board_instance`; MCP `kicad_link` (`pcb_text`, optional `locator`/`resolved_commit`) and `entity_meta` (both in `MIGRATED`, the manifest regenerated, the routing pins extended); app: `importKicadFromText` / `importKicad` (file picker) / `linkKicadFromLink`, the link dialog in a `kicad` kind (toolbar "KiCad" and "Link KiCad", and a "link KiCad…" button in the Sources panel — the panel only renders once a source exists, so the toolbar carries the first link), `KicadHoverCard.svelte` + `KicadDetailPanel.svelte` (`app/src/lib/viewport/`), the store's `proposeEntityCard` / `openEntityDetail` with one cached `QueryEntityMeta` per (tab, body); `kicad-link.spec.js` drives the real dialog against a mocked GitHub API, hovers the board and R1 with the real pointer, clicks for the panel (O8 in `kicad_link_tests.rs`, O10 in the GUI spec). **Two things the GUI found:** (1) the card's visibility follows the store's arbitrated `hoveredRef`, not any listener's leave event — the vertex/edge overlays propose from DOM listeners and Threlte's mesh pointer-out lands a frame later, so a leave-driven hide raced the proposal off; the overlays now propose the card too, since a small placeholder on screen is mostly its corners; (2) a mesh record's `instancePath` is a `$state` proxy and cannot be structured-cloned to the worker (`DataCloneError`) — the query copies it to a plain array. `sides`/`mounting_holes` still not on the wire; O9 open for C5. | goal 4 |
| C5 | Re-sync: `ProvideSource` on `KicadPcb` regenerates Derived content by rule, reconciles by `external_key`; `DerivedFeatureReadOnly` for instances/connectors; O7. **LANDED 2026-09-26**: `feature_engine::kicad::{resync_features, resync_assembly, MateTargetGone}` and the bridge's `resync_kicad` on `ProvideSource` (R6: same hash ⇒ nothing regenerated, the commit still recorded; unreadable new bytes ⇒ refused BEFORE the entry changes); features keep their ids by (rule, ordinal) — a fresh cutout lands after the last derived feature, a lost one drops with its provenance row; instances and connectors keep their ids by (rule, key, ordinal), `suppressed` always, `name` when it differs from the name the rule minted (now stamped as `x-derived.name` on every derived instance and connector); a removed footprint takes every connector on it and each mate that used one is reported `MateTargetGone{mate, instance, footprint}` in the warnings and left in place (the evaluation names it as its one error); placeholder tabs: regenerated in place per footprint name, added for a new shape, closed for an unused shape unless the user built on it (warning). The re-sync locates its tabs from the tabs themselves — `FeatureTree.extra["x-derived"]` (`kicad.board_part` / `kicad.placeholder` with the footprint name) and the `kicad.board` instance — so **`EngineState.kicad_boards` is now rebuilt from the sources and the tabs** (`refresh_kicad_records`, also at `LoadProject`): O9's offline half holds (`kicad_resync_tests.rs`: save → reload → save is byte-identical apart from `modified`, and the reloaded engine answers `QueryEntityMeta` with the new values from the embed alone). `instance_edit` refuses `transform`/`fixed` and `connector_edit` refuses `anchor`/`flip_z`/`rotation_deg`/`offset_m` on `x-derived` content with `DerivedFeatureReadOnly{refused}`. The UI's own assembly panel (`EditAssembly`) is not gated — a whole-tree write from the panel can still move a derived instance, and the next re-sync puts it back. `sides`/`mounting_holes` still not on the wire. O5/O6 (C3) still open. | goal 5 |
| C6 (later spec) | Path-variable table (`${KICAD*_3DMODEL_DIR}` → a git locator template), per-model `Step` sources, `listDirectory` for repo-level links | boards without a committed STEP |

Exit for this spec's Phase 1 (FIP §3.3): every parameter enumerated (§2),
every branch (§3, 20 rows) has a numeric oracle (§5), failure modes listed
(§6).
