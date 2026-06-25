# KV6d — torus booleans: scope & increment plan

**Status:** scoping, 2026-06-25. The deferred milestone that is the real ceiling
for the corpus's revolve-circle cases (and R0015/R0038's residue chains).

---

## 1. Why it matters (corpus demand)

**23 corpus cases contain a `revolve(circle)` op → a torus**, and ALL are
multi-op (2–3 ops), so every one needs a torus to go through a boolean:

| Torus op | Cases |
|---|---|
| **boss (union)** | ~15 (R0015, R0026, R0028, R0045, R0050, R0057, R0059, R0062, R0074, R0077, R0085, R0087, R0091, R0096, …) |
| **cut (subtract)** | ~9 (R0025, R0027, R0038, R0046, R0047, R0050, R0051, R0065, R0094, R0096) |

Both union and cut **trim the torus** where it meets the other solid → the output
has **torus PATCHES** (a torus face bounded by the intersection curve), not a
whole torus. So nearly every case needs trimmed-torus-patch output, not just the
operand passing through. (None are single-op / trivially torus-disjoint.)

This is also R0015/R0038's *real* ceiling: their other residues were all
resolved this session (sliver + 2 edge-splits shipped), and the precisely-
measured `LabelMismatch` is a 1-ulp conformality break that only torus-blocked
cases reach — so KV6d torus booleans, not more Stage-0/cherchi work, is the lever.

---

## 2. What's already DONE (shipped)

- **Torus MODELING** (kernel-v2): `Surface::Torus`, `geom::torus_residual`,
  `validate_torus_face`, `build_torus_revolve` (construct.rs: 2 disk caps +
  toroidal lateral + 2 longitude-arc seams), `tessellate_torus_lateral` (θ×φ
  quad grid). Partial-torus revolve is app-usable + renders (KV6d incr 1–3).
- **Torus INGESTION** (yang): `Surface::Torus` variant, analytics
  (`signed_distance_to_surface`, `face_eval` with `u=φ,v=θ` over `ortho_basis`),
  and `tessellate_torus_face` (lib.rs:2755 — 2D bijective (θ×φ) grid;
  watertight test) (KV6d incr 4a/4b).

So a torus B-Rep can be built, validated, tessellated, and a yang torus face can
be tessellated. The pipeline plumbing exists on both ends.

---

## 3. What's MISSING (the increments)

### Increment 5a — interop re-wire (SMALL; was prototyped + reverted)

The kernel-v2→yang boundary still walls a torus operand:

```
crates/kernel-v2/src/boolean.rs:520   Some(Surface::Torus { .. }) => CurvedGeometryMismatch{ "KV6d boolean increment" }
```

Re-apply the reverted increment-5 interop (it traversed the pipeline before the
revert — see this session's git history / `kernel_v2_kv6d_torus` memo):
- **to_yang** (boolean.rs ~520): merge Torus into the curved arm; emit the torus
  loop pattern `[Circle, Arc, Circle, Arc]` (the two profile/meridian circles +
  the two longitude seam-arc twins).
- **from_yang** (boolean.rs: `enum FaceSurf` ~695, the reconstruction ~1318):
  add `FaceSurf::Torus`; recognize yang `Surface::Torus`; map `FaceSurf::Torus`
  → kernel `Surface::Torus`; add the torus rim-normal derivation arm.
- **Stage-5/6 reassembly** (yang lib.rs): add `Torus` to the CURVED-branch gate
  (the `matches!(inherited, Cylinder|Sphere|Cone)` site) so a torus patch isn't
  routed to the loud arm.

After 5a a torus boolean RUNS end-to-end and the torus face survives — but its
output boundary comes back as a **mesh POLYLINE**, which is the wall:

### Increment 5b — torus OUTPUT BOUNDARY RECOVERY (the hard tail)

The surviving/trimmed torus face's loop is a polyline (yang reassembly is
mesh-based); `tessellate_torus_lateral`/`tessellate_torus_face` need analytic
boundary curves (it fails "missing +axis seam arc"). Two sub-cases:

- **(b1) survive-whole / boundary-modified** (a union where the torus is mostly
  intact): recover the **meridian profile circles + longitude seam arc** from the
  polyline. Analogous to the cone-5c "canonicalize the surviving band to seamed
  form" trick — but HARDER: the cone boundary is a ⊥-axis latitude circle
  recoverable by plane-retag; the torus boundary (meridian circles, normal ±m̂;
  longitude seam, a latitude arc) is NOT a ⊥-plane section. MEDIUM effort.

