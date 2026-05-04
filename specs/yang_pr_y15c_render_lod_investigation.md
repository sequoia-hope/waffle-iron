# PR-Y15c — Render-LOD Stage E Phase-0 investigation

**Status:** INVESTIGATION SPEC (pre-FIP-§3.2). NOT a fix spec.
**Anchor empirical evidence:** `docs/audits/pr_y15a_validation.md` §6
(Step 7 half-edge construction empirically refuted via `TWIN_DEBUG=1`
data showing `paired=N, unpaired=0` on all 10 cases of F0031–F0040).
**Reproducer pair:** F0031 + F0040 (operand-order coverage; cluster
10/10 homogeneous per `pr_y15a_validation.md` §2).
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md`
sub-phase 0a.

---

## 1. Goal

Localize whether the well-formed Stage C conformal mesh becomes
non-watertight inside `tessellate_waffle_solid`'s render-LOD
retessellation path. Output: a Phase-0 instrumentation memo
(`docs/audits/pr_y15c_phase0_diagnostic.md`) that confirms or refutes
the long-banked PR14 Render-LOD anchor for the F0031–F0040 cohort.
Investigation only — no fix code. The fix spec (PR-Y15c-fix) is
written ONLY AFTER Phase 0 names the anchor.

## 2. Why this is not yet a fix spec

Per `~/.claude/projects/-home-claude-workspace/memory/feedback_anchor_before_fix.md`:
the F0002 / twin-pairing class consumed five wrong-anchor cycles
(PR12, PR13, PR-Y14a, PR-Y14b, PR-Y14c) before the PR-S1 sidecar
oracle pinned the actual defect. The strategic-escalation rule
("three wrong anchors in a row → stop bisecting, build a reference
comparison") fired at PR-Y14c.

The PR14 Render-LOD anchor is documented in
`~/.claude/projects/-home-claude-workspace/memory/yang_implementation_status.md`
(2026-05-02 entry: *"PR14 anchor = `tessellate_waffle_solid` Render LOD
per-face byte-identity defect"*). That anchor was banked as a working
hypothesis for R0020/R0021, never empirically confirmed for the
F0031–F0040 cohort. Writing a fix spec now would be a sixth
wrong-anchor cycle with non-trivial probability. Phase 0 instrumentation
prevents that.

**A15.6 cross-domain flag:** Render LOD lives in `tessellation::`
(`crates/kernel/src/tessellation/mod.rs:218` per the 2026-05-02 memory
entry). Per `governance/ARCHITECTURAL_INVARIANTS.md` A15.6, the Yang
Boolean pipeline scope ends at B-Rep assembly (step 7); render LOD is
architecturally OUTSIDE that scope. Phase 0 is observation-only and
crosses no boundary; PR-Y15c-fix may require cross-domain coordination
(deferred).

## 3. What PR-Y15a Phase 0 established

`docs/audits/pr_y15a_phase0_diagnostic.md` (implementer-e, 2026-05-02)
captured all 10 cases of F0031–F0040 firing decision-tree row 4
(Stage A/Bb/B/C all `well_formed=true`, Waffle still fails downstream
watertight). Stage C reports `verts=28 tris=48` for F0031; downstream
oracle measures `V=26 E=60 F=36` — the mesh shrinks by 2 verts and 12
tris between Stage C and the watertight check.

`docs/audits/pr_y15a_validation.md` §6 (adversary-2, 2026-05-04) ran
`TWIN_DEBUG=1` and observed `[topo-extract] summary: paired=N,
unpaired=0, ambiguous=0` on all 10 F0031–F0040 cases. **Step 7
half-edge construction is empirically innocent**, narrowing the
PR-Y15c candidate set from 2 anchors to 1: render-LOD retessellation
in `tessellate_waffle_solid`.

## 4. Phase 0 instrumentation requirements

### 4.1 Reuse `YANG_CONFORMAL_PROBE=1` (no new env var)

Same gate as Stages A/Bb/B/C (see `topology_extract.rs:36-75`
`emit_conformal_probe`). Stage E joins the existing probe family.

### 4.2 SKIP the Stage D probe

Quoting `pr_y15a_validation.md` §6.4 directive 1 verbatim:

> "Skip the Stage D probe — explicitly cite this validation memo's §6.
> The TWIN_DEBUG `[topo-extract] summary: unpaired=0` data is the
> Stage D signal."

The `[topo-extract] summary:` instrumentation gated on `TWIN_DEBUG=1`
already reports the post-Step-7 half-edge graph state on all 10 cases.
Re-implementing Stage D would duplicate signal already collected.

### 4.3 Add Stage E inside `tessellate_waffle_solid`

Per `pr_y15a_validation.md` §6.4 directive 2: *"Add Stage E at the
post-`tessellate_waffle_solid` render mesh."*

Site: `crates/kernel/src/boolean/yang_integration.rs:1022-1039` (the
`tessellate_waffle_solid` body). Probe fires after
`tessellate_solid_ext_with_lod` returns (L1037 area), inside the
function, so it captures every call regardless of which call site
invoked it. Stage names MUST be LOD-tagged to discriminate call sites:
`E_lod=Render` (the production caller at L1012) vs `E_lod=Adaptive`
(operand-mesh callers per the per-LOD dispatch).

Reuses existing helpers (no new conversion code):

- `render_mesh_to_arrays` at `yang_integration.rs:46-69` (already
  `pub(crate)`; converts `RenderMesh` f32 flat arrays → `(Vec<[f64;3]>,
  Vec<[usize;3]>)`).
- `check_conformal` at `oracles/conformal_mesh.rs:97-138`. The
  function internally canonicalizes verts at nanometer-quant precision
  per L118-138 (`QUANT_NANOMETER_SCALE`), so per-face vert duplication
  in `RenderMesh` is handled transparently *if* the f32→f64
  round-trip stays inside the quant grid.

**Open question for implementer-f:** f32 precision (~10⁻⁷ m) is ~100×
coarser than the nanometer-quant grid (10⁻⁹ m). f32 round-trip drift
may push two B-Rep-coincident verts into different quant cells and
cause a *false* `well_formed=false` reading on `RenderMesh`. Before
drawing conclusions about Stage E_lod=Render at the L1012 production
caller, implementer-f MUST verify on a known-good Stage E_lod=Adaptive
operand mesh that the oracle reads `well_formed=true`. If that
known-good baseline reads false, the oracle CANNOT process render-LOD
verts as-is and implementer-f must add explicit `dedup_mesh_vertices`
(`yang_integration.rs:1344-1376`) before `check_conformal`. Document
the outcome either way.

### 4.4 Anchor pre-verification canary

Per `~/.claude/projects/-home-claude-workspace/memory/feedback_anchor_before_fix.md`
(strategic escalation rule: three wrong anchors → reference
comparison; `tessellate_waffle_solid` is dispatch-hop-removed from
the test surface):

Before writing the real probe, insert at the planned site
(`yang_integration.rs:1037`, after `tessellate_solid_ext_with_lod`
returns):

```rust
eprintln!("[stage-e-canary] reached after tessellate_waffle_solid lod={lod:?}");
```

Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1` and confirm canary
fires ≥3 times per case (operand A + operand B + result). Remove
canary BEFORE the real probe lands. Anchor-canary fired ≠ anchor
correct, but anchor-canary unfired = anchor definitely wrong.

