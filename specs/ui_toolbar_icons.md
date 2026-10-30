# Toolbar and menu icons — plan (2026-10-30, ui-chrome M13)

Status: PLAN, not started. Research-only pass over `app/`; no code changed.
Goal: every toolbar and menu button — Part, Sketch (tools + constraints),
Drawing, context menus — carries an icon that makes its function clear.

## 0. Findings that shape the plan

- **No icon system exists.** No icon dependency in `app/package.json`; all
  buttons are text. Only glyphs: `⚙`, `⋮`, `▾`, `Rect ⌖`, `✓ ` prefixes in the
  ⋮ menu; `FeatureTree.svelte` `featureIcon()` uses Unicode. `DocumentCard`
  has two inline Lucide-style SVGs (24 grid, `currentColor`).
- **Two toolbars.** `+page.svelte` mounts `DrawingToolbar.svelte` on Drawing
  tabs and `Toolbar.svelte` elsewhere (Assembly reuses the Part toolbar).
- **Tool lists are data** (`modelingTools`, `sketchTools`, `constraintButtons`
  in `Toolbar.svelte`; `DRAWING_TOOLS`, `VIEW_PLACEMENT_TOOLS`). An `icon`
  field on each descriptor covers inline, "More ▾" and mobile placements. File,
  view, Undo/Redo buttons are hand-written twice (inline + ⋮).
- **Sweep / loft / patterns / mirror have no toolbar button** (tree types
  only); icons are registered anyway for the tree and future buttons.
- **Fillet / chamfer / shell** are not disabled in the toolbar (dialog shows
  the deferred banner, Apply disabled). Icon only — no behaviour change.
- **Theming**: 8 themes (3 light) via CSS tokens; button states set `color`,
  so `stroke="currentColor"` themes icons for free.
- **Collapse ladder**: `contentFits()` sums child widths synchronously → icons
  must be fixed-size inline SVG (no web font: async load mis-measures).
- **Width budget**: level 0 is ~2011 px; `layout-overflow.spec.js` asserts
  level 0 at 2400 px. Icon + gap on every labelled button adds ~480 px — that
  assertion will break and must be re-measured.
- **Flake risk**: 13 specs click toolbar buttons by RAW testid (against the GUI
  rules): `deferred-operations` ×12, `agent-authoring` ×4, `offset-plane` ×3,
  `sketch-center-rectangle` ×3, `planetary-gear`, `sketch-plane-face-select`,
  `sketch-slot-tool` ×2, `boolean-dialog`, `constraint-toolbar`,
  `open-from-link`, `script-dialog`, `sketch-offset-tool`, `snap-inference`.
  They pass only because those tools sit inline at 1280. `clickCenterRectangle()`
  clicks `toolbar-btn-rectangle-menu` directly.
- **Text-dependent tests**: `undo-redo.spec.js` `toHaveText('Undo'/'Redo')`,
  `sketch-dof-display` → labels must stay in the DOM (sr-only when hidden).
  `ui-toolbar.png` baseline needs regenerating.
- **Constraint badges** on the canvas (`constraintBadges.js`: H V M ‖ ⟂ = T •
  ↔ × 📌) — toolbar constraint icons echo them.

## 1. Inventory and icon concepts

Notation: **E** = created entity (solid stroke); **c** = construction /
reference (dashed, 55 %); **•** = point dot; **dim** = dimension line with
arrowheads and extension lines. Testids are `toolbar-btn-<id>`.

### 1a. Part / Assembly toolbar

| Button | Icon concept |
|---|---|
| home | Lucide `house` |
| sketch (S) | E pencil tip over a c dashed square plane |
| extrude (E) | c flat profile, E box rising from it, up arrow |
| revolve | c dash-dot axis, E profile beside it, curved arrow around axis |
| pipe | E tube following an S-curve with an end-cap ellipse |
| script | `{ }` braces around a small cube |
| fillet / chamfer / shell (deferred) | box corner rounded / 45° cut / open double-walled box, old corner c dashed |
| boolean | two overlapping squares, overlap shaded |
| datum-plane | parallelogram with normal arrow and c offset copy |
| mate-connector | triad on a • origin in a circle |
| import-step / import-link | down arrow into cube / cube + chain links |
| import-kicad / link-kicad | PCB with pads and trace + down arrow / + chain links |
| modeling-dropdown, more-tools | extrude glyph + ▾ / Lucide `chevrons-right` |
| toggle-planes / toggle-axes / section | three planes / origin triad / cube cut by hatched plane |
| section-flip / section-clear | plane with curved double arrow / section glyph + × |
| undo / redo / overflow / settings | Lucide `undo-2` / `redo-2` / `ellipsis-vertical` / `settings` |
| save / open / export-waffle | Lucide `save` (spinner when busy) / `folder-open` / `download` + waffle-grid page |
| export-stl / export-step | page-with-arrow + mesh facet tag / + solid cube tag |
| tests / examples / assay / debug / reload | `flask-conical` / `book-open` / `microscope` / `bug` / `rotate-cw` |

