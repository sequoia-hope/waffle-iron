# 08 — UI Chrome: Agent Instructions

You are working on **ui-chrome**. Read ARCHITECTURE.md in this directory first.

## Your Job

Build the Svelte application shell: feature tree panel, toolbar, property editor, status bar, and context menus. This is the entire UI surrounding the 3D viewport.

## Critical Rules

1. **Communicate via wasm-bridge only.** Send UiToEngine messages. Receive EngineToUi messages. Never import Rust types directly.
2. **The feature tree is the primary navigation UI.** It must show all features, support reorder via drag-and-drop, and feel responsive.
3. **Property editor changes trigger rebuilds.** When the user edits a parameter (e.g., extrude depth), produce an EditFeature message with the updated Operation. Debounce rapid changes.
4. **Show errors clearly.** Failed features should have visible error indicators in the tree and detailed error messages in the property editor.
5. **Keyboard shortcuts matter.** CAD users rely on shortcuts. Implement the Onshape-inspired shortcut table.

## Key Files

Paths are under `app/`.

- `src/routes/+page.svelte` — Main application layout
- `src/lib/ui/FeatureTree.svelte` — Feature tree panel, with its feature, origin and body context menus
- `src/lib/ui/Toolbar.svelte` — Part / Sketch toolbar: tool descriptors, collapse ladder
- `src/lib/ui/DrawingToolbar.svelte` — Drawing-tab toolbar
- `src/lib/ui/PropertyEditor.svelte` — Parameter editing (and the sketch constraint list)
- `src/lib/ui/StatusBar.svelte` — Status information
- `src/lib/viewport/ViewportContextMenu.svelte` — Viewport right-click menu
- `src/lib/sketch/ConstraintMenu.svelte` — Sketch right-click constraint menu
- `src/lib/ui/icons/` — Icon registry (`icons.js`) and `Icon.svelte`; see ARCHITECTURE.md "Icons"
- `src/lib/ui/iconOnly.svelte.js` — Phone check: icons without labels

## Dependencies

- SvelteKit
- 3d-viewport (embedded as viewport component)
- sketch-ui (embedded during sketch mode)
- wasm-bridge (communication)
- No Rust/WASM dependencies
