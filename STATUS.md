# Waffle Iron — System Status

**Last updated:** 2026-02-21

## System Status Matrix

Status codes: **W** = Working end-to-end, **P** = Partial, **DEFERRED** = Deferred indefinitely

| Feature | Status | Notes |
|---------|:------:|-------|
| Sketch drawing (line/rect/circle/arc) | **W** | Click-click and click-drag modes |
| Sketch constraints (all types) | **W** | Via libslvs (Emscripten WASM) |
| Sketch profile extraction | **W** | Closed profile detection for extrude/revolve |
| Construction geometry | **W** | |
| Extrude | **W** | Depth, direction, profile selector, cut toggle |
| Revolve | **W** | Angle, axis quick-pick (X/Y/Z), sketch line axis |
| Fillet | **DEFERRED** | MockKernel tests pass; TruckKernel experimental; UI disabled |
| Chamfer | **DEFERRED** | MockKernel tests pass; TruckKernel experimental; UI disabled |
| Shell | **DEFERRED** | MockKernel tests pass; TruckKernel experimental; UI disabled |
| Boolean union/subtract/intersect | **P** | Box-box offset works; box-cylinder and coplanar fragile |
| Feature tree CRUD | **W** | Add, edit, delete, rename, reorder, suppress |
| Undo/redo | **W** | Full command-pattern undo/redo |
| Rollback slider | **W** | |
| GeomRef persistent naming | **W** | Role-based + signature fallback; tested with MockKernel |
| 3D viewport rendering | **W** | Threlte v8, face picking, edge overlay, view cube |
| Camera controls | **W** | Orbit, snap, fit, zoom-to-cursor |
| Sketch plane selection | **W** | XY/XZ/YZ quick-pick dialog |
| Datum planes | **W** | Visual indicators with low opacity |
| File save/load (JSON) | **P** | Engine works; no browser file dialogs |
| STEP export | **P** | Native only; not available in WASM |
| Sketch-on-face | **P** | Button exists; plane extraction not wired |

## Test Counts

| Crate / Suite | Tests | Notes |
|---------------|------:|-------|
| kernel-fork | 174 | +2 ignored; includes boolean workflow tests |
| sketch-solver | 31 | Constraint solving, profile extraction |
| wasm-bridge | 22 | Message dispatch, pipeline tests |
| feature-engine | 124 | Feature tree, rebuild, GeomRef, undo/redo |
| modeling-ops | 54 | Extrude/revolve/fillet/chamfer/shell/boolean provenance |
| file-format | 26 | Save/load round-trip, format validation |
| test-harness | 270 | Cross-crate integration, multi-operation scenarios |
| **Rust total** | **~700** | |
| GUI (Playwright) | 425 | 47 spec files; click-click + click-drag |
| **Grand total** | **~1125** | |

## What's Working

The core parametric pipeline is functional end-to-end:

1. **Sketch on plane** — Select XY/XZ/YZ plane, enter sketch mode
2. **Draw geometry** — Line, rectangle, circle, arc (click-click and click-drag)
3. **Apply constraints** — All constraint types via libslvs solver
4. **Extrude/Revolve** — Create 3D solids from sketch profiles with parameter dialogs
5. **Feature tree management** — Full CRUD, undo/redo, rollback, reorder, suppress
6. **3D viewport** — Shaded rendering, face picking, edge overlay, camera controls
7. **Persistent naming** — GeomRef system with role-based + signature fallback resolution

## What Needs Work

1. **Boolean reliability** — truck boolean operations fail for box-cylinder, coplanar faces, and chained operations. This is the #1 technical risk. Work happens in `vendor/truck/`.
2. **GUI test coverage** — 425 Playwright tests exist but many scenarios remain untested. Expand coverage in `app/tests/gui/`.
3. **Integration tests** — Cross-crate scenarios in `crates/test-harness/` need more multi-operation workflows.
4. **Sketch-on-face** — Plane extraction from selected face GeomRef is not wired.
5. **File save/load UI** — Engine supports JSON save/load but browser dialogs are missing.
6. **STEP/STL export in browser** — STEP export works natively but not in WASM context.

## DEFERRED INDEFINITELY: Fillet, Chamfer, Shell

**These operations are deferred indefinitely. Do not work on them.**

- MockKernel implementations exist and tests pass
- Experimental TruckKernel implementations were added in Sprint 18
- UI dialogs display warning banners with disabled Apply buttons
- These depend on boolean reliability, which is itself fragile
- Priority should go to boolean shapeops, GUI tests, and integration tests instead
