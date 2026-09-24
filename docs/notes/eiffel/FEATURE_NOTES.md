# Waffle Iron feature notes — from building the Eiffel Tower example

Observations gathered while modelling the Eiffel Tower (2026-09-24,
`app/static/examples/eiffel-tower.py`, 674 agent-tool calls, ~1,200 bodies).
Each entry is what the model *wanted* to say and what it had to say instead.
Nothing here is a bug report — the kernel answered correctly and loudly every
time. These are capability and ergonomics gaps.

## 0. Building a tab is O(N²) — the biggest single finding

Every `sketch_create` and every `feature_add` costs time proportional to how
many features the tab ALREADY holds, so building a tab of N features costs
O(N²). Measured against `waffle-host` (release, one tab, identical 4-point
sketch + extrude repeated 400 times):

| features already in the tab | `sketch_create` | `feature_add` |
|---:|---:|---:|
| 0 | 4.6 ms | 4.7 ms |
| 200 | 25.1 ms | 25.4 ms |
| 400 | 51.8 ms | 52.4 ms |
| 600 | 88.0 ms | 88.0 ms |
| 700 | 108.3 ms | 108.3 ms |

800 calls took 40 s in total, and the **last fifty pairs alone took 10.8 s** —
a quarter of the run for the last 12% of the work. The tower's 1,052 calls take
83 s; a linear-cost engine would finish the same work in about 10 s.

The tell is that `sketch_create` and `feature_add` cost *exactly the same* at
every size. The cost is therefore not the geometry — a 4-point sketch and a box
extrude are not remotely comparable amounts of kernel work — it is a per-call
pass over the whole tab: a full rebuild, a full snapshot, or a full
serialization, on every call.

**Suggestion.** Find that per-call whole-tab pass and make it incremental. A
feature appended at the tip of the tree invalidates nothing before it, so
neither a rebuild nor a snapshot needs to walk the prefix. This is the
difference between "an agent can build a thousand-feature model" and "an agent
should keep its models small", and it will bite every generated document, every
imported STEP assembly and every script that emits geometry in a loop.

Worth checking whether the same quadratic is in the browser path: the app takes
7 s to open this document, which is a single load rather than 1,052 calls, but
the GUI's own per-edit cost would show the same curve.

## 1. A lathe profile may only be a 3-gon or a 4-gon

`kernel_v2::construct::revolve::on_axis` takes an on-axis profile only as an
apex cone (3-gon) or a solid frustum (4-gon); anything else comes back as
`RevolveAxisIntersectsProfile`. So the campanile's dome — one quarter-ellipse
polyline touching the axis at both ends, the single most natural lathe shape
there is — cannot be revolved as authored. It ships as **six stacked frusta**
(`Dome band 0..5`), which is six bodies and six sketches where one would do,
and which leaves visible steps in the silhouette.

**Suggestion.** Widen the on-axis arm to the general case: an N-gon with
exactly one on-axis edge is a fan of frusta/cones about a shared axis, and the
topology is the same cap + lateral-strip census the 4-gon arm already builds,
repeated. The gates the arm already applies (one on-axis edge, no holes, no
crossing) are the same ones. This is the difference between "revolve is for
cylinders and cones" and "revolve is a lathe".

Related: a lathe profile cannot contain `Arc` entities, so even a true circular
dome has to be pre-faceted by the caller. Honouring arcs in a revolve profile
would give analytic spheres and tori from the profile the user actually drew.

## 2. Every structural member costs two calls and its own sketch

A straight bar of rectangular section between two 3D points is the atom of any
lattice, truss, frame or space structure. Today it is `sketch_create` (four
points + four lines, projected into a plane the caller has to construct) plus
`feature_add`/Extrude. 1,200 members in the tower are ~660 calls and **794 KB
of stored sketch geometry** — almost all of it four points and four lines
restating a rectangle.

**Suggestion.** A `Beam`/`Member` operation taking `(p0, p1, profile, orient)`,
or more generally an extrude that accepts a *parametric* section (rectangle,
angle, channel, I) instead of a sketch. It collapses the two calls to one, it
removes the caller's plane-basis arithmetic (see §3), and it shrinks the
document by roughly the same factor. Structural and furniture models are the
obvious customers, but so is every enclosure, bracket and chassis.

## 3. Sketch planes make the caller do the basis arithmetic

