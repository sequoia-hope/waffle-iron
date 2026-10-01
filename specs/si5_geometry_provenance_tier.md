# SI5 — a solid records who placed its geometry, and the debug tripwires band at that tier

Status: **LANDED**, 2026-10-01. Increment of record for
`specs/step_import_si5_exact_analytic_ingestion.md` §5.5 ("the fix is
provenance, not a band"), which names this as its own checkpoint because it
changes a kernel-v2 core type. Sits between **C4a** (landed 2026-10-01) and
**C4b** (arc patches).

---

## 1. The defect, stated as a tier mismatch

`CURVED_SURFACE_DEBUG_TOLERANCE` (`validate.rs:230`, 1e-12 absolute) and
`planarity_band` (`validate.rs:88`, 1e-12 relative) are **construction-bug
tripwires**, and they say so:

> curved geometry is exact **by construction** (the assembler places rim
> anchors at `center + r·û`), so this is a construction-bug tripwire compiled
> only under `debug_assertions`, not a production gate.

C4a is the first producer of a full-circle edge whose coordinates were **not
constructed**. They were *asserted by a file somebody else wrote*, and that
file disagrees with itself: ABC `00000007_…` writes
`CIRCLE('', #107, 0.0910485145000000)` — ten significant digits, zero-padded —
against an anchor vertex of `0.0910485144535982` at fifteen. Two independent
roundings of one quantity, 4.6e-11 apart.

The validator has been telling the two tiers apart by a **proxy on the curve
form**: a `Curve::Arc` endpoint gets `import_band` (1e-9 relative), a
`Curve::Circle` anchor gets the construction band. That proxy was *sound* while
the only producers of a full circle were the arena's own constructors and
`recover.rs`'s exact re-mint. C4a is where it runs out.

### 1.1 Anchored — the whole class and nothing else

`ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 KV2_OFFSURF_PROBE=1 cargo test -p
test-harness --test si5_analytic --release -- --ignored --nocapture
ingestion_over_the_corpus`, at `366a3179` (C4a):

```
  ingested              64  (16.0 %)  -> 256 solids, 5533 faces
  already in vocabulary 75, of which ingested 64 (85.3 %)
```

All **11** in-vocabulary misses are `VertexOffSurface`, one site each, every
one a vertex against **its own circle** (never against a surface):

| site | n | residuals | band |
|---|---|---|---|
| `planar-circle-anchor` (`faces.rs:488`) | 10 trips / 9 models | 9.1e-12 … 4.8e-11 | 1e-12 |
| `cyl-canonical-vertex` (`faces.rs:665`) | 1 | 3.9e-11 | 1e-12 |
| `cone-vertex` (`cone.rs:151`) | 1 | 1.3e-12 | 1e-12 |

That is the entire gap between 85.3 % in-vocabulary success and 100 %. Against
`import_band` — the tier the same validator already applies to arc endpoints of
the same files — **every one of them is inside, with ≥20× margin** (§5.5
measured the whole population: 4 932 rim anchors, p99 4.0e-11, max 5.0e-11,
100 % within 1e-9).

### 1.2 Why this is not a band widening (P9/P10)

The forbidden move is loosening a gate so a case squeaks through. This is not
that, in three independent senses:

1. **No gate loses coverage.** The claim the tripwire was checking — *an
   anchor lies on its own circle* — is already held, for C4a's whole
   vocabulary, by three gates that are not `cfg`-gated at all. §4 measures
   that bracket as a sweep and pins it as a test. (This started as "add a
   production on-curve gate"; the sweep refuted the need and the gate was
   removed before landing — see §4.1.)
2. **No band moves for any existing producer.** The rule is a `max` (§3.2), so
   a band can only grow, and only for solids whose provenance is `Asserted` —
   a class that did not exist before C3/C4a. Every constructed solid and every
   boolean output keeps its current band, byte for byte.
3. **The tripwire keeps its teeth where its premise holds.** The same 4e-11
   defect on a *constructed* solid is still refused, and that is a pinned test
   (§5), not a hope.

What a band widening would have looked like: raising
`CURVED_SURFACE_DEBUG_TOLERANCE` to 1e-10. That is refused — it would blind the
tripwire on exactly the producers whose premise it states.

---

## 2. The reconciliation that cannot work, recorded

Snapping each anchor onto its own circle (`c + r·ĝ`, which the C4a seam
alignment already computes) would fix `planar-circle-anchor` and
`cyl-canonical-vertex`. It **cannot** fix `cone-vertex`: there the rim radius,
the half-angle and the apex are three independent roundings, and no choice of
anchor makes all three agree to 1e-12. Reconciliation is the wrong lever for a
tier mismatch — it repairs data to satisfy a band that was never meant for it.

---

## 3. The design

### 3.1 The core type

`kernel_v2::arena`:

```rust
/// Who placed a solid's coordinates — and therefore which band the
/// debug-tier geometric tripwires may hold them to.
pub enum GeometryProvenance {
    /// The kernel itself: the arena's constructors from closed form
    /// (primitives, extrude, revolve, sweep, pipe) or the boolean
    /// reassembly. Exact by construction, which is what the construction
    /// tripwires assert.
    Constructed,
    /// An exchange file somebody else wrote (`ingest_analytic`). Every
    /// coordinate is the file's own rounding, and the file may disagree
    /// with itself by more than a construction tripwire allows.
    Asserted,
}
```

A **field on `Solid`**, not a side table: every producer then has to say which
it is, at the point where it knows.

Within a `Constructed` solid the existing two-tier split by curve form stays
exactly as it is, because it is correct: `recover.rs`-minted canonical circles
*are* construction-exact (it checks its own seams at
`CURVED_SURFACE_DEBUG_TOLERANCE`), while Stage-4-relocated patch vertices are
f64-computed and already band at `import_band`. Provenance does not replace
that split; it adds the one distinction the form cannot express.

### 3.2 The band rule

Every debug-tier site computes its band as today, then floors it at the
solid's tier:

```
band = site_band.max(provenance_floor)

provenance_floor = 0.0                        for Constructed
                 = import_band(scale, p)      for Asserted
```

Monotone by construction: **a band can only grow, and only for `Asserted`
solids.** That is why this increment cannot regress any existing case, and the
rewrite tier is the measurement of it rather than the hope.

Sites that take the floor (all five face validators, reached from the single
dispatch in `validate_solid:461-490`):

| site | file | tier today |
|---|---|---|
| planar face vertices vs plane | `faces.rs:138,162` | construction |
| circle/arc centre vs face plane | `faces.rs:455` | proxy by form |
| `planar-circle-anchor` / `planar-arc-endpoint` | `faces.rs:488` | proxy by form |
| `cyl-canonical-vertex`, rim centre on axis, seam ruling | `faces.rs:665,679,706` | construction |
| torus / sphere loop vertices | `faces.rs:783,838` | construction |
| `cone-vertex` | `cone.rs:151` | construction |
| cylinder / cone patch sites | `faces.rs:995,1217`, `cone.rs:301,476` | already `import_band` |

### 3.3 Propagation — a lattice, failing loud

`Asserted ≥ Constructed`, and the join is taken wherever geometry meets:

- `ingest_analytic` → `Asserted`.
- `boolean_op(a, b)` output → `Asserted` if **either** operand is; else
  `Constructed`. A carried face keeps the file's numbers, so the output keeps
  the file's tier.
- `transform` / mirror / pattern copies → the source's provenance.
- every other constructor → `Constructed`, written at the literal.

**Failure polarity.** A producer that forgets to carry `Asserted` *tightens*
the band and gets a loud `VertexOffSurface` refusal. It can never silently
admit worse geometry. The lattice fails in the direction that reports itself.

---

## 4. What still holds the claim in production — measured, not assumed

Relaxing a debug-tier tripwire is only honest if the claim it was making is
held somewhere that always compiles. It is. Sweeping the two shapes of the
real defect over the `cylinder` fixture (r = 5 mm; the anchor pushed radially
off its circle, and the `CIRCLE` radius record coarsened) gives the exact
ownership map:

| residual | anchor pushed off its circle | radius record coarsened |
|---|---|---|
| 1e-13 … 3e-12 | accepted | accepted |
| 1e-11 … 1e-9 | **accepted** (the tier this increment assigns) | `CurvedGeometryMismatch` "rim circle radius disagrees with the surface" |
| ≥ 5e-9 | `AnalyticIngestUnsupported` "a rim needing a re-anchored seam shares its anchor vertex with another edge" | same |

So three PRODUCTION gates bracket the on-curve claim:

1. **`ingest` 1g, the on-surface gate** — a vertex off its own face's surface
   by more than `TAU_EVAL·(1 + ‖p‖∞)`. For a rim, the two surfaces whose
   intersection IS the circle, so moving an anchor off its circle moves it off
   one of them.
2. **The rim-radius agreement** in `validate_cylinder_face` /
   `validate_cone_face` — `|r_rim − r_surface| > 1e-9·r_surface`, which owns a
   coarsened radius record from 1e-11 at this scale.
3. **`ingest` 1e, the seam-anchor reconciliation** — a rim whose anchor cannot
   be placed on its own circle needs re-anchoring, and a re-anchor is refused
   when the vertex is named by anything else.

`production_gates_bracket_the_on_curve_claim` pins rows 2 and 3 of that table,
so a later loosening of either gate turns red here rather than silently opening
the window the tripwire used to cover.

### 4.1 The gate this increment started with, and why it was removed

The first implementation added a fourth gate — `ingest` step 1g(ii), every
boundary vertex on the curve of the edge it anchors, at the import band — on
the reasoning that §5.5 had found "a dimension nobody had measured" and
therefore nobody was gating. The sweep above refuted the second half: **inside
C4a's vocabulary that gate is unreachable.** Every full circle is a band rim,
so gates 2 and 3 fire first on every residual large enough to matter, in both
perturbation shapes and at every magnitude.

It was removed (along with its `AnalyticVertexOffCurve` variant) rather than
shipped. A gate that cannot fire is not coverage — it reads like coverage in a
review and in the error vocabulary, which makes it worse than nothing.

**C4b is where it earns its place**, and that is where it should land: an arc
endpoint can sit exactly on its planar face while lying off its own arc, with
no rim-radius agreement and no seam azimuth to catch it — the arc's centre and
radius are independent of the plane's records. Deferred to that checkpoint,
with this section as the reason.

## 5. Tests (each pins a claim this spec makes)

In `crates/test-harness/tests/si5_analytic.rs`:

1. `an_asserted_anchor_off_its_own_circle_ingests` — the ABC `00000007`
   geometry in miniature (anchor 4e-11 off its circle): ingests, and the solid
   says `Asserted`.
2. `the_same_defect_on_a_constructed_solid_is_still_refused` — the same arena
   with its provenance flipped to `Constructed` is refused `VertexOffSurface`.
   This is what makes §1.2(3) a fact rather than an intention.
3. `production_gates_bracket_the_on_curve_claim` — §4's table, so the bracket
   cannot be loosened silently.
4. `provenance_joins_through_a_boolean` — ingested ∪ constructed is
   `Asserted`; constructed ∪ constructed stays `Constructed`.
5. `provenance_survives_a_transform` — a moved copy of an ingested solid is
   still `Asserted`, and validates (which it could not if the tier were lost).
6. `ingestion_over_the_corpus` now **asserts** in-vocabulary success is 100 %,
   the claim its doc comment has made since C3. The next miss is a red test.

## 6. Measurement

`ABC_DIR=/tmp/abc/chunk0000 ABC_N=400`, release, before and after:

| | C4a (`366a3179`) | this increment |
|---|---|---|
| reach, 400 models | 64 (16.0 %) | **75 (18.8 %)** |
| solids / faces | 256 / 5 533 | **273 / 5 708** |
| in-vocabulary success | 64/75 (85.3 %) | **75/75 (100 %)** |
| `VertexOffSurface` refusals | 12 models | **0** |

The bookkeeping closes exactly: **12** models tripped the mis-tiered tripwire,
**11** of them in-vocabulary and all 11 now ingest; the twelfth was never
in-vocabulary and now reaches its real wall one stage later (`unsupported
curve: circular arc (C4b)`, 89 → 90). No new refusal class appeared.

`./scripts/test.sh rewrite` green — the measurement behind §3.2's claim that no
band moves for any pre-existing producer.
