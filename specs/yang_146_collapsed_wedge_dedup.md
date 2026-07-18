# #146 increment 3a — post-weld collapsed-wedge dedup at the I6 site

Task #146 / epic #169 Phase 3a. Parent spec:
`specs/yang_146_conformal_junction_sampling.md` §4 "Blocker (1)
CHARACTERIZED" (2026-07-18 crossing-provenance probe, commit 9c8745ca).
Read that section first — this spec builds exactly the "next increment"
it names.

## 1. Problem

Flush/chained operands carry INTENDED-EXACT contacts (vertex-on-vertex,
vertex-on-face) at sub-weld f64 authoring residue (measured 1e-18…5e-15).
The exact arrangement — correctly, upstream-faithfully — mints LPI
crossings that are sub-weld twins of an explicit vertex wherever a
triangulation edge passes near such a contact. The I6 weld
(`TAU_WORK·(1+scale)`, PR-KV10) then rightly fuses each twin cluster.

The un-handled consequence: a hair-thin strip of sub-triangles between
the fused twins collapses. Its interior sliver triangles either

- weld to a repeated-index triple → already dropped (the existing
  degenerate-drop arm, "mutual opposite edges cancel"), or
- **two surviving sub-triangles weld onto the SAME vertex triple with
  the same winding** → the I6 coincident-tri guard STOPs
  (`NonManifoldInput`) with no discrimination.

Measured instance (F0016, Extrude-3 union, gate-ON
`YANG_JUNCTION_SAMPLING_ENABLE`): raw tris `[98,84,41]` / `[83,41,98]`
(sources `(B,46)` / `(B,47)`), weld clusters `{41,42,43}` /
`{83,84,85}`; after welding both become the directed triple `(98,83,41)`
— same cyclic order, same surface label, sharing raw edge `(41,98)`,
tips 84/83 weld-fused. The welded triple is a near-degenerate sliver
(the three points are collinear to ~3e-5 relative).

The junction insertion AMPLIFIES this pre-existing class (3 → 33
sub-weld pairs on F0016) by densifying CDT edges near shared junction
corners, so P3a increment 3 (always-on) is gated on resolving it.

## 2. Contract

At the kept-tri compaction loop in `yang_rs::boolean()` (the "(4)"
block), after the weld + degenerate-drop + per-op winding flip and
BEFORE vertex remapping, a surviving triangle whose welded post-flip
triple coincides with an ALREADY-KEPT triangle's triple is **dropped
iff it is a collapsed wedge** — an exact structural signature, no new
tolerance (the only tolerance in play remains the existing I6 weld
band):

Let T_first be the kept representative and T_cur the candidate, with
raw (pre-weld) triples R_f, R_c, welded post-flip triples W_f, W_c, and
`la` triangle indices o_f, o_c. T_cur is a collapsed wedge iff ALL of:

1. **Same final winding**: W_f and W_c are cyclically equal (a genuine
   two-sided pocket — opposite winding — is NOT a wedge; see §4).
2. **Same surface label**: `la.surface[o_f] == la.surface[o_c]` (this
   also forces equal per-op flip decisions).
3. **Shared raw edge**: R_f and R_c share exactly 2 raw indices, and
   the two remaining tip indices are distinct raw vertices that weld to
   the same root (`weld[tip_f] == weld[tip_c]`) — the pair became
   coincident THROUGH the weld, tiling one strip side-by-side.
4. **Locally-connected provenance**: `la.source[o_f]` and
   `la.source[o_c]` are both single-valued, name the SAME input, name
   DIFFERENT parent triangles, and those parents tessellate the SAME
   B-Rep FACE of that operand (the `tri_face_a`/`tri_face_b` provenance
   maps already bound at the I6 site). A collapsed wedge is one surface
   strip folding shut inside one face; two independent coincident
   sheets (genuine non-manifold input) are different B-Rep faces — or
   carry no lineage at all (the a4 adversary class) — and still STOP.
   *Measured correction (first F0016 run): the spec's original stricter
   arm — parent triangles ADJACENT in the operand mesh — REJECTED the
   lead case (`parents-not-adjacent`): B's parents 46/47 share the face
   but no mesh edge, because the strip's shared raw edge is
   INTERSECTION-MINTED, not inherited from the parents. Face-level
   locality is the correct notion.*

On match: skip T_cur (do not push; do not allocate compact verts for
it), optionally logging under `NONMANIFOLD_SITE_PROBE`
(`i6-wedge-dedup` lines: kept/dropped raw triples, sources, and — when
the signature REJECTS — the reject reason, so a near-miss is
observable). On non-match: fall through unchanged — the post-loop I6
coincident-tri guard remains VERBATIM as the loud backstop
(`NonManifoldInput`), preserving the a4 adversary contract
(`m3_adversary.rs::a4_*`, whose mock `la` has `source: Vec::new()` →
reject reason `no-lineage`).

