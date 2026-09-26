# KiCad board links — gap assessment (2026-09-26)

> Question: the v4 document-model spec (`specs/waffle_v4_document_model.md`
> §9, Phase 3b) describes linking a `.kicad_pcb` on GitHub, pulling in the
> associated STEP and board metadata, and showing rich board data on hover
> and click. What would it take to drive that forward?
>
> This is an assessment, not a spec. The Phase 3b spec is the first
> checkpoint below — written the same day as `specs/kicad_board_link.md`,
> whose §8 tracks what has landed (C1, the parser crate, 2026-09-26). Priority note: this is app / file-format work and sits
> below the Yang pipeline in `CLAUDE.md`'s priority order.

## 1. What the substrate already delivers

| Piece | State | Where |
|---|---|---|
| Git-aware locators, pin / update-to-tip, blob-hash cache | LANDED (Phase 2, 2026-09-08) | `app/src/lib/storage/git/*`, `app/src/lib/storage/sources.js` |
| Host adapters: resolve ref → commit, fetch a file at a commit | GitHub, GitLab, Gitea/Forgejo; `generic` only for pinned commits | `app/src/lib/storage/git/hosts.js` |
| Pasted file URL → locator | LANDED | `sources.js` `locatorForImportLink` |
| "Link STEP" from a locator | LANDED | `wasm-bridge` `ImportStepFromLocator` (`dispatch.rs`) |
| `SourceKind::KicadPcb` | wire tag only; no handler anywhere; app never mentions KiCad | `crates/file-format/src/sources.rs` |
| `Instance.external_key` ("a KiCad footprint UUID") | in the wire format and schema golden; written by no Rust code, read by nothing | `crates/feature-engine/src/assembly.rs` |
| `ProvenanceOrigin::Derived{source_id, rule}` | defined; MCP refuses to edit Derived features; **never produced** | `crates/feature-engine/src/types.rs`, `wasm-bridge/src/tools/author.rs` |
| STEP import | mesh-backed composite body; assembly walked then **flattened**; product names, colours and instance identity discarded; b-splines as `Freeform` | `crates/step-import/`, `docs/step_import_roadmap.md` |
| Assemblies: instances with `PartRef{source_id?, tab_id}`, connectors, mates, linked-source instances, in-context editing | LANDED | `projects/10-assemblies/PLAN.md` |
| Viewport hover | picked `GeomRef` + client coordinates already reach the store; used only for recolouring | `app/src/lib/viewport/CadModel.svelte` |
| Tooltip / popover / info panel component | **none exists** | — |
| S-expression parser | **none**; no dependency could parse `.kicad_pcb` | — |
| Feature-level re-sync from a changed source | **none**; update-to-tip only re-provides bytes | `store.svelte.js` `updateSourceToTip` |

## 2. The two decisions the spec must settle

### 2.1 Where the board solid comes from — recommend DERIVE

Two candidates:

- **Derive it** (what §9 3b says): parse `Edge.Cuts` into a sketch (lines,
  arcs, circles, rects), extrude by the stackup thickness, tag the sketch and
  extrude `Derived{source_id, rule: "kicad.board"}`. Result is a native
  kernel-v2 solid: sketch on it, offset planes from it, boolean an enclosure
  against it. Cost: a parser and an outline → sketch mapper (arc chaining,
  closed-loop detection, Y-down → Y-up, mm → m).
