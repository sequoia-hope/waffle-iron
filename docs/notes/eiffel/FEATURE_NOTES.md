# Waffle Iron feature notes — from building the Eiffel Tower example

Observations gathered while modelling the Eiffel Tower (2026-09-24,
`app/static/examples/eiffel-tower.py`, 674 agent-tool calls, ~1,200 bodies).
Each entry is what the model *wanted* to say and what it had to say instead.
Nothing here is a bug report — the kernel answered correctly and loudly every
time. These are capability and ergonomics gaps.

## 0. Building a tab was O(N²) — FIXED 2026-09-24

Every `sketch_create` and every `feature_add` cost time proportional to how
many features the tab ALREADY held. Measured against `waffle-host` (release,
one tab, identical 4-point sketch + extrude repeated 400 times):

| features already in the tab | `sketch_create` | `feature_add` |
|---:|---:|---:|
| 0 | 4.6 ms | 4.7 ms |
| 200 | 25.1 ms | 25.4 ms |
| 400 | 51.8 ms | 52.4 ms |
| 600 | 88.0 ms | 88.0 ms |
| 700 | 108.3 ms | 108.3 ms |

The tell was that `sketch_create` and `feature_add` cost *exactly the same* at
every size — so the cost was never the geometry.

### Where it actually was

The first guess (and the obvious one) was the rebuild. It was wrong, and worth
recording as a method note: **the geometry rebuild is 1% of an authoring call.**
`feature-engine` is already incremental (`rebuild::Changed::Features` +
`from_index`), and at 300 features `Engine::rebuild` costs 0.9 ms of an 83 ms
call. Profiling the host's three top-level phases instead gave, per
sketch+extrude pair at 600 features:

| phase | before | after |
|---|---:|---:|
| viewer snapshot | 228.8 ms | 0 ms (unwatched) / ~90 ms (watched) |
| engine tool | 148.8 ms | 50.7 ms |
| autosave | 80.3 ms | 86.0 ms (untouched) |
| *(engine rebuild, inside the tool)* | *4.1 ms* | *4.1 ms* |
| **total** | **474.5 ms** | **140.9 ms** |

Three separate defects, none of them in the kernel:

1. **The render-view body accessors each re-derived the flat body list.**
   `body_mesh`, `body_edges`, `body_face_entries`, … all began with
   `collect_renderable_bodies(state).into_iter().nth(i)`. The viewer snapshot
   called six of them per body, so encoding B bodies walked a B-long list 6B
   times. Fixed by adding `_at(&BodyAddr)` forms and collecting once
   (`render_view.rs`, `viewer.rs`, `tools::rendered_bodies`). `body_metadata`
   also scanned the feature array per body for a display name, when `BodyAddr`
   already carries the feature's index.
2. **`model_delta` was genuinely O(N²).** Membership tested with
   `Vec::contains` inside loops over the ids, and `features_changed` compared
   `feature_record(id)` — a scan of the serialized feature array plus two
   `Value` clones — once per common id, twice. Now a `HashSet` and a borrowed
   index.
3. **The host pushed a viewer snapshot after every committed tool even with no
   viewer anywhere.** A snapshot is a pass over every body. The host now pushes
   only once something downstream has asked for a snapshot or a blob
   (`Host::viewers_watching`), which is safe because the relay holds no
   snapshot until it asks for one and asks the moment a viewer attaches.

Net: the Eiffel Tower's 1,052-call build went from **84 s to 30 s**, producing
a structurally and numerically identical document, and the per-call cost is now
linear in the tab rather than super-linear.

### What is still O(document) per call

Linear per call is still quadratic over a session, and two passes remain:

- **Autosave (now ~61% of a call).** §4.8 makes it deliberate: "the document on
  disk is never more than one committed tool behind." Untouched — it is a
  durability contract, not a defect. But see §0a, which is a defect.
- **The authoring snapshot serializes the whole tree, twice per call**, because
  the delta is derived by diffing two JSON trees. The engine already knows
  exactly which features re-executed (`rebuild.rs` `reran`) and drops it on the
  floor; a delta built from that would be O(changed).

## 0a. The save-side self-check costs 12× the save it checks

Of the 79.9 ms an autosave takes at 600 features, **69.6 ms is
`save_document_verified` re-parsing the document it just serialized**; the
serialization itself is 5.7 ms and the write 1.8 ms. That check runs on every
mutating tool call, in the host and in the browser alike, and it re-verifies the
entire document — including the features that did not change and that the
previous call already verified.

The check earns its place: it catches non-finite floats, which serde writes as
`null` and every reader then rejects, and it does so without enumerating float
fields (`save.rs`). But paying a full parse of the whole document per keystroke-
equivalent is not the only way to have it. Options, cheapest first:

- Verify the ACTIVE TAB's serialization only. Every other tab's bytes are
  identical to the last save, which was verified when that tab last changed, so
  the invariant still holds for every byte.
- Verify on the saves that hand a file to someone (`document_save`, export) and
  on the first autosave after a crash-relevant change, rather than on all of
  them.
- Keep it per call, but make the autosave itself incremental.

This is the single biggest remaining cost of authoring, and it is a decision
about a correctness check, so it is left as a recommendation rather than taken
unilaterally.

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

## 8a. Orbit turned about a point zoom had dragged off the model — FIXED

Reported from the tower on mobile: "it rotates about some point outside that
body." It did. `controls.target` is both what the camera looks at AND what an
orbit turned about, and `zoomTowardScreenPoint` moves the target toward the
cursor on every wheel and every pinch — including when the ray hits nothing,
where it fabricated a hit on a plane through the current target and lerped to
it anyway. On a model that is mostly air, most pinches land on background.

Measured on the tower: eight wheel-zooms over empty sky moved the target from
(0, 0, 163.8) to **(36.5, 76.6, 50.7)** — 76 m outside the structure. Every
orbit after that swung the tower about that point. Worse on touch, where a
two-finger gesture pans, zooms and twists at once, so the pivot drifts
constantly.

Fixed by separating the two roles. `controls.orbitPivot` is a new point the
rotation turns about; the camera AND the look-at target both rotate rigidly
about it, so the camera keeps looking at the target and nothing jumps when the
pivot is set. It is re-anchored at the start of every rotate to the model point
under the cursor — probed as a small rosette, because a single ray down the
middle of a lattice usually passes between two members, and restricted to
`waffleType === 'model'` so a datum plane cannot become the pivot — falling
back to the visible model's bounding-box centre on a miss. With no pivot set
the behaviour is bit-identical to before. Pinned by
`app/tests/gui/orbit-pivot.spec.js`.

**The root cause is fixed too.** `zoomTowardScreenPoint` now pans the target
only when the cursor is on something: the perspective path no longer invents a
hit on a plane through the target when the ray misses (it falls through to the
dolly that was already there), and the ortho path raycasts before panning
instead of always using that plane. Sketch mode is unchanged — there the sketch
plane IS the surface you are pointing at.

Measured on the tower, one wheel zoom at a background corner: **45.8 m of
target drift before, 0.000 m after**, with the frustum changing identically
(249.4 → 217.1) in both. Pinned by `app/tests/gui/zoom-anchor.spec.js`, which
fails by 9 m without the change.

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
