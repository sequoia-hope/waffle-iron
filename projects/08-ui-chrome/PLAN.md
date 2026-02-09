# 08 — UI Chrome: Plan

## Milestones

### M1: Application Shell Layout ✅
- [x] SvelteKit app setup
- [x] Three-column layout (tree + viewport + properties)
- [x] Toolbar at top
- [x] Status bar at bottom
- [x] Responsive resizing (draggable panel borders)

### M2: Feature Tree Display ✅
- [x] Render feature list with icons
- [x] Feature name display
- [x] Suppressed feature styling (gray + strikethrough)
- [x] Selected feature highlight
- [x] After-rollback feature styling (dimmed)

### M3: Feature Tree Interactions ✅
- [x] Click to select
- [x] Double-click to rename (wired to RenameFeature message)
- [x] Right-click context menu (suppress/delete)
- [x] Drag-and-drop reorder (wired to ReorderFeature message)
- [x] Produce DeleteFeature/SuppressFeature messages via store

### M4: Toolbar with Tool State ✅
- [x] Tool buttons for all modeling tools (Sketch, Extrude, Revolve, Fillet, Chamfer, Shell)
- [x] Sketch mode tools (Select, Line, Rect, Circle, Arc)
- [x] Active tool indicator
- [x] Tool state management (one active at a time)
- [x] Keyboard shortcuts for tools (S, E, L, R, C, A)
- [x] Finish Sketch button in sketch mode

### M5: Property Editor Display ✅
- [x] Show parameters for selected feature
- [x] Type-appropriate input fields (number, checkbox)
- [x] Parameter labels
- [x] Info display for Sketch entities/constraints count

### M6: Property Editor Editing ✅
- [x] Edit parameter values
- [x] Produce EditFeature message on change
- [x] Debounce rapid changes (300ms)

### M7: Status Bar ✅
- [x] Engine status display
- [x] Selection info display
- [x] Rebuild time display
- [x] Sketch mode + active tool display
- [x] Error indicator (red background)

### M8: Context Menus ✅
- [x] Feature tree right-click menu (suppress/delete)
- [ ] Viewport right-click menu — deferred

### M9: Keyboard Shortcuts ✅
- [x] Modeling shortcuts (S=Sketch, E=Extrude)
- [x] Sketch shortcuts (L=Line, R=Rect, C=Circle, A=Arc)
- [x] Undo/Redo (Ctrl+Z / Ctrl+Shift+Z)
- [x] Escape to finish sketch / deselect tool
- [x] Shortcut hints in button tooltips

### M10: Rollback Slider ✅
- [x] Slider at bottom of feature tree
- [x] Drag to set active_index
- [x] Visual feedback (features after slider grayed out)
- [x] Produce SetRollbackIndex message

## Blockers

(None — all dependencies resolved)

## Interface Change Requests

(None)

## Notes

- Viewport context menu deferred to future iteration
- Feature rename wired to RenameFeature message through wasm-bridge
- Feature reorder wired to ReorderFeature message through wasm-bridge
- Responsive panel resizing uses draggable dividers with min/max constraints