- **Import the STEP board body**: zero parsing, but the result is mesh-backed
  (`docs/step_import_roadmap.md` §2: "an imported body is NOT an arena
  solid"), so no booleans and only the SI3 slice of sketch references. This
  is exactly the "first-class body" gap the STEP roadmap still lists.

Derive wins: the board outline is the thing a mechanical user references
most, and it is the only part of a PCB that is planar-and-analytic, so it
is cheap to make exact. The STEP export is still useful for the components.

### 2.2 Where component models come from — recommend the BOARD STEP, per product

Footprint `model` paths are almost always
`${KICAD9_3DMODEL_DIR}/Package.3dshapes/X.step`: they point at the KiCad
library, which is not in the user's repository. Candidates:

- **Path-variable table → upstream library** (what §9 3b says): resolve
  `${KICAD*_3DMODEL_DIR}` to the `kicad-packages3D` repo on GitLab and fetch
  each model as its own `Step` source. Fidelity is per-component and models
  dedupe across boards, but: one git fetch per distinct footprint (a board
  has dozens), the library is CC-BY-SA-4.0-with-exception (fine to link, not
  to commit), the variable name changes per KiCad major (`KICAD7_`, `KICAD8_`,
  `KICAD9_`), `.wrl` models have no STEP twin, and user-local models
  (`${KIPRJMOD}/...`) still need the repo path anyway.
- **One committed board STEP, split per product** (`kicad-cli pcb export
  step` output, checked in or attached to a release): one fetch, exactly what
  the user sees in KiCad, already positioned. Needs `step-import` to stop
  flattening: keep the product tree and per-product names (KiCad names each
  component product by reference designator — verify on a fixture; if it
  does not, position matching against footprint `at` is the fallback). Cost:
  per-product sub-bodies in `ImportedBodyData` (STEP roadmap SI4) and an
  `ImportedBodyData` → N instances mapping.

Board STEP wins for the first landing: it turns the problem into "split one
file we already parse" instead of "resolve N external URLs through a table
that drifts per KiCad release". The path-variable table can come later for
projects that do not commit a STEP. Either way the STEP is text (ISO 10303-21),
so the UTF-8-only fetch path is not a blocker.

## 3. Gaps, in dependency order

1. **`.kicad_pcb` parser** — new crate `crates/kicad-pcb/` (pure Rust,
   WASM-clean, no `truck`). Extract: `(general (thickness))` and
   `(setup (stackup …))`, `(title_block …)`, `Edge.Cuts` graphics (`gr_line`,
   `gr_arc start/mid/end`, `gr_circle`, `gr_rect`, `gr_poly`), footprints
   (`uuid`, `layer`, `at x y rot`, properties `Reference`/`Value`/
   `Footprint`/`Datasheet`, `attr`, pads with `net`, `model path (offset)
   (scale) (rotate)`), and nets. Tolerate unknown tokens (KiCad 7/8/9 differ;
   the format is versioned by `(version N)`). Fixtures: tiny boards we author
   ourselves; real KiCad-library boards stay in gitignored `refs/`.
2. **Git substrate** — a `listDirectory` per host (`/git/trees`,
   `/repository/tree`, `/contents/<dir>`) so a repo or folder link can find
   the `.kicad_pcb` and a sibling `.step`/`.kicad_pro`. Not needed if the
   first UX is "paste the file URL", which already parses.
3. **`step-import` per-product split** — stop folding every path into one
   `Vec`; return `shells` grouped by product with name and the folded
   transform. Roadmap SI4. Keep the composite as the default for plain
   `ImportStep`.
4. **Engine handler for `KicadPcb`** — bridge `LinkKicadFromLocator`
   mirroring `ImportStepFromLocator`; produces a board Part tab (Derived
   sketch + extrude), an Assembly tab with one instance per footprint
   (`external_key = footprint uuid`, transform from `at` + layer flip +
   model offset/rotate), and connectors at mounting holes (pads with
   `attr through_hole` and no net, or a `MountingHole` footprint name). The
   first-ever constructor of `Derived` provenance lives here.
5. **Metadata channel** — a per-source `BoardMeta` (title, rev, layer count,
   thickness, net count) and per-instance `ComponentMeta` (refdes, value,
   footprint, layer, datasheet, net names on pads) held by the engine and
   exposed by a bridge query keyed by instance id / body id. Keep it out of
   feature params (§2.6: no `x-` stuffing inside `Feature`).
6. **Hover card + click panel** — the first tooltip component in the app.
   Hover already yields the picked `GeomRef` and pointer position; add a
   positioned overlay showing refdes / value / footprint, and a click detail
   panel with the full record and a "open on GitHub at this commit" link.
7. **Re-sync** — on update-to-tip of a `KicadPcb` source, regenerate every
   feature/instance/connector with `Derived{source_id}` provenance, preserve
   user-authored features and mates, and reconcile instances by
   `external_key` (footprint uuid is stable across edits; refdes is not).
   This is new machinery: nothing today regenerates features from bytes.

## 4. Checkpoints

Each is an atomic landing with its own tests; the order is the dependency
order, and the first visible win is at C2.

| # | Checkpoint | Visible result |
|---|---|---|
| C0 | Phase 3b spec (`specs/kicad_board_link.md`) settling §2.1 and §2.2, fixture policy, metadata schema | — |
| C1 | `crates/kicad-pcb/` parser + authored fixtures + golden JSON | `cargo test -p kicad-pcb` |
| C2 | Bridge `LinkKicadFromLocator` → Derived board sketch + extrude; Sources panel shows the board; hover shows the board name | paste a GitHub `.kicad_pcb` URL, get an exact board solid |
| C3 | `step-import` per-product split (SI4) + sibling board STEP as a linked `Step` source; one instance per footprint with `external_key`; mounting-hole connectors | populated board in the assembly tab |
| C4 | Metadata query + hover card + click panel | refdes/value/datasheet on hover; details and GitHub link on click |
| C5 | Re-sync of Derived features on update-to-tip | edit the board upstream, update, keep your enclosure |
| C6 (optional) | Path-variable table → upstream library models; `listDirectory` for repo links | boards with no committed STEP |

## 5. Risks and traps

- **Fixture licensing**: the KiCad footprint/3D libraries are CC-BY-SA-4.0
  with exception; commit only boards and models we author. Same rule the
  STEP roadmap already follows.
- **Coordinate conventions**: `.kicad_pcb` is mm, Y-down, rotation
  counter-clockwise in degrees; back-layer footprints are mirrored about Y
  and their `model rotate` composes after the flip. Pin this with a
  two-sided fixture and compare against the board STEP's product transforms.
- **Arc outlines**: KiCad 6+ stores arcs as start/mid/end; a closed outline
  may be several disconnected primitives that only chain within tolerance.
  Closed-loop detection must be loud when it fails (no silent gap-closing;
  P9/P10).
- **Product naming in the board STEP**: the per-product refdes mapping is
  the load-bearing assumption of §2.2; verify on a real export before C3
  and keep position matching as the loud fallback.
- **STEP size**: a board export with many footprints exceeds the 1 MiB
  GitHub contents limit; the raw-content fallback handles this but the
  IndexedDB cache and `embed` inflation cap (§6) should be checked at the
  sizes a real board produces.
- **Instance identity across re-sync**: key on footprint uuid, never on
  refdes; annotate/renumber in KiCad changes every refdes.
- **Priority**: this is below the Yang pipeline. Land it as the checkpoints
  above rather than one long branch, so any checkpoint is a stable stop.
