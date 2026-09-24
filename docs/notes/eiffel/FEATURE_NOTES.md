# Waffle Iron feature notes — from building the Eiffel Tower example

Observations gathered while modelling the Eiffel Tower (2026-09-24,
`app/static/examples/eiffel-tower.py`, 1,052 agent-tool calls, 1,964 bodies).
Entries marked FIXED were found here and fixed in the same few days; the rest
is what the model *wanted* to say and what it had to say instead. None of it is
a kernel bug — the kernel answered correctly and loudly every time. These are
capability, performance and ergonomics gaps.

As of 2026-09-24 the open ones are §2 (a beam/member operation), §5
(`model_summary` has no body count), and the perf tails noted in §0 and §10.

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
| autosave | 80.3 ms | ~25 ms (§0a) |
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

- **Autosave.** §4.8 makes writing every call deliberate: "the document on disk
  is never more than one committed tool behind." That stands; what was a defect
  was the cost of its self-check, and §0a fixes it.
- **The authoring snapshot serializes the whole tree, twice per call**, because
  the delta is derived by diffing two JSON trees. The engine already knows
  exactly which features re-executed (`rebuild.rs` `reran`) and drops it on the
  floor; a delta built from that would be O(changed).

## 0a. The save-side self-check costs 12× the save it checks — FIXED

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

**Both were done.**

`load_document` was parsing the file into a `serde_json::Value` and then
re-deserializing the typed model out of that tree with `from_value`. The raw
parse costs 4.5 ms; `from_value` on top cost another 22. It now parses straight
from the text for the current shape, deciding which shape it is with a probe
pass that tokenizes but allocates nothing (1.5 ms). 32 ms → 22 ms, and that is
every document open as well as every verify.

`SaveVerifier` then makes the check proportional to the change. A document is
its tabs plus a small envelope, so it verifies each the way the loader would
and remembers the tab payloads it has already accepted: a tab whose bytes are
what was verified before cannot have become unparseable. Steady state 25.6 ms →
**10.1 ms**; a save that changed the tower's largest tab, 22 ms.

