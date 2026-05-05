# PR-VIZ-1 — per-stage OBJ-dump for visual Yang-pipeline debugging

**Status:** SPEC (FIP §3 dev-tooling spec, NOT §8 fix-spec). **Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` 0a.

## 1. Goal

Add a per-stage OBJ-dump capability to the Yang pipeline so investigators can visually inspect mesh state (verts, tris, per-tri labels) at each existing conformal-probe site by opening the dumps in MeshLab / Blender / F3D. After 12+ PRs of text-mode probe debugging, the next defect under investigation (cylindrical quad-strip winding orientation) is *visually obvious* but text probes report only counts. User-approved MVP. Motivated by Yang 2025 §4 Fig 2 (the per-stage pipeline diagram). **Dev tooling, not a defect fix** — no test-author role; smoke test bundled into implementer-n's deliverable.

## 2. Env-var contract

Piggy-backs on the existing `YANG_CONFORMAL_PROBE=1` gate — both must be set for dumps to fire. New env var `YANG_STAGE_DUMP=<dir>`. When unset (either var): zero filesystem writes, zero overhead, byte-identical behavior. Pattern verbatim from `dump_mesh_as_obj` consumers and the existing probe gates at `crates/kernel/src/boolean/topology_extract.rs:779,1669,1820,1907` and `crates/kernel/src/boolean/yang_integration.rs:1041`. No new sub-env-vars (no `_VERBOSE`, no `_FORMAT`).

## 3. File layout

```
<YANG_STAGE_DUMP>/
  <case_or_seq>/
    stage_A.obj             stage_A_labels.csv
    stage_Bb.obj            stage_Bb_labels.csv
    stage_B.obj             stage_B_labels.csv
    stage_C.obj             stage_C_labels.csv
    stage_E_lod=Render.obj  stage_E_lod=Render_labels.csv
    stage_E_lod=Adaptive.obj stage_E_lod=Adaptive_labels.csv
    stage_F.0.obj           stage_F.0_labels.csv
    stage_F.1.obj  …  stage_F.4.obj  (+ matching _labels.csv)
```

`<case_or_seq>` is a literal case-id (e.g. `F0031`) when set; otherwise `seq_<N>` (per-stage incrementing counter). OBJ is the existing `dump_mesh_as_obj` format at `yang_integration.rs:1466` — full f64 precision (`%.20e`), 1-indexed `f` lines, no normals, no groups. CSV columns are stage-dependent per §5; first line is the header. Stages whose labels would be empty still emit the OBJ but skip the CSV. mkdir errors and write errors are silently ignored (dev tooling MUST NOT crash production code).

## 4. Case-ID strategy

A thread-local `CURRENT_CASE_ID: RefCell<Option<String>>` lives in the kernel (suggested home: `crates/kernel/src/boolean/yang_integration.rs` alongside `dump_mesh_as_obj`) with `pub(crate) fn set_current_case_id(Option<String>)` + `pub(crate) fn current_case_id() -> Option<String>`. `crates/test-harness/src/assay/randomized_runner.rs::run_single_case` (L140) sets it to `Some(case.id.clone())` before `replay_and_validate` and clears it after. Probe sites read `current_case_id()`; on `None`, fall back to `format!("seq_{}", N)` from a per-stage `AtomicUsize` counter. `randomized_runner.rs` is sequential and synchronous (kernel runs on the same thread as the harness invocation), so thread-local propagation is the simplest correct model. No env var pollution.

## 5. Per-stage label table (CSV columns)

| Stage | A/B Origin | Inside/Outside | Face ID | CSV columns |
|-------|-----------|----------------|---------|-------------|
| A     | yes (split vecs)   | no              | no | `tri_idx,origin` |
| Bb    | yes (split vecs)   | yes (parallel labels) | no | `tri_idx,origin,inside` |
| B     | yes (inherited)    | yes (inherited)       | no | `tri_idx,origin,inside` |
| C     | yes (from src)     | yes (from src)        | no | `tri_idx,origin,inside` |
| E     | no                 | no              | yes (`face_ranges`) | `tri_idx,face_id` |
| F.0–F.4 | no               | no              | yes (`face_ranges`) | `tri_idx,face_id` |

`origin` ∈ {`A`,`B`}. `inside` ∈ {`0`,`1`}. `face_id` is the `KernelId(u64)` decimal value taken from the `face_ranges[k].face_id` whose `[start_index,end_index)` contains `tri_idx*3`. CSV omits unavailable columns (the table is the contract, not a stricture to emit blanks).

## 6. Reuse

- `dump_mesh_as_obj` at `crates/kernel/src/boolean/yang_integration.rs:1466` — used as-is (don't modify). Note its env-gating pattern is intrinsic (it just writes whatever it's given); the gate lives in callers per §2.
- New helper `dump_labels_as_csv(rows: &[LabelRow], path: &str) -> Result<(), String>` (~20 LOC) in the same module, mirroring `dump_mesh_as_obj`'s shape (open, writeln! loop, propagate errors as String).
- For Stages E / F.0–F.4 the in-memory mesh is `RenderMesh` (f32). Reuse the existing `render_mesh_to_arrays` at `yang_integration.rs:46-69` for f32→f64 widening (lossless).

## 7. Smoke test

`crates/test-harness/tests/pr_viz_1_smoke.rs` — one `#[test] #[ignore]` test: create a unique tempdir, set `YANG_CONFORMAL_PROBE=1` + `YANG_STAGE_DUMP=<tempdir>` + `YANG_BOOLEAN=1`, invoke `assay::randomized_runner::run_single_case` on `F0031`, assert at least one `stage_*.obj` file exists under `<tempdir>/F0031/`, and spot-check the first OBJ contains both a `v ` line and an `f ` line. Bundled into implementer-n's deliverable per FIP §3 dev-tooling shape (no separate test-author).

## 8. Out of scope

In-app integration; 3D viewer page; mesh diffing across stages; real-time updates; app dialog UI; additional sub-env-vars (`_VERBOSE`, `_FORMAT`); modifying existing probe data shapes; Cherchi-side stage dumps; the cylindrical-winding investigation itself (deferred to the next PR after this tool ships); R0071 kernel hang; PR-Y15a/PR-Y15b.1 follow-ups; removing the deprecated S-H clipping pipeline.
