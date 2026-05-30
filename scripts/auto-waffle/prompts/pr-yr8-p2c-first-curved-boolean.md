# PR-YR8 (P2c) — yang-rs first curved boolean end-to-end: cylinder ∪ box (mesh-approximate)

Context: P2a (PR-YR7) gave `yang-rs` a watertight, `d_ε`-bounded, bijective
**cylinder** tessellation + point-to-surface cylinder face resolution, verified by
a 4-part oracle. PR-YR6 added the curved `Surface`/`Curve` types. This PR is
**P2c**: run a curved solid through the WHOLE pipeline for the first time —
`cylinder ∪ box` — and prove that (a) curved geometry flows through Stages 2/5/6
and (b) the **analytic surface survives** into the output B-Rep (governance A15:
the mesh is a tool, the analytic surface is the truth).

**This is the roadmap's Phase-2 exit example (`cylinder ∪ box`), MINUS the exact
edges.** Intersection edges stay mesh-approximate (`Curve::LineSegment` polylines)
in this PR; replacing them with exact `ssi-rs` curves is **P3**.

**Hard scope limits (do NOT exceed):**
- **No `ssi-rs` call, no exact intersection curves.** Do not import or call
  `ssi-rs`. Intersection edges remain `LineSegment`. (P3 wires ssi-rs.)
- **Cylinder + box only.** No sphere (P2b), no cone, no two-curved-solids case.
- **Union first.** Target `BoolOp::Union` (the roadmap example). If subtract/
  intersect fall out for free, fine, but Union is the required, asserted case.
- Reuse the existing Stage-2 sidecar path (`cherchi-sidecar-rs` /
  `SidecarBoolean`) and the existing Stage-5/6 `reconstruct_topology` — extend
  them for curved faces; do NOT rewrite the planar path.

Read `crates/yang-rs/CLAUDE.md`, `refs/text/yang2025_hybrid_boolean.txt` (§4.2
mesh intersection, §4.4 topology update, and the bijectivity argument that an
intersection loop maps back to either B-Rep), and `crates/yang-rs/src/lib.rs`:
`boolean()` (the end-to-end dispatch), Stage-1 cylinder tessellation (PR-YR7),
`reconstruct_topology` (~line 889; YR5 — flood-fill same-attribution patches →
boundary cycles → BRepEdge/BRepFace, inheriting `Surface` from the input face),
and the face-resolution point-to-surface code (PR-YR7). Note YR5's documented gap:
None-attributed cut-surface triangles are skipped → output can be non-2-manifold.

## What to build

1. **End-to-end curved boolean.** Make `boolean(cylinder_brep, box_brep,
   BoolOp::Union, &backend)` run: Stage 1 tessellates both (cylinder via PR-YR7,
   box via the planar path), Stage 2 obtains the `LabeledArrangement` from the
   sidecar, Stages 5/6 reconstruct the output B-Rep. Fix whatever breaks for
   curved input along that path.
2. **Curved-face reassembly (Stage 6).** A kept patch attributed to the cylinder's
   lateral face must emit a `BRepFace` carrying `Surface::Cylinder` (inherited from
   the input face) — the analytic surface SURVIVES. Box patches stay `Plane`.
   Patch attribution uses the PR-YR7 point-to-surface resolution. The surviving
   curved face's rim/intersection boundary edges are `Curve::LineSegment` /
   `Curve::Circle` as the mesh provides (exact SSI is P3).
3. **2-manifold honesty.** Union of two solids has no internal cut-surface faces in
   its result shell, so the output should close 2-manifold. Require it. If a
   genuine YR5-class cut-surface/closure gap blocks 2-manifold closure for this
   case, **STOP and report** (P9/P10) with the specific gap — do NOT emit a wrong
   shell or fake closure with snapping.

## Oracle (RED contract)

A first curved boolean that's wrong-but-green must fail. Author RED tests on
`cylinder ∪ box` (cylinder axis through/over a box face; pick a config with a
real curved intersection):
1. **Runs & is Ok**: `boolean(...)` returns `Ok` (output BRep + mesh), no panic.
2. **Analytic surface survival**: the output B-Rep has ≥1 face with
   `Surface::Cylinder` whose parameters equal the input cylinder's (the curved
   surface is preserved exactly, not re-fit) — this is the whole point.
3. **Sidecar mesh-parity** (the Stage-2 reference oracle, now with curved input):
   the output mesh matches the C++ sidecar's mesh boolean of the SAME two input
   tessellations (canonicalized compare). Env-gate if the sidecar binary is
   unavailable, and `log`/note the skip — do not silently pass.
4. **Geometric soundness**: every output-mesh vertex attributed to the cylinder
   lateral lies within `d_ε` of the analytic cylinder surface; vertices on box
   faces lie on their planes.
5. **2-manifold** (per item 3 above) OR a documented, asserted `NonManifoldOutput`
   with the specific deferred reason — not a silent wrong shell.
6. **Determinism**: identical inputs → identical output.

Also: existing **planar** box-boolean tests (incl. the box-boolean fuzz) pass
byte-for-byte unchanged.

## CI gate (FULL crate suite)
`cargo test -p yang-rs` (whole crate — a Stage-5/6 change regresses the planar
boolean if wrong), `cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs
--all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` — record PR-YR8/P2c
(first curved boolean, cylinder ∪ box, mesh-approximate, analytic surface
survives) done; note this hits the Phase-2 exit case MINUS exact edges, and the
remaining Phase-2 work is P2b (sphere tessellation) and P3 (Stage-3 ssi-rs wiring
→ exact intersection edges). No ssi-rs work in this PR.
