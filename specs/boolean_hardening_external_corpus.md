# Boolean hardening: nightly fuzzer + external corpus ingestion

**Status:** DRAFT — inventory complete, plan proposed, awaiting a scope decision
on §7 (the SI5 fork). Created 2026-09-30.

**Motivation.** Declare the boolean pipeline done against a large, complex,
*independent* corpus before fillet/chamfer/shell work starts, so that latent
tolerance and intersection defects surface now rather than later as mysterious
fillet failures.

---

## 1. Inventory — what already exists

Surveyed before writing anything. Reuse is the default; this section records
what each existing piece is and whether it is reused, extended, or bypassed.

### 1.1 Assay — the regression corpus (REUSED AS-IS)

| Piece | Where | Role |
|---|---|---|
| Runner | `crates/test-harness/tests/assay_kv2.rs` (1942 ln) | `full_corpus_categorized` (all 321), `single_case` (one, by `ASSAY_CASE`), 13 always-on `smoke_*`, `smoke_corpus_boundary_categories` (~100 per-case category pins) |
| Verdict logic | `src/assay/categorize.rs::categorize` | Shared by the runner AND the prospector — one definition of `SupportedCorrect / SupportedWrong / Unsupported(r) / ExpectedError / Error / Timeout / SkippedSlow` |
| Corpus | `app/tests/cases/assay/` | 321 cases = `<ID>.waffle` (a real document) + `<ID>.meta.json` (`AssayMeta`: scale, ops, `OracleExpectations`) + `manifest.json`. R=100 randomized, F=94 featured, C=118 complexity, P=9 prospector promotions |
| Scheduling | `ASSAY_JOBS` subprocess pool, **CPU-time** budgets (`/proc/<pid>/stat`), `ASSAY_CASE_TIMEOUT_SECS` | Load-insensitive verdicts; a killable child per case |

**Current score: 310C / 0W / 7E / 4EE / 0T over 321 cases.** The 7 ERRORs are
loud-by-design C-series walls.

**Decision: Assay stays exactly what it is** — the fast, per-commit memory.
Nothing in this plan changes its format, its runner, or its budget model. New
findings enter through the existing P-series promotion path.

### 1.2 The oracle stack (REUSED — this is the crown jewels)

This is much stronger than a typical project has, and the whole plan is built
on top of it rather than beside it.

| Oracle | Where | What it actually checks |
|---|---|---|
| 9 mesh checks | `src/oracle.rs::run_all_mesh_checks` | watertight (T-junction aware), consistent normals, no degenerate tris, unit normals, face-range coverage, valid indices, outward normals, positive signed volume, **no self-intersection** |
| Euler | `oracle.rs::check_mesh_euler_characteristic_with_shells` | χ against target, shell-count aware |
| Exact membership | `src/assay/exact_membership.rs` (2402 ln) | **Mesh-free closed-form point predicate** re-deriving the document's solid analytically; `exact_volume_verdict` at 256³. Covers cut chains. |
| Independent volume | `src/assay/volume_oracle.rs` | Column-scan, exact in z, discretised in xy, golden-ratio column offsets; band **measured** (n vs 2n), never chosen |
| Independent topology | `src/assay/topology_oracle.rs` | Voxel cubical-complex χ of the set union of isolated operands |
| Reference parity | `src/cherchi_sidecar.rs` + `crates/cherchi-sidecar-rs` | Cherchi 2022 C++ `mesh_booleans` as a black-box oracle. **Binary is built and present.** |

Two known oracle defects, both carried forward as risks:
- **Exact-membership lattice is rotation-dependent on revolves** (6 KV6 cases
  read 16–70 % apart under rotation). Worked around by downgrading such rows to
  `ORACLE`, not fixed. §6 of this plan addresses it, because *transform
  equivariance is one of the invariants we are asked to check*.
