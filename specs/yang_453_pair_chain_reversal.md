# Spec: §4.5.3 reversal sweep for PAIR-relocated (untyped) chains

**Status: FLIPPED ALWAYS-ON 2026-08-24 (same session). Pin: census fires
exactly at v66; acting, v66 then v67 collapse and R0028 goes
SUPPORTED_CORRECT. Corpus under the flipped default: 267C/0W/40E/1EE/0T
with THREE explained deltas — R0028 and R0025 (both recorded ring-fold
family cases) ERROR→SUPPORTED_CORRECT with all oracles passing, and R0032
ERROR→UNSUPPORTED(curved-profile) (its Stage-6 non-2-manifold double-cover
wall peeled; the chain advances to the recognized curved partial-patch
NotSupported boundary). Off-knob `YANG_453_PAIR=0` restores each case's
prior error (uuid-identical). R0044 — whose ledger row named this sweep as
its vehicle — measured census-quiet: no eligible pair site fires; its wall
is the §4-I9 `RelocationCrossedCarrierVertex` STOP at v8 (junction-adjacent
crease shape, next investigation recorded in its row). Deviation ledgered
RESOLVED as N59.** Companion to `yang_451_optimize_across_boundaries.md` §14–15
(which unmasked the customer) and `yang_453_junction_protected_collapse.md`
(whose victim-safety argument this arm inherits in a stronger form).

## 1. The gap (a §4.5.3 completeness deviation)

`sweep_reversed_intersections` implements Yang §4.5.3 for TYPED chains:
all-conic cycles by the angle band, mixed-cycle straight runs
(`LineSegment`-typed) by the band, mixed-cycle shared-conic sites by exact
parameter order (task #145). The torus block's population — intersection
edges relocated by `relocate_onto_implicit_pair`/`_triple`, which carry NO
`Curve` entry — has never been swept: an untyped site today always falls
into the shared-conic arm's `continue`. The paper's §4.5.3 scope is "after
optimization", surface-agnostic (Fig. 15's tangent is `n_A × n_B` — any
pair).

## 2. The measured customer (R0028, 2026-08-24 — every number live)

After §4.5.1 inc-4 repairs v64 to its exact torus∩plane∩cylinder junction,
R0028 fails at `TessellationFailed FaceId(29)`: the torus-side ring
(hole0 of the band CDT) SELF-INTERSECTS at its closure — segs 0×40 and
40×42 — the case's long-recorded fold shape. Anchor:

- Chain [39]=v118 → [40]=v68 → [41]=v67 → [42]=v66 → [0]=v64, all ON the
  torus∩cylinder curve (pair residuals ≲1e-9): cap-plane coordinates
  −5.5e-4 → −2.1e-4 → **+1.97e-4 → +2.02e-4** → 0. The tail overshoots
  PAST the junction and the closure folds back.
- MINTED BY RELOCATION, not Stage 2: v66/v67 ENTERED Stage 4 at cap
  −3.56e-5/−3.65e-5 (inside the face) and the pair-Newton moved them
  (rho ≈ 2.5e-4, corridor gate 6.9e-4) to the nearest point of the
  INFINITE pair curve — on the far side of the junction. This also
  retracts the 2026-08-04 "Stage 4 relocated NOTHING" exclusion for this
  case: `n_relocations` counts only conic moves; torus-block moves join
  `moved` — the instrument was blind exactly as its own caveat warned.
- The §4-I9 domain postcondition cannot see it: its leg (1) needs the
  crossed neighbour ON the pre→post segment (exact collinearity); these
  moves pass NEAR v64, not through it.
- Paper's Fig-15 test, run live: angle(t̃, n_T×n_C) = **91.4°** at [42]
  (legs cancel, |t̃|=0.046) — a reversal; 177–179° at [40]/[41]/ordered
  sites (anti-parallel = ordered; the band is two-sided by design).
- Resolution gate: d_ε = 2.964e-4, so the two collapses the sweep needs
  ([42]→[41] at 1.6e-5, then [41]→v64 at 4.9e-4) both pass 2·d_ε=5.9e-4.

## 3. Design

Inside the existing sweep's mixed-cycle branch, BEFORE the shared-conic
arm: if BOTH incident edges are UNTYPED (`curves.get == None`), the site is
a candidate **pair site**. Eligibility (all read from Phase-A `incidence`):

1. each edge's incidence dedups to EXACTLY 2 distinct `(input, surface)`
   entries with BOTH inputs represented (an intersection edge; ≥3 entries
   = a junction edge — no verdict), and
2. the two edges carry the SAME pair (order-independent).

Test (the paper's criterion in its general-surface form): at p_r,
`t = normalize(n₀ × n₁)` from `surface_value_and_normal` on the pair
(|n₀×n₁| < 1e-6 — Yang §5's angular tolerance — ⇒ near-tangential, no
verdict); `d1 = (p_r−p_b)·t`, `d2 = (p_n−p_r)·t`; **reversal ⇔ d1·d2 < 0**.
This is `conic_param_deltas`' progression-sign test with the tangent as
the local parameter — NOT the angle band, whose coarse-chord false
positives are P10-disproven for conics (spec §3c) and unmeasured here.

Victim/survivor mirror the #145 arm (branches 9a/9b): victim = p_r,
survivor = the tangent-nearer bracketing neighbour (|d1| ≤ |d2| ? p_b :
p_n). Everything downstream is the EXISTING shared path unchanged: the
2·d_ε resolution gate, `collapse_vertex`, restart-and-resweep, the
pass cap, `Stage4ReversalUnresolved` on no-progress.

**Junction safety is structural, not a check**: eligibility requires
p_r's two edges to carry the SAME 2-surface pair, so p_r is always
chain-interior — a junction vertex (≥3 inc0 surfaces, or a pair change)
can never be the victim. This is the same argument the conic arm records
("both edges at p_r carry the SAME curve, so p_r is never itself a
junction") with the pair identity in place of the curve identity.

## 4. Gate & measurements

`YANG_453_PAIR`: unset/`0`/`off` = arm dormant (byte-identical today);
`census` = print detected pair-site reversals, act on none; `1` = act.
Measurements before any flip: (a) R0028 pin under `=1` — expect [42] then
[41] collapsed, the fold gone, and the case's next honest wall named;
(b) R0003/C0065 unchanged; (c) full corpus under `=1` vs the committed
baseline — every delta explained or the flip waits. Flip flips unset→act
with `0|off` as the off-knob (the YANG_451 lifecycle).
