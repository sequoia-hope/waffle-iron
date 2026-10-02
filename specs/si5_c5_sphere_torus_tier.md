# SI5 C5 — spheres and tori

Status: **C5-M measured and C5a LANDED 2026-10-02** (§2, §4); C5b next. Checkpoint C5 of
`specs/step_import_si5_exact_analytic_ingestion.md` §7, following C4b
(`specs/si5_c4b_arc_patch_tier.md`). Split into increments by the forms the
corpus actually writes (§2), the way C4 was split by band vs patch.

---

## 1. What C5 is

C4b leaves every face whose surface is a `SPHERICAL_SURFACE` or
`TOROIDAL_SURFACE` at the 1a vocabulary wall. C5 admits them — and the
measurement below says that for the dominant form, the fillet band around a
circular edge, the work is **not** in ingestion: the arena validates such a
face today but cannot measure its volume, cannot render it, and would emit it
to the boolean pipeline as a different shape. Each of those three is a silent
wrong answer waiting behind a vocabulary wall, and C5 is the increment that
has to make them loud or correct before the wall comes down.

Reach at stake: **31 of 400** ABC models (7.8 points) on top of C4b's 37.0 %.

---

## 2. C5-M — the measurements, taken first (2026-10-02)

`ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 cargo test -p test-harness --test
si5_analytic --release -- --ignored --nocapture c5_sphere_torus_census`:

```
  text: models with a sphere or torus 160, of which with a VERTEX_LOOP (refused at C2) 43
  in C5 surface vocabulary 197 (49.2 %), of which with a sphere/torus face 31 (7.8 %)
  sphere/torus faces: {"sphere": 36, "torus": 183}
  forms:
    x2     sphere: band (closed circles only)            — 2 loops  [O|O]
    x8     sphere: mixed (closed circle + open edges)    — 2 loops  [CCCCCCCCCCCC|O]
    x26    sphere: patch (open edges only)               — 1 loop   [CCC]
    x62    torus: band (closed circles only)             — 2 loops  [O|O]
    x121   torus: patch (open edges only)                — 1 loop   [CCCC]
  torus CLOSED circles by kind: {"latitude": 124}
  torus ARCS by kind: {"latitude": 242, "poloidal": 242}
  sphere arcs by kind: {"great": 78, "small": 96}; sphere closed circles 12
  models by the forms they need:
    x16    torus band
    x6     torus patch
    x3     sphere mixed + torus band
    x2     sphere band
    x1     sphere patch
    x1     sphere patch + torus band
    x1     sphere patch + torus band + torus patch
    x1     sphere patch + torus patch
  sphere/torus BAND rims — what is across the two rims:
    x14    cylindrical band / cylindrical band
    x14    cylindrical band / planar
    x12    cylindrical band / planar band
    x8     cylindrical band / spherical
    x5     cylindrical band / toroidal band
    x6     cylindrical (NOT a band) / planar
    x3     conical band / conical band
    x1     planar band / toroidal band
    x1     toroidal band / toroidal band
  sphere/torus BANDS — sense seeds available in the rim component:
    x58    cyl/cone band + plane
    x6     plane only
  cylinder/cone band rims: FILE flag vs the C4a material law: {"DISAGREE": 60, "agree": 1484}
```

Six findings, each deciding a piece of the design.

### 2.1 The torus LATITUDE BAND is the tier — and no kernel path carries it

62 of 183 torus faces, needed by **21 of the 31** models: a torus between two
**closed latitude circles** (coaxial with the torus axis; 124 of 124 closed
circles on tori are latitude circles, zero are poloidal). It is the fillet
band around a boss or a hole, and it is a form **no kernel-v2 constructor
produces** — revolve and the B2 pipe build the *bent tube* (two poloidal
profile circles + longitude seam arcs), fillets are deferred, and the M5 torus
arm's boolean outputs are chord-polyline patches. Checked against the tree:

| path | what it does with a latitude band today |
|---|---|
| `validate_torus_face` | topology-agnostic — accepts it |
| `geom::signed_volume` | `torus_band_flux` assumes profile rims: refuses `rim radius disagrees with the minor radius` (loud) |
| `tessellate` | `face_has_circle_edge` ⇒ `tessellate_torus_lateral`, which looks for a +axis seam arc: `torus lateral missing its +axis seam arc` (loud) |
| `to_yang_brep` | the structured `(Circle, Arc, Circle, Arc)` arm matches it and emits a yang `Torus` face **without checking the circles are profile circles** — a latitude band would re-enter Stage 1 as a bent tube: **SILENT WRONG** |

So C5a is two capability additions (volume, render) and one P10 guard.

### 2.2 A torus band's rim sense cannot be derived from the band alone — and the file's flag is not the answer