- The sidecar read a spurious genus on R0053 because the *operand tessellation*
  contained an interior closed shell — sidecar disagreement is a prompt to
  adjudicate, never an automatic kernel defect.

### 1.3 The prospector — the fuzzer we have begun (EXTENDED)

`specs/assay_prospector.md` (416 ln) + `crates/test-harness/tests/prospect.rs`
(1070 ln) + `src/assay/prospect/` (3152 ln: `gen3`, `mutate`, `minimize`,
`metamorphic`, `promote`, `mod`).

Already built and directly reusable:
- **Seeded, resumable, subprocess-isolated search.** splitmix64, `(seed, index)`
  fully determines a candidate; CPU-budgeted killable children; `report.jsonl`
  append-only with resume-by-id.
- **Signature + dedup.** `signature()` strips numbers/uuids from the error tail;
  `wrong[<sorted oracle names>]`; one finding per signature.
- **Minimizer.** Truncate → drop middle step → un-snap → simplify profile →
  simplify op → round coordinates, each kept only on a byte-identical signature,
  budget `PROSPECT_MIN_STEPS`.
- **Promotion.** `promote::write_case` → next `P00NN` + manifest entry.
- **Metamorphic pairs.** explicit-axes, rigid motion, uniform scale.

Gaps that matter for this task (from the spec's own ledger plus this survey):

| Gap | Impact here |
|---|---|
| **P4 degeneracy snapping is a data-model stub** — `Step.snapped` exists, nothing sets it | This IS the task's "deliberately sample near-degenerate transforms". The single highest-value gap. |
| **No imported models.** `ModelBuilder` has no STEP entrypoint | Blocks external-corpus ingestion at the B-Rep layer entirely |
| **No pair + rigid-transform primitive.** Second operand is drawn from the first's AABB, never posed | Blocks "random pair, random relative transform" |
| Metamorphic: 3 of 6 identities (missing inclusion–exclusion, commutativity, de Morgan, boss order) | Three of these are explicitly requested invariants |
| Minimizer has no document arm; mutant/metamorphic findings cannot shrink at all | Shrinking requirement |
| TIMEOUT is not re-run at 4× before being called a finding | Budget artifacts become fake findings |
| Only **seed 1** has ever been run (≈40 generated candidates) | The search has barely started |

### 1.4 Other fuzzers already in the tree

- `crates/yang-rs/tests/fuzz_boxes.rs` (677 ln) — 900 random box booleans,
  correct-or-loud against the sidecar; 100 % correct.
- `crates/yang-rs/tests/fuzz_curved.rs` (1739 ln, `specs/yang_pr_cf1_*`) —
  N=300 curved fuzz. **Blocked** on `curved_fuzz_sidecar_zombie_blocker`.
- Proptest suites: `assay_generative{,_chain}.rs`, `assay_box_box.rs`,
  `assay_{chain_,}determinism.rs`, with committed `.proptest-regressions`.

### 1.5 STEP import (THE BLOCKER — see §2)

`crates/step-import/` (974 ln), truck git-pinned. `parse_step(text, name) ->
ImportedBodyData` produces **per-face triangle meshes plus an analytic
*classification*** (`Plane{origin,normal}` carries parameters; Cylindrical /
Conical / Spherical / Toroidal / Freeform do not). Roadmap
`docs/step_import_roadmap.md`: SI1 shipped, SI3 partial, SI4 first slice done.
**SI2 (booleans on imported bodies) and SI5 (exact ingestion) are unbuilt.**

### 1.6 Mass properties

- Exact: `kernel_v2::geom::signed_volume` (divergence theorem with an **exact
  `RBig` π coefficient**), `introspect::surface_area`, `geom::face_centroid`.
- Mesh: `test_harness::helpers::{mesh_volume, mesh_surface_area}`.
- Dual-path reporting already exists: `MeasureMethod::{Exact,Mesh}` with
  `exact_unavailable` — never an unlabelled approximation.
- **Missing: solid centroid (centre of mass) and inertia tensor.** Zero hits
  repo-wide for `inertia|center_of_mass|mass_propert`. Needed to consume
  CAx-IF/NIST centroid validation properties.

### 1.7 CI

Five workflows (`deploy`, `gui-tests`, `relay-tests`, `rust-lint`, `rust-test`).
All trigger on push/PR/dispatch. **There is no scheduled job anywhere** —
`grep -rn 'schedule\|cron' .github/` is empty. `rust-test.yml` has a 120-minute
cap and ends with the release-mode `assay_kv2` smoke gate.
`docs/TESTING-STRATEGY.md:143` states the standing policy: *"No Random Fuzzing
in CI — property tests use fixed seeds; random exploration happens locally."*
**A nightly fuzz job is a deliberate amendment to that policy** and this spec
is where it gets made: the nightly runs random seeds, but it never gates a
commit — it only opens a PR.

---

## 2. The blocker, stated plainly

> **The kernel cannot boolean an imported STEP model today.**
> `step-import` yields a mesh-backed `ImportedBody`; `KernelV2Adapter` returns
> typed `KernelError::NotSupported` for booleans on it. That is SI2, unbuilt.

So the literal task — *import two ABC models, transform one, boolean them
through the kernel* — is blocked at the import layer, not at the boolean.

But the pipeline is layered, and the layer where tolerance and intersection
bugs actually live is reachable right now:

```
Yang stage:   0 coplanar → 1 tessellate → 2 MESH BOOLEAN → 3 topology → 4 SSI refine → 5 assemble
              └───────── B-Rep, needs native solids (or SI5) ─────────┘
                                   ↑
                    cherchi-rs NativeBoolean::boolean(&Mesh, &Mesh, BoolOp)
                    takes ARBITRARY triangle soup — and every ABC model
                    becomes triangle soup via step-import, NURBS included.
                    Differential oracle: the Cherchi 2022 C++ sidecar,
                    same signature, binary already built.
```

**Therefore the corpus splits by layer, not by usefulness:**

| Layer | Operand source available today | Reference oracle | Exercises |
|---|---|---|---|
| **M — mesh** | **100 % of ABC** (10 000/chunk; NURBS tessellate fine) | **Cherchi C++ sidecar** | Cherchi 2020 §4 exact arrangement, Cherchi 2022 §5 ray-cast in/out — the exact-predicate core |
| **B — B-Rep** | authored parametric solids only | exact-membership + lattice | Stages 0, 3, 4, 5 — coplanar preprocessing, topology extraction, SSI refinement, assembly |
| **B+ — B-Rep on real CAD** | needs **SI5** (67 % of ABC is analytic-only) | exact-membership | all stages, on real-world geometry |

---

## 3. Corpus ingestion (deliverable 2)

### 3.1 ABC — measured, not assumed

Already downloaded and characterised:

- Index `https://deep-geometry.github.io/abc-dataset/data/step_v00.txt` →
  **100 chunks**, each a `.7z` on `archive.nyu.edu`. (Note: the task said "a few
  thousand chunks"; there are only 100, ≈10 000 models each ≈ the full 1 M.)
- Chunk 0000: 1.59 GB compressed, **13 GB / 10 000 STEP files** extracted,
  ~62 s download at 25 MB/s, <1 s extract. Server **ignores Range requests**
  (returns 200, whole file) — the downloader must not assume resume.
- Schema AP214 `AUTOMOTIVE_DESIGN`, advanced B-Rep.
- Per-model composition of chunk 0000 (measured over all 10 000):
  - **67.3 % analytic-only** (no b-spline / revolution / extrusion / offset surface)
  - 36.3 % multi-solid, 0.0 % with no `MANIFOLD_SOLID_BREP`
  - `ADVANCED_FACE` count p50 **40**, p90 359, p99 3608, max 146 520
  - file size p50 125 KB, p90 1.6 MB, p99 21 MB, max **540 MB**
  - surface entities over a 500-model sample: PLANE 55 932, CYLINDRICAL 49 417,
    B_SPLINE 41 738(+37 212 with-knots), CONICAL 11 732, TOROIDAL 7 044,
    SURFACE_OF_REVOLUTION 3 060, SPHERICAL 1 961, LINEAR_EXTRUSION 1 710

**Cache layout** (out of git, as required):

```
$WAFFLE_CORPUS_DIR                      # default ~/.cache/waffle-iron/corpus
  abc/archives/abc_NNNN_step_v00.7z     # as downloaded
  abc/step/NNNN/<8-digit>/<id>.step     # extracted
  abc/manifest.json                     # committed → crates/test-harness/corpora/abc.manifest.json
  nist/…                                # §3.2
```

The **manifest is committed** (model id + sha256 + chunk + face count + analytic
flag + byte size); the models are not. A fuzz finding cites a model id + hash,
and the hash is what makes the finding reproducible years later.

### 3.2 CAx-IF / ground truth

`cax-if.org` is behind Cloudflare (403 to us). The successor **`mbx-if.org` is
reachable**; its resources page links the public **NIST CAD models** (CTC-01..05,
FTC-06..11, MTC assembly). Sourcing is in flight; the deliverable is the set of
STEP files that carry `GEOMETRIC_VALIDATION_PROPERTY` (certified volume, surface
area, centroid) to use as ground truth on **import fidelity and our own mass
properties** — independent of any boolean.

This is also what motivates adding **solid centroid** to kernel-v2 (§1.6): we
cannot consume a centroid validation property without computing one.

---

## 4. Track M — mesh-layer differential fuzz (build first)

New: `crates/test-harness/tests/corpus_fuzz_mesh.rs` + `src/corpus/` module.

Per case, everything derived from one logged `u64` seed:
1. Draw model A and model B from the manifest (or A + a generated primitive).
2. Import both, weld faces into one indexed `Mesh` (STEP faces tessellate
   independently, so the shared boundary arrives duplicated — quantized weld).
3. Screen each operand with `cherchi_rs::census` → a **closed manifold** input,
   or it is an *input* finding, not a boolean finding.
4. Pose B by a random rigid transform + uniform scale, biased so the solids
   overlap: place B's centre inside A's bbox with a drawn overlap fraction.
5. **Degenerate poses on purpose** — snap to coincident faces, tangent contact,
   axis-aligned rotations, and offsets at {0, 1e-12, 1e-9, TAU_MODEL 1e-7,
   MIN_FEATURE 1e-6}·scale, signed. Reuses the ε ladder the prospector spec
   already defines for P4.
6. Run `union`, `intersect`, `subtract` **both orders**.
7. Oracle:
   - no panic (caught), no hang (CPU-budgeted child), no NaN/inf
   - output closed, manifold, consistently oriented, χ sane
   - **vol(A∪B) + vol(A∩B) = vol(A) + vol(B)**
   - **vol(A−B) = vol(A) − vol(A∩B)**
   - commutativity: A∪B ≡ B∪A, A∩B ≡ B∩A (volume + topology counts)
   - equivariance: `boolean(T·A, T·B) ≈ T·boolean(A,B)` for a random rigid T
   - **differential parity vs the Cherchi 2022 C++ sidecar** on canonicalized
     output (volume, χ, shell count, genus)
8. Classify: `IMPORT_FAIL | INPUT_NOT_CLOSED | PANIC | TIMEOUT | NOT_WATERTIGHT |
   INVARIANT(<identity>) | PARITY(<quantity>) | OK`.

**Import failures are counted and reported as their own bug class**, per the
task, and they are the direct input to SI2/SI5 planning.

Reuse: `cherchi_rs::{Mesh, census, NativeBoolean}`, `cherchi_sidecar::sidecar_boolean`,
`oracle.rs` mesh checks, the prospector's subprocess/CPU-budget/signature/resume
machinery (lifted to `src/corpus/` so both tracks share one implementation).

## 5. Track B — B-Rep pair fuzz (extend the prospector)

Not a new tool. The four prospector gaps that this task needs, in order:

1. **P4 degeneracy snapping** — implement the producer for the existing
   `Step.snapped` data model. The spec's own gate applies: *a snapped run's
   finding rate per candidate must exceed the generic run's, measured and
   recorded.*
2. **Pair + pose primitive** — a recipe step that authors an independent second
   body and places it by a drawn rigid transform with a controlled contact class
   (0-D vertex / 1-D edge / 2-D face / transversal / disjoint), instead of
   drawing its plane from the first body's AABB.
3. **The three missing metamorphic identities** — inclusion–exclusion,
   commutativity, de Morgan. (Rigid-motion and scale already exist.)
4. **Minimizer document arm + TIMEOUT re-run at 4×.**

## 6. The equivariance oracle defect (must be resolved, not carried)

Transform equivariance is a required invariant, and the strongest oracle we have
for it — the exact-membership lattice — is **known to move 16–70 % under
rotation on revolve geometry** because its lattice frame is the first sketch's.
Today that is detected and downgraded to `ORACLE`.

That workaround is acceptable for a score; it is **not** acceptable when
equivariance is the property under test, because it makes every rotation finding
unfalsifiable. Fix: evaluate the lattice in a frame derived from the *document's
own* geometry rather than the first sketch, and pin the six KV6 trackers
(C0059, C0061, C0062, C0064, C0066, C0069) as the acceptance test.

## 7. The scope fork — SI5

Tracks M and B are buildable now and cover the exact-predicate core and the
B-Rep stages respectively. Neither puts *real-world CAD* through the *full*
pipeline. Only SI5 does, and 67 % of ABC qualifies for it.

SI5 is kernel capability work (ingest plane/cylinder/cone/sphere/torus +
line/arc-bounded faces into the kernel-v2 arena as exact solids), not harness
work. It is a genuine fork in scope and is the one decision this plan defers to
the user.

**DECIDED 2026-10-01: take the SI5 fork.** Plan of record is
`specs/step_import_si5_exact_analytic_ingestion.md`. Two corrections to the
numbers above, both measured there over the same chunk:

- The 67 % figure is a **surface-only** gate. Adding the edge-curve gate the
  arena also requires (`LINE`/`CIRCLE`/`ELLIPSE` only) takes SI5's reach to
  **54.0 %** of the chunk — b-spline *edges* on otherwise-analytic models cost
  13.3 points that this §7 had not accounted for.
- Of those, **98.9 %** are already exact to the arena's own `import_band`, so
  SI5's reach is not gated on a reconciliation stage; the 1.1 % remainder is a
  loud refusal, and the mesh path keeps serving it.

Track B+ should therefore be planned against ~54 %, not 67 %, of each chunk.

## 8. Nightly (deliverable 5)

`.github/workflows/nightly-fuzz.yml`, `schedule:` + `workflow_dispatch`:
fresh seed each night, time-boxed, both tracks; logs seed + full config; shrinks
findings; dedups by signature; opens a PR adding new P-series cases with a
summary (cases run, pass rate by operation, new signatures, import failures).
**It never gates a commit.** Assay keeps running per-commit, unchanged.

Replay: `cargo test … corpus_fuzz_mesh -- --ignored` with `FUZZ_SEED=<seed>`
`FUZZ_INDEX=<i>` reproduces exactly one case, by construction.

## 9. Thresholds for "booleans done"

Deliberately not proposed yet. The task asks for concrete thresholds *after the
first run*, and proposing them before we have a base rate would be inventing
numbers. §10 records them once measured.

## 10. Measurements ledger

All from `crates/test-harness/tests/abc_probe.rs` (scratch probe) against ABC
chunk 0000, release build, this host.

### 2026-09-30 — corpus characterised (§3.1)

100 chunks exist, not thousands. Chunk 0000 = 10 000 models, 13 GB extracted,
67.3 % analytic-only, face count p50 40 / p99 3 608 / max 146 520, file size
p50 125 KB / p99 21 MB / max 540 MB.

### 2026-09-30 — import is not safe in-process

An uncapped in-process import loop reached **41 GB RSS and 14 CPU-minutes on a
single model** before being SIGKILLed. Import MUST run in a budgeted
subprocess. Two models out of 400 accounted for 57 % of import CPU (28.1 s and
27.5 s of 97.2 s).

### 2026-09-30 — import failure classes (400 models, ≤2 MB)

393 ok / 4 failed / 3 oversize. Two signatures:
- `panic: tolerance must be no less than 1e-6` ×2 — **truck panics rather than
  erroring** (`truck-geometry/src/specifieds/sphere.rs:134`), reached through
  `crates/step-import/`. A panicking import is a crash class and must be caught
  at the harness boundary.
- `STEP file contains no solids or shells` ×2 — clean typed error.

### 2026-09-30 — WELD SWEEP: loosening tolerance CORRUPTS (196 models, 12 pairs)

| weld | closed/196 | boolean NOT_WATERTIGHT | inclusion–exclusion violations |
|---|---|---|---|
| **1e-9** | 94 | 1 | **0** |
| 1e-7 | 103 | 3 | **1** |
| 1e-6 | 98 | 5 | **2** |

Closure does **not** improve monotonically (94 → 103 → 98: noise), while output
watertightness and the volume identity both degrade monotonically. A looser
weld merges genuinely distinct vertices and the boolean then returns
confidently wrong answers. **This is P9/P10 reproduced from scratch on external
data** — and it was only visible because the oracle checks an identity rather
than "did it return a mesh". **Weld stays at 1e-9.**

### 2026-09-30 — CLOSURE DIAGNOSIS: non-conformal discretisation, not a weld miss

400 models, weld 1e-9:

```
CLOSURE  analytic-only 179/268 (67%)    has-freeform 13/125 (10%)
open models: boundary-edge count p10=0 p50=77 p90=816 max=13351
boundary edges PER FACE  p10=0.00  p50=0.94  p90=20.72
```

Boundary edges scale **with face count** (p50 ≈ 1 per face, p90 ≈ 21), which is
the signature of two faces sampling a shared curve a different number of times.
No vertex weld can close that seam — the vertex counts do not match. Confirmed
by the split: models containing b-spline/revolution/extrusion/offset surfaces
close at **10 %**, analytic-only models at **67 %**.

Consequences:
1. The freeform third of ABC is effectively **unavailable** at the mesh layer
   through the current importer. Track M's usable corpus is the analytic-only
   subset plus whatever freeform models happen to close.
2. Even analytic-only loses a third. Those are precisely the models **SI5**
   would ingest exactly, where the problem does not arise: exact analytic faces
   never need their shared curve re-sampled consistently.
3. A "repair the mesh" stage would be a tolerance band by another name — see
   the weld sweep above. Rejected as a default.

### 2026-09-30 — first boolean signal (12 pairs × 3 ops, real ABC geometry)

32 OK / 3 ERROR / 1 silently non-watertight; **inclusion–exclusion 11/11 clean**
at weld 1e-9. Two distinct signatures in twelve pairs:
- `arrangement failed: DeepRecursionRequired { base_tri: 114, detail: DegenerateTpi }`
  (one pair, all three ops — loud, correct behaviour)
- a **union returning success with 244 non-manifold edges** — silent, caught
  only by the output census. This is the class the invariant oracle exists for.

Read: the boolean core holds the volume identity on real geometry wherever it
succeeds; failures cluster in degenerate configurations, as expected. Two
signatures per twelve pairs implies the search will not be starved for findings.
