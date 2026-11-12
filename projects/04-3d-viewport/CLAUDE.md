# 04 — 3D Viewport: Agent Instructions

You are working on the viewport, `app/src/lib/viewport/`. Read
ARCHITECTURE.md in this directory first.

## Rules

1. **No WASM calls here.** The viewport reads meshes and state from the store
   and sends selections back through it.
2. **Picking resolves to a `GeomRef`.** A hover or click identifies a logical
   face, edge or vertex, never a raw triangle.
3. **Thresholds are in screen pixels.** Convert with `worldPerPixel`; never
   hard-code a world-unit pick distance.
4. **Every layer applies the instance transform.** Faces, edges and vertices
   of an assembly instance all go through `placement.js`.
5. **Framing math lives in `framing.js`.** The live camera and the agent
   capture share it; do not duplicate it.
6. **Colors come from CSS custom properties**, with a fallback hex.

## Test

GUI tests are in `app/tests/gui/`. Run `./scripts/test.sh gui-fast` for the
smoke tier and `./scripts/test.sh gui-full` for everything.

## Dependencies

`three`, `@threlte/core`, `@threlte/extras`, Svelte 5.
