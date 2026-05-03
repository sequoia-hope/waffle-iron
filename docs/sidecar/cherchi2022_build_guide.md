# Cherchi 2022 mesh_booleans — Sidecar Build Guide

**What this is.** The C++ reference implementation of [#38] Cherchi et al. 2022
*Interactive and Robust Mesh Booleans* (the boolean pipeline Yang 2025 cites for
the exact mesh-boolean stage [#24] §4.2). We use it as a black-box differential-
testing oracle: feed our pre-Cherchi mesh into both Waffle and the C++ reference,
diff the outputs.

**Why we need it.** Per CLAUDE.md commit `4808f2e` and the strategic-escalation
rule (three wrong-or-incomplete anchors on F0002 → reference parity becomes
load-bearing), our internal probes can confirm a mesh is broken but cannot
identify which sub-stage of OUR port introduces the defect that the reference
does not. The sidecar IS the load-bearing oracle for PR-Y15+.

---

## Where the binary lives

**Default path (this Docker image, post-PR-S1):**

```
/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans
/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck
```

The sidecar repo lives **outside** `/home/claude/workspace` to keep the
in-repo footprint small and avoid accidentally committing C++ sources. Repo
size ~150 MB; build artifacts ~8 GB.

**Override:** the integration tests look for the binary via the
`CHERCHI2022_BIN` env var, defaulting to the path above. Set it to relocate.

---

## Build instructions

Tested 2026-05-03 in this Docker container (Ubuntu 24.04, GCC 13.3.0,
Clang 18.1.3, CMake 3.28.3). Total wall-clock ~22 minutes.

```bash
# 1. Clone (shallow, ~30s, ~150 MB)
mkdir -p /home/claude/cherchi2022 && cd /home/claude/cherchi2022
git clone --depth 1 https://github.com/gcherchi/InteractiveAndRobustMeshBooleans.git

# 2. Configure (~5s; enable Release for AVX2 + optimization)
cd InteractiveAndRobustMeshBooleans
mkdir -p build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release

# 3. Build (~20 min, parallelized; expect non-fatal OpenGL warnings on headless
#    boxes — the GUI demos won't link, but the headless `mesh_booleans` CLI
#    binary we want will. `make` may exit non-zero from those failures; that is
#    HARMLESS as long as `mesh_booleans` was built.)
make -j$(nproc)

# 4. Verify the headless binary built
ls -la mesh_booleans mesh_booleans_inputcheck
# Expected: both ~800 KB executables
```

**Exit-non-zero from `make` is acceptable** if the failures are confined to the
GUI demos (`mesh_booleans_arap`, `mesh_booleans_rotation`) which require OpenGL
that this container lacks. The headless `mesh_booleans` and
`mesh_booleans_inputcheck` are what we need.

---

## CLI signature

```
mesh_booleans <intersection|union|subtraction|xor> in1.obj in2.obj [in3.obj ...] out.obj
```

Multi-input variadic Boolean: takes 2+ input meshes. OBJ format only (no STL,
PLY, or OFF). 1-indexed face vertices. The `mesh_booleans_inputcheck` validator
takes a single OBJ:

```
mesh_booleans_inputcheck mesh.obj
```

It prints 5 line-per-check results (`passed` / `failed`) to stderr:

```
Manifold check:                   passed
Watertight check:                 passed
Local  Orientation check:         passed
Global Orientation check:         passed
Intersection check:               passed
```

Cherchi 2022 §3 explicitly axiomatizes that the boolean pipeline assumes
manifold + watertight + intersection-free + well-oriented input. Feeding it
malformed input is undefined behavior — observed empirically as **infinite
loops** (the F0002 runaway burned 6 hours at 99% CPU before being killed).
**Always wrap subprocess calls with a timeout cap** — see
`crates/test-harness/tests/cherchi2022_reference_parity.rs::run_with_timeout`
for the pattern (30s default, `child.kill()` on overflow).

---

## License

MIT (upstream). Vendor-friendly. We do NOT vendor any source — the sidecar
lives outside the repo entirely; only the build guide + integration test
wrappers are committed.

---

## Static F0002 input meshes

After running the F0002 reference-parity test (or any case with
`YANG_DUMP_OBJ_BASE` set), the dumped OBJ files persist at:

```
/tmp/waffle_cherchi_parity_f0002/f0002_a.obj
/tmp/waffle_cherchi_parity_f0002/f0002_b.obj
```

These are 16-vertex / 32-triangle pre-Cherchi meshes that fail
`mesh_booleans_inputcheck` on all three of {manifold, watertight, intersect}
(verbatim output captured in `docs/audits/cherchi2022_sidecar_feasibility.md`
§"Build verified 2026-05-03"). They are the canonical reproducer for whatever
PR-Y15 ends up fixing.

---

## See also

- `docs/audits/cherchi2022_sidecar_feasibility.md` — original feasibility memo
  + post-build empirical findings
- `crates/test-harness/tests/cherchi2022_reference_parity.rs` — the F0002
  + smoke integration tests
- `crates/test-harness/tests/cherchi_inputcheck_corpus_sweep.rs` (PR-S2) —
  corpus-wide sweep using `mesh_booleans_inputcheck`
- [#38] Cherchi et al. 2022 paper PDF: `refs/cherchi2022_interactive_robust_mesh_booleans.pdf`
- [#24] Yang et al. 2025 paper PDF: `refs/yang2025_hybrid_boolean.pdf` (§4.2 cites Cherchi 2022)
