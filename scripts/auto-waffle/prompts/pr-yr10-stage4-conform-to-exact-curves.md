# PR-YR10 (Stage 4a) — yang-rs: conform the mesh to the exact intersection curves

Context: P3 (PR-YR9) made `cylinder ∪ box` intersection edges carry EXACT
analytical curves (`Curve::Circle`/`Ellipse` from `ssi-rs`). But the underlying
**mesh** is still the Stage-1 faceted approximation — its boundary along the
intersection is an inscribed polygon (chord error ≤ `d_ε`), not the exact arc.
Yang 2025 §4.4.1 ("mesh updating") re-triangulates the mesh so it **conforms** to
the refined intersection curves. This PR is the first, tractable slice of that.

**Scope decision (READ — this is a deliberately minimal Stage-4 slice):** There is
NO general constrained-Delaunay (CDT) implementation in these crates, and a robust
general CDT is multi-cycle. This PR does the *conforming refinement* only:
- For each output intersection edge that carries an exact `Curve::Circle`/`Ellipse`
  (from P3), **subdivide the mesh boundary onto the exact curve**: between
  consecutive intersection-loop mesh vertices, insert new vertices sampled
  **exactly on the analytic curve** (closed-form evaluation of the circle/ellipse
  at intermediate parameters — NO Newton/optimization; our curves are exact
  quadric sections), and **locally re-triangulate** the two incident faces (split
  the boundary edge + re-fan/flip the adjacent triangles) so the mesh boundary
  follows the exact arc and every boundary vertex lies on the exact curve.
- Maintain watertight 2-manifold validity and the `TessellationMap` bijection
  (new boundary vertices get `TessellationSource::BRepEdge { edge, t }` on the
  exact curve).
- **Explicitly DEFERRED (do NOT attempt):** general CDT / Delaunay optimality,
  oblique-only complications beyond what the cylinder∪box case needs, the §4.3
  numerical correspondence optimization (unneeded for closed-form quadric curves),
  curved Subtract. Note these as future increments.

Operate on yang-rs's OWN mesh type (cherchi-rs's `FastTrimesh` split/flip are on
its own mesh and not reusable across the boundary — implement the minimal
edge-split + local retriangulation on yang's mesh). No `unsafe`, no panic in
production paths.

Read `crates/yang-rs/CLAUDE.md`, `refs/text/yang2025_hybrid_boolean.txt` §4.4.1
(lines ~605–640 mesh updating), and in `crates/yang-rs/src/lib.rs`: the P3 exact
`Curve` assignment (PR-YR9), the mesh + `TessellationMap` types, `reconstruct_
topology`, and the cap-circle eval (`eval_source` Circle branch, PR-YR7).

## Oracle (RED contract) — refinement must do REAL, measured work

For `cylinder ∪ box` (perpendicular cap → Circle; include an oblique cut → Ellipse
if P3 supports it, else note it deferred):
1. **Boundary on the exact curve (to `TAU`)**: after Stage 4, every
   intersection-loop mesh vertex (original AND newly inserted) lies on the exact
   `Curve` within `TAU_MODEL`.
2. **Chord deviation strictly decreases**: the max distance from the mesh boundary
   polyline to the exact curve is strictly smaller after Stage 4 than before
   (proves the refinement actually conformed the mesh, not a no-op). Assert a
   concrete improvement (e.g. ≤ half the pre-refinement deviation, or ≤ a stated
   tighter bound).
3. **Still watertight 2-manifold**: 0 unpaired half-edges, Euler V−E+F=2 — the
   local retriangulation did not crack or create T-junctions.
4. **No degenerate/inverted triangles** introduced (positive area; consistent
   winding vs the analytic surface normal).
5. **Bijection round-trips**: every new vertex's `BRepEdge { edge, t }` source
   evals (closed-form) back to its position within tolerance.
6. **Determinism**; **planar boolean unregressed** (`fuzz_boxes`); **scope held**
   (only intersection-edge boundaries refined; interior/rim/seam untouched).
   Provide a sidecar-independent direct path (hand-built mesh + exact `Curve` →
   Stage-4 conform → assert) so the GREEN gate doesn't need the sidecar.

**STOP-and-report (P9/P10)** if local retriangulation cannot maintain validity for
the cylinder∪box case without an unavoidable flip/sliver (that would mean a real
CDT is required — report it as the boundary rather than hacking).

## CI gate (FULL crate suite)
`cargo test -p yang-rs`, `cargo fmt -p yang-rs -- --check`, `cargo clippy -p
yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` — PR-YR10 (Stage 4a:
mesh conforms to exact intersection curves via on-curve subdivision; general CDT
deferred). Note remaining: general CDT remesh, P2b sphere, curved Subtract.