On a cylinder two rims bound one band, so "each rim's traversal axis points
toward the other rim" fixes the sense (C4a). On a torus two latitude circles
bound **two complementary regions** (genus 1): the quarter-round between φ₁
and φ₂, or the three-quarter round the other way. The sense is load-bearing
information the band does not carry.

The obvious source is the file's own `ORIENTED_EDGE` flag. C4a refused to read
it on the argument that three independent signs (truck's curve inversion, the
`Processor` flag, `EDGE_CURVE.same_sense`) make it untrustworthy — an argument
from mechanism, never measured. **Now measured**: on 1 544 cylinder/cone band
rims where the C4a law gives the answer, the flag **disagrees 60 times
(3.9 %)**, with `same_sense` both true and false among the disagreements. The
flag is wrong one time in 26. It cannot seed a torus band.

What can: **every sphere/torus band's rim component contains a face whose own
law fixes the sense** — 58 of 64 via a cylinder/cone band, 6 via a plane only.
And the sense PROPAGATES across a torus band exactly: for the region running
+φ from rim *s* to rim *e* (either region), the start rim is traversed CCW
about `+â` and the end rim CCW about `−â` — opposite, whichever region it is
(derivation in `ingest.rs` 1d). So one rim fixed by a neighbor fixes the
other, and the region follows from which rim got `+â`.

The 6 plane-only components are all in one model whose torus band's other
neighbor is a **non-band cylinder** (a mixed form C4a refuses by name), so the
model is walled upstream of the sense question. Plane-seeded sense
(containment: outer CCW, ring CW) is therefore a **named refusal with no
customer**, not machinery — the C4a′/C4b rule applied again.

### 2.3 A sphere band IS self-seeding — but it is 2 models and not coaxial by law

Two disjoint circles on a sphere bound three regions and exactly one of them
has both as boundary (genus 0), so the C4a rule transfers: each rim's axis
oriented toward the other circle's centre is its traversal axis. The two in
the sample happen to be coaxial (a zone), but nothing requires it, and a
non-coaxial pair has no canonical seam curve. **Deferred by name** (C5c).

### 2.4 Patches are rectangles in parameter space; sphere patches are spherical triangles

Every torus patch is `CCCC`: 242 latitude + 242 poloidal arcs over 121 faces —
exactly two of each per face, a rectangle `[θ₁,θ₂]×[φ₁,φ₂]`. Not one
Villarceau arc. Sphere patches are `CCC` of great and small circles — the
three-fillet corner blend. Both forms reach the existing UV-CDT tessellators
(`tessellate_torus_patch`, `tessellate_sphere_patch`), and both are refused by
`signed_volume` (the torus by `torus_band_flux`, the sphere by having **no
sphere term at all**). That is C5b's work: two closed-form fluxes.

### 2.5 The windowed sphere (3 models) is C4b's windowed-patch refusal on a sphere

`[CCCCCCCCCCCC|O]`: a sphere patch with a closed-circle ring (a hole drilled
into a ball), 8 faces in 3 models, every one of which also needs a torus band.
Which loop is "outer" on a sphere is a convention the UV-CDT consumer needs
answered (pole bridging), and C4b already refuses a multi-loop curved patch by
name. **Deferred by name** (C5c); the typed refusal moves from 1a to 1c.

### 2.6 Forty-three sphere/torus models are already lost to `VERTEX_LOOP`

Of 160 text-level sphere/torus models, 43 carry a `VERTEX_LOOP` (a closed
sphere's poles, a cone apex) and are refused file-wide at C2 (spec §5.3,
truck drops the loop silently). C5 cannot recover them and must not be
credited with them; the reach ceiling here is the 31, not the 160.

---

## 3. Increments

### C5a — the torus latitude band (21 models)