Multi-copy groups (3+ coincident survivors) resolve pairwise against
the kept representative; each copy must independently satisfy the
signature or the backstop STOPs.

## 3. Why dropping one copy is sound (and what still guards it)

The degenerate-drop arm's precedent one level up: a welded
repeated-index triangle is dropped because its directed edges cancel.
The two-triangle analog: same-winding coincident survivors double-cover
their three directed edges; keeping exactly one restores each directed
edge to a single copy on that sheet. Whether the SURROUNDING welded
complex then pairs every directed edge with its reverse is not decided
here — and deliberately so: the existing downstream half-edge-pairing /
2-manifoldness gates (Stage 3/5 reassembly, kernel-v2 validation, the
#173 selfx render gate, and the assay volume/χ oracles) all remain in
force. A wedge collapse this rule mis-handles fails LOUDLY downstream;
it cannot fail silently. (P10: the dedup can only convert a STOP into
either a correct result or a different loud STOP.)

Always-on (not env-gated): the rule is exact, structural, and the
collapse it resolves can arise in production today (the KV10 weld fuses
femto-twins gate-OFF as well — F0016's baseline survives by
triangulation luck, not absence of the class). §5 measures both gate
states; the 0-WRONG ratchet is the acceptance bar. This mirrors the
N55/N56 lesson: a paper-shaped reconciliation op (§4.3 point dedup, here
its triangle-level shadow) ships always-on with retightened criteria,
not banked behind a flag.

## 4. Non-goals (each deferred LOUD, not silently widened)

- **Opposite-winding cancel** (both copies dropped — a collapsed
  zero-volume pocket): no observed corpus case; the backstop STOPs.
  Add only against a measured instance.
- **Edge-level shadow** (F0084's fwd=1/rev=2 over-used edge at Stage-4
  reassembly): the same collapse class expressed one simplex down;
  needs its own signature at the half-edge-pairing site. Separate
  increment; F0084 is expected to KEEP failing gate-ON after this spec
  (its failing op never reaches the I6 guard).
  *RESOLVED OTHERWISE (2026-07-18, task #179): this framing was wrong —
  F0084's over-use entered on the OPERAND meshes (Stage-1 parity-flap
  zero-area triangles, spec `yang_stage1_cdt_parity_flap.md`); the
  flood-fill classifier migration fixes it and NO edge-level wedge
  resolution is needed. See the parent spec §4 correction.*
- **Input contact canonicalization** (snapping the 1e-18…5e-15 residue
  exact pre-arrangement): rejected — R0091-adjacent, N54-warned.
- No change to the weld itself, the keep-rules, or the arrangement.

## 5. Oracles & measurement plan

Unit (new `tests_unit/p3a_wedge_dedup.rs`, driving the extracted
signature fn directly):
- F0016-shape wedge → accepted (None);
- winding mismatch → `winding`;
- tips not weld-fused → `tips-not-welded`;
- shared raw indices ≠ 2 → `raw-shared`;
- same parent / cross-input / non-adjacent parents / multi-valued or
  missing lineage → each named reject.

Integration:
- `m3_adversary.rs` a4 guard tests stay green UNCHANGED (no-lineage →
  backstop STOP);
- full yang-rs lib + rewrite tier green.

Corpus (release assay, 312):
- gate-OFF vs committed baseline (250C/0W/55E + timeout flakes): NO new
  WRONG (hard abort if any — P10 revert to env-gated), no C→E
  regression; E→C conversions are wins to be individually verified
  against the committed per-case expectations;
- gate-ON (`YANG_JUNCTION_SAMPLING_ENABLE=1`): F0016's Extrude-3 union
  must pass the I6 site (dedup fires — observed via probe) and the case
  must end CORRECT or at a LOUD downstream gate (measured; the P3a
  ledger in the parent spec records the outcome either way);
- 0 WRONG in both gate states — non-negotiable.

## 6. Measured outcome (2026-07-18, SHIPPED always-on)

- Unit: 11 classifier fixtures green (incl. the measured §2.4
  correction); `m3_adversary` a4 guards green unchanged; yang-rs lib
  379 green; rewrite tier green.
- F0016 single-case gate-ON: the dedup fires exactly ONCE
  (`i6-wedge-dedup: DROP orig_t 248`, sources `(B,46)/(B,47)`) and the
  case is SUPPORTED_CORRECT — the gated I6 regression is fixed at the
  root, not by luck.
- Full assay gate-OFF: 251C/0W/55E/2T; the SOLE per-case delta vs the
  committed baseline is F0090 TIMEOUT→CORRECT (the known flake, flips
  with no code change). The dedup fires ZERO times gate-OFF —
  production behavior on the corpus is unchanged.
- Full assay gate-ON: 250C/0W/56E/2T; deltas vs baseline = F0084 C→E
  (the §4 edge-level shadow, expected and loud) + the F0090 flake. The
  P3a gate-ON regression set shrank {F0016, F0084, F0085} → {F0084}.
- 0 WRONG in both gate states — ratchet holds.
