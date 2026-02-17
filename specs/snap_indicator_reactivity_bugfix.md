# Spec: Snap Indicator Reactivity Bug Fix

## Goal

`.snap-label` DOM element must render when `getSnapIndicator()` returns non-null during
active sketch drawing.

## Bug Description

`SketchRenderer.svelte` imports `getSnapIndicator` from `sketchToolState.svelte.js` (a
`.svelte.js` file that uses `$state` runes for reactivity). The old code imported from
`tools.js` (a plain `.js` file), which meant the `$derived.by(() => getSnapIndicator())`
in the component never re-ran when snap state changed. The `snapLabelData` was always
null, so `{#if snapLabelData}` never rendered the `.snap-label` DOM element.

## Branch Table

| Scenario                      | Snap type   | Expected DOM                               |
|-------------------------------|------------|---------------------------------------------|
| Hover endpoint during drawing | coincident | `.snap-label` visible, text "Coincident"    |
| Move away from all targets    | none       | `.snap-label` absent                        |
| Return to snap target         | coincident | `.snap-label` reappears                     |

## Invariant

- **API non-null => DOM visible**: When `getSnapIndicator()` returns non-null, `.snap-label` must be visible in the DOM
- **API null => DOM absent**: When `getSnapIndicator()` returns null, `.snap-label` must not exist in the DOM

## Oracle

- `page.locator('.snap-label')` visibility assertion + `textContent()` text matching
- Cross-check: `__waffle.getSnapIndicator()` API data agrees with DOM state
