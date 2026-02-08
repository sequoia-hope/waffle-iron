# 10 — Assemblies: Architecture

## Status: DEFERRED

This sub-project is deferred until all other sub-projects reach MVP level. This document captures the eventual design for future implementation.

## Assembly Concepts

### Assembly Tree
- Hierarchical tree of parts and sub-assemblies.
- Each node is either a **Part** (references a Waffle Iron project file) or a **Sub-Assembly** (references another assembly).
- Parts can appear multiple times (instances) with different positions.

### Mate Connectors
- Named coordinate frames attached to part geometry (faces, edges, vertices).
- Each mate connector has: origin point, primary axis (Z), secondary axis (X).
- Mate connectors are the "plugs" that mates connect.

### Mate Types

| Mate | DOF Removed | Description |
|------|-------------|-------------|
| Fastened | 6 | Rigidly locked (zero DOF) |
| Revolute | 5 | Rotation about one axis |
| Slider | 5 | Translation along one axis |
| Cylindrical | 4 | Rotation + translation along one axis |
| Ball | 3 | Rotation about a point |
| Planar | 3 | Translation in plane + rotation about normal |

### Mate Solver
- Constrain relative positions/orientations of parts based on mates.
- Similar in concept to the 2D sketch solver but in 3D.
- libslvs may be usable for 3D mate solving (it supports 3D constraints).
- Alternative: custom 3D constraint solver.

### In-Context Editing
- Edit a part within the context of the assembly.
- See other parts as reference geometry.
- Changes to the part propagate to all instances in the assembly.

## truck-assembly Status

truck-assembly is recently added, not published to crates.io. It provides:
- DAG representation of part-to-assembly relationships from STEP files.
- Purely positional assembly (no constraints, no mates).

Waffle Iron will build its own assembly structure. truck-assembly may be useful for STEP assembly import/export only.

## Prerequisites

All other sub-projects at MVP:
- Parts can be created (sketch → extrude → fillet workflow).
- Parts can be saved/loaded (file-format).
- The UI supports part editing (ui-chrome).
- The WASM bridge handles assembly messages.
