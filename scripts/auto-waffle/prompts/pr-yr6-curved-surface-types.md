# PR-YR6 — yang-rs curved Surface + Curve variants (types only; pipeline rejects loudly)

Context: `yang-rs` has a functional **planar-convex** boolean (Stages 1, 2, 5,
6). The strategic next direction is wiring the `ssi-rs` analytical SSI curves
into the boolean (Stage 3/4) so analytical surfaces survive to the output B-Rep.
That is gated on curved-geometry support. **This PR is the first, smallest,
lowest-risk step of that path** (roadmap M5 / Phase 2 entry): extend the
`Surface` and `Curve` enums with the curved variants a first curved boolean needs
(target cases `sphere − cylinder`, `cylinder ∪ box`), make the pipeline **accept
them at the type level** while **rejecting them LOUDLY** (explicit `Err`, never a
panic, never a silent skip, never a wrong planar approximation — A15.2) at the
stages that do not yet handle curved geometry.

**Out of scope (do NOT do these here):** curved tessellation, curved face
resolution, any `ssi-rs` call, any yang::Surface → ssi::QuadricSurface mapping.
Those are later PRs (P2 = curved Stage-1 tessellation; P3 = Stage-3 SSI wiring).
Keep this PR a pure type extension + loud rejection.

Read `crates/yang-rs/CLAUDE.md` (scope rules), `crates/yang-rs/src/lib.rs` — the
`Surface` enum (~line 91) and `Curve` enum (~line 102), the tessellation winding
canonicalization that destructures `Surface::Plane` (~line 395), the
face-resolution point-to-plane distance (~line 784), and `reconstruct_topology`
(~line 889). Also read the `QuadricSurface` and `SsiCurve` enums in
`crates/ssi-rs/src/lib.rs` and **mirror their field shapes** so a future Stage-3
mapping is trivial — but DO NOT add that mapping now.

Scope:
- **Add to `Surface`** (mirror ssi-rs `QuadricSurface` fields):
  `Sphere { center: Point3, radius: f64 }`,
  `Cylinder { axis_point: Point3, axis_dir: Vector3, radius: f64 }`,
  `Cone { apex: Point3, axis_dir: Vector3, half_angle: f64 }`.
  yang's `Plane` normal "points OUTWARD from the solid"; the curved variants need
  an equivalent outward-side convention. If a `sense`/`bool` field is required to
  disambiguate solid-inside vs solid-outside (e.g. a spherical cavity vs a ball),
  include it and document it; otherwise document the implicit convention
  explicitly in the doc comment. Justify the choice.
- **Add to `Curve`** (mirror ssi-rs `SsiCurve` fields): `Circle`, `Ellipse` —
  the curve types the demo pairs (plane∩sphere, plane∩cylinder, sphere∩cylinder)
  produce. (`LineSegment` stays; `Parabola`/`Hyperbola` are not needed for the
  demo pairs — skip them, note them as banked.)
- **Make the crate compile and keep planar paths byte-for-byte unchanged.**
  Update EVERY site that destructures `Surface::Plane` or matches on `Curve` so
  the existing planar behavior is identical. The two known sites are the
  tessellation winding canonicalization (~line 395) and face resolution (~line
  784) — **find any others**. At each such site a curved variant must produce a
  **loud, explicit `Err`** (add a `YangError` variant such as
  `CurvedSurfaceNotYetSupported`, or reuse an apt existing one — justify),
  NEVER a panic (`unwrap`/`unreachable!`/`let-else`-panic), NEVER a silent skip,
  NEVER a planar approximation of a curved surface.

Tests (RED, role-separated — distinct RED author / GREEN implementer / Adversary):
- **Construction:** each new `Surface` and `Curve` variant constructs and
  round-trips its fields.
- **Planar pipeline UNCHANGED:** the existing yang-rs suite (planar box booleans,
  topology reconstruction, the box-boolean fuzz if present) passes identically.
  **Run the FULL `cargo test -p yang-rs`, not just the new test file** — a new
  enum variant changes match exhaustiveness across the crate, so a scoped run
  hides regressions (this is a hard requirement, see CI gate).
- **Loud rejection:** a `BRep` containing a curved face (Sphere, Cylinder, AND
  Cone), fed through `boolean()` / the tessellation stage, returns the **exact**
  `Err` variant (assert the variant), never `Ok`-with-wrong-geometry and never a
  panic. (No on-surface oracle — there is no geometry processing in this PR.)

CI gate (ALL must be clean, and the test step is the FULL crate suite):
`cargo test -p yang-rs` (whole crate) + `cargo fmt -p yang-rs -- --check` +
`cargo clippy -p yang-rs --all-targets -- -D warnings`. Do not scope the test run
to the new file — confirm the existing planar tests still pass.

On completion: update `docs/yang_functional_roadmap.md` — record PR-YR6 (curved
`Surface`/`Curve` types + loud rejection) as the first step of M5 / Phase 2;
note the next steps are P2 (curved Stage-1 tessellation) then P3 (Stage-3
`ssi-rs` wiring), and that NO ssi-rs call or curved tessellation exists yet.
