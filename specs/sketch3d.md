# 3D sketch — spatial reference geometry, and the path a sweep runs along

Prerequisite for: `specs/b6_general_sweep.md` §5.3/§6/§8 (3D sweep paths) and
the frame feature (B6 §10). Also serves routing, reference geometry and
anything that wants a spatial polyline.

Owner crates: `waffle-types` (S1), `feature-engine` (S2), `wasm-bridge` (S3),
`app` (S5–S6).

Status: **S1, S2 and S3 LANDED 2026-09-25**; S4 onward is design. The §4 fork was
decided in favour of (A), no 3D constraint solver — additively, so (B) remains
open as its own epic. §3 carries a correction S2 forced.

---

## 1. Scope

A 3D sketch is **an open or closed chain of lines and arcs in space, plus the
points that define them**. It is reference geometry: it produces no body, and
it is **not extrudable** — there is no region, no profile, no area. Its
consumers read a chain out of it.

What it is for, in order of why it is being built:

1. A **sweep path** that leaves a single plane (B6 §5.3).
2. The **centre-line graph of a frame or truss** — many chains in one sketch,
   one member per edge (B6 §10).
3. Reference geometry in general: a spatial polyline to measure, snap to, or
   hang a mate connector on.

Explicitly not in scope: splines in 3D (B6 §7 says a spline path is not
sweepable analytically anyway), surfaces, and any notion of a 3D region.

## 2. A separate operation, not a flag on `Sketch`

`Sketch` is planar in its bones: `plane` / `plane_origin` / `plane_normal` /
`plane_x_axis` (`crates/waffle-types/src/sketch.rs:50-71`), entities in `(u,v)`,
`solved_profiles`, and a region detector. `plane_normal` alone has 49 uses in
`feature-engine/src/rebuild.rs`. Making the plane optional would put an
`if planar` in every one of those and in every consumer that assumes a profile
exists.

So: a **new type `Sketch3d` behind a new `Operation::Sketch3d`**, modelled on
`Operation::MateConnector` and `Operation::DatumPlane` — operations that own
reference geometry, produce no `outputs`, and whose rebuild only proves the
thing resolves (`rebuild.rs:555`, `:575`). They are already excluded from the
body-bearing feature scans (`rebuild.rs:1668`, `:1725`); `Sketch3d` joins those
lists.

