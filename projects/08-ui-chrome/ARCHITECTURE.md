# 08 — UI Chrome: Architecture

The UI chrome is the Svelte 5 / SvelteKit application shell in `app/src/`:
everything around the 3D viewport. It reads state from the store
(`app/src/lib/engine/store.svelte.js`) and changes the model by calling store
functions, which send `UiToEngine` messages through the bridge. It imports no
Rust types.

## Routes

| Route | Page |
|---|---|
| `/` | The editor |
| `/home` | Document grid: open, create, connect a git host |
| `/doc/[id]` | Opens a stored document in the editor |
| `/open` | Opens a document from a link |
| `/view` | Read-only viewer |
| `/agent` | Agent link page |
| `/dev/icons` | Icon gallery |

## Editor layout (`routes/+page.svelte`)

```
┌──────────────────────────────────────────────────────────────┐
│ Toolbar (Part/Sketch) or DrawingToolbar                      │
├──────────────────────────────────────────────────────────────┤
│ TabBar — one tab per Part, Assembly or Drawing               │
├────────────┬──────────────────────────────┬──────────────────┤
│ FeatureTree│  Viewport                    │ PropertyEditor   │
│  or        │   or                         │                  │
│ Assembly-  │  DrawingSheet                │                  │
│ Panel /    │                              │                  │
│ Drawing-   │                              │                  │
│ Panel      │                              │                  │
├────────────┴──────────────────────────────┴──────────────────┤
│ StatusBar                                                    │
└──────────────────────────────────────────────────────────────┘
```

The active tab's kind decides the left panel, the main area and the toolbar. A
Part tab shows the feature tree and the 3D viewport. An Assembly tab shows
`AssemblyPanel` in place of the feature tree. A Drawing tab shows
`DrawingPanel` and `DrawingSheet` in place of the tree and the viewport.

The side panels are resizable by dragging their dividers. Their widths are
clamped, on drag and on window resize, so the viewport keeps at least 320 px.
On phones the side panels are hidden; two buttons on the viewport open them
as overlays.

## Components (`app/src/lib/ui/`)

- **`Toolbar.svelte`** — modeling tools (Sketch, Extrude, Revolve, Pipe,
  Script, Boolean, Plane, Connector, Import, …) and, in sketch mode, the
  drawing and constraint tools. One tool is active at a time. It also owns
  the keyboard shortcuts.
- **`DrawingToolbar.svelte`** — the toolbar of a Drawing tab.
- **`FeatureTree.svelte`** — the feature list with icons, error and warning
  marks, suppressed and rolled-back styling; origin planes and axes; bodies.
  Click selects, double-click renames or edits, drag reorders, right-click
  opens a context menu. The rollback slider at the bottom sets the tree's
  `active_index`.
- **`AssemblyPanel.svelte`**, **`DrawingPanel.svelte`** — the left panel of
  Assembly and Drawing tabs.
- **`PropertyEditor.svelte`** — the selected feature's parameters. An edit is
  debounced and sent as `EditFeature`. In sketch mode it also holds the
  constraint list.
- **`StatusBar.svelte`** — engine status, selection, sketch solve status and
  degrees of freedom, rebuild time, errors.
- **`TabBar.svelte`** — add, close, rename, reorder and switch tabs.
- **Feature dialogs** — `ExtrudeDialog`, `RevolveDialog`, `PipeDialog`,
  `BooleanDialog`, `ScriptDialog` / `ScriptEditor`, `MateConnectorDialog`,
  `SketchPlaneDialog`, `GearDialog`, `SprocketDialog`,
  `PlanetaryGearDialog`, `ImportStepDialog`, `ImportLinkDialog`,
  `PlaceViewDialog`. `FilletDialog`, `ChamferDialog` and `ShellDialog` show a
  warning and cannot be applied.
- **Documents** — `DocumentGrid`, `DocumentCard`, `HomeHeader`,
  `AutoRestoreDialog`, `DocumentConflictDialog`, `LinkedDocBanner`,
  `GitHubConnectDialog`, `GitHostConnectDialog`. Storage providers are in
  `app/src/lib/storage/` (IndexedDB locally, git hosts remotely).
- **Other** — `SettingsModal`, `ThemeSwitcher`, `ToastContainer`,
  `ContextBanner` (in-context editing), `ExamplesBrowser`, `AssayBrowser`,
  `TestCaseBrowser`.

Context menus: the feature tree's (features, origin, bodies),
`viewport/ViewportContextMenu.svelte`, and `sketch/ConstraintMenu.svelte`.

## Nothing overflows the window

The page cannot scroll, so anything that outgrows its box would be
unreachable. Every region scrolls, collapses, or is sized relative to the
window.

- The toolbar measures itself after every content or width change and walks a
  collapse ladder (`collapseLevel` in `Toolbar.svelte`): the file group moves
  into the ⋮ menu, then the view toggles, the brand, the project name and
  Undo/Redo, then trailing tools one at a time into "More ▾".
- Dialogs have a `max-height` and scroll. Overlay browsers are
  `min(300px, 40vw)` wide.
- `tests/gui/layout-overflow.spec.js` sweeps desktop widths and fails on any
  interactive element outside the window.

## Icons

Every toolbar button, menu row and feature-tree row takes its icon from one
registry, `app/src/lib/ui/icons/icons.js`. Each entry is the inner markup of a
24×24 stroke SVG: Lucide (ISC) for generic actions, hand-drawn glyphs for CAD
ones. `Icon.svelte` renders it inline with `currentColor`. Glyph parts use
classes, not colors: `.c` construction, `.axis`, `.p` points, `.d` dimension
strokes, `.a` arrowheads, `.f` shaded faces. `scripts/check-icons.mjs` rejects
a color in the registry and any reference to an icon that does not exist.

On desktop a button shows its icon and label. On phones
(`iconOnly.svelte.js`) it shows the icon only. A feature-tree row uses the
icon of the toolbar button that makes that feature. Spec:
`specs/ui_toolbar_icons.md`.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `S` | Sketch |
| `E` | Extrude |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / Redo |
| `Ctrl+S` / `Ctrl+O` | Save / Open |
| `Delete`, `Backspace` | Delete the selection |
| `Escape` | In a sketch: back to the Select tool, then clear the selection. It never leaves the sketch. |
| `F` | Fit all |

In sketch mode: `L` line, `R` rectangle, `P` polyline, `C` circle, `A` arc,
`T` slot, `F` sketch fillet (it also fits the view), `D` dimension,
`X` toggle construction, `J` project, `O` offset, `G` gear, `K` sprocket.
