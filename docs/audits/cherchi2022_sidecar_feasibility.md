# Cherchi 2022 C++ Sidecar — Feasibility Memo (PR-Y14a, Phase 0)

**Author:** Adversary (PR-Y14a)
**Date:** 2026-05-02
**Scope:** Reconnaissance only. No clone, no build, no code changes.
**Decision required of:** Manager / team lead — whether to spec a follow-up
PR that vendors and builds the Cherchi 2022 sidecar.

## 1. TL;DR

**Verdict: FEASIBLE, but with one significant caveat — disk space.**
The upstream repo (`gcherchi/InteractiveAndRobustMeshBooleans`, MIT licensed)
ships five ready-to-build CLI executables, including `mesh_booleans` which
takes the exact CLI form we want: `mesh_booleans <op> in1.obj in2.obj out.obj`.
All host-toolchain prerequisites (CMake 3.28, Clang 18, GCC 13, GNU Make,
pkg-config) are already installed. All third-party dependencies (oneTBB,
cinolib, Indirect_Predicates, abseil-cpp, parallel-hashmap, yocto) are
**bundled in `arrangements/external/`** — no system packages, no apt installs,
no submodule checkout required. Linux is an officially-tested platform
(GCC 7.3.0). **Best-estimate time-to-first-diff: 1–2 engineer-days** if disk
space is solved; the work is a thin Rust→FS wrapper and an OBJ writer, not
upstream porting.

**Single risk that warrants escalation now:** the workspace volume is at
**99 % full (24 GB free of 1.8 TB)**. A full source clone + Debug + Release
build of an Abseil-bundling C++ project will plausibly consume 4–10 GB of
that. Recommend the sidecar PR be planned against a fresh container image
or a different volume, not the current `/home/claude/workspace`.

## 2. Build environment (this container, 2026-05-02)

| Component   | Version            | Sufficient? |
|-------------|--------------------|-------------|
| CMake       | 3.28.3             | Yes (upstream needs ≥ 3.x) |
| Clang       | 18.1.3 (Ubuntu)    | Yes (upstream tested with Clang 14) |
| GCC         | 13.3.0 (Ubuntu)    | Yes (upstream tested with GCC 7.3) |
| GNU Make    | 4.3                | Yes |
| pkg-config  | 1.8.1              | Yes |
| Git         | 2.43.0             | Yes |
| Disk free   | **24 GB / 1.8 TB (99 % used)** | **Marginal — see §7** |

No detected gaps in toolchain. The upstream README states tested platforms
are macOS (Clang 14, 64-bit) and Linux (GCC 7.3.0, 64-bit); both of our
compilers are newer than tested baselines, which usually works but should
be verified in the build PR.

## 3. Upstream repo (https://github.com/gcherchi/InteractiveAndRobustMeshBooleans)

**License:** MIT — compatible with vendoring or as a sidecar binary.

**Build system:** CMake. Typical invocation per README:
`mkdir build && cd build && cmake .. -DCMAKE_BUILD_TYPE=Release && make`.

**CLI:** Yes — five executables produced by the build:
- `mesh_booleans` (the one we want)
- `mesh_booleans_arap` (interactive ARAP demo)
- `mesh_booleans_rotation` (rotation animation demo)
- `mesh_booleans_stencil` (stencil-style booleans)
- `mesh_booleans_inputcheck` (validates input meshes — likely useful as a
  pre-check oracle in addition to the main differential test)

**`mesh_booleans` invocation** (from `main.cpp`, fetched from upstream):
```
./mesh_booleans <intersection|union|subtraction|xor> in1.obj in2.obj [...] out.obj
```
Parsed via `argc < 5` guard, then `strcmp` on `argv[1]` for the operator.
Multiple inputs are supported (variadic Boolean) — each input file goes
into a vector via `loadMultipleFiles(files, in_coords, in_tris, in_labels)`.

**I/O formats:**
- Input: **OBJ** (per `loadMultipleFiles` and the README example invocation).
  Cinolib also supports OFF/PLY/STL via its own `read_*` family, but the
  shipped `main.cpp` is OBJ-coded.
- Output: **OBJ**, written via `cinolib::write_OBJ(file_out.c_str(), bool_coords, bool_tris, {})`.

**Dependencies (all bundled under `arrangements/external/`):**
- `oneTBB` — Intel parallelism library
- `Cinolib` — C++ mesh data-structure library [Livesu 2019]
- `Indirect_Predicates` — Attene's exact predicates library
- `abseil-cpp` — Google Abseil (Swiss-table hash maps per Cherchi 2022 §4)
- `parallel-hashmap`
- `yocto` (yocto-gl helper utilities)
- *Implicit:* Shewchuk predicates (referenced via `CINOLIB_USES_SHEWCHUK_PREDICATES`)
  — likely embedded inside the bundled cinolib

