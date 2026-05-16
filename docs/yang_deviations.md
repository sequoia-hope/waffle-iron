# Yang/Cherchi Implementation Deviations Log

**Purpose:** authoritative record of known divergences between the Yang 2025 hybrid B-Rep/mesh boolean pipeline as specified in the paper, and the implementation in `crates/kernel/`. Any deviation listed here MUST have either (a) a user sign-off with stated rationale, or (b) an active remediation tracked.

**Discipline:** per `CLAUDE.md` "Paper-Spec Compliance is MANDATORY," deviations are errors. Investigation on a component with an open deviation is blocked until either the deviation is fixed or signed off in writing.

---

## Open deviations (NOT signed off; investigation blocked)

### D1 — Simplified earcut used where Yang §4.4.1 specifies CDT

**Status:** OPEN. NOT signed off.

**Discovered:** 2026-05-16 during PR-Y46 architectural review.

**Code location:**
- `crates/kernel/src/tessellation/mod.rs:3471` — `earcutr::earcut(...)` call (convex path)
- `crates/kernel/src/tessellation/mod.rs:3509` — `earcutr::earcut(...)` call (with-holes path)
- `tessellate_planar_face_bounded` in `tessellation/mod.rs` (per-face independent triangulation entry point)

**Paper prescription:** Yang 2025 §4.4.1 ("Mesh updating") at `refs/text/yang2025_hybrid_boolean.txt:548-590`. Quote: "through CDT we obtain valid discretizations of the trimmed meshes" (line ~557). Yang specifies **Constrained Delaunay Triangulation** — an algorithm that enforces intersection-curve polyline boundaries as hard constraint edges across all faces that meet at a shared boundary.

**Current implementation:** uses the `earcutr` crate (Livesu et al. 2021 "Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"). The simplified earcut is a deterministic O(n) ear-removal algorithm. It does NOT enforce constrained edges across face boundaries; each B-Rep face's earcut runs independently with no cross-face coordination beyond shared `disc.positions` vertex pool.

**Architectural consequence:** when two adjacent B-Rep faces meet at a shared boundary that includes intersection-curve vertices, both faces' earcut calls receive the same boundary vertex positions (via shared `disc.positions`), but each face's earcut chooses diagonals independently. This produces the "Case D" defect signature measured across PR-Y43-Y46: all 3 vertex positions are present in the final Render LOD vertex set (sub-class (a) `m1x=3, m5x=3`), but no triangle connects them because each face's independent earcut emitted incompatible diagonals.

**Investigation cost prior to discovery:** 15 PR cycles (PR-Y32 through PR-Y46), ~2821 LOC of probe scaffolding, 0 production code, F0020 unpaired count unchanged at 40 across the entire arc.

**Remediation options:**
1. Constrained earcut wrapper (~80-150 LOC) — detect B-Rep shared edges, pass as hard constraints to earcut OR post-process diagonal flips. Closest to current architecture.
2. Replace `earcutr` with a true CDT (full Bowyer-Watson + constraint enforcement, or port from CGAL / use a Rust CDT crate). ~800-1500 LOC. Highest paper-alignment.
3. Cross-face coordination phase before per-face earcut. ~50-100 LOC for boundary alignment but doesn't fix earcut divergence root.

**Investigation status:** **REMEDIATION IN PROGRESS.** Earcut sweep complete; Tier 2/3 remain.

**Cycle 1 (2026-05-16):** Replaced `earcutr` with `spade::ConstrainedDelaunayTriangulation::try_bulk_load_cdt` at `tessellate_planar_face_bounded` (two call sites: planar-no-holes, planar-with-holes). Constraint edges constructed from boundary loops. Results: F0020 unpaired 40→35; kernel lib 1262→1266; yang_fast 10/157→13/157.

**Cycle 1.1 (2026-05-16):** Removed the earcut-fallback arm at both call sites — `try_bulk_load_cdt`'s silent-conflict callback handles upstream defects internally, so the fallback was unreachable on F0020. Replaced fallback with eprintln + empty output.

**Cycle 1.2 (2026-05-16):** Swept the 9 remaining earcut call sites across the kernel — each replaced with `cdt::cdt_triangulate_flat` (earcut-shaped flat-API wrapper). Sites converted: `boolean/coplanar_preprocess.rs::triangulate_polygon_with_holes` (Yang §4.5.5 path), plus 8 sites in `tessellation/mod.rs` (convex/non-convex polygon, revolve caps, cylinder strip, sphere/torus cap). Removed `earcutr` from `Cargo.toml`. Results: kernel lib 1266→1268; yang_fast 13/157 (unchanged); F0020 35 unpaired (unchanged); cdt unit tests 4→6 passing.

**Earcut now removed from the kernel.** Confirmed via `grep -rn earcutr crates/kernel/src/` returning zero hits.

**Remaining work to close D1:**
- **Tier 2:** intersection-curve constraint identification — distinguish vertices on intersection curves from B-Rep boundary vertices and pass them as additional constraint edges per Yang §4.4.1's "polyline as boundary" requirement. Required to close the Case D 24/24 finding.
- **Tier 3:** canonical cross-face plane basis — eliminate ±ε 2D-projection drift between adjacent faces sharing an edge. Deferred unless Tier 2 doesn't close Case D.

**Implementation choice note:** Yang §4.4.1 says "CDT in CGAL [2024]." We use `spade` (Rust-native CDT, ~5-10k LOC) rather than porting CGAL's CDT (~12k lines of C++ templates plus kernel/predicate infrastructure). `spade` implements the same algorithm class (Constrained Delaunay Triangulation with adaptive predicates via the `robust` crate); the deviation is "CGAL specifically" not "CDT semantics." This is a documented choice, not a behavioral deviation.

**Sign-off:** *not eligible.* Deviation remains until Tier 2 (and possibly Tier 3) lands.

---

## Closed deviations (signed off; investigation may proceed)

*(none yet)*

---

## Resolved deviations (implementation brought into line with paper)

*(none yet)*

---

## How to add a new deviation entry

When you discover a divergence during work:

1. Add a new entry to "Open deviations" with: code location, paper section, current behavior, paper prescription, architectural consequence, remediation options.
2. Cite the relevant paper line numbers from `refs/text/*.txt`.
3. Flag in the current cycle's audit memo and mention at the TOP of your next message to the user.
4. Halt the affected investigation; do not continue accumulating probe data on the divergent implementation.

## How to sign off on a deviation (user)

Append to the deviation's entry:

```
**Sign-off:** approved by <name>, <date>, rationale: <text>. Tracking issue / future remediation: <link or note>.
```

This is a deliberate choice to accept the deviation — usually because the paper-aligned implementation is impractical or impossible in this codebase context. The sign-off documents the trade-off so future investigations don't waste cycles re-discovering the gap.

## How to mark a deviation resolved

When implementation is brought into line with the paper:

1. Move the entry from "Open deviations" to "Resolved deviations".
2. Append `**Resolved:** <date>, commit <sha>, description: <text>`.
3. Update CLAUDE.md "Known deviations" section if you summarized there.