One trap worth recording: the obvious implementation does not work. Hashing
`serde_json::to_string(tab)` never matches, because the tree holds HashMaps
(`body_names`, `provenance`, a sketch's `solved_positions`) and the same tab
serializes differently every time. The hash is taken over
`serde_json::to_value(tab)` instead, whose maps are BTreeMaps and so come out
sorted. (That non-determinism is also why `corpus_backcompat` has to compare
saves structurally rather than byte-for-byte.)

End to end, with the earlier work: the tower's 1,052-call build went 84 s → 30 s
→ **24 s**.

## 10. The viewport drew one call per FACE — 2 fps on the tower — FIXED

Rotating the Eiffel Tower ran at 2–3 fps. The renderer's own counters said why:

| | before | after |
|---|---:|---:|
| draw calls per frame | 25,447 | 3,947 |
| triangles per frame | 26,436 | 26,436 |
| distinct materials in the scene | 25,447 | 18 |
| idle | 2.3 fps | 20.0 fps |
| orbit | 2.2 fps | 15.0 fps |
| hover | 0.9 fps | 3.3 fps |

**26,436 triangles in 25,447 draw calls** — about one triangle per call.

Face highlighting needs a material per face range, and a material ARRAY makes
three.js draw one call per geometry group. Every body got an array whether or
not anything on it was lit, so a 12-triangle box cost six draw calls for its
faces and twelve more for its edges. `buildEdgeMaterials` went further and
allocated a brand-new `LineBasicMaterial` per edge of every body — hence 25,447
materials, each its own program bind and uniform refresh, every frame.

Three changes, no behavioural difference:

1. **Collapse a uniform array to one material.** If no face of a body is
   hovered, selected or feature-lit, it draws in a single call. The per-face
   split still happens, but only for the body that has a lit face.
2. **Share the plain material** across every body in the scene, cached across
   rebuilds so the array identity is stable — a hover then leaves every other
   body's material prop untouched and Svelte updates one mesh, not two thousand.
3. **Skip bodies that cannot be lit.** A `GeomRef` names the feature it points
   into, so a body whose feature is not the hovered or selected one takes the
   shared array without walking its face ranges. That walk was comparing
   GeomRefs by canonical JSON — tens of thousands of `JSON.stringify` calls per
   pointer move.

Measured in headless Chromium on software GL, so the absolute numbers are
pessimistic against real hardware; the ratios are the point.

`app/tests/gui/render-cost.spec.js` pins it (38 draw calls for one box before,
inside the bound after), and `window.__waffle.getRenderStats()` reports the
counters.

**Still open.** ~3,900 objects is still one mesh and one `LineSegments` per
body, which is the floor for this architecture; merging bodies into a few
batched geometries (with a triangle-range → body table for picking, which
`face_ranges` already is) would take it to a handful. And hover still raycasts
every body on every pointer move — 3.3 fps — which wants a spatial index or a
GPU picking pass.

## 1. A lathe profile may only be a 3-gon or a 4-gon — FIXED 2026-09-24

`kernel_v2::construct::revolve::on_axis` takes an on-axis profile only as an
apex cone (3-gon) or a solid frustum (4-gon); anything else comes back as
`RevolveAxisIntersectsProfile`. So the campanile's dome — one quarter-ellipse
polyline touching the axis at both ends, the single most natural lathe shape
there is — cannot be revolved as authored. It ships as **six stacked frusta**
(`Dome band 0..5`), which is six bodies and six sketches where one would do,
and which leaves visible steps in the silhouette.

**FIXED.** `build_on_axis_lathe` takes a full-turn revolve of ANY simple
polygon with exactly one on-axis edge. The profile is a chain from one
on-axis vertex through the off-axis vertices to the other, and each chain
edge sweeps exactly one face: an END edge (one endpoint on the axis) a disc
or an apex cone, bounded by the single rim circle of its off-axis endpoint; a
MIDDLE edge a cylinder or a cone frustum, bounded by the
`[rim, seam, rim, seam]` single-fake-edge loop the frustum builder already
used. Census V = p, E = 2p − 1, F = p + 1 for p off-axis vertices, so χ = 2 at
every band count — and at p = 2 and p = 1 it is exactly the frustum's and the
apex cone's own census.

The 3- and 4-gon builders keep the shapes they already build, bit for bit
(the corpus depends on the cylinder's extrude-of-circle delegation being
bit-canonical). The general builder only takes what they turned down, so this
is a widening: the dome, the stepped shaft, the cup with a blind bore, the
bicone, the "pencil".

One shape needed its own loop form: a MIDDLE band that is
axis-perpendicular sweeps a planar ANNULUS, and a planar loop may not mix a
full circle with other edges (`validate`). So a washer band takes the form
the full-revolve caps already use — the outer rim in the outer loop, the
inner rim in a `LoopKind::Inner` ring, and no seam ruling between them. It
trades its seam for a ring, so the census moves on both sides at once:
V − E + F − R = 2 still.

**The whole thing turns on one sign.** Everything orientational — each face's
surface normal, a cylinder's or cone's `reversed` cavity flag, and every rim's
traversal axis — is derived from the sign of the profile's area in the
`(radius, axial)` half-plane. That is what lets a blind bore's wall face the
axis while the outer wall faces away, with no special case for either. Two
faces sharing a rim derive OPPOSITE traversals from it, which is what makes
them twins; `finalize_solid` checks that rather than the builder assuming it.
`revolve_general_lathe.rs` pins each shape against its Pappus volume.

Still refused, loudly: a PARTIAL sweep of a many-band profile (the wedge
vocabulary — two pie-sector caps plus a swept face per edge — is a different
construction, and no case asks for it yet).

The shipped tower still carries its six stacked frusta: re-authoring the
campanile's dome as one revolve changes the example's body count (which
`examples.spec.js` pins) and re-records a 3.5 MB file, so it is its own
change, not a side effect of the capability.

Still open, and the other half of this item: a lathe profile cannot contain
`Arc` entities, so even a true circular dome has to be pre-faceted by the
caller — the dome above is a polyline, not an arc. Honouring arcs in a
revolve profile would give analytic spheres and tori from the profile the
user actually drew, and is a separate piece of work (the profile ingestion,
not the assembler).

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

## 3. Sketch planes make the caller do the basis arithmetic — FIXED 2026-09-24

`sketch_create` takes `{origin, normal}` and picks its own in-plane u/v basis
(`basis()` in the generator mirrors the engine's choice: a fixed reference
vector crossed with the normal). That is fine for a circle, but for anything
oriented — a rectangular member, a keyway, a slot — the caller must reproduce
the engine's basis exactly, or project 3D points through it, to know which way
"up" is in the sketch. Every generator in this repo carries the same `basis()`
and `uv()` pair.

**FIXED.** Two halves, because either alone leaves the caller guessing:

1. **A sketch can carry its own x axis.** `Sketch.plane_x_axis` (optional,
   absent in every document written before this) is the world direction the
   sketch's +u points along; it is orthogonalized against the normal, so a
   caller may hand over any vector with an in-plane part — an edge direction,
   a world axis — without projecting it first. `sketch_create` takes it on
   EITHER plane form (a bare `{origin, normal}` or a face/datum `GeomRef`),
   and the engine builds the sketch on it: the profile faces, the pipe path
   and the share-a-face scan all go through one `sketch_x_axis`, so there is
   one answer to "which way is up in this sketch".
2. **`sketch_create` answers with the basis it used** — `plane: {origin,
   normal, x_axis, y_axis}`. Even a caller that does not care which basis it
   gets no longer has to reproduce the derivation to find out; it reads it
   back. That is what the generators in this repo were carrying `basis()` and
   `uv()` for.

An axis that cannot orient the plane — zero-length, non-finite, or parallel
to the normal — is refused by the tool and, if one reaches the tree anyway,
fails the SKETCH feature loudly. It is never a silent fallback to the derived
basis, which would move every point in the sketch.

The UI honours it everywhere it draws: `buildSketchPlane` takes the axis, the
sketch-mode state carries it while editing, and the inactive-sketch renderer
reads each sketch's own. The UI does not yet author one (nothing in the
toolbar asks "which way is up"), so every sketch a person draws still takes
the derived basis, bit for bit.