### 4.5 Reproducer harness

Reuse `batch_enclosed_subtract_fix` at
`crates/test-harness/tests/assay_randomized.rs:439-458`. F0031 + F0040
form the 2-case spot-check pair (operand-order coverage:
box-minus-enclosed-cyl vs cyl-minus-enclosed-box) per
`pr_y15a_validation.md` §6.4 directive 3. The full F0031–F0040 batch
output MUST also be captured for cluster-homogeneity confirmation
(directive 3 cites cluster homogeneity at 10/10 per validation §2).

**Capture command** (the libtest `--nocapture` quirk re-discovered
twice already per `pr_y15a_validation.md` §7):

```
TWIN_DEBUG=1 YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized --release -- \
  batch_enclosed_subtract_fix --ignored --nocapture --test-threads=1 \
  2>stderr_capture 1>stdout_capture
```

`--test-threads=1` and separated streams are MANDATORY — libtest's
`--nocapture` only releases stderr without stdout merging when single-
threaded with separated redirects.

## 5. Decision tree

| Stage E (lod=Render at L1012) | Anchor | Next PR |
|---|---|---|
| `well_formed=false` | `tessellate_waffle_solid` retessellation IS the defect (confirms PR14 anchor). Diagnostic MUST report exact vert/tri/edge delta vs Stage C. | PR-Y15c-fix targets `tessellate_solid_ext_with_lod` / fan welding / per-face byte-identity. Cross-domain coordination needed (A15.6). |
| `well_formed=true` | Defect is NOT in render-LOD topology. Candidates: `face_ranges` semantics, `RenderMesh` normals, watertight oracle's f32-quantize disagreeing with conformal oracle's nanometer-quantize. | PR-Y15d Stage F (face_ranges/normals integrity + watertight-vs-conformal reconciliation). |
| Stage E_lod=Render `well_formed=true` BUT Stage E_lod=Adaptive `well_formed=false` | Operand-mesh tessellation broken pre-Cherchi. Surprising; would invalidate Stage A invariants from PR-Y15a. | Re-examine Stage A; possible cohort re-segmentation. |

