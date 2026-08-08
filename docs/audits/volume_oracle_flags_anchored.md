# The 5 Volume-Oracle Flags, Anchored: ALL FIVE ARE KERNEL SILENT-WRONGS

**Date:** 2026-08-08 (follow-up to the oracle build, commit 4cf0f131).
**Verdict wanted:** per flagged case, "the flag is the oracle's composition bug"
vs "the kernel's output is geometrically wrong." The first sweep's base rate
warned both ways (3 of the first 4 flags were oracle-side composition bugs).

**Result: 5/5 are kernel-side.** Two catastrophic (a union that discards its
target body), three deficit-class (material lost at revolve-union steps). The
oracle survived adversarial review on every case; no oracle bug found.

## Method

Three independent measurements per case, none sharing code with the verdict
under test:

1. **In-context volume trajectory** (`examples/volume_trajectory.rs`, new):
   rebuild the document truncated after each op, tessellate ALL live bodies,
   report per-body volumes. No operand isolation involved — this is the
   kernel's own account of its own chain, so composition-semantics doubts
   about the oracle cannot explain what it shows.
2. **Integrator cross-check:** divergence-theorem volume vs the oracle's
   column-scan winding sweep on the SAME operand meshes. Two integrators,
   zero shared code. Agreement ≤ 1e-4 rel on every operand of every case.
3. **Analytic anchor (Pappus)** for the circle-revolve operands, computed from
   the `.waffle` sketch + axis parameters alone (no kernel, no oracle):
   - R0057 op1: V = θ·R_c·πr² = 2.3287e5 vs kernel operand 2.3252e5 ✓ (chord-level)
   - R0059 op1: V = 4.7995e6 vs kernel operand 4.7922e6 ✓

Composition semantics are airtight for these five: every op is `merge:true`
boss with ABSOLUTE parameters (plane from datum-anchored sketch record;
extrude depth; revolve axis_origin/axis_direction/angle in world coords), so
an isolated operand is definitionally the in-context tool. The union of the
operands is then the unique correct output set, and the oracle computes that
union exactly per column over certified operand scans.

## Per-case verdicts

### R0090 — KERNEL WRONG, −58% (base body discarded by a union)

```
after op 0: bodies=1 live total=1.7412e8   [base circle tower]
after op 1: bodies=2 live total=2.0102e8   [gear 2.6903e7, base 1.7412e8]
after op 2: bodies=2 live total=1.2375e8   [op2-tool-alone 9.6849e7, gear 2.6903e7]
```

Op2 is a `merge:true` union. Its output equals the TOOL ALONE (9.6849e7 —
byte-close to the isolated op2 solid), and the 1.7412e8 base body is gone.
`vol(A∪B) ≥ vol(A)` is not a tolerance question; the output is impossible.
Final output = op1 ∪ op2 exactly (1.23752e8 vs 1.23747e8 measured).

### R0030 — KERNEL WRONG, −30% (same mechanism, tiny scale)

Same signature at scale 1.78e-4: base 3.667e-13 discarded at op2's union;
output = op1 ∪ op2 (8.5481e-13 vs 8.5488e-13).

Shared shape of both cases: three stacked towers (every sketch plane is an
`EndCapPositive` datum — face-on-face COPLANAR contact), op1's tool became a
standalone live body despite `merge:true`, and op2's union then discarded the
base. The composition is the M8 coplanar-contact class — the same family
F0064/F0072 STOP loudly on, here returning tool-only SILENTLY.

### R0040 / R0057 / R0059 — KERNEL WRONG, deficit class (−2.8% / −1.3% / −1.0%)

Every deficit enters at a REVOLVE union step (R0040 op2, R0057 op1, R0059
op1). Operand solids certified by both integrators (+ Pappus where the profile
is a centered circle); the exact per-column union of those operands exceeds
the kernel's merged output. Wrong-face-survival scale material loss at curved
union seams. All three flags are invariant to 100× finer oracle tessellation
(measured 2026-08-08), ruling out chord error.

## The three masks that let these grade SUPPORTED_CORRECT

1. **The categorized runner never checks the meta's `volume_monotonicity`
   oracle.** `assay_kv2.rs` reads euler/watertight/bbox/magnitude/
   expected_volume — the per-op direction oracle in every meta is dead data on
   this path.
