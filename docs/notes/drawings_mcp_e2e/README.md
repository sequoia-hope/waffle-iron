# The drawing tools, end to end over the agent link

`drawings_e2e.mjs` drives **every drawing tool** through the real MCP path —
a local `waffle-mcp-relay` spoken to as an MCP client over stdio, paired with
a headless Chromium page on the dev server — on a part it authors through the
same link. It is the measurement behind D4f's "the agent's drawing door,
completed" (`specs/drawings_and_mbd.md` §8) and the Drawings section of
`specs/waffle_mcp_server.md` §2.5.

Run from the repo root with the dev server on `http://localhost:5173`:

```
node docs/notes/drawings_mcp_e2e/drawings_e2e.mjs
```

It writes `out/` (the committed copy is the 2026-10-05 run): `calls.jsonl`
(every call and its `structuredContent`), `report.json` (the checks), and the
sheet as `plate.svg`, `plate.pdf`, `plate-sheet.dxf`, plus the one-view
`plate-iso.dxf` from the part tab.

## What it measured (2026-10-05)

55 checks, 0 failures, 24 distinct tools, ≈ 10 s wall for the whole run.

The part is a 60 × 40 × 6 mm plate with a Ø10 bore, authored with
`sketch_create` + `feature_add`; its volume is checked against the closed form
(13 928.761 mm³) before any drawing is made, so a dimension can only be right
by measuring.

| step | tool | what came back |
|---|---|---|
| gate | `drawing_get` on the Part tab | `TabKindNotSupported{kind: "Part"}` |
| top view 2:1 | `drawing_view_add` (`include_anchors`) | 10 curves, 20 anchors (`shape`/`kind` as `{type}` objects, pids as decimal strings), no errors |
| distance between the two walls | `drawing_annotation_add` | index 0, measured **0.060 m** |
| bore diameter | `drawing_annotation_add` | index 1, measured **0.010 m** |
| a typed `value` | `drawing_annotation_add` | refused by the relay's schema check (`-32602`, pointer `/arguments`) before the page saw it; the engine's own by-name refusal is pinned in `tool_drawing.rs` |
| read back | `drawing_get` (`include_annotations`) | three records with kind, anchors, precision, `value`, `resolved: true`; a record's anchor passes back into `_add` verbatim |
| edit in place | `drawing_annotation_edit` | precision 3, dual unit `in`, placement — index kept; `precision` on a note refused as "a Note has no `precision` to change"; `expr: no_such_parameter` → `AnnotationNotMeasurable`, rolled back; `expr: plate_w / 2` → prints 0.030 |
| delete | `drawing_annotation_delete` | the note goes, the centre mark moves up; index 7 → `NotFound{count: 3}` |
| projected right | `drawing_view_add` | bbox spans 6 mm × 40 mm — the thickness, so the frame came from the parent |
| section A-A through the bore | `drawing_view_add` (`section_mm` from the parent's bbox) | hatched cap with the hole (2 loops) |
| detail B 4:1 | `drawing_view_add` (`detail_mm`) | 4 views on the sheet |
| move a view | `drawing_view_edit` | placement and `hidden_lines: false` applied |
| title block | `drawing_sheet_edit` | A3 landscape, derived rows filled (`2:1`, `Third angle`), `Mass (g)` expression row prints **37.6077** from `volume(Plate) * 0.0027`; typing `SheetNumber` refused by name |
| exports | `export_svg`, `export_pdf`, `export_dxf` | SVG carries `60.000 [2.362 in]` and `⌀10.0`; PDF is one page (rendered and inspected — section hatch, bore gap, dimensions, title block all present); sheet DXF has the `HATCH` layer; `view: "top"` on the drawing tab refused |
| delete views | `drawing_view_delete` | the side view alone → `deleted: [side]`; the top view → `deleted` names the top, its section and its detail (3), sheet empty; deleting again → `NotFound` |
| iso of the model | `export_dxf {view: "iso"}` on the part tab | 12+ `LINE`s and the bore's curves — the named view the one-view export lacked before D4f |
| save, undo | `document_save`, `undo` on the Drawing tab | saved; undo restored the three deleted views (the drawing's own stack, D4d) |

## What the run found, and what was changed because of it

- **A blind cut with no direction and a plane with no `x_axis` both fail
  silently.** The first run's bore removed nothing (volume unchanged, no
  error): the sketch plane given as origin + normal got engine-derived axes
  (+x along world −y), putting the circle outside the plate, and a disjoint
  cut is a no-op. Both are known traps (the `sketch_create` answer's `plane`
  exists to expose the first); the script now pins `x_axis` and cuts
  symmetrically. Not a drawing defect — recorded because it is the first
  thing an agent authoring a test part will hit.
- **An `anchor_list` entry could not be passed back as an anchor.** The
  `anchors` item schema had `additionalProperties: false`, so an entry
  carrying `shape`/`at`/`radius` was rejected by the relay's validation.
  Opened (D4f) — an anchor read off `drawing_get` now passes back verbatim.
- **`undo` / `redo` were refused on a Drawing tab by the page's gate**
  ("agent edits work on Part tabs"), although the engine's `Undo` reaches the
  drawing's own stack there and the drawing tab's feature tree is empty. The
  gate now passes them on a Drawing tab; the engine behaviour is pinned in
  `tool_drawing.rs::the_undo_and_redo_tools_reach_the_drawings_own_stack_on_a_drawing_tab`.
- **A title-block mass is a volume with a density, and prints a volume
  unit.** Expressions evaluate in millimetre space, so `volume(Plate) *
  0.0027` is the mass in grams — printed as `37.6077 mm³`, because the
  dimension tracker has no density. A real `mass()` with a material is M1's.
- **`volume(<feature name>)` names nothing.** The measurement functions
  resolve body and entity NAMES; the plate's body had to be named with
  `body_rename` first. The engine said so by name in `errors`.
- The two label collisions visible in the PDF (the 60.000 label under the
  SECTION A-A title, DETAIL B over the title block) are the script's own
  placements on an A3 sheet, not layout defects.
