# 08 — UI Chrome: Agent Instructions

You are working on the application shell, `app/src/routes/` and
`app/src/lib/ui/`. Read ARCHITECTURE.md in this directory first.

## Rules

1. **Go through the store.** Components call store functions; the store sends
   `UiToEngine` messages. No WASM imports in components.
2. **The engine is authoritative.** Tabs, document name and the feature tree
   are mirrored from the engine's answers. Do not keep a second copy that can
   diverge.
3. **Chrome scrolls or collapses; it never overflows.** New regions need
   `overflow-y: auto`, a `max-height`, or a size relative to the window.
   `tests/gui/layout-overflow.spec.js` enforces this.
4. **Icons come from the registry.** Add a glyph to `icons/icons.js`; no
   inline SVG in components and no colors in the registry.
5. **Show errors where they happen.** A failed feature is marked in the tree
   and its message is shown in the property editor.
6. **`$state` proxies cannot be posted to the worker.** Clone before
   `bridge.send`.
7. **Fillet, chamfer and shell stay disabled.**

## Tests

GUI tests are in `app/tests/gui/`. In a spec, reach a toolbar button with
`clickTool()` / `clickToolbarAction()` from `helpers/toolbar.js`, never by raw
test id: the button may be inline, in "More ▾" or in the ⋮ menu depending on
width.

## Key Files

- `src/routes/+page.svelte` — editor layout
- `src/lib/ui/Toolbar.svelte`, `DrawingToolbar.svelte`
- `src/lib/ui/FeatureTree.svelte`, `AssemblyPanel.svelte`, `DrawingPanel.svelte`
- `src/lib/ui/PropertyEditor.svelte`, `StatusBar.svelte`, `TabBar.svelte`
- `src/lib/ui/icons/` — `icons.js`, `Icon.svelte`
- `src/lib/engine/store.svelte.js` — application state