### 1b. Sketch toolbar

| Button | Icon concept |
|---|---|
| select | Lucide `mouse-pointer-2` |
| point | large • with c crosshair |
| line (L) | diagonal E segment, • both ends |
| polyline (P) | E zig-zag, • every vertex |
| rectangle (R), split | square, • at two opposite corners; main face shows the active variant |
| rectangle-center | square, • centre + corner, c dashed diagonal (replaces `⌖`) |
| circle (C) | E circle, • centre, c radius to • on rim |
| arc (A) | E arc, • ends and centre, c radii |
| project (J) | c dashed 3D edge above, arrow down, E segment on a flat line |
| offset (O) | c dashed path with parallel E copy and ↔ |
| slot (T) | E obround, c centreline, • arc centres |
| gear / sprocket / planetary | 8-tooth gear / pointed teeth + rollers / ring with sun + planets |
| trim | crossing segments, one arm c dashed past the crossing, scissors |
| sketch-fillet (F) | 2D L-corner rounded by an E arc, old corner c dashed |
| construction (X) | line drawn entirely c dashed + toggle mark |
| constraints-dropdown | right-angle mark + ▾ |
| dimension (D) | dim line with extension lines and a gap for the number |
| finish-sketch | Lucide `check` + label that is NEVER dropped ("Finish" at icon-only rung) |

### 1c. Constraints (dropdown grid + right-click `ConstraintMenu`)

horizontal "H"-tagged segment · vertical "V" · coincident: endpoints meeting at
a ringed • · perpendicular: 90° + square mark · parallel: two slants + ‖ ·
equal: two segments with = ticks · tangent: line touching circle at • ·
midpoint: ring at segment middle + "M" · fix: outline pushpin on • · angle:
two rays + arrowed arc · symmetricH/V: two • mirrored across a dash-dot line ·
pointOnLine: • on a line · hDistance/vDistance: horizontal/vertical dim. Menu
extras reuse the drawing dim glyphs (distance, radius, diameter, point-line);
pointOnCircle, equalRadius (two circles "="), lengthRatio (":"), symmetric.
The grid becomes icon tiles (icon above a 10 px abbreviation); `title` keeps
the full name.

### 1d. Drawing toolbar

Shared icons for home/undo/redo/file/settings. select: pointer. place-view:
sheet with view frame and "+". project-view: two frames with arrow and c
projection lines. dim-distance / hdistance / vdistance / pointline / angle /
radius ("R") / diameter ("⌀"). note: box with arrowed leader. datum: boxed "A"
on a leader ending in a filled triangle (GD&T).

### 1e. Context menus (16 px leading icon column)

Viewport: sketch-on-face, new-sketch, fit-all (`maximize`), view front/top/
right/iso (cube with the matching face shaded). Feature tree: edit sketch,
edit (`pencil`), rename (`text-cursor-input`), suppress (`eye-off`/`eye`),
delete (`trash-2`), hide/show groups, export STL. Out of scope for the first
pass: AssemblyPanel buttons, ViewCube, browser-panel ↻/✕, dialog buttons.

## 2. Approach: one hand-held registry, no new npm dependency

- `app/src/lib/ui/icons/icons.js` — `ICONS: Record<string, string>` of inner
  SVG markup on a `0 0 24 24` viewBox; `Icon.svelte` (`name`, `size = 16`)
  renders `<svg class="wi-icon" … fill="none" stroke="currentColor"
  stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
  aria-hidden="true">{@html ICONS[name]}</svg>`. Step 2 pins the SVG
  namespace of `{@html}` children; fallback is `{@html}` of the whole SVG.
- ~20 generic icons COPIED from Lucide (ISC; notice in a header comment).
  ~60 custom CAD glyphs drawn on Lucide's grid rules so they read as one set.
  Not `@lucide/svelte`: a second icon API and a dependency, and the CAD glyphs
  must be hand-drawn anyway. No icon font or image files (async / no
  `currentColor`).
- Cost ~20 KB raw / ~5 KB gzip in the JS chunk. Rust/WASM build untouched.
- Descriptors gain `icon` and `name`; title = `name (shortcut)`.
  `featureIcon()` moves to the registry later (toolbar icon = tree icon).

## 3. Design system

- 24 grid, 2-unit margin; 16 px inline/menu rows, 20 px tiles and mobile.
  Stroke 2; detail 1.5.
- `currentColor` only (enforced by `scripts/check-icons.mjs`). Inherits
  `--text-primary` / `--accent` (active) / `--text-muted` (disabled) /
  `--success` (Finish) / danger (Delete) in all 8 themes.
- SVG child classes: `.e` entity (full), `.c` construction (dash 2.5 2.5,
  opacity .55; dash-dot for axes/mirror lines), `.p` filled points r 1.75,
  `.d` dimensions (1.5, filled arrowheads, 1-unit gap extension lines), `.f`
  fills at .3. Monochrome; accent only via active state.
