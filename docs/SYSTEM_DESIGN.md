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
│  └─ sketch-solver: 2D constraint solving (LM)      │
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
│  ├─ Tessellation: curvature-adaptive [#34, #35]     │
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

### Geometry: Three-Tier Surface Hierarchy

**References**: [#32] Piegl & Tiller — NURBS evaluation algorithms. [#36] Parasolid
— 3-tier surface architecture (analytic/procedural/NURBS). [#1 Ch.5-6] Patrikalakis
— surface interrogation and quadric SSI. [#33 Appendix A] Stroud — surface data
definitions. [#37] Mistry — swept volume B-Rep construction.

The kernel uses a three-tier surface hierarchy (ADR-11):

- **Tier 1 — Analytic** (plane, cylinder, cone, sphere, torus): Compact parameter
  storage, O(1) evaluation, exact SSI for all 15 pair combinations (A15). These
  are the workhorse surfaces for mechanical CAD.
- **Tier 2 — Procedural** (swept, spun, ruled, lofted, offset, pipe): Stored as
  construction recipes (profile + spine + orientation law). Converted to NURBS
  only when needed for SSI with freeform surfaces. Preserves editing capability.
- **Tier 3 — Freeform** (BSpline/NURBS): Universal fallback for imported geometry
  and surfaces that don't fit Tier 1/2. Cox-de Boor evaluation [#32]. Numerical
  SSI via topology-guaranteed tracing (ADR-2).

Conversion is lazy and upward-only (Tier 1 → 2 → 3). Unmodified faces preserve
their surface tier through boolean operations (A15.5). A unified `SurfaceEval`
trait provides evaluation, normals, derivatives, and point inversion across all
tiers. See `specs/surface_type_taxonomy.md` for the full taxonomy and ADR-11 for
the decision rationale.

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

2D geometric constraint solving via clean-room Rust solver (Levenberg-Marquardt
least-squares minimization with rank-revealing QR for DOF analysis).

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