**The file format cost is one operation tag and no version bump.** Since v4
Phase 1b an unknown `{"type": …}` operation round-trips through
`Operation::Unknown` and fails loudly only at rebuild
(`feature-engine/src/types.rs`, the `Unknown` variant's note), so adding an
operation kind is explicitly *not* a `MIN_READER_VERSION` bump. An older
reader opening a document with a 3D sketch keeps it intact and refuses to
rebuild that one feature.

## 3. The type

> **Corrected at S2 (2026-09-25).** This section first gave `Sketch3d` a
> `resolved` map and a `status`, "written by the engine at rebuild, exactly as
> `Sketch::solved_positions` is". That is not possible, and the reason is
> worth keeping: `solved_positions` is written by the engine's **mutable
> pre-pass** (`params::apply_parameters`), which runs before the rebuild and
> has no kernel and no feature results. A 3D sketch needs both — a point
> attached to a vertex of feature N can only resolve after feature N has
> rebuilt — so it must be evaluated during the rebuild **walk**, where the
> tree is `&FeatureTree`. A cache that could only ever be filled in for the
> unattached points would be worse than none. So the type is declarative and
> the evaluation is carried out of the rebuild in `RebuildState::sketch3d`,
> read afterwards from `Engine::sketch3d` — the `MateConnector` precedent,
> whose frame is likewise "read after the rebuild".

```rust
/// Declarative; no engine-written cache (see the correction above).
pub struct Sketch3d {
    pub id: Uuid,
    pub entities: Vec<Sketch3dEntity>,
}

/// What evaluating one produces — carried in the rebuild result.
pub struct Sketch3dEvaluation {
    pub resolved: BTreeMap<u32, [f64; 3]>,
    pub chains: Vec<Chain3d>,
}

pub enum Sketch3dEntity {
    Point {
        id: u32,
        /// Literal coordinates, always present and always the last evaluated
        /// value — so a reader that does not evaluate still sees the shape.
        xyz: [f64; 3],
        /// Optional attachment that DERIVES `xyz` at rebuild (§4).
        attach: Option<Attachment>,
        /// Optional driving expressions, per component, mm-space like every
        /// other `*_expr` in the tree (`ExtrudeParams::depth_expr`).
        xyz_expr: Option<[Option<String>; 3]>,
        construction: bool,
    },
    Line { id: u32, start_id: u32, end_id: u32, construction: bool },
    /// Explicit 3-point arc: start, end, and any interior point on it. Three
    /// distinct non-collinear points determine centre, radius and plane, so
    /// the arc needs no stored frame that could disagree with its ends.
    Arc { id: u32, start_id: u32, end_id: u32, via_id: u32, construction: bool },
    /// A tangent fillet of `radius` at the joint between the two segments
    /// meeting at `at_point_id`. A GENERATOR entity: expanded at evaluation
    /// into a concrete arc plus the two trimmed segment ends, the way
    /// `SketchEntity::Gear` / `Sprocket` are expanded by `expand_generators`
    /// (`sketch.rs:222`). Tangency is exact by construction, never solved.
    Fillet { id: u32, at_point_id: u32, radius: f64, radius_expr: Option<String> },
}
```

Ids are `u32` and unique within the sketch, matching `SketchEntity`. Generator
expansion offsets into the entity's own id range the way the sprocket does
(`generated_entity_id_base`), so a fillet's arc has a stable id nothing else
can collide with — enforced, not assumed: a fillet id too large for the range
and a sketch entity sitting on a minted id are both typed refusals (§5 step 3).

Why `Fillet` is a generator rather than a stored arc: a sweep wants a G1 bend
at some corners and a sharp mitre at others (B6 §4), and the difference must
survive editing the lines. A stored arc goes stale the moment a segment moves;
a generator re-derives, and its tangency is exact rather than solved to
`SOLVE_TOL`.

## 4. Positioning — and the one open decision

The 2D sketch is positioned by a Levenberg–Marquardt solver over a parameter
layout (`sketch-solver/src/solver.rs:167`), with **2,670 lines** of 2D residual
and Jacobian mapping in `constraint_mapping.rs`. A 3D constraint solver means a
3D twin of that.

**The fork.** Two honest options:

- **(A) No LM solver in 3D.** Points are literal coordinates, driven where
  wanted by expressions over design parameters, and derived where wanted by an
  **attachment** that is resolved *by construction*, not by iteration:

  ```rust
  pub enum Attachment {
      Vertex(GeomRef),                    // coincident with a model vertex
      EdgePoint { edge: GeomRef, t: f64 },// a parameter along a model edge
      OnPlane { plane: GeomRef, uv: [f64; 2] },
      Offset { from: u32, delta: [f64; 3] },   // relative to another sketch point
      AlongAxis { from: u32, axis: Axis, distance: f64 }, // the X/Y/Z run
  }
  ```

  Coincidence between two sketch entities is not a constraint at all — it is
  a *shared point id*, which is already how `Line { start_id, end_id }` works.
  Tangency at a bend is the `Fillet` generator. Everything resolves in one
  deterministic pass with no convergence, no `dof`, no over-constraint
  diagnosis.

- **(B) A real 3D constraint solver.** Generalize `ParamLayout` to three
  components and write 3D residuals and Jacobians for coincident, along-axis,
  parallel, perpendicular, tangent, distance, angle. Weeks, and it is the
  single largest piece of the whole 3D-sketch effort — larger than the GUI.

**Recommendation: (A), and design (B) out of the way rather than out of the
future.** The reasons:

- The parametric story this repo already has is *expressions over design
  parameters*, not solved dimensions — `depth_expr`, `radius_expr`,
  `parameters_set`, `expression_evaluate`. A 3D path driven by
  `x: "tower_base/2"` is parametric in exactly the way the rest of the tree is.
- The two things a path genuinely needs to be *exact* — coincidence and
  tangency — are exact under (A) and only `SOLVE_TOL`-exact under (B).
- (A)'s stored form is a strict subset of (B)'s: literal coordinates plus
  attachments are precisely the initial guess and the constraint set a solver
  would consume later. Adding (B) never invalidates a document written
  under (A).
- 3D constraint solving is where commercial CAD is least loved, and the
  failure mode — an under-constrained spatial sketch that flips to a mirrored
  solution on rebuild — is one this codebase's determinism rules would hate.

If (B) is wanted anyway, it should be its own spec and its own epic, sequenced
after a 3D sketch exists and there is evidence of what constraints people
actually reach for.

## 5. Evaluation

One deterministic pass, all of it pure — `Sketch3d::evaluate(&self, &dyn
ExternalAnchors) -> Result<Sketch3dEvaluation, Sketch3dError>`. Step 1 is the
exception to "at rebuild": expressions are evaluated in the engine's mutable
pre-pass with every other `*_expr` in the tree, because they need only the
design parameters. Steps 2–4 run in the rebuild walk.

1. **Expressions** — evaluate `xyz_expr` components against the design
   parameters; write results into `xyz`. Same mm→m convention as every other
   `*_expr`. A point cannot carry both an expression and an attachment: the
   attachment derives the point and never reads `xyz`, so the expression could
   only be evaluated into a value nothing looks at — that is
   `ExpressionOnAttachedPoint`, loud, not a silent no-op.
2. **Attachments** — resolve in dependency order (`Offset` and `AlongAxis`
   reference other sketch points, so the graph is topologically sorted; a cycle
   is `Sketch3dCyclicAttachment`, loud). A model-geometry attachment resolves
   through `ExternalAnchors`, which answers `Result<AnchorHit, String>`: a
   refusal carries the resolver's own reason, repeated verbatim in
   `UnresolvedAttachment`, and a success may carry warnings — a `BestEffort`
   pick that re-bound onto the NEAREST entity after the geometry moved says so.
   Those warnings are collected in `Sketch3dEvaluation::warnings`, prefixed
   with the point, and the engine reports them as the feature's own warnings.
   A picked entity (a `Position` selector) resolves the same way for a face as
   for a vertex or an edge — through the nearest-entity resolver, which is what
   lets it re-bind.
3. **Generators** — expand each `Fillet` into its arc and trim the two
   segments. A radius that does not fit between the joint's neighbours is
   `Sketch3dFilletTooLarge { at_point }`, loud, and the sketch fails as a whole
   rather than silently dropping the bend. The joint ids a fillet mints come
   from `checked_generated_entity_id_base`: a fillet id above
   `MAX_GENERATOR_ENTITY_ID` is `FilletIdOutOfRange` (not a `u32` overflow), and
   a sketch entity already using a minted id is `GeneratedIdCollision` (not a
   three-way junction where the author drew a corner).
4. **Chains** — walk the line/arc entities into maximal chains by shared point
   id, each open or closed. This is the consumable output.

A `Sketch3d` with a degenerate segment (coincident endpoints), a non-manifold
junction (three segments at one point) *inside a chain a consumer asked for*,
or a non-collinear-failing 3-point arc is a typed error. Note a **branching
junction is legal in the sketch** — a truss centre-line graph is full of them —
and only a consumer that demanded a single chain complains. That keeps the
frame feature (B6 §10) able to read the whole graph.

## 6. What a consumer reads

```rust
pub struct Chain3d {
    pub edges: Vec<Edge3d>,   // Line { a, b } | Arc { a, b, centre, normal, radius }
    pub closed: bool,
    /// Per-joint tangent continuity, precomputed: the sweep uses it to choose
    /// mitre vs. smooth (B6 §4) without recomputing the classification.
    pub g1: Vec<bool>,
}
```

`Sketch3d::chains() -> Result<Vec<Chain3d>, _>` is the whole consumer API.
B6's `SweepPath::new` takes a `Chain3d`; the planar-path form of B6 §8
constructs the same `Chain3d` out of an ordinary `Sketch`, so **the sweep
never learns which kind of sketch its path came from**.

## 7. Drawing it — the GUI problem

A 2D pointer cannot place a 3D point, so every CAD that has a 3D sketch
supplies the missing degree of freedom from somewhere. Four sources, in the
order they should be consulted for a click:

1. **Snap to existing geometry.** Vertices, edge midpoints and endpoints, face
   centres, other sketch points, the origin. Most points in a real 3D sketch
   are on something. `app/src/lib/sketch/snap.js` (543 lines) is a 2D snapper
   today; the 3D one is the same idea against model geometry, and it is the
   single highest-value piece of this GUI.
2. **Axis lock (the space handle).** While rubber-banding from the previous
   point, lock to X, Y or Z — or to the previous segment's direction, or its
   perpendiculars — with the locked axis drawn. This is how the overwhelming
   majority of frame geometry gets drawn: runs along axes.
3. **Typed entry.** An `(x, y, z)` / `(Δx, Δy, Δz)` field that accepts
   expressions, sharing the parser `expression_evaluate` already uses. Cheap,
   exact, and the thing an engineer reaches for when the number is known.
4. **The fallback plane.** Nothing snapped and no axis locked ⇒ project the
   cursor ray onto a plane through the last point, parallel to a chosen datum
   (cycled with a key, shown in the HUD). Never a silent guess: the plane is
   drawn while it is in force.

A click that resolves through none of these is refused rather than placed
somewhere arbitrary.

## 8. Agent surface

The agent link does not have the 2D-pointer problem at all — it names
coordinates. So the MCP surface is small and lands long before the GUI.

**As landed (S3, 2026-09-25) it is smaller than this section first proposed,
and deliberately so.** There is no `sketch3d_create` and no `sketch3d_edit`:
a 3D sketch is a *declarative* operation, so `feature_add` and `feature_edit`
carry it like any other kind once `Sketch3d` is on the authorable list, and
they bring the rollback-on-error, provenance and undo semantics with them.
`sketch_create` is its own tool only because a 2D sketch orchestrates four
engine messages and builds a profile payload between the solve and the commit;
a 3D sketch does none of that, and a bespoke twin would have been a second
authoring path to keep in step with the first.

What genuinely needs a tool is reading it back — hence **`sketch3d_get`**
alone. The resolved coordinates and the chains are not in the document (an
attached point resolves only during the rebuild walk), so `feature_get`
returns the declaration and nothing else. `sketch3d_get` answers where every
point landed, what chains the segments formed, each chain's length and
closure, and `tangent_joints` — which is what tells a sweep a mitre from a
smooth bend. A suppressed, rolled-back or failed sketch answers
`NotEvaluated` rather than an empty chain list, because "no evaluation" and
"a path with no segments" are different facts.

Input validation gets no separate pass either. `sketch_create` has A13 shape
checks because a 2D sketch's dangling reference would otherwise reach the
solver unnoticed; a 3D sketch's evaluation already refuses `PointNotFound`,
`DuplicateEntityId` and the rest by entity id, and the authoring step rolls
back on them. A second validator could only drift from the first.

**`ctx.sketch3d` is deferred**, not dropped: a Rhai binding with no caller is
speculative surface. Its customer is the frame feature (B6 §10), and it
should land with it.

**This is the sequencing lever.** A 3D path is authorable by an agent as soon
as S1–S3 land, which means B6's sweep can be built and proven on real 3D paths
while the 3D-sketch GUI is still being written. The GUI is not on the critical
path to swept geometry.

## 9. Increments

| # | What | Gate |
|---|---|---|
| **S1 ✅ DONE (2026-09-25)** | `Sketch3d` + entities + `Attachment` in `waffle-types`; evaluation (attachments, fillet expansion, chain extraction) as pure functions; `Chain3d`. 21 unit tests: fillet tangency exact (in plane and out), two fillets sharing a segment, chain walking (open/closed/branching), every typed refusal. | — |
| **S2 ✅ DONE (2026-09-25)** | `Operation::Sketch3d` in `feature-engine`: evaluated in the rebuild walk, no outputs, joined to all four reference-geometry exclusion lists, carried in `RebuildState::sketch3d` / `Engine::sketch3d` and carried forward for a feature that did not re-execute; `EngineAnchors` resolves `Vertex` / `EdgePoint` (by arc length) / `OnPlane` (datum or planar face, `SketchPlaneBasis`); expressions in `params::apply_sketch3d`; `OPERATION_TAGS` (16 → 17) and the schema golden, both purely additive; `file-format` round-trip with no version bump. 9 engine tests + 2 format tests. | S1 |
| **S3 ✅ DONE (2026-09-25)** | `Sketch3d` authorable through `feature_add` / `feature_edit`; `sketch3d_get` reads the evaluation; the agent manifest and `feature_add`'s operation note document the entity and attachment vocabulary; a feature-tree icon. `ctx.sketch3d` deferred to its first caller (§8). WASM rebuilt in the same commit. **3D paths are now authorable.** | S2 |
| **S4 ✅ DONE (2026-09-25)** | B6 consumes it: `SweepPath::new(&Chain3d, &Profile)` and the parallel-transport frame law (B6 §6) landed together as **B6 S1**, so the consumer arrived before the sweep assembler rather than after it. `Chain3d` needed no change; `g1` is advisory to the sweep, which reclassifies at the same band. | S3 |
| **S5** | Viewport rendering of a 3D sketch (lines, arcs, points, construction styling) + selection/hover. | S2 |
| **S6** | The drawing GUI of §7: snapping, axis lock, typed entry, fallback plane. GUI specs per §7 source, both click-click and click-drag per the session guide. | S5 |

## 10. Oracles

- **Fillet tangency is exact**, not approximate: the arc's end tangents equal
  the segment directions to within floating-point representation, asserted
  directly rather than against a tolerance band.
- **Evaluation is deterministic and idempotent**: evaluating the same sketch
  twice is bit-identical; no `HashMap` iteration in the path (the solver's own
  rule, `solver.rs:14`).
- **Chain extraction** round-trips: a chain walked out and rebuilt gives the
  same edge sequence; a closed chain is detected as closed regardless of which
  entity is listed first.
- **Attachment dependency order** is independent of entity declaration order.
- **A planar 3D sketch equals the 2D one**: a `Sketch3d` whose points are
  coplanar must produce a `Chain3d` identical to the one the equivalent planar
  `Sketch` produces — the oracle that keeps B6 honest about not caring which
  sketch its path came from.
- **Back-compat**: an older reader round-trips a document containing a
  `Sketch3d` byte-identically through `Operation::Unknown`.
