# 08 — UI Chrome: Architecture

## Purpose

Application shell in Svelte/SvelteKit. The complete user interface surrounding the 3D viewport: feature tree panel, toolbar, property editor, status bar, and context menus. All communication with the WASM engine is via wasm-bridge messages.

## Layout

```
┌────────────────────────────────────────────────────────────────┐
│  Toolbar (top)                                                  │
│  [Select] [Line] [Rect] [Circle] [Arc] | [Extrude] [Revolve]  │
│  [Fillet] [Chamfer] [Shell] | [Undo] [Redo]                   │
├────────────┬───────────────────────────────┬───────────────────┤
│            │                               │                   │
│  Feature   │      3D Viewport              │  Property         │
│  Tree      │      (04-3d-viewport)         │  Editor           │
│  (left)    │                               │  (right)          │
│            │                               │                   │
│  Sketch 1  │                               │  Extrude 1        │
│  Extrude 1 │                               │  ─────────        │
│  Fillet 1  │                               │  Depth: 25mm      │
│  > Sketch 2│                               │  Direction: +Z    │
│    ...     │                               │  Symmetric: No    │
│            │                               │  Cut: No          │
│  ──────────│                               │                   │
│  [Rollback │                               │                   │
│   slider]  │                               │                   │
├────────────┴───────────────────────────────┴───────────────────┤
│  Status Bar (bottom)                                            │
│  DOF: 0 | Selection: Face 3 of Extrude 1 | Rebuild: 45ms      │
└────────────────────────────────────────────────────────────────┘
```

## Components

### Feature Tree Panel (Left Sidebar)

The primary navigation and manipulation UI for the parametric model.

**Display:**
- Ordered list of features with type-specific icons.
- Feature name (editable via double-click).
- Suppressed features shown grayed out with strikethrough.
- Currently selected feature highlighted.
- Error indicator on features with failed GeomRef resolution.

**Interactions:**
- **Click:** Select feature (shows properties in right panel, highlights in viewport).
- **Double-click name:** Rename feature.
- **Right-click:** Context menu (Edit, Delete, Suppress/Unsuppress, Move Up/Down).
- **Drag-and-drop:** Reorder features in the tree.
- **Rollback slider:** At the bottom of the tree. Drag to set `active_index` — features after the slider are grayed out and suppressed during rebuild. This lets users "go back in time."

**Messages produced:**
- `EditFeature`, `DeleteFeature`, `SuppressFeature`, `SetRollbackIndex`

### Toolbar (Top)

Drawing and operation tools with active-tool state management.

**Tool groups:**
- **Drawing tools** (active during sketch mode): Select, Line, Rectangle, Circle, Arc, Construction toggle.
- **Operations:** Extrude, Revolve, Fillet, Chamfer, Shell, Boolean.
- **History:** Undo, Redo.
- **View:** Fit All, Standard Views.

**State:** Only one tool is active at a time. Active tool has visual indicator (pressed/highlighted state). Tool changes are communicated to sketch-ui and viewport.

### Property Editor (Right Sidebar)

Display and edit parameters for the selected feature.

**Display:**
- Feature type header ("Extrude 1").
- Parameter fields with labels and current values.
- Fields are type-appropriate: numbers get spinners, booleans get checkboxes, references show entity names.

**Editing:**
- Change a value → produce `EditFeature` message with updated parameters → triggers rebuild.
- Validate input before sending (e.g., depth must be positive).
- Show rebuild errors inline if the edit causes a failure.

### Status Bar (Bottom)

- **Solve status:** "Fully Constrained" / "DOF: 3" / "Over-Constrained" during sketch mode.
- **Selection info:** "Face 3 of Extrude 1" or "Edge 2 of Fillet 1" when entity is selected.
- **Rebuild time:** "Rebuild: 45ms" after each rebuild.
- **Error indicator:** Brief error message if last operation failed.

### Context Menus

Right-click context menus for:
- **Feature tree items:** Edit, Delete, Suppress, Move Up/Down, Insert Before/After.
- **Viewport entities:** Select Face/Edge, Begin Sketch on Face, Apply Fillet to Edge.
- **Toolbar buttons:** (none, but tooltips on hover).

## Keyboard Shortcuts (Onshape-inspired)

| Key | Action |
|-----|--------|
| S | Sketch mode (on selected face) |
| E | Extrude |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Delete | Delete selected feature |
| Escape | Cancel current operation / exit sketch mode |
| F | Fit all in viewport |
| N | Normal to selected face |

## Communication

All communication with the engine is via wasm-bridge messages:
- UI produces `UiToEngine` messages (AddFeature, EditFeature, DeleteFeature, etc.).
- UI consumes `EngineToUi` messages (ModelUpdated, SketchSolved, Error, etc.).
- The `ModelUpdated` message includes the current FeatureTree state for display.

No Rust types are imported directly. The UI works with JSON-deserialized data from the bridge.