`sketch_create` takes `{origin, normal}` and picks its own in-plane u/v basis
(`basis()` in the generator mirrors the engine's choice: a fixed reference
vector crossed with the normal). That is fine for a circle, but for anything
oriented — a rectangular member, a keyway, a slot — the caller must reproduce
the engine's basis exactly, or project 3D points through it, to know which way
"up" is in the sketch. Every generator in this repo carries the same `basis()`
and `uv()` pair.

**Suggestion.** Let `sketch_create` take an optional `x_axis` (as
`MateConnector` frames already do). The caller says which way is up once; the
engine stops being asked to guess and the callers stop reimplementing it.

## 4. No mirror

The tower has four-fold symmetry about the vertical axis, which
`PatternCircular` handles beautifully — that single feature is what turns ~170
authored members into ~680 and keeps this example buildable. But each *leg*
also has mirror symmetry about its own diagonal plane, and there is no
`PatternMirror`, so the leg's 164 members are all authored explicitly when 90
would do.

**Suggestion.** `PatternMirror { seeds, plane }`. It is the same
seed-consuming, transform-and-re-emit machinery `PatternLinear` and
`PatternCircular` already share (`feature-engine/src/pattern.rs`), with a
reflection instead of a rigid motion — with the caveat that a reflection flips
orientation, so the emitted shells need their sense reversed.

## 5. `model_summary` has no body count

`model_summary` came back without a `body_count` field, so the generator has no
cheap way to assert "this build produced the 1,204 bodies it should have". The
build's own correctness oracle ended up being `assembly_get().errors == []`,
which says nothing about how much geometry arrived. A silent half-build would
have passed.

**Suggestion.** Put a body/solid count (per tab, and for the active assembly)
in `model_summary`. A generator that can assert a count can be a regression
test; one that cannot is a script.

## 6. Documents are stored pretty-printed

The tower's features are 794 KB of JSON but the `.waffle` on disk is 2.2 MB —
indentation is ~60% of the shipped file. For a format that is mostly
machine-written and machine-read, and that we ship inside the app bundle, a
compact save (or gzip) would be free.

**Suggestion.** Save compact by default with a `--pretty` affordance for
debugging, or gzip the payload. Either roughly halves every example, fixture
and assay case in the repo.

## 7. Patterning a whole tab

Each part tab here ends with the same gesture: "take everything I just built
and turn it four times." Expressing that needs an explicit list of every seed
(`quarter_turns()` in the generator collects them), and a seed list of 164
`GeomRef`s is most of that feature's JSON.

**Suggestion.** Let a pattern's `seeds` accept "every solid alive at this point
in the tree" as a selector, the way `UnionAll` already takes the live body set
(landed 2026-09-23, B4). Same idea, same live-body query, applied to patterns.

## 8. `viewport_view` names assume Y-up, but the scene is Z-up

`standardViews` in `app/src/lib/viewport/CameraControls.svelte:155` is the
stock three.js table — `top: {pos: [0,1,0], up: [0,0,-1]}`, `iso: {pos:
[1,1,1], up: [0,1,0]}` — while models are Z-up. So on this tower
`viewport_view: "front"` gives a **plan** view, `"top"` gives an upside-down
elevation, and `"iso"` lays the tower on its side and runs it off the edge of
the frame. Every one of them is a legal camera; none is the view the name
promises. Any agent asking for a standard view of a tall Z-up model gets a
picture it cannot use, with nothing to say it went wrong.

**Suggestion.** Define the table in model space (up = +Z) so `front` is an
elevation and `iso` is the three-quarter view everyone means. This is a
one-table change and it is visible in every agent screenshot.

## 9. No way to frame a region from the agent side

`viewport_view` offers a named view and Fit All, which fits the *whole* model.
Framing the arch under the first platform — 15% of a 330 m model — meant
dispatching `waffle-restore-camera` with a hand-computed position, up, target
and, because the camera is orthographic, a `frustumTop`; Fit All had to be
suppressed or it would zoom straight back out. An agent reviewing its own work
inspects details far more often than it looks at the whole model.

**Suggestion.** Give `viewport_view` a `target` (a point, a body, or a named
connector) and a `radius`/`fit_to` — "frame this body", "frame 40 m about this
point". The camera code already has `fitToBox`; it only needs a box that is not
always the whole scene.