CMakeLists.txt patterns observed: `add_subdirectory(arrangements/external/oneTBB)`,
`find_package(cinolib REQUIRED)`. Each executable links: `cinolib`, `tbb`.
**No `find_package(CGAL)`, `find_package(Boost)`, no MPFR/GMP** — this is
significant: all dependencies are header-style or bundled, none come from
the system package manager. That removes the "needs Dockerfile changes"
escalation path entirely.

**Codebase scale:** ~75 commits, primarily C++ (97.9 %). Top-level files
visible from upstream: `arrangements/`, `code/`, `data/`, `CMakeLists.txt`,
`LICENSE`, `README.md`, plus five top-level driver mains (`main.cpp`,
`main-arap.cpp`, `main-inputcheck.cpp`, `main-rotation.cpp`, `main-stencil.cpp`).

**No git submodules** — `arrangements/external/` directory listing shows
six fully-checked-in subdirectories; nothing is a zero-size submodule
pointer that would require a separate `git submodule update --init`.

## 4. Wrapper requirements

Because a real CLI exists, the wrapper is shallow. Two pieces of glue work:

1. **OBJ writer in Rust.** Our existing diagnostic dumper
   (`yang_integration.rs:1272 fn dump_merged_mesh_as_stl`) writes binary
   STL. The Cherchi CLI consumes OBJ. Either:
   - (a) add a small `dump_mesh_as_obj` next to `dump_merged_mesh_as_stl`
     (≈30 lines of `vertex/face` ASCII writes — simpler than the STL
     binary path), or
   - (b) reuse `crates/wasm-bridge/src/stl_export.rs` and convert
     STL→OBJ at the comparison step (uglier, adds an external `meshlab`
     or `assimp` dependency to the test rig).
   Option (a) is preferred — pure Rust, no extra deps, ≈1 hour.

2. **Subprocess invocation + parser** in the test harness
   (`crates/test-harness/`). Runs `mesh_booleans <op> stage_a.obj
   stage_b.obj cherchi_out.obj`, then reads `cherchi_out.obj` back into
   `Vec<[f64;3]>` + `Vec<[usize;3]>` for the diff step. Path to the
   binary discoverable via env var `CHERCHI_SIDECAR_BIN` (CI sets it,
   local devs can override). Skip the test if the env var is unset, so
   the sidecar is opt-in for any developer who hasn't built it.

Total wrapper effort: ~0.5 day if (1a) is taken.

## 5. Diff metric — recommend the conformal-mesh oracle as the diff axis

**The PR-Y14a conformal-mesh oracle (`check_conformal`) is the right diff
axis** — not byte-identity of triangle lists, not face-count parity. The
diagnostic value of the sidecar is: feed both pipelines the same Stage-1
input (the bijective-tessellated mesh pair), run Cherchi to completion,
run our pipeline up to the first conformal-probe failure, then assert
`check_conformal(cherchi_output) == well_formed=true`. If the answer is
yes, that *proves* the pair of meshes is geometrically reachable and the
defect is in our port, not in the upstream algorithm or the tessellation.
If the answer is no, the upstream tool itself is unhappy with the input,
which redirects the investigation upstream to Stage 0 / Stage 1.

Two calibration cases also matter:
- A `pass-genuine` corpus case: both pipelines should produce well-formed
  output; difference = 0 unpaired edges in both.
- A `auto-union-failed` corpus case unrelated to coplanar overlap: both
  pipelines should produce well-formed output even on a case our pipeline
  fails to *interpret* topologically — confirms the defect is post-Cherchi.

**What we are NOT measuring** at first contact: triangle-set equality,
patch-id equality, ray-cast result equality. Those are downstream from
"is the mesh well-formed?" and only worth measuring once that question is
settled. Including them up front would inflate scope.

## 6. Decision matrix

| Scenario observed by reconnaissance | Recommendation | Effort estimate |
|---|---|---|
| Cherchi has a CLI + OBJ I/O + builds in <30 min on a clean box | **GO** — spec a 2-day sidecar PR (build + wrapper + 4 corpus cases wired) | 2 eng-days |
| Library-only (no CLI) | ADD a small `main()` that calls `customBooleanPipeline()` directly | +1 eng-day |
| CMake pulls in CGAL/Boost/MPFR requiring `apt install` | **ESCALATE** to user — needs Dockerfile changes | 1+ eng-day plus build infra review |
| **Disk-space risk realized (24 GB free, 99 % used)** — observed now | **PIN** the build to a fresh container image or alternate volume; do *not* perform the build inside `/home/claude/workspace` | Unblocks GO path; ~0.5 day to set up isolated build env |

**Observed scenario: row 1 + row 4 simultaneously.** CLI exists, formats
match, deps are all bundled (no apt). Disk-space is the only friction.

## 7. Decision deferred to a separate PR