- Accessibility: SVG `aria-hidden`; label stays in DOM as `.tb-label`, made
  `.sr-only` at the icon-only rung (keeps accessible name, `toHaveText`, and is
  excluded from `contentFits()`); `aria-label` where there is no text; `title`
  always full name + shortcut; ⋮ toggles become `menuitemcheckbox` with
  `aria-checked` and a trailing check.

## 4. Layout and the collapse ladder

| Placement | Presentation |
|---|---|
| Inline, before the label rung | icon 16 + 4 px gap + label |
| Inline, at/after the label rung | icon only, `min-width: 28px`; Finish keeps a short label |
| More ▾ / mobile dropdowns / Constraints grid | 3-col tiles, 20 px icon over 11 px label |
| ⋮ menu rows, context menus | 20 px leading icon, full label, trailing check |

`Toolbar.svelte` (`FIXED_RUNGS` 5 → 6): file group → ⋮; view toggles → ⋮;
brand; project name; **NEW labels → icon-only** (`iconOnly = collapseLevel >=
5`, exposed as `data-icon-only`); Undo/Redo → ⋮; then trailing tools → More.
`DrawingToolbar.svelte` (`FIXED_RUNGS` 4 → 5) likewise. The new rung only
narrows, so the no-oscillation invariant holds; `contentFits()` is unchanged.
Re-measure the 2400 px re-expand assertion (step 1). At 1280 the sketch
toolbar likely sits icon-only with MORE tools inline than today.

## 5. Ordered steps (one commit each)

Run `./scripts/test.sh gui-fast` at every step; `gui-full` at †; the
`sketch-drawing-regression.spec.js` canary on any sketch-toolbar change.

1. **Width-independent tests** (no UI change) †: replace raw-testid clicks in
   the 13 specs with `clickTool` / `clickToolbarAction` / `isToolOffered`;
   `clickCenterRectangle()` falls back to `clickTool(page,'rectangle-center')`.
   Locally record `data-collapse-level` and content width per mode/width.
2. **Icon module + generic set**: `Icon.svelte`, `icons.js` (Lucide ×20, ISC
   header), `.sr-only` in `app.css`, `scripts/check-icons.mjs` wired into
   `run_gui_fast` (no hex/rgb/fill other than currentColor/none; every
   descriptor `icon` exists). Optional dev gallery route
   `app/src/routes/dev/icons/+page.svelte`.
3. **Custom CAD glyphs** (registry only; may split 3a sketch+constraints /
   3b features+drawing+views), reviewed in a dark and a light theme. Include
   sweep, loft, pattern-linear, pattern-circular, mirror.
4. **Wire Part + Sketch toolbars, label rung** †: descriptors, `<Icon>` +
   `.tb-label` in every copy, ladder comment + `FIXED_RUNGS = 6`, tile / ⋮ CSS;
   `layout-overflow.spec.js` re-expand width + a `data-icon-only` case; new
   `toolbar-icons.spec.js` in `GUI_FAST_SPECS`: at 2400/1280/900, every visible
   `toolbar-btn-*` (inline, More, ⋮, Constraints — opened via helpers) has one
   `svg.wi-icon` with SVG-namespace children, an accessible name and a title;
   at icon-only width the label is hidden but `toHaveText` still matches. No
   try/catch; mobile projects run it against the tile layout.
5. **Drawing toolbar** †: `DRAWING_TOOLS` / `VIEW_PLACEMENT_TOOLS` icons,
   label rung, `FIXED_RUNGS = 5`; drawing tab case in `toolbar-icons.spec.js`.
6. **Context menus**: `ViewportContextMenu.svelte`, FeatureTree context menu,
   `ConstraintMenu.svelte`; spec asserts one icon per `ctx-item`.
7. **Feature-tree icons** from the registry (`featureIcon()` → icon name).
8. **Baselines + docs** †: regenerate `ui-toolbar*.png`; tick M13 in
   `projects/08-ui-chrome/PLAN.md`; note `lib/ui/icons/` in the dossier's
   ARCHITECTURE.md; fix stale paths in its CLAUDE.md.

## 6. Risks and open questions

Risks: width growth shifting thresholds (step 1 + label rung mitigate);
`{@html}` in `<svg>` namespace (fallback ready); perf budget spec (check
gui-full); 16 px legibility of planetary/sprocket/KiCad (≤ 4 primitives);
inline vs ⋮ copies drifting (optional `{#snippet}` refactor).

Open questions for the user:
1. Ladder order: drop labels before or after hiding brand/project name — or
   icon-only on desktop at all widths (Onshape-style)?
2. Should fillet/chamfer/shell LOOK deferred (muted icon + tooltip note)?
3. Lucide paths copied in (ISC) acceptable, or all hand-authored?
4. Constraint icons keep letter tags (H, V, M) to match canvas badges, or
   purely pictorial?
5. AssemblyPanel buttons and browser-panel ↻/✕ in scope now or later?
6. Commit the dev icon gallery route, or local review only?
