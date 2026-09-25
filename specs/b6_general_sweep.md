# B6: general sweep — an arbitrary section along a chain of lines and arcs

Roadmap: `specs/custom_features_and_modeling_roadmap.md` (Part B; supersedes
that document's §B5 line "general sweep and loft: need surfaces the kernel
does not carry" for the analytic family defined below — loft is untouched).
Motivating evidence: `docs/notes/eiffel/FEATURE_NOTES.md` §2 (1,200 structural
members cost ~660 calls and 794 KB of restated rectangles).

Owner crates: `kernel-v2` (increments S1–S5), `waffle-types` +
`feature-engine` + `wasm-bridge` (S6), `app` (S7).

Status: **DESIGN**, feature set approved 2026-09-25. Nothing below is
implemented.

**Sequencing decided 2026-09-25: 3D sketch first** (`specs/sketch3d.md`), then
this. The user's target is a genuinely 3D member run, so the planar-path-only
v1 of §8 is not the thing to build — a `Chain3d` from a 3D sketch is, and §6's
parallel-transport frame law is in scope from the start. The planar `Sketch`
still constructs the same `Chain3d` (sketch3d §6), so a planar path is not a
separate code path; it is a coplanar input.

**`Operation::Pipe` is to be subsumed, not paralleled.** It was scoped too
narrowly (memory `feedback-build-the-general-feature-not-the-special-case`):
a circle section along a tangent-continuous planar chain is one cell of §2's
table. The §9 bit-identical pipe-continuity oracle is therefore not just a
regression net — it is the evidence that lets `construct::pipe` be deleted and
`Operation::Pipe` become a thin lowering onto `Sweep`, keeping its typed errors
and its dialog as the shipped contract they are.

---

## 1. The claim this design rests on

**A sweep of a planar section along a chain of line and arc segments, with
the section held perpendicular to the path, is a sequence of extrudes and
partial revolves. It needs no surface the kernel does not already carry.**

The extrude half is obvious. The revolve half is the load-bearing part:

> Let a path segment be a circular arc of centre `C` in a plane `P` with unit
> normal `n̂`; its axis is the line through `C` along `n̂`. At any point `p` on
> the arc the tangent `t̂` lies in `P`, so the section plane (through `p`,
> perpendicular to `t̂`) contains `n̂`. It also contains the radial direction
> `p − C`, which is in `P` and perpendicular to `t̂`. A plane containing both
> `n̂` and the radial direction through `p` contains the point `C`, hence
> contains the whole axis line.
>
> So the section plane contains the revolve axis at every point of the arc,
> and sweeping the section along the arc is *identical* to revolving it about
> that axis through the arc's sweep angle.

Two consequences worth stating plainly:

- The surfaces produced are exactly the surfaces of revolution of the
  section's edges: `Plane`, `Cylinder`, `Cone`, `Torus` — every one of them
  already in `kernel_v2::arena::Surface` (`arena.rs:175`), and every one of
  them already produced by `construct::revolve`.
- Nothing above requires the *path* to be planar. Each arc segment supplies
  its own plane and axis; the argument is local to the segment. A 3D chain of
  lines and arcs sweeps analytically too (§6).

`construct::pipe` (`pipe.rs:312`) is this design's bottom-right corner already
built: a circle section, cylinders for line segments, tori for arc segments,
consecutive laterals sharing their rim circle as one edge. B6 is that
structure with the section generalized and the path's restrictions lifted.

## 2. What is missing, exactly

Per path-segment kind × section kind. ✅ = an assembler exists today.

| section \ segment | **Line** (extrude) | **Arc** (partial revolve) |
|---|---|---|
| Polygon, no holes | ✅ `extrude` | **Parallel / Perpendicular edges ✅** (KV6a wedge); **Oblique edge ⛔** — `EdgeClass::Oblique` is full-turn only, "a partial revolve of an oblique edge sweeps an arc-bounded cone patch (KV6c increment 5) and is rejected typed" (`revolve.rs:74`) |
| Polygon with holes | ✅ KV14 | ⛔ `RevolveProfileHolesUnsupported` (`revolve.rs:216`) |
| `ArcPolygon` (mixed line/arc, holes) | ✅ KV12 Tier 2 E4/E4b | ⛔ `ArcPolygonProfileUnsupported` (`revolve.rs:220`) |
| Full circle | ✅ `extrude_circle` | ✅ torus (KV6d) — *this row is `pipe`* |

So the kernel work is **the right-hand column**: three typed walls to convert,
each of which is a capability the roadmap wants anyway (a partial revolve of a
holed or arc-bearing profile is not a sweep-only need — it is the general
lathe finishing its job).

Note what the table says about the motivating case: a rectangular section in
its natural orientation on a bend has two edges parallel to the bend axis
(`Parallel` → cylinders) and two perpendicular to it (`Perpendicular` →
planar annular sectors). **A mitred rectangular elbow needs nothing new**; it
is the KV6a partial-revolve wedge with a section that is not a lathe profile.
`Oblique` is reached by a *rotated* rectangle, an I-beam's tapered flange, or
any section whose edge is neither along nor across the bend axis.

## 3. Assembly: rims are shared, there are no booleans

Following `b2_pipe_sweep.md` §1, which settled this for the pipe and gave the
reasons (a union of two solids meeting on a full disc is the hardest M8
Stage-0 coplanar class; direct assembly has zero tolerance content). A sweep
is ONE solid, assembled directly:

- Each path segment contributes one lateral face per section edge, plus its
  share of the two rims.
- Consecutive segments **share their junction rim loop** — the same vertices
  and the same edges, bit-identical, not two coincident copies.
- The section's own corners split the rim into edges, so a polygon or
  `ArcPolygon` section needs **no seam vertex at all**. Pipe's whole seam-phase
  problem (`b2_pipe_sweep.md` §2: the binormal seam, the phase-general torus
  tessellator) exists only because a full circle rim is a single closed edge
  that must be anchored somewhere. A general section is *topologically
  simpler* than the pipe, not harder.
- Caps: an open path gets one planar face per end (the section itself,
  possibly on a mitre plane, §4). A closed path (§5) gets none.

Euler check for an open path of `k` segments and a section with `e` edges and
`h` holes: `F = k·e + 2·(1+h)` … the census is pinned per increment rather
than asserted in the abstract here; `χ = 2 − 2h` is the invariant the oracle
checks.

## 4. Corners: the bisector mitre

Pipe requires a tangent-continuous chain. A frame, a truss and a bent bracket
are all made of sharp corners, so B6 must treat them, and the treatment is the
standard one:

At a joint where the incoming tangent `t̂₀` and outgoing `t̂₁` differ, the
**mitre plane** is the plane through the joint whose normal bisects them
(`n̂ = normalize(t̂₀ − t̂₁)`, the interior bisector). Both segments are
truncated at that plane, and the truncated section becomes the shared rim
loop — one loop, used by both, so the join is exact by construction and no
boolean, no coplanar Stage 0 and no tolerance is involved.

Loud refusals, all checked before the first arena mutation:

- **`SweepCornerTooTight`** — the mitred rim self-intersects, or the truncation
  of one segment eats past the far end of that segment. Geometrically: the
  section's in-plane extent perpendicular to the bisector exceeds what the
  corner leaves. This is the real limit on a tight corner with a fat section
  and it must be said, not approximated.
- **`SweepCornerReversal`** — `t̂₁ = −t̂₀` (a 180° doubling back); the bisector
  is undefined.
- A G1 joint (`t̂₀ = t̂₁` within tolerance) takes the degenerate mitre, which is
  the plane perpendicular to the common tangent — i.e. exactly what pipe does
  today. **Byte-identical continuity with pipe is an oracle** (§9).

Mitre is the only corner treatment in v1. Butt and coped joints are frame
features (they need the *other* member as an operand) and belong with the
frame feature of §10, not with the sweep primitive.

## 5. Paths

`SweepPath` is the generalization of `PipePath` (`pipe.rs:43`), which is a
plane frame plus a chain of `ProfileEdge`s in plane coordinates. The
generalizations, in increment order:

1. **Sharp corners** — drop pipe's tangent-continuity requirement (§4).
2. **Closed paths** — pipe rejects them (`PipeClosedPathUnsupported`). A closed
   path is a ring: no caps, every joint mitred, and the first rim is the last
   rim. A rectangular picture-frame profile is the archetype and is one
   feature instead of four mitred members.
3. **3D paths** — a chain of lines and arcs not confined to one plane (§6).
   This one is gated on there being a 3D sketch to draw it in (§8).

## 6. The section's frame along the path

The section plane is always perpendicular to the tangent (§1 requires it for
the revolve identity to hold). What is left free is the section's *rotation
within that plane*, and that is the frame law.

- **Planar path.** The path plane's normal `n̂` is constant, so the frame is
  constant: no twist, nothing to decide. The section is used exactly as drawn.
- **3D path.** Adjacent segments' planes differ, and a frame carried by the
  Frenet apparatus twists (and is undefined on a straight segment). The law is
  **rotation-minimizing (parallel transport)**: at each joint, rotate the frame
  by the minimal rotation carrying `t̂₀` to `t̂₁`. Computed once per joint and
  used by *both* adjacent segments, so the shared rim is shared by
  construction.

  The revolve identity survives this: §1 needs the section plane to contain the
  axis, and it says nothing about the section's rotation *within* that plane.
  A parallel-transported section on an arc segment is still a planar region in
  a plane containing that segment's axis, so it still revolves to
  cylinders/cones/tori/planes.

Two options that do **not** survive, and are therefore out of v1:

- **A twist angle applied along a segment** turns a line-segment sweep into a
  helicoidal surface and an arc-segment sweep into something worse. Genuinely a
  new surface. (Twist *at a joint*, i.e. a section rotated by a fixed angle
  for the whole of the next segment, is free — it is just a different constant
  rotation, and the rim then has a kink but stays shared. Worth offering later
  as a per-segment `roll`.)
- **A section that scales along the path** is a new surface for every edge
  kind. Out.

## 7. Explicitly out of scope, with the reason

| Wanted | Why not here |
|---|---|
| Guide rails / two-rail sweep | The section is re-derived per station; the lateral is a general ruled or NURBS surface. Needs a surface class the kernel does not have. |
| Spline path | Same: a swept surface over a non-analytic spine is not in the vocabulary. A spline path chord-approximated into arcs is a legitimate *caller-side* answer and needs nothing from B6. |
| Helical path (threads, springs) | Genuinely new — a helical/screw surface. Highly wanted (it is the honest way to get a thread), and it is its own spec, not a corner of this one. |
| Variable section (scale, morph) | New surfaces per edge. |
| Loft | Unrelated machinery. |

## 8. Where the path comes from

`Sketch` is planar by construction (`plane_origin` / `plane_normal` /
`plane_x_axis`, `waffle-types/src/sketch.rs:50-71`); **there is no 3D sketch in
the tree.** So:

- **v1 paths are one planar sketch**, exactly like `PipeParams` — which already
  buys closed frames, bent brackets, mitred elbows, gaskets and every
  single-plane member run.
- **3D paths wait for a 3D sketch**, which is its own piece of work with its
  own customers (sweep paths, routing, reference geometry, the frame feature
  of §10). §5.3 and §6 are written so that the kernel side does not have to
  change when it lands: a `SweepPath` carrying per-joint frames is already the
  3D-ready shape.

The section comes from an ordinary sketch profile. **The pierce rule** (loud,
v1): the section's sketch plane must be perpendicular to the path's start
tangent, and the path's start point must lie in that plane
(`SweepProfileNotPerpendicular` / `SweepPathDoesNotPierceProfile`). This is
what a user draws anyway, it matches SolidWorks' pierce requirement, and it
keeps the section's *position within the plane* meaningful — which matters:
a member offset from its centreline is the normal case in a frame, and an
auto-centring rule would silently destroy it. An auto-transport mode ("move my
section to the path start for me") is a v2 convenience, never the default.

## 9. Oracles

Per increment, and all exact:

- **Volume, closed form.** A line segment of length `L` (between its two mitre
  planes, measured along the section's centroid) contributes `A·L`; an arc
  segment of sweep `θ` contributes `A·θ·R_c` by Pappus, `R_c` the axis→centroid
  distance — the same second-Guldinus oracle `revolve` is already pinned
  against. The whole solid's volume is the sum, and it is compared against the
  exact-volume oracle over the assembled B-Rep.
- **`χ = 2 − 2h`**, watertight, manifold, `validate_solid` clean.
- **Pipe continuity.** A circular section on a tangent-continuous open planar
  path must produce a solid **bit-identical** to `construct::pipe` on the same
  input. This is the strongest oracle available and it makes the pipe a special
  case of the sweep rather than a parallel implementation. (Whether `pipe` is
  then *deleted* in favour of the sweep is decided at S5, not assumed here —
  its typed errors and its GUI dialog are a shipped contract.)
- **Extrude continuity.** A single-line-segment path must equal the
  corresponding `extrude` bit-for-bit.
- **Boolean re-entry.** Every increment's output goes back through a union and
  a subtract against a box and a cylinder, in the assay. The pipe found four
  latent Stage-4/Stage-1 gaps this way (`b2_pipe_sweep.md` §2.1) and a sweep
  produces strictly more surface pairs than a pipe does.
- **Determinism.** Bit-identical rebuild; the balanced-union/pattern rules.

## 10. What this unlocks, and the frame feature

A frame/structural-member feature (SolidWorks Weldments, Onshape Frame) is
*this* plus a per-edge driver: select N path edges, one section, and get one
member per edge with corner treatments between them. That feature is where
butt/cope joints and corner gap rules live. It is out of B6's scope but B6 is
its whole geometric substrate, so B6 must not assume a single chain: the
`SweepPath` API takes one chain, and the frame feature calls it N times.

Combined with a parametric section entity (the `Gear`/`Sprocket` precedent —
a compact `SketchEntity` expanded by `expand_generators`), the Eiffel tower's
1,200 members become a path sketch plus a section, which is both the honest
model intent and roughly three orders of magnitude less stored geometry.

## 11. Increments

Each lands with its oracle, `cargo clippy --all-targets`, and the roadmap note.
S1–S4 are kernel-only and gated off from the app; nothing user-visible moves
until S6.

| # | What | Gate |
|---|---|---|
| **S1** | `SweepPath` (planar, open, **sharp corners allowed**) + the mitre solver and its refusals, as a pure validated value. No arena mutation. Unit tests on mitre geometry, tight-corner refusal, the G1 degenerate case. | — |
| **S2** | Sweep assembler for a **polygon section, no holes**, `Parallel`/`Perpendicular` edges only — line and arc segments, shared rims, mitred corners, caps. The mitred rectangular elbow. Oracles: Pappus volume, χ, extrude continuity. | S1 |
| **S3** | **`Oblique` section edges on a partial revolve** — the arc-bounded cone patch, KV6c increment 5. Converts a typed wall the general lathe wants anyway. | S2 |
| **S4** | **Holed and `ArcPolygon` sections on a partial revolve** — converts `RevolveProfileHolesUnsupported` and `ArcPolygonProfileUnsupported`. Hollow and rounded sections round a bend; tube sections. | S3 |
| **S5** | **Closed paths** (no caps, every joint mitred) + the pipe-continuity oracle. | S2 |
| **S6** | `Operation::Sweep` + `SweepParams`, `feature-engine` execution, the pierce rule, `sweep` over the bridge and MCP, `ctx.sweep` in scripts. WASM rebuilt in the same commit. | S2 |
| **S7** | Sweep dialog: pick section, pick path, corner treatment, combine. GUI spec. | S6 |
| **(later)** | 3D paths (§5.3, §6) once a 3D sketch exists; per-joint `roll`; the frame feature (§10); helical paths as their own spec. | 3D sketch |

The critical path to something a user can hold is **S1 → S2 → S6 → S7**: a
general polygon section round mitred corners and bends, which is the Eiffel
member, the bracket and the frame rail. S3–S5 widen the section vocabulary
behind it.