## 6. FIP role assignments

Per `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3.2 (spec writer
≠ implementer ≠ adversary):

| Sub-phase | Agent | Reads required | Writes |
|---|---|---|---|
| 0a Spec | spec-writer-d | governance docs (FIP §3, A15.6, DoD §6); MEMORY.md feedback memos; PR-Y15a spec (template); PR-Y15a validation §6 (bound directives); PR-Y15a phase0 diagnostic (cohort evidence); Yang 2025 §4.4-§4.5; Cherchi 2022 §5; `yang_implementation_status.md` 2026-05-02 entry | `specs/yang_pr_y15c_render_lod_investigation.md` |
| 0b Implement | implementer-f (NOT spec-writer-d, per FIP §3.2) | The spec from 0a; DoD §6; `yang_integration.rs:1022-1039` + L46-69 + L1344-1376; `oracles/conformal_mesh.rs:97-138`; `topology_extract.rs:36-75`; `assay_randomized.rs:439-458`; Yang 2025 §4.5; Cherchi 2020 §5 | Probe code + canary + diagnostic memo |
| 0c Adversary | adversary-3 (NOT spec-writer-d, NOT implementer-f) | All 0a + 0b deliverables; PR-Y15a validation memo (template); FIP §4 (adversary duties); Yang 2025 §4.4.2 | `docs/audits/pr_y15c_validation.md` |
| 0d Commit | team-lead | All 0a-0c | Memory updates, git commit |

## 7. Out of scope

- Fix code (PR-Y15c-fix follows ONLY after Phase 0 names the anchor).
- Stage D probe (TWIN_DEBUG existing instrumentation IS the Stage D signal per `pr_y15a_validation.md` §6.4 directive 1).
- Stage F probe (deferred to PR-Y15d if Stage E reports `well_formed=true`).
- R-class cases (separate cohort).
- PR-Y15b.1 follow-ups (residual F0002/F0004 I-axis at Yang §4.1.1 fan unification).
- TSV re-segmentation by failure-mode signature (PR-Y15a validation §5; separate work).
- WASM rebuild (probe is env-gated, default-off; production behavior unchanged; PR-Y15c-fix WILL need WASM rebuild).
- Cross-domain A15.6 coordination for the eventual fix (deferred to PR-Y15c-fix).
- Cherchi 2022 reference comparison for the render-LOD step (Cherchi has no render-LOD; Waffle-specific).
- R0071 kernel hang (separate defect class).
- Removing the deprecated S-H clipping pipeline (per A15.6, blocked on Yang being operational).

## 8. Phase 0 deliverable checklist

When Phase 0 is complete, implementer-f SHALL produce:

1. `crates/kernel/src/boolean/topology_extract.rs` — change
   `fn emit_conformal_probe` at L36 to `pub(crate) fn` (~1 LOC).
2. `crates/kernel/src/boolean/yang_integration.rs` — Stage E probe at
   ~L1037 inside `tessellate_waffle_solid` (~10 LOC), env-gated on
   `YANG_CONFORMAL_PROBE=1`, LOD-tagged stage names.
3. `docs/audits/pr_y15c_phase0_diagnostic.md` (~150 LOC):
   - Verbatim probe output for F0031 + F0040 (all stages
     A/Bb/B/C/E plus per-call-site E_lod tag).
   - Cluster-homogeneity table for F0031–F0040.
   - Decision-tree row determination.
   - Named anchor function (file + line range), even if "anchor
     unknown — escalate to PR-Y15d".
   - **Vert/tri/edge delta between Stage C and Stage E_lod=Render**
     (load-bearing — adversary-3 will reject pure assertion without
     the delta).
   - F32 round-trip verification outcome (per §4.3 open question).
4. Production safety verification:
   - `YANG_CONFORMAL_PROBE` unset → F0002 trace byte-identical to
     current main; 0 `[conformal-probe]` lines; 0 `[stage-e-canary]`
     lines.
   - `cargo clippy -p kernel --no-deps`: 91 warnings (post-PR-Y15a
     baseline); net delta MUST be 0.
   - `rustfmt --check` on `yang_integration.rs` and
     `topology_extract.rs`: clean.