- **(b2) CUT-tube arbitrary patch** (the dominant corpus case — the intersection
  curve cuts the tube into an arbitrary-boundary patch): needs **UV-CDT
  torus-patch tessellation** — invert `face_eval` to project the boundary
  polyline → `(u=φ, v=θ)`, constrained-Delaunay-triangulate in `(u,v)` WITH
  interior Steiner points (to bound chord error on a non-tiny patch), map back to
  3D, weld to the watertight result. The UV-CDT **consumer is now BUILT**
  (`yang-rs::tessellate_torus_patch`, 2026-06-25): project → unwrap → snap to
  TAU_WORK → `cdt_polygon_with_holes_refined` → map back (boundary verts EXACT,
  Steiner = `face_eval`, on-torus). Test: synthetic UV-rect patch is watertight,
  every vert on-surface, chorded area = analytic. Remaining for 5b2: wire it into
  Stage-5/6 (feed the surviving torus patch's recovered boundary polyline). See
  §4.

---

## 4. The long pole: an interior-Steiner CDT primitive — **NOW BUILT (2026-06-25)**

**UPDATE:** the interior-Steiner CDT primitive is shipped:
`cherchi_rs::triangulation::cdt_polygon_with_holes_refined(verts, outer, holes,
max_area)` → `(fresh_verts, tris)`. It builds the constrained CDT and runs
spade's Delaunay refinement (`with_max_allowed_area(max_area)
.keep_constraint_edges()`) to add interior Steiner points until every triangle is
≤ `max_area` (the caller maps that from the surface chord-error budget). Boundary
verts preserved (conformal); non-convex interior-only; wasm-clean; rewrite-tier
green. So §5 step 3 is **DONE** — (b2) is now unblocked at the primitive level.

**The torus UV-CDT consumer is also now BUILT** (2026-06-25,
`yang-rs::tessellate_torus_patch`). Two robustness fixes to the primitive were
needed to make it usable on real (sampled-curve) boundaries:
- **Angle limit disabled** (`with_angle_limit(0°)`): spade's default 30° Ruppert
  limit chases the ~1e-16 kinks on a near-collinear (straight-seam) boundary run
  and over-refines into a slivers storm (352 vs 72 verts on the same rectangle).
  We want ONLY the area bound.
- **Topological exterior emit** (flood-fill from the convex hull across
  non-constraint edges) replaced the float point-in-polygon centroid test, which
  dropped near-boundary slivers and SLIT the mesh (watertight 2D area but
  count-1 interior edges → 3D gaps). The flood-fill keeps every interior face
  (slivers harmless) and still excludes the non-convex notch (L-shape preserved).
The consumer additionally snaps the inverted `(u,v)` to the TAU_WORK (1e-12) grid
to kill the trig round-trip noise at its source (geometrically safe: output
boundary verts are the EXACT input 3D points, Steiner are `face_eval`, on-torus).

The original gap (now closed) was a capability the codebase did not have
(verified in the KV6d UV-CDT feasibility investigation, 2026-06-24):

- **No reusable interior-Steiner CDT.** kernel-v2 tessellate = exact-rational
  EAR-CLIP (boundary-only, no interior points); yang's planar CDT path "adds NO
  interior Steiner points" (lib.rs:453). An arbitrary torus boolean patch needs
  interior Steiner points or it bulges (chord error) for any non-tiny patch. So
  UV-CDT needs a **NEW constrained-Delaunay-with-Steiner primitive** —
  correctness-critical (wants exact predicates), and vendoring a crate fights the
  WASM-clean / pure-Rust / no-new-dep discipline (cherchi hard-rule #7).
- **Testability blocker.** Testing torus-patch tessellation needs a torus PATCH,
  which needs a clean cutting boolean — but box cuts of a bent tube hit coplanar
  caps or contain-the-torus, and hand-building a valid arbitrary-boundary
  torus-patch B-Rep is itself hard. A rectangular UV sub-patch (4 arcs)
  tessellates as a structured grid w/o CDT and IS testable, but boolean outputs
  are not rectangular.

So the torus surface flux/volume is also degree-4 (no analytic `signed_volume`
arm — the modeling tests use MESH volume), reinforcing that the analytic tail is
genuinely hard.

---

## 5. Recommended sequencing

1. **Increment 5a (interop re-wire)** — small, low-risk; re-apply from git
   history. Lands the operand path. Verify against the assay: how many of the 23
   cases are torus-DISJOINT from the other solid (a multi-shell result, no patch)
   — those flip with 5a alone. (Likely few, but free to confirm.)
2. **Increment 5b1 (survive-whole recovery)** — medium; extends the cone-5c idea
   to meridian-circle + seam recovery. Flips the union cases where the torus
   isn't cut by the intersection (if any).
3. **The interior-Steiner CDT primitive** — ✅ **DONE (2026-06-25):**
   `cdt_polygon_with_holes_refined` (spade refinement; see §4). The gateway to
   5b2 and the non-convex-CDT profile tail is now in place.
4. **Increment 5b2 (UV-CDT torus-patch tessellation)** — the dominant corpus
   case; built on (3). The UV-CDT **consumer is DONE** (2026-06-25,
   `tessellate_torus_patch`); the remaining piece is wiring it into Stage-5/6
   (recover the surviving torus patch's boundary polyline, feed it in, weld).
5. **Increment 6** — corpus verify the 23 cases (full assay; SUPPORTED_WRONG==0
   gate; watertight + bbox/vol oracles per case).

**Effort:** 5a is hours; 5b1 is a focused session; (3)+(5b2) is a multi-session
research effort whose long pole is the Steiner-CDT primitive. **The honest
headline: KV6d torus booleans for the corpus are gated on building an
interior-Steiner constrained-Delaunay primitive** — that is the real cost, not
the torus-specific plumbing.

---

## 6. References

- kernel-v2: `boolean.rs:520` (to_yang torus wall), `:695` (`enum FaceSurf`),
  `:728` (`from_yang_brep`), `:1318` (output surface reconstruction);
  `construct.rs` `build_torus_revolve`; `tessellate.rs` `tessellate_torus_lateral`.
- yang: `Surface::Torus`, `lib.rs:2755` `tessellate_torus_face`, `lib.rs:453`
  (planar CDT "no interior Steiner" note), the Stage-5/6 curved-branch gate.
- Memory: `kernel_v2_kv6d_torus` (the full increment 1–4b state + the increment-5
  prototype/revert + the UV-CDT feasibility findings), `kernel_v2_kv6c_cone` (the
  cone-5c canonicalize precedent), `kernel_v2_kv7_output_curve_recovery`,
  `non_convex_tessellation_must_be_cdt`.
- Paper: `refs/text/yang2025_hybrid_boolean.txt` — torus is a degree-4 surface;
  the paper's CDT is CGAL-based (we need a pure-Rust equivalent).