**Ingest (`ingest.rs`).**
- 1a admits `AnalyticSurface::Torus` as `FaceSurface::Torus { center, axis_dir,
  major, minor, reversed }`; residual = `|√((ρ−R)² + τ²) − r|` (a length,
  unlike `geom::torus_residual`'s length²).
- 1c: a torus face is a band (two `Rim` loops, or our exporter's seamed
  form) or a refusal naming C5b (patch) / C5c (mixed, sphere).
- 1d, generalized into **seed + propagate**: cylinder/cone bands derive their
  own sense (unchanged); each torus band's rims are checked (declared axis ∥
  torus axis, centre on the axis, radius and height consistent with ONE
  poloidal angle φ on the declared torus, the two φ distinct); then, to a
  fixpoint, a torus band whose rim has a sense from the face across it takes
  the negation and gives its other rim the opposite sign about `â`. A band
  with both rims fixed by neighbors whose signs are not opposite is a refusal
  (the file's faces disagree about the region); a band with neither is the
  named plane-seed refusal (§2.2).
- 1e: the minted seam is a **poloidal arc**, not a ruling: centre
  `C + R·ĝ`, radius `r`, normal `±(ĝ × â)` so the arc runs +φ from the start
  rim (`+â` sense, `−â` when `reversed`) to the end rim — it lies on the face
  by construction. A file-supplied seam (`classify_seamed`) must be that arc,
  checked. The seam-azimuth re-anchoring is unchanged: `ĝ` is the component's
  shared azimuth, and a latitude rim's anchor direction IS an azimuth.
- 1f/1g/1h/2: unchanged (the `Arc` twin rule already covers the seam).

**Volume (`geom.rs`).** In the arc-bearing torus arm, a face whose `Circle`
rims are latitude circles takes the new closed form; profile rims keep
`torus_band_flux`. With `x = C + (R + r cos φ) ŵ(θ) + r sin φ â` and outward
`n = cos φ ŵ + sin φ â`: `x·n = C·ŵ cos φ + (C·â) sin φ + R cos φ + r`, the
`C·ŵ(θ)` term vanishes over a full turn, and

    Φ = ±(2πr/3)·[G(φ_e) − G(φ_s)],
    G(φ) = (C·â)(−R cos φ + (r/2) sin²φ) + (R² + r²) sin φ + Rr(φ/2 + sin 2φ/4) + Rrφ,

sign `−` for `reversed`; `φ_e − φ_s ∈ (0, 2π)` in the +φ direction from the
start rim. Check: a full turn gives `G(2π) − G(0) = 3πRr` ⇒ `2π²Rr²`.

**Render (`tessellate`).** A new `tessellate_torus_latitude_band`: a
`(θ, φ)` grid, θ wrapping over the full turn (the closed-torus row recipe),
φ from the start rim to the end rim; each rim row sampled **bitwise** with the
neighbour's recipe — `circle_frame(centre, −ν, anchor)` at `n_seg`, the
PR-KV7 rule the cylinder lateral uses — so the shared rim is watertight by
construction. Dispatch: a torus face with a latitude `Circle` edge takes it;
a profile `Circle` keeps the lateral; no `Circle` keeps the patch.

**Boolean (`to_yang.rs`).** The structured torus arm requires its two
`Circle`s to be **profile** circles (radius `r`, normal ⊥ axis); a latitude
rim is `UnsupportedCurvedBoolean` naming C5a. A tier that renders, measures
and exports but refuses booleans loudly is the deliverable (C4b §1.1).

**Oracles.** A first-party fixture — a *rounded puck*: a cylinder of radius
`R_c` and height `H` whose top edge is filleted at radius `ρ` — bottom disc,
cylinder band, torus latitude band (quarter round, `R = R_c − ρ`), top disc
of radius `R`; its volume is closed-form by Pappus,
`πR_c²(H−ρ) + πR²ρ + π²ρ²R/2 + 2πρ³/3`, and the torus centre sits at
`z = H − ρ ≠ 0` so the `C·â` term is exercised. Pinned: exact `(V,E,F,R,S,G)
= (3,5,4,0,1,0)` with two minted seams; the volume; the sense NOT read from
the file (flip every rim flag — same solid); the two-plane bead refused by
name; `to_yang_brep` a typed wall; the tessellated mesh's divergence volume
agreeing with `signed_volume`; export → extract → ingest a fixed point (the
seamed form with an arc seam). Then the corpus census, with reach and the
refusal buckets re-measured.

### C5b — torus and sphere patches (8 + 4 models, overlapping)

Ingest: `FaceSurface::Sphere` + residual; both surfaces take the C4b patch
path (one loop of arcs, sides from `interior`). Volume: the torus rectangle
by the parameter-domain Green identity (`∫∫ g(φ) dθ dφ = ∮ θ·g(φ) dφ`, only
poloidal edges contribute), the sphere by `Φ = (1/3)[r·A + C·½∮x×dx]` with
`A` from Gauss–Bonnet over circular arcs. Render: the existing UV-CDT paths,
verified on the corpus. Boolean: both already typed walls.

### C5c — named refusals, no customer yet

Sphere band (§2.3, 2 models); windowed sphere (§2.5, 3 models); torus band
with poloidal rims (the bent tube our own exporter writes — accepting it is
the pipe fixed-point oracle, worth doing when the pipe round-trip is needed);
Villarceau arcs (0 faces); plane-only seeded bands (§2.2, walled upstream).

---

## 4. C5a outcome (2026-10-02)

| | C4b | C5a |
|---|---|---|
| reach, 400 models | 148 (37.0 %) | **160 (40.0 %)** |
| solids / faces | 518 / 10 233 | **533 / 10 440** |

+12 models; the sphere/torus-bearing models still refused wall only on the
named C5b/C5c forms (`spherical (C5b)` ×9, `toroidal patch (C5b)` ×5) or on
C4b's named refusals in another shell. In-vocabulary success stays 100 %; no
new validation-tier finding (`00000062` face 35 remains the only one).

### 4.1 What landed, against §3

Ingest exactly as designed (1a torus, `torus_rim_phi`, the seed + propagate
1d with its two refusals, the poloidal-arc seam in 1e, a file-written seam
checked against that arc). `signed_volume`'s latitude closed form and the
`(θ, φ)` render grid share one region reader
(`geom::torus_latitude_band_phis`), so the two paths cannot disagree about
which region a band is. `to_yang_brep`'s structured torus arm now refuses a
latitude rim by name. One correction to the first cut: the latitude detector
must be `all` circles coaxial, not `any` — the CLOSED torus carries a coaxial
equator seam beside its profile circle and would have been misrouted.

Pinned (`ingest::tests`): the rounded puck at `(3, 5, 4, 0, 1, 0)` with its
Pappus volume to 1e-12 and the seam arc's exact centre; the boss-on-plate
(concave fillet, `reversed`, a ring loop) at `(5, 8, 6, 1, 1, 0)` and its
Pappus volume; the tessellated mesh within 3e-3 of both; every rim flag
flipped ⇒ the same solid; the two-plane bead and the contradicting-neighbours
shell refused by name; `to_yang_brep` the typed wall; a torus patch named
C5b. In the harness, `rounded_puck.step` (written by the ingest path, since
no constructor can) is a fixed point through export → extract → ingest with
its arc seam, and matches its own re-ingest on topology and volume.

### 4.2 Spec §8 oracle 5, run for the first time

`ingested_volume_agrees_with_the_mesh_tier` — the exact arena volume against
truck's own tessellation of the same file, two paths that share nothing:

```
  planar (C3)            n=28   rel |exact − mesh|: p50 4.00e-16  p90 6.76e-15  max 3.06e-14
  curved (C4a/C4b)       n=120  rel |exact − mesh|: p50 1.39e-3   p90 5.41e-3   max 7.93e-2
  torus-bearing (C5a)    n=12   rel |exact − mesh|: p50 2.94e-3   p90 6.16e-3   max 3.41e-2
```

The planar tier agrees to rounding; the curved tiers to the chord band at
p90. The two outliers above 2e-2 were anchored with `si5_volume_probe`
(per-face fluxes from a fine tessellation of ours beside truck's, and the
poloidal range truck's vertices occupy on each torus against ours):

- `00000251` (7.93 %) is a plain cylinder truck meshes as a **9-gon** —
  18 triangles; the inscribed-area deficit `1 − (9/2π) sin(2π/9) = 7.93 %`
  matches the deviation to three digits, on the lateral and the caps alike.
- `00000103` (3.41 %) has 17-gon cylinders (2.3 %) and four quarter-round
  fillets at one chord row each; on **all four** — two convex, two
  `reversed` — truck's vertices span exactly our `(φ_s, φ_e)`
  (`−π/2..0`, `π/2..π`, `π..3π/2`, `0..π/2`). The region is right; the
  residual is the chord.

So the oracle's resolution on real parts is set by truck's coarseness, not
by ours — which is why the fixtures carry the exact claims and the corpus
carries the independence.

### 4.3 Left open, by name

- The windowed sphere, the sphere band, the bent tube with poloidal rims and
  the plane-seeded sense: §3 C5c, each a typed refusal.
- `validate_torus_face` stays topology-agnostic: nothing in the validator
  checks a torus band's winding the way `validate_cylinder_patch` checks a
  cylinder's. The band's region is established upstream (1d) and read by two
  consumers through one function; a validator-side law would be a P10 net,
  not a capability.

---

## 5. Ledger

- 2026-10-02 — **C5-M.** `c5_sphere_torus_census` written and run; §2
  recorded. The decisive numbers: the latitude band is 21 of 31 models and
  no kernel path carries it; the file's orientation flag is wrong on 3.9 %
  of rims where the truth is known, which closes the "just read the flag"
  option with a measurement rather than an argument; every band's rim
  component has a derivable seed. Next: **C5a**.
- 2026-10-02 — **C5a DONE.** §4. Reach 37.0 % → 40.0 %; the band in all
  three kernel paths that lacked it; the first corpus run of oracle 5, with
  both outliers anchored to truck's chord coarseness and every fillet's
  region confirmed against truck's own vertices. Next: **C5b**.