## 4. No mirror — FIXED 2026-09-24

The tower has four-fold symmetry about the vertical axis, which
`PatternCircular` handles beautifully — that single feature is what turns ~170
authored members into ~680 and keeps this example buildable. But each *leg*
also has mirror symmetry about its own diagonal plane, and there is no
`PatternMirror`, so the leg's 164 members are all authored explicitly when 90
would do.

**FIXED.** `Operation::PatternMirror { seeds, plane, combine, targets }`, on
exactly the machinery the other two share: same custody of its seeds, same
instance-major outputs (instance 0 the seed, instance 1 its reflection), same
combine against explicit targets.

The caveat was the whole job. A reflection is improper, so it is not a
`RigidPlacement` and the kernel refuses one there — rightly; it would turn
every face of the copy inside out. It gets its own entry point instead:
`MirrorPlane` + `Kernel::mirror_body`, implemented by `kernel_v2::mirror_solid`
as the SAME deep copy `transform_solid` does, over a shared affine map, plus
one thing — every loop of the copy is traversed the other way round
(`next` ↔ `prev`, and each half-edge starts where it used to end). The twin
pairing is untouched: both half-edges of an edge reverse together, so they
still traverse it oppositely. Nothing else changes: the geometry maps by the
reflection matrix exactly as it does by a rotation (a mirrored cylinder is a
cylinder of the same radius), and the cavity flags survive (a hole stays a
hole).

The oracle for "did the orientation bookkeeping work" is the SIGN of the
volume: a copy turned inside out satisfies every per-face invariant and
integrates to −V. `transform_mirror_copy.rs` (kernel) and `pattern_kv2.rs`
(feature) both take it that way, and the mirrored body is exercised as a
boolean operand, which is where an inside-out solid would really bite.

`OpTag::Mirror` is distinct from `OpTag::Transform` in the lineage journal —
a face that has passed through a reflection has changed handedness, and a
reader asking "which face is this" deserves to be told.

## 5. `model_summary` has no body count

`model_summary` came back without a `body_count` field, so the generator has no
cheap way to assert "this build produced the 1,204 bodies it should have". The
build's own correctness oracle ended up being `assembly_get().errors == []`,
which says nothing about how much geometry arrived. A silent half-build would
have passed.

