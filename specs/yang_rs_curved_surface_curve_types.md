# PR-YR6 — yang-rs curved `Surface` + `Curve` variants (types only; pipeline rejects loudly)

## Context

`yang-rs` has a functional **planar-convex** boolean (Stages 1, 2, 5, 6). The
strategic next direction (roadmap M5 / Phase 2) is wiring `ssi-rs` analytical
SSI curves into the pipeline so analytical surfaces survive to the output
B-Rep. That work is gated on curved-geometry support.

This PR is the **first, smallest, lowest-risk step** of that path: extend the
`Surface` and `Curve` enums with the curved variants a first curved boolean
needs (target demo cases `sphere − cylinder`, `cylinder ∪ box`), make the
pipeline **accept them at the type level** while **rejecting them LOUDLY** at
the stages that do not yet process curved geometry — explicit `Err`, never a
panic, never a silent skip, never a planar approximation (governance A15.2,
P9/P10).

**Out of scope (explicitly NOT this PR):** curved tessellation, curved face
resolution, any `ssi-rs` call, any yang→ssi `Surface`→`QuadricSurface` mapping.
Those are later PRs (P2 = curved Stage-1 tessellation; P3 = Stage-3 SSI wiring).
This is a pure type extension + loud rejection.

## Design decisions

### Field shapes mirror `crates/ssi-rs/src/lib.rs` `QuadricSurface` / `SsiCurve`
So a future Stage-3 yang→ssi mapping is a trivial field-for-field copy.

- **`Surface`** (add to enum):
  - `Sphere { center: Point3, radius: f64 }`
  - `Cylinder { axis_point: Point3, axis_dir: Vector3, radius: f64 }`
  - `Cone { apex: Point3, axis_dir: Vector3, half_angle: f64 }`
- **`Curve`** (add to enum):
  - `Circle { center: Point3, normal: Vector3, radius: f64 }`
  - `Ellipse { center: Point3, normal: Vector3, major_axis: Vector3, major_radius: f64, minor_radius: f64 }`
  - `LineSegment` stays. `Parabola`/`Hyperbola` are **not** needed for the demo
    pairs (plane∩sphere → circle, plane∩cylinder → circle/ellipse,
    sphere∩cylinder → circle/ellipse) — skip them; banked for a later PR.

Both enums keep deriving `Copy, Clone, Debug, PartialEq` (all new fields are
`Copy`).

### Outward-side convention: NO `sense` field (mirror ssi-rs exactly)
yang's `Plane` normal "points OUTWARD from the solid." For the curved variants
we adopt an **implicit radially-outward convention** and document it in each
doc comment:

- `Sphere`: outward = radially **away from `center`** (a positive-radius solid
  ball).
- `Cylinder`/`Cone`: outward = radially **away from the axis** (a solid
  cylinder/cone).

**Why no `sense: bool`:** (1) Mirroring ssi-rs field-for-field keeps the future
mapping trivial — ssi-rs has no sense field, and adding an unused, untested
field now risks encoding it wrong. (2) yang already derives outward orientation
from **mesh winding** at reconstruction time (`reconstruct_topology` flips the
stored normal when the largest boundary cycle's winding opposes the inherited
normal) — it does not rely on a stored sense even for planes. The curved analog
(cavity sense for a subtracted curved face) will be derived the same way when
curved Stage-6 reassembly is implemented. (3) This PR produces **no** curved
output — every curved face is rejected loudly before any reconstruction — so a
sense field would be dead, untested code today (YAGNI). Subtracted/cavity
curved-face sense is deferred to the curved Stage-6 PR.

### New error variant: `YangError::CurvedSurfaceNotYetSupported { face: usize }`
Add to `YangError` with a `Display` arm. No existing variant fits:
`MalformedTopology` is for *malformed* input (a curved face is well-formed),
`UnsupportedOp` is keyed on `BoolOp`, `DegenerateFace` is geometric degeneracy.
The new variant carries the offending input B-Rep **face index** (parallel to
`DegenerateFace { face }` / `FaceResolutionFailed { tri }`). It is a
non-`source` error (like the others).

## Production sites to migrate (all in `crates/yang-rs/src/lib.rs`)

All three currently use an **irrefutable** `let Surface::Plane { .. } = …;`,
which becomes refutable once variants are added → must become a `match` whose
curved arms return the loud `Err`. Planar arms stay byte-for-byte identical.

1. **`BRep::new` Stage-1 winding canonicalization** — the **primary,
   observable** rejection point. `BRep::new` eagerly tessellates, so
   constructing a B-Rep with any curved face returns
   `Err(CurvedSurfaceNotYetSupported { face: f_idx })` here.
2. **`boolean()` face-resolution `plane_dist` closure** — convert to a `match`;
   curved arm returns the loud `Err` (carry the input face index).
   Defensive/unreachable in practice; must compile + be loud — **never**
   `unreachable!` or a panic.
3. **`reconstruct_topology` surface inheritance** — convert to a `match`; curved
   arm returns `Err(CurvedSurfaceNotYetSupported { face: face_idx })`. Also
   defensive/unreachable in practice; must compile + be loud.

`Curve` has **no** production `match` sites, so adding `Circle`/`Ellipse` breaks
no production exhaustiveness — but no silent curve handling may be added either.

## Test-site migrations (faithful contract migration — RED author owns these)

- `surface_plane_construction`: single-arm `match s` becomes non-exhaustive.
  Keep the `assert_eq!` checks; make the match exhaustive (add a catch-all arm
  that fails the test, since the value is constructed as `Plane`).
- `resolve_face` test helper: irrefutable `let Surface::Plane { .. } = f.surface;`
  becomes refutable. Migrate to
  `let Surface::Plane { normal, d } = f.surface else { continue; };` — faithful
  for the planar fixtures it serves; preserves all downstream assertions.

## Role-separated TDD cycle (P5)

1. **Spec (Manager)** — this doc.
2. **RED** — construction round-trip tests for each new variant + loud-rejection
   tests (Sphere, Cylinder, AND Cone) asserting the exact
   `Err(YangError::CurvedSurfaceNotYetSupported { .. })` variant; migrate the two
   breaking tests.
3. **GREEN** — production only: enum variants + doc comments, the `YangError`
   variant + `Display` arm, convert the three `let Surface::Plane` sites to
   loud-rejecting `match`es.
4. **Adversary** — verify reachability + exact variant, NO panic/unwrap/
   unreachable/silent skip/planar approx on any curved path, test migrations not
   weakened, planar paths byte-for-byte unchanged.
5. **Roadmap + push (Manager)**.

## CI gate (ALL must be clean — FULL crate suite)

- `cargo test -p yang-rs`
- `cargo fmt -p yang-rs -- --check`
- `cargo clippy -p yang-rs --all-targets -- -D warnings`

## Stop conditions (no hack-to-green)

If a genuine conflict surfaces (existing test asserting behavior incompatible
with loud rejection, or a fourth non-defensive `Surface::Plane` destructure),
**STOP and report** — do not widen tolerances, add fallbacks, or special-case.

## Next steps after this PR

- **P2**: curved Stage-1 tessellation (replace loud rejection at `BRep::new`
  with real bijective tessellation of curved faces).
- **P3**: Stage-3 `ssi-rs` wiring (yang `Surface` → ssi `QuadricSurface` mapping;
  analytical SSI curves survive to the output B-Rep).

No `ssi-rs` call or curved tessellation exists yet after this PR.
