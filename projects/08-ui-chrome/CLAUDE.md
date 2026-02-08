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

- `src/routes/+page.svelte` — Main application layout
- `src/lib/FeatureTree.svelte` — Feature tree panel
- `src/lib/Toolbar.svelte` — Tool buttons and state
- `src/lib/PropertyEditor.svelte` — Parameter editing
- `src/lib/StatusBar.svelte` — Status information
- `src/lib/ContextMenu.svelte` — Right-click menus
- `src/lib/RollbackSlider.svelte` — Feature tree rollback control

## Dependencies

- SvelteKit
- 3d-viewport (embedded as viewport component)
- sketch-ui (embedded during sketch mode)
- wasm-bridge (communication)
- No Rust/WASM dependencies