**Suggestion.** Put a body/solid count (per tab, and for the active assembly)
in `model_summary`. A generator that can assert a count can be a regression
test; one that cannot is a script.

## 6. Documents are stored pretty-printed — the case, and what was done

Measured on the shipped examples (2026-09-24):

| | on disk (pretty) | compact | gzip(compact) | **as shipped** (gzip of the pretty file) |
|---|---:|---:|---:|---:|
| `eiffel-tower.waffle` | 3,490,341 | 1,226,695 (35%) | 150,117 (4.3%) | **177,031 (5.1%)** |
| `gravel-bike-v2.waffle` | 815,956 | 298,772 (37%) | 53,855 (6.6%) | **68,071 (8.3%)** |

The shipped column is what landed: gzip of the writer's own bytes, so nothing
about the format or the writer had to change. Compacting first would save
another ~15%, which is not worth a second way to write a document.

So **~65% of a `.waffle` is indentation and the newlines around it** — the
tower is 2.3 MB of whitespace. There are 318 tracked `.waffle` files in the
repo, 11.2 MB of working tree between them.

### Where it comes from

One line: `save_document` (`crates/file-format/src/save.rs`) ends in
`serde_json::to_string_pretty`. It is "the writer" — every production save
path composes its bytes there (v4 §4 invariant 7) — so this is a one-line
change with a very wide blast radius, which is exactly why it deserves a
decision rather than a patch.

### What it actually costs, and where it does not

- **The repository: almost nothing.** Git zlib-compresses every blob, and
  whitespace is the most compressible thing in the file: those 318 files take
  **0.8 MB** in the pack. Roughly what compact-and-then-zlib would take. The
  11.2 MB is checkout size, not repository size.
- **The app bundle and the wire: full price.** `app/static/examples/` is
  copied verbatim into the build, and the dev server hands the tower over as
  `Content-Length: 3490341` with no `content-encoding` — `.waffle` has no
  registered media type, so it does not even get a `Content-Type`, let alone
  gzip. Opening the tower example downloads 3.5 MB where 150 KB would do.
  That is the real bill, and it is paid by every viewer of the examples.
- **Load time: a little.** The parse walks every byte, so ~65% of the bytes
  it walks are whitespace. `load_document` is 22 ms for the tower (§0a), so
  this is worth single-digit milliseconds, not the headline.

### What it buys, which is not nothing

A pretty-printed document is **diffable**. The assay corpus, the file-format
fixtures and the examples all live in git, and a reviewer reading "what did
this fixture change do" gets one line per field instead of one line per file.
Every one of those 318 files would become a single unreadable line. That is
the argument against, and for a repo whose corpus IS its test suite it is a
serious one.

### What was done (2026-09-24)

The two audiences want opposite things, so they were separated rather than
traded off:

1. **The examples ship compressed.** `gravel-bike-v2.waffle.gz` (68 KB from
   816 KB) and `eiffel-tower.waffle.gz` (177 KB from 3.49 MB) — 4.2 MB out of
   the working tree and out of the bundle, with no change to the format, the
   writer, or a single committed fixture. `fetchExampleDocument` inflates with
   `DecompressionStream('gzip')`, sniffing the GZIP MAGIC rather than the file
   extension: Vite's dev server labels a `.gz` with `Content-Encoding: gzip`
   and the browser has already inflated it by the time we look, while a static
   host hands over the raw bytes, and one code path has to be right on both.
   The generators write gzip when the output path ends in `.gz`, and the
   dev-only "save as example" endpoint does the same — deterministically (no
   embedded filename, `mtime 0`), so rebuilding the same document twice gives
   the same bytes.
2. **The writer stays pretty** for everything that lands in git as source.
   The assay corpus and the fixtures keep their line-per-field diffs.
3. The corpus pins were taught to inflate rather than allowed to lose the
   examples: `corpus_backcompat` and the schema golden walk `*.waffle.gz` too.
   That is the failure this change had to avoid — a compressed file silently
   dropping out of the walk that exists to catch exactly this class of
   regression.

Not done, deliberately: a compact writer. If one is ever wanted it belongs
behind an explicit argument on `save_document`, never a default and never a
global setting — two writers reachable by ambient state is how a corpus ends
up half in each format.

