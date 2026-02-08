# 08 — UI Chrome: Plan

## Milestones

### M1: Application Shell Layout
- [ ] SvelteKit app setup
- [ ] Three-column layout (tree + viewport + properties)
- [ ] Toolbar at top
- [ ] Status bar at bottom
- [ ] Responsive resizing (draggable panel borders)

### M2: Feature Tree Display
- [ ] Render feature list with icons
- [ ] Feature name display
- [ ] Suppressed feature styling (gray + strikethrough)
- [ ] Selected feature highlight
- [ ] Error indicator for failed features

### M3: Feature Tree Interactions
- [ ] Click to select
- [ ] Double-click to rename
- [ ] Right-click context menu
- [ ] Drag-and-drop reorder
- [ ] Produce EditFeature/DeleteFeature/SuppressFeature messages

### M4: Toolbar with Tool State
- [ ] Tool buttons for all drawing tools and operations
- [ ] Active tool indicator
- [ ] Tool state management (one active at a time)
- [ ] Keyboard shortcuts for tools

### M5: Property Editor Display
- [ ] Show parameters for selected feature
- [ ] Type-appropriate input fields (number, checkbox, dropdown)
- [ ] Parameter labels
- [ ] Reference display (show entity name for GeomRef params)

### M6: Property Editor Editing
- [ ] Edit parameter values
- [ ] Validate input
- [ ] Produce EditFeature message on change
- [ ] Show rebuild errors inline
- [ ] Debounce rapid changes

### M7: Status Bar
- [ ] Solve status display
- [ ] Selection info display
- [ ] Rebuild time display
- [ ] Error indicator

### M8: Context Menus
- [ ] Feature tree right-click menu
- [ ] Viewport right-click menu
- [ ] Context-sensitive options based on selection

### M9: Keyboard Shortcuts
- [ ] Implement shortcut table
- [ ] Shortcut hints in tooltips and menus
- [ ] Customizable shortcuts (stretch goal)

### M10: Rollback Slider
- [ ] Slider at bottom of feature tree
- [ ] Drag to set active_index
- [ ] Visual feedback (features after slider grayed out)
- [ ] Produce SetRollbackIndex message

## Blockers

- Depends on sketch-ui (for sketch mode integration)
- Depends on feature-engine (for FeatureTree data structure in ModelUpdated)
- Depends on modeling-ops (for operation parameter types)

## Interface Change Requests

(None yet)

## Notes

- The feature tree is the primary navigation UI. It must feel responsive.
- Property editor changes should trigger rebuilds after a short debounce.
- Look at Onshape's UI for reference on layout and interaction patterns.
