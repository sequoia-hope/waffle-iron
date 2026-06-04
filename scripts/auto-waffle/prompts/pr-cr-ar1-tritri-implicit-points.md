# PR-CR-AR1 — cherchi-rs arrangement: tri-tri intersection → implicit points

**Crate: `crates/cherchi-rs/` — obey its `CLAUDE.md` strictly** (MIT attribution
header on every ported file; **reference parity is the correctness oracle**; NO
`unsafe`; NO `panic!` in production paths — all `Result<>`; single-threaded;
exact arithmetic via `dashu` for explicit predicates and the
`indirect-predicates-sidecar-rs` FFI for the implicit/LPI/TPI ones; **predicates
are demand-driven** — add an IP wrapper only when this slice calls it).

This is the **first increment of M6** (native port of the MIT Cherchi C++,
removing the `cherchi-sidecar-rs` subprocess). See `docs/yang_functional_roadmap.md`
§M6 for the full PR-CR-AR*/CR-BL* decomposition. This PR ports the **per-pair
tri-tri intersection → implicit intersection-point construction**.

## Port source + spec
- **C++ source to port (MIT — attribute):**
  `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/intersection_classification.cpp`
  (+ `.h`). Read it as the spec; reproduce its classification + implicit-point
  construction faithfully (do NOT invent mechanism — port what Cherchi does).
- **Paper context:** Cherchi 2020 §4 (indirect predicates / implicit points) +
  the cherchi2022 text (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt`).

## Build on what `cherchi-rs` already has (do NOT re-port)
- `arrangements::intersection_detection` (CR13) — candidate intersecting pairs.
- `predicates::triangle_intersect` (CR9) — tri-tri intersection classification
  (Disjoint / Intersects / Coplanar + the Sign-pattern detail).
- The IP FFI handles + coordinate constructors:
  `ExplicitPoint3D` (CR-IP5), `ImplicitPoint3DLpi`/`Tpi` (CR-IP5b),
  `lambda3d_lpi_*` / `lambda3d_tpi_*` (CR-IP2–4), `orient3d` (CR-IP6). **Heed the
  CR-IP6 gotcha** (memory): the `_II/_IIII` predicate variants segfault on
  explicit input — use `genericPoint::`-style static dispatch.

## Scope (this PR only)
For each CR13 candidate pair that CR9 reports as intersecting (non-coplanar this
PR), construct the **intersection vertices** of the two triangles, each correctly
typed:
- an **existing input vertex** (an endpoint coincides) — the explicit point;
- an **LPI** (an edge of one triangle pierces the plane of the other) — an
  `ImplicitPoint3DLpi` built via `lambda3d_lpi`;
- a **TPI** (the three supporting planes meet) — an `ImplicitPoint3DTpi` via
  `lambda3d_tpi`,
exactly as `intersection_classification.cpp` decides. Output a typed
intersection-vertex set per pair (the input to AR2's re-triangulation). **OUT of
scope:** the per-triangle re-triangulation (AR2), the global conforming soup +
welding (AR3), coplanar tri-tri (defer — loud/skip with a classified marker), and
any boolean labeling (BL*). Add only the IP predicate wrappers this construction
calls.

## Oracle (be honest about what's checkable at this stage)
The full-pipeline reference-parity diff against the C++ `mesh_booleans` binary
engages at **AR3/BL3** (the binary exposes the final boolean, not the per-stage
intersection points). For AR1 the oracle is:
1. **Exact geometric self-consistency (load-bearing):** each constructed implicit
   point lies on BOTH supporting triangles' planes — verify via the exact
   predicates (e.g. `orient3d(point, plane_tri_a) == 0` and `== 0` for the other),
   NOT a float tolerance. An LPI lies on the piercing edge AND the pierced plane;
   a TPI on all three planes.
3. **Small hand-verified cases:** a handful of tri-tri pairs with
   known-by-construction intersection points (axis-aligned + a rotated case),
   asserting the constructed point type (LPI/TPI/explicit) and exact on-plane
   incidence.
4. **Classification agreement with CR9** on the same pairs (Intersects vs
   Disjoint vs Coplanar).
Document clearly that end-to-end C++ parity is deferred to AR3/BL3.

## CI gate
`cargo test -p cherchi-rs`, `cargo fmt -p cherchi-rs -- --check`, `cargo clippy
-p cherchi-rs --all-targets -- -D warnings`. All prior cherchi-rs tests
unregressed. No `unsafe`, no `panic!` in production. MIT attribution header on the
new/ported file(s). Single-threaded.

Role-separated FIP: Spec (you) → RED (one sub-agent) → GREEN (another) →
Adversary (a third). If the C++ source reveals the construction is materially
more involved than this scope (e.g. needs coplanar handling or a predicate that
segfaults via FFI), **STOP and report** — do not improvise around a Cherchi
deviation (track it in `docs/yang_deviations.md`).

On completion: update `docs/yang_functional_roadmap.md` §M6 (mark PR-CR-AR1 done;
note any new IP predicate wrapper added + any deviation) and the cherchi-rs
`LICENSE-THIRD-PARTY.md` if a new ported file was added.