## 7. Patterning a whole tab — FIXED 2026-09-24

Each part tab here ends with the same gesture: "take everything I just built
and turn it four times." Expressing that needs an explicit list of every seed
(`quarter_turns()` in the generator collects them), and a seed list of 164
`GeomRef`s is most of that feature's JSON.

**FIXED.** A pattern's `seeds` now takes `{"type": "All"}` — every live solid
at that point in the tree — and it is literally the same walk `UnionAll` uses
(`union_all::live_features_before`, now shared). `consumed_feature_ids` asks
the resolver rather than reading feature ids off the references, because with
`All` there are no references to read.

The array form is untouched and is still what a list of picked bodies writes,
so every existing document round-trips byte for byte: `PatternSeeds`
deserializes from either a JSON array or the tagged object, and serializes
back as whichever it is. Anything else is a parse error naming what it saw,
not a silently empty pattern.

## 8. `viewport_view` names assume Y-up, but the scene is Z-up

`standardViews` in `app/src/lib/viewport/CameraControls.svelte:155` is the
stock three.js table — `top: {pos: [0,1,0], up: [0,0,-1]}`, `iso: {pos:
[1,1,1], up: [0,1,0]}` — while models are Z-up. So on this tower
`viewport_view: "front"` gives a **plan** view, `"top"` gives an upside-down
elevation, and `"iso"` lays the tower on its side and runs it off the edge of
the frame. Every one of them is a legal camera; none is the view the name
promises. Any agent asking for a standard view of a tall Z-up model gets a
picture it cannot use, with nothing to say it went wrong.

**FIXED 2026-09-24.** The table is now model space, up = +Z:

| | pos | up |
|---|---|---|
| front | (0, −1, 0) | +Z |
| back | (0, 1, 0) | +Z |
| top | (0, 0, 1) | +Y |
| bottom | (0, 0, −1) | −Y |
| left / right | (∓1, 0, 0) | +Z |
| iso | (1, −1, 1) | +Z |

The View Cube had to move with it, because the cube is drawn in WORLD axes
(its transform is the inverted camera quaternion), so each label has to sit
on the world face that view looks at: `top` is now the +Z face, `front` the
−Y face, and four of the six carry a `rotateZ` roll so the text reads upright
from its own view. Verified face by face from a screenshot of each view —
the sign of a CSS `rotateX`/`rotateZ` in this cube is not what the obvious
derivation says, so guess and look rather than reason.

**Still crooked, and NOT fixed here:** the built-in datum planes keep
SolidWorks' Y-up names — "Front" is the XY plane (normal +Z), "Top" is XZ.
That was coherent with the old camera table and is now the odd one out: the
plane named "Front" is the one the `top` view looks at. Renaming them (Front
→ Top, Top → Front) is display-only (the ids are stable UUIDs) but it changes
what every existing document's UI says about where its sketches live, so it
is its own decision.

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

**FIXED 2026-09-24.** `viewport_view` takes a `frame`: either `body_ids` (the
union of those bodies' boxes) or `point` + `radius` (a cube of half-size
`radius` about a world point). It goes through the same `fitToBox` Fit All
uses, so the ortho frustum and the clipping planes come out right, and the
answer echoes the box as `framed`. A `body_ids` entry the view does not have
is `BodyNotFound` with the camera **unmoved** — the frame is resolved before
anything is snapped, so a refusal never leaves the camera half-moved. Model
meshes now carry their `bodyId` in `userData`, which is what makes framing by
body possible at all.

## 9a. Opening a document left the camera where it was — FIXED

An open replaces everything the camera was pointed at, but only the *example*
browser asked for a fit; `document_open`, the `/doc/[id]` handoff and the
startup restore all left the camera on the default framing of the 200 mm datum
planes. A 330 m tower was then a speck, and a 3 mm screw invisible, until the
user pressed F. The fit now belongs to `openDocumentRecord` (so every open path
gets it), still deferred to the first model update that actually carries
geometry — an assembly evaluates after the file lands, so the geometry can be
several rebuilds away. Pinned by `agent-documents.spec.js` ("opening a document
frames its model").
