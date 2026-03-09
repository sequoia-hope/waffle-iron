# Waffle Iron — System Design (Research-Annotated)

Research references use `[#N]` notation from REFERENCES.md.

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│  Presentation Layer (Svelte + three.js/Threlte)     │
│  Rendering, interaction, UI chrome                  │
└──────────────────────┬──────────────────────────────┘
                       │ postMessage
┌──────────────────────┴──────────────────────────────┐
│  Bridge Layer (wasm-bridge)                         │
│  Command/query protocol, mesh transfer              │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────┐
│  Engine Layer                                        │
│  ├─ feature-engine: parametric tree, rebuild, undo  │
│  ├─ modeling-ops: extrude, revolve, boolean         │
│  └─ sketch-solver: 2D constraint solving (slvs)    │
└──────────────────────┬──────────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────────┐
│  Kernel Layer (new clean-sheet)                      │
│  ├─ Topology: Euler operators [#16, #33 Ch.4]       │
│  ├─ Geometry: NURBS [#32], Bezier, analytic surfs   │
│  ├─ SSI: topology-guaranteed [#25, #29], survey [#27]│
│  ├─ Boolean: hybrid B-Rep/mesh [#24]                │
│  │   ├─ Mesh extraction: bijective mapping [#24]    │
│  │   ├─ Exact mesh boolean [#8, #9]                 │
│  │   ├─ Classification: GWN on NURBS [#30]          │
│  │   ├─ Coplanar: overlap extraction [#26]          │
│  │   └─ Assembly: stepwise Euler ops [#33 §6.1]     │
│  ├─ Predicates: exact adaptive [#4], SoS [#5]      │
│  ├─ Validation: self-intersection [#31], body check │
│  │   [#33 §14.1]                                    │
│  ├─ Tessellation: curvature-adaptive                │
│  └─ Tolerances: 6-type policy [#33 Ch.16]           │
└─────────────────────────────────────────────────────┘
```

## Kernel Layer — Subsystem Details

### Topology: Euler Operators

**References**: [#16] Mantyla — completeness proof for Euler operator spanning sets.
[#33 Ch.4] Stroud — 99 operators, matrix decomposition, spanning sets.

All topology mutations go through Euler operators (MEV, MEF, MEKL, etc.),
which preserve the Euler-Poincare relation V-E+F-2(S-G)=0 by construction.
This guarantees manifoldness at every intermediate step.

### Geometry: NURBS and Analytic Surfaces

**References**: [#32] Piegl & Tiller — comprehensive NURBS algorithms (evaluation,
derivatives, knot insertion, refinement, degree elevation, point inversion).
[#1 Ch.5-6] Patrikalakis — surface interrogation and intersection curve properties.

The kernel represents geometry via NURBS curves/surfaces with analytic
specializations (plane, cylinder, cone, sphere, torus) for exact operations
where applicable.

### Surface-Surface Intersection (SSI)

**References**: [#25] Yang, Jia & Yan — topology-guaranteed tracing via Dixon
resultant and characteristic points. [#29] Cheng et al. — IATA hybrid
symbolic-numeric for tangent points and tiny loops. [#27] Li et al. — comprehensive
survey comparing OCCT, ACIS, SolidWorks approaches. [#1 Ch.5] Patrikalakis —
lattice, marching, subdivision methods.

SSI determines intersection curves between surface pairs. The topology of
intersection curves (number of branches, start/end points, tangencies) is
determined algebraically before numerical tracing begins, preventing missed
branches and spurious loops.

### Boolean Pipeline: Hybrid B-Rep/Mesh

**References**: [#24] Yang et al. — 6-stage hybrid pipeline (bijective mesh
extraction, exact mesh boolean, NURBS re-mapping). [#8] Zhou et al. — mesh
arrangements with winding numbers. [#9] Cherchi et al. — fast exact mesh
arrangements with indirect predicates. [#33 §6.1] Stroud — stepwise boolean
assembly with Euler operators.

The boolean pipeline:
1. **Bijective mesh extraction** [#24]: Map NURBS surfaces to triangle meshes
   preserving the surface↔mesh correspondence for later re-mapping.
2. **Exact mesh boolean** [#8, #9]: Compute the boolean on triangle meshes using
   exact predicates, producing a topologically correct arrangement.
3. **Classification** [#7, #30]: Use generalized winding numbers to classify
   faces as inside/outside/on-boundary. GWN on trimmed NURBS [#30] avoids
   tessellation-dependent classification errors.
4. **Coplanar handling** [#26]: Extract overlap regions as a 2D phenomenon
   (bilevel optimization on parameter domains).
5. **Re-mapping** [#24]: Transfer the mesh boolean result back to NURBS,
   producing exact B-Rep output.
6. **Assembly** [#33 §6.1]: Construct the result solid using Euler operators,
   guaranteeing manifoldness.

### Predicates: Exact Adaptive Arithmetic

**References**: [#4] Shewchuk — adaptive precision floating-point for orient2d,
orient3d, incircle, insphere. [#5] Edelsbrunner & Mucke — Simulation of
Simplicity for degenerate configurations. [#19] Devillers & Preparata —
filter failure probabilities.

Geometric predicates (orientation, containment) use Shewchuk's adaptive
expansions. Degenerate cases (four coplanar points, three collinear points)
are resolved via SoS perturbation rather than ad-hoc epsilon comparisons.

### Validation: Body Checking

**References**: [#31] Li et al. — fast self-intersection detection for NURBS via
algebraic signatures and control-point analysis. [#33 §14.1] Stroud — ACIS body
checker (edge convexity, containment, self-intersection tests).

Post-boolean validation checks: edge convexity consistency, face containment,
no self-intersection, Euler characteristic, watertightness.

### Tolerances: 6-Type Policy

**References**: [#33 Ch.16] Stroud — six tolerance types (modelling, Boolean,
approximation, intersection, simplification, visualization) with consistency rules.

A centralized tolerance policy governs all numerical comparisons. No ad-hoc
epsilons. Each tolerance type has a defined relationship to the others.

## Engine Layer

### Feature Engine

**References**: [#33 Ch.9] Stroud — facesets, frames, design-by-features,
feature recognition and verification.

Parametric feature tree with deterministic rebuild. Persistent naming via
GeomRef (semantic geometry references that survive rebuild). Undo/redo.

### Sketch Solver

2D geometric constraint solving via libslvs (SolveSpace's solver).
Newton-Raphson iteration with DOF analysis.

## Presentation Layer

Svelte 5 + three.js via Threlte v8. All rendering on main thread.
Mesh data transferred from WASM as TypedArrays. Raycasting with
face-range metadata for entity picking.

## Cross-Cutting Concerns

### Scale Normalization

**References**: [#24] Yang et al. — unit-cube normalization before mesh boolean.
[#27] Li et al. — all production systems normalize. [#31] Li et al. — unit-cube
normalization with epsilon=1e-6 post-normalization.

All geometry is normalized to a consistent scale range before kernel operations
to ensure absolute-tolerance algorithms work correctly.

### Determinism

**References**: [#20] Astarlioglu — determinism comparison across boolean methods.

Same inputs must produce same outputs. No nondeterministic iteration orders.
Deterministic hashing for tessellation. Sequential ID counters reset between
independent operations.