PR-Y14a does not build the sidecar. It ships only:
- the `check_conformal` oracle (works internally without any sidecar),
- this memo,
- the findings memo (which uses only internal probe data).

The sidecar build belongs in **PR-Y14c** (or whatever the next-numbered
slot is after PR-Y14b's fix lands). That PR must:
1. Cite this memo and the disk-space observation; pick a build location
   *outside* `/home/claude/workspace`.
2. Vendor the upstream repo at a pinned commit SHA into
   `external/cherchi2022/` (not under `crates/`, not under `archive/`).
3. Add a top-level `Makefile` target or `scripts/build_cherchi.sh` that
   runs the CMake build into `target/cherchi2022/` (gitignored).
4. Add the OBJ writer (option 1a above) and the subprocess test harness
   (§4).
5. Wire one corpus case (F0002) end-to-end as an opt-in test
   (`#[ignore]` or env-var-gated), prove the round-trip works, then
   open expansion to F0004 + the two control cases.

The build decision is therefore **GO, conditional on choosing a build
location with adequate disk space**. Recommend the team-lead direct the
PR-Y14c implementer to coordinate with whoever owns the container/volume
provisioning before starting the vendor step.

---

## Build verified 2026-05-03 (PR-S1)

**Outcome:** GO confirmed. Sidecar built, smoke-tested, and fed F0002 —
producing a finding that supersedes the PR-Y14c spec's anchor.

### Build metrics

| Metric | Value |
|---|---|
| Repo size (shallow clone) | 147 MB |
| Build time, headless `mesh_booleans` | ~22 min wall (single-shot, `make -j$(nproc)`) |
| `mesh_booleans` binary size (Release, AVX2) | 827 KB |
| `mesh_booleans_inputcheck` binary size | similar |
| Disk free pre-build (under `/home/claude/workspace`) | 1.6 GB *(crisis level — required `cargo clean` of 60 GB to make room)* |
| Disk free post-build | 213 GB |
| Total Cherchi footprint at `/home/claude/cherchi2022/` | 159 MB (sources + Release build artifacts) |
| `make` exit code | 2 (non-zero — GUI demos failed to link due to missing OpenGL; HARMLESS, headless binaries built fine) |

The disk crisis (24 GB free per the original feasibility memo dropped to
1.6 GB by the time we were ready to build) was resolved with `cargo clean`,
which freed 60 GB of stale `target/` artifacts. The next 5–10 min of
`cargo test` runs after a clean rebuild costs the working set back; that
is acceptable. **Build location was kept at `/home/claude/cherchi2022/`,
outside `/home/claude/workspace`,** to avoid accidental commit of C++ sources.

### Smoke test (two-tetrahedra union)

```
[reference-parity] smoke union (two tetrahedra) :
    verts=10 tris=16 unique_edges=24 unpaired=0 multi_paired=0
    euler_chi=2 well_formed=true
```

Cherchi's output on a clean two-tetrahedra union: 10 verts (8 originals
shared, plus 3 new intersection-point verts after dedup of one), 16 tris,
χ=V−E+F=10−24+16=**2** (closed orientable manifold, exactly one shell).
**Well-formed.** The reference implementation works as advertised on
clean input.

### F0002 finding — supersedes all prior anchors

Fed F0002's preprocessed A and B meshes (each 16 verts × 32 tris,
~2.2 KB OBJ files at `/tmp/waffle_cherchi_parity_f0002/`) into:

**Cherchi's `mesh_booleans_inputcheck`** (single-mesh validator):

```
Manifold check:                   FAILED
Watertight check:                 FAILED
Local  Orientation check:         passed
Global Orientation check:         passed
Intersection check:               FAILED
```

Both A and B fail identically on 3 of the 5 checks. Cherchi 2022 §3
explicitly axiomatizes that the boolean pipeline assumes manifold +
watertight + intersection-free + well-oriented input. Our pre-Cherchi
mesh violates 3 of those 4 axioms.

**Cherchi's `mesh_booleans union`** (the actual boolean):

Ran for **6 hours at 99% CPU** before being killed manually. The paper
provides no graceful behavior on malformed input; in practice the
algorithm appears to loop indefinitely. The PR-S1 integration test now
caps subprocess runs at a 30-second timeout.

### Implication for the anchor chain

This single invocation invalidated **all four prior anchors** in the
PR12 → PR13 → PR-Y14a → PR-Y14b → PR-Y14c chain. The actual defect is
upstream of `subdivide_mesh_pair_full_cherchi` entirely — it is in
**tessellation** (and/or coplanar preprocess injection) producing a mesh
that violates Cherchi's input axioms. PR-Y14c spec at Cherchi LPI is
superseded; PR-S3 will write the replacement.

The reference oracle paid for itself on first invocation. Future
investigations should stand it up BEFORE the third anchor attempt.