2. **The v2 property pipeline downgrades I9–I12 volume-invariant failures to
   advisory PASSES** (`assay/properties_v2.rs:261-274`, "known-class issues
   that shouldn't block the test suite") — a P9/P10 violation in the harness
   itself: it is a standing decision to accommodate wrong boolean volume.
3. **`distinct_solid_count()` reports the REGISTRY's body count, not the live
   geometry's.** R0090/R0030 hold 2 live bodies from op1 onward while the
   registry says 1 — the auto-union bookkeeping records a merge the geometry
   never got, so the "merge incomplete" check is blind exactly when the merge
   machinery is the thing that failed. **Fixed 2026-08-08** (this session):
   the check now takes `max(registry, live)` and reports both.

Mask 3's registry/live disagreement is not just a mask — it is a defect
signature of its own (the engine believes a merge happened; the geometry
disagrees) and is likely one anchor of the base-drop mechanism.

### Mask-3 code anchor + base-drop mechanism hypothesis (read-only recon)

`feature-engine/src/rebuild.rs` CombineMode::Add (≈:1236-1345): the fold reads
the kernel union's output count as a disjointness proof — `1 output ⇒ merged
(target lump CONSUMED, replaced by the result)`, `>1 ⇒ disjoint (keep the
original lump as a leftover)`. Leftovers are emitted under `Body{index}`
output keys; `workflow.rs distinct_solid_count()` counts only features with an
`OutputKey::Main` output — so leftover standalone bodies are INVISIBLE to the
registry count while fully live for tessellation. That is mask 3, confirmed in
code.

The base-drop then needs exactly one kernel-side wrong answer: at op2,
`boolean_union(base_tower, stacked_tool)` returning a SINGLE output that is
the TOOL ALONE. The engine trusts the single-lump shape, consumes the base
feature, and its geometry is gone — while op1's gear survives as a
`Body{index}` leftover. This reproduces every measured number (live=2,
registry=1, output = op1 ∪ op2). **One measurement short of confirmed:** call
the kernel boolean directly on the two operand solids and inspect the output
lump — the first task of the mechanism session.

## The full-corpus measurement: 29 cases unmask, not 2

Re-running the categorized assay with the live-body merge check (347s,
2026-08-08): **232C / 29W / 47E / 0T** (was 261C/0W/47E/0T). All 29 new
SUPPORTED_WRONGs are multi-op cases holding ≥2 live bodies behind a registry
count of 1 — `merge incomplete: N operations produced 2 separate solids
(registry 1, live 2)`:

> C0042 C0051 C0055 C0057 C0110 · F0011 F0012 F0013 F0014 F0015 · R0001 R0006
> R0010 R0030 R0052 R0055 R0061 R0062 R0066 R0068 R0069 R0073 R0076 R0078
> R0082 R0083 R0088 R0090 R0097

Two sub-classes, split by volume conservation:

- **base-drop** (R0090, R0030 — anchored above): the union returned the tool
  alone; target material LOST.
- **silently-unfused** (spot-checked R0001, F0011): both bodies live with
  volumes conserved — the `merge:true` union quietly never happened; the
  registry recorded a merge anyway. A union of solids sharing a 2D contact
  face is ONE solid; not fusing is wrong (or a loud coplanar STOP, as
  F0064/F0072 correctly do).

The 3 deficit-class wrongs (R0040/R0057/R0059) still grade CORRECT — only the
independent oracle sees them — so the honest count of known-wrong passing
cases is 232C **minus 3 known-wrong** pending the oracle's integration into
the assay.

## What this changes

- The **0-WRONG headline was an artifact of oracle coverage**, exactly as the
  2026-08-06 review feared: the first absolute geometric oracle over the F/R
  passing population found 5 wrongs in its first sweep (of 121 covered), and
  the harness fix those 5 motivated unmasked 24 more.
- **Priority inversion:** confirmed silent-wrongs outrank loud ERRORs (P10).
  The base-drop class (R0090/R0030) is now the top kernel item, ahead of the
  ERROR-tail epic; the revolve-deficit class follows it.
- The masks get their own increments (monotonicity into the categorized
  runner; advisory downgrade retired after a census; registry/live
  disagreement anchored in feature-engine).

Next steps and ordering: `docs/yang_functional_roadmap.md` §0 (2026-08-08
addendum) and `specs/yang_441_trim_cdt_construction.md`.
