# Literature Check — `crates/cherchi-rs` & `yang-rs` Stage-2 Boundary vs. Cherchi et al. 2022

**Audited commit:** `a2058a0151ceea120ee7be2543075dd5b49c7b55` (rebased onto `main`, branch `agy/lit-cherchi2022`, 2026-10-27)
**Primary paper:** Gianmarco Cherchi, Fabio Pellacini, Marco Attene, and Marco Livesu. 2022. *Interactive and Robust Mesh Booleans.* ACM Trans. Graph. (`arXiv:2205.14151v1`, May 2022) — extracted text `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt` (854 lines; two-column `pdftotext` layout — line citations below note `L` / `R` where left and right columns share line numbers).
**Foundational companion papers invoked by Cherchi 2022:**
- Cherchi, Livesu, Scateni, Attene 2020 (*Fast and Robust Mesh Arrangements Using Floating-Point Arithmetic*) — `refs/text/mesh_arrangement.txt` (see companion audit `docs/audits/cherchi2020_lit_check.md`)
- Attene 2020 (*Indirect Predicates for Geometric Constructions*) — `refs/text/attene-predicates.txt`
- Yang, Jia, Wang, Yang, Xin, Yan 2025 (*Hybrid B-Rep and Mesh Booleans*, consumer spec for Stage 2) — `refs/text/yang2025_hybrid_boolean.txt`

**Prior audits superseded in full by this document:**
- `docs/audits/yang_audit_c_cherchi2022.md` (2026-04-30; audited deleted `crates/kernel/src/boolean/cherchi/*`)
- `docs/audits/cherchi_port_audit.md` (2026-04-28; audited deleted `crates/kernel/src/boolean/cherchi/*`)

**Method & rules:** Read-only static literature check (zero builds or test runs executed), updated with independent reviewer verification (2026-10-27). Every `FAITHFUL` row quotes verbatim **both** the governing paper statement (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:<lines>`) and the satisfying code (`path:line` at `a2058a01`). Runtime questions are marked `needs measurement:` with exact reproduction commands. No ledger entry is marked `PERMANENT`.

---

## Status Vocabulary & Summary Tally

| Status | Meaning | Paper rows (`C22-01..36`) | `yang-rs` consumer rows (`YC22-01..08`) | Combined (`44` rows) |
|---|---|---:|---:|---:|
| **`FAITHFUL`** | Implements the paper's procedural mechanism; verbatim paper + code quoted | 15 | 4 | **19** |
| **`PARTIAL`** | Implements core mechanism with an unclosed edge-case bug, missing sub-step, or scope restriction | 12 | 2 | **14** |
| **`DEVIATES`** | Substitutes a different mechanism or alters a procedural rule of the paper | 4 | 1 | **5** |
| **`ORDER`** | Implements the mechanism at a different pipeline stage than specified | 0 | 1 | **1** |
| **`MISSING`** | Procedural mechanism described in the paper is absent from `cherchi-rs` | 2 | 0 | **2** |
| **`N/A`** | Interactive GUI demo, external benchmark dataset, or paper's own future-work item | 3 | 0 | **3** |
| **Total** | | **36** | **8** | **44** |

---

## 1. Paper-Ordered Compliance Matrix — Cherchi et al. 2022 (`C22-01` .. `C22-36`)

### §1–§2 Pipeline Framing, Numerical Model & Snap Rounding

#### `C22-01` — §1 / §2.2 / §3 Two-stage surface-based Boolean pipeline
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:51-56 R, 85-90 L, 121-123 R, 246-256 R`):**
  > *"In many existing methods, the calculation of a mesh Boolean is framed as a two step process. In the first step, conflicts between mesh elements are resolved, splitting triangles in order to incorporate intersection lines in the connectivity. In the second step, each mesh element is deemed as being inside or outside each input object. The result of a Boolean is eventually computed as a subset of the mesh elements generated in the first step, filtered according to the inside/outside labeling computed in the second step."* (`:51-56 R, 85-88 L`)
  > *"Surface based methods are a bit more convoluted but in general more performing. Our method belongs to this latter category."* (`:122-123 R`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:109-138, 530-565`; `crates/cherchi-rs/src/labeling/inside_out.rs:143-167`):**
  - `native_labeled_arrangement` (`native.rs:109-217`) runs Stage 1 (`mesh_arrangement`), partitions into manifold surface patches (`compute_all_patches`), and runs Stage 2 inside/outside patch classification (`compute_inside_out`, which builds `TriOctree::build(soup)` at `inside_out.rs:155`):
    ```rust
    let soup =
        mesh_arrangement(&coords, &tris, &labels).map_err(NativeBooleanError::Arrangement)?;
    let t_arr = lap(&t_probe);
    let patches = compute_all_patches(&soup).map_err(NativeBooleanError::Patches)?;
    let t_pat = lap(&t_probe);
    let inner = compute_inside_out(&soup, &patches).map_err(NativeBooleanError::InsideOut)?;
    ```
  - `NativeBoolean::boolean` (`native.rs:530-565`) calls `let la = native_labeled_arrangement(a, b)?; let keep = la.keep_set(op);` (`native.rs:536-537`) and compacts the surviving surface triangles without building any tetrahedral/volumetric mesh.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776` (18-fixture × 4-op native-vs-sidecar parity suite).

#### `C22-02` — §2.1 Exact geometric predicates via arithmetic filtering + arbitrary-precision fallback
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:140-150 L, 174-186 L`):**
  > *"This is done by evaluating geometric predicates exactly and quickly through arithmetic filtering [Lévy 2016; Shewchuk 1997]. A typical predicate calculates the sign of a polynomial and, as long as the sign is correct, the program flow is guaranteed to be consistent. Arithmetic filtering [Devillers and Preparata 1998] makes it possible to evaluate a polynomial using floating point arithmetic but, along with it, an upper bound for the rounding error is computed. If the magnitude of the evaluated expression is larger than the error bound, its sign is guaranteed correct. If not, the filter fails, and the predicate is re-evaluated using arbitrary precision."* (`:140-149 L`)
- **Code (`crates/cherchi-rs/src/predicates/orient.rs:58-95, 134-156`; `crates/cherchi-rs/src/predicates/indirect/mod.rs:32-75, 495-680`; `orientation.rs:55-110`; `collinearity.rs:30-33`; `point_in_triangle.rs:97, 141-143`; `simplex_location.rs:89-97`):**
  - Explicit `orient3d` (`orient.rs:58-95`) and `orient2d` (`orient.rs:134-156`, re-exported at `predicates/mod.rs:26`) call `geometry_predicates::{orient3d, orient2d}` (Shewchuk 1997 adaptive expansions) and re-certify any `0.0` return in `dashu::rational::RBig` (`orient3d_rational_sign` / `orient2d_rational_sign`) to close Shewchuk's subnormal underflow hole (`N24`). All ray-cast 2D/3D predicates in `inside_out.rs` (`:63, 1040-1042, 1078-1080, 1405`) go through these certified predicates.
  - Indirect predicates (`predicate-gen/` + `predicates/indirect/mod.rs:32-75`) implement a 3-stage cascade: FPG semi-static `f64` filter → outward-rounded IEEE-754 interval arithmetic (`indirect/interval.rs`) → exact `dashu::rational::RBig` evaluation (`{name}_exact`, substituting `RBig` for Attene 2020's expansion tier; sign-identical).
  - **Gaps (`N99` + latent `UL-04`):**
    1. **Unsound raw-coordinate projection filter (`N99`, `orientation.rs:82-108`):** `max_component_filtered` builds `max_var` from raw vertex coordinates (`max(|a|, |b|, |c|)`) rather than edge differences (`max(|b - a|, |c - a|)`) and uses a single-`eps` margin, allowing it on few-ULP slivers to select an axis along which the exact normal is zero (documented in `docs/audits/cherchi2020_lit_check.md` `E-1` and `docs/yang_deviations.md:7169-7183`).
    2. **Latent `N24` scope gap (`UL-04`):** Raw `geometry_predicates::orient2d` calls survive in `simplex_location.rs:89-97`, `collinearity.rs:30-33`, and `point_in_triangle.rs:97, 141-143` without `orient::orient2d`'s `RBig` zero-certification (latent only — requires coordinate differences `~1e-146`, matching C++ Shewchuk semantics; another agent is routing them through `crate::predicates::orient2d`).
- **Status:** **`PARTIAL`**
- **Ledger:** `N24`, `N99` (`UL-04` latent `N24` bypass being closed).
- **Test pin:** `crates/cherchi-rs/src/predicates/orient.rs:285-355`; `crates/cherchi-rs/tests/indirect_filter_soundness.rs:66-295`; `crates/cherchi-rs/tests/indirect_catalog_soundness.rs:189-1010`. Unpinned for `UL-04` subnormal floor → see `TC-05` (and `cherchi2020_lit_check.md` `TC-1` for `N99`).

#### `C22-03` — §2.1 Implicit intersection point representations (LPI & TPI)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:172-186 L`):**
  > *"Luckily, for the case of mesh Booleans the only intermediate constructions are the intersection points. As recently shown in [Attene 2020], such points can be implicitly represented as the intersection of input primitives and the point’s expression can be composed with the predicate’s expression, enabling the use of arithmetic filtering. Our method exploits this latter technique to perform fast and exact geometric queries involving any combination of input and intersection points."* (`:172-186 L`)
- **Code (`crates/cherchi-rs/src/arrangements/fast_trimesh.rs:182-200`; `crates/cherchi-rs/src/predicates/indirect/mod.rs:141-295`):**
  - `VertexCoords` (`fast_trimesh.rs:182-200`) represents every arrangement vertex implicitly from exact `Point3` input generators:
    ```rust
    pub enum VertexCoords {
        Explicit(Point3),
        Lpi { line: [Point3; 2], plane: [Point3; 3] },
        Tpi { v: [Point3; 3], w: [Point3; 3], u: [Point3; 3] },
    }
    ```
  - `GenericPoint3D` (`predicates/indirect/mod.rs:156-205`) mirrors those three variants and composes their homogeneous $\lambda = (\lambda_x, \lambda_y, \lambda_z, d)$ expressions directly into `orient2d_indirect_*`, `less_than_on_{x,y,z}_indirect`, `point_in_triangle_indirect`, `inner_segments_cross_indirect`, and `point_in_*_segment_indirect` (`predicates/indirect/mod.rs:495-920`).
- **Status:** **`FAITHFUL`**
- **Ledger:** `N13`, `N15`
- **Test pin:** `crates/cherchi-rs/tests/indirect_ffi_parity.rs:155-280`; `crates/cherchi-rs/tests/indirect_catalog_ffi_parity.rs:192-815`; `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:968-1090` (`tpi_xcrossing_arrangement_parity`).

#### `C22-04` — §2.3 Final `f64` snap rounding & manifold-watertight output guarantee on non-tangential inputs
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:238-256 L`):**
  > *"In these cases exact or implicit coordinates must be converted to inexact floating point values and the necessary approximation may invalidate the model by introducing degenerate or intersecting elements. ... For example, the result of a Boolean operation between two watertight manifold meshes that do not touch tangentially is guaranteed to be manifold watertight."* (`:240-243 L, 250-253 L`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:491-521, 536-565`):**
  - All arrangement and ray-casting steps keep `VertexCoords` symbolic; conversion to `f64` happens only in `emit_vertex` (`native.rs:515-521`): `exact_point_coords(&soup.verts[v as usize])` computes exact `[RBig; 3]`, then `xc[i].to_f64().value() / m` rounds once to `Point3`.
  - **Gaps:**
    1. `NativeBoolean::boolean` (`native.rs:536-565`) does not weld distinct `ArrangementSoup` vertex IDs that snap to bit-identical `f64` coordinates if they differed in exact rationals, and on tangential/coplanar contacts (`surface = {0, 1}`) emits internal opposite-normal walls on `Union` and open membranes on `Intersect` (consistent with the paper's *"that do not touch tangentially"* qualifier, noted in `coplanar_pocket_parity.rs:28-42`).
    2. Even on non-tangential watertight meshes, **confirmed `UL-02` (`N101`)** — the fixed 8-offset `next_up`/`next_down` perturbation cap in `inside_out.rs:1087-1153` — silently drops a transversal crossing through an octagonal pyramid apex (all 8 offsets land in 45° spoke planes) and mislabels an enclosed patch as `outside`.
- **Status:** **`PARTIAL`**
- **Ledger:** `N101` (`UL-02`, fix in progress by a separate agent).
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776` (asserts Euler characteristic & manifoldness on 18 clean fixtures). Unpinned for `UL-02` → see `TC-01`.

---

### §3 Overview — Inputs, Operators, Provenance & Patch Partition

#### `C22-05` — §3 Variadic input meshes $M_1, \dots, M_n$ and operators `{union, intersection, subtraction}`
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:234-236 R`):**
  > *"Our method takes as input a set of input meshes $M_1, M_2, \dots, M_n$, and a Boolean operator, namely union, intersection, subtraction."* (`:234-235 R`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:712-724`; `crates/cherchi-rs/src/labeling/inside_out.rs:143-167, 176-280`; `crates/cherchi-rs/src/labeled_arrangement.rs:113-157`; `crates/cherchi-rs/src/labeling/native.rs:109-125, 172, 215, 529-574`; `crates/cherchi-rs/src/boolean.rs:31-58`):**
  - Internal stages (`mesh_arrangement`, `compute_all_patches`, `compute_inside_out` returning `Vec<Label>` of containing `InputId`s, and `LabeledArrangement::keep_set` parameterized by `self.num_inputs`) support arbitrary `Label = Vec<InputId>` and `num_inputs >= 2` (plus an extra `BoolOp::Xor` operator from upstream `booleans.cpp:1463-1510`).
  - **Gap:** The public entry points `native_labeled_arrangement(a: &Mesh, b: &Mesh)` (`native.rs:109`) and `MeshBoolean::{boolean, labeled_arrangement}` (`boolean.rs:34-57`) are hardcoded to binary `(a, b)` with `(0..2u32)` / `num_inputs: 2` (`native.rs:172, 215`). No public variadic `native_boolean_variadic(&[&Mesh], BoolOp)` API is exposed.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:640-725` (internal 3-solid `compute_inside_out` tests); binary public API pinned in `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs`. Unpinned for public variadic API → see `TC-06`.

#### `C22-06` — §3 Input mesh preconditions (manifold, watertight, no self-intersections)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:235-237 R`):**
  > *"Input meshes are always assumed to unambiguously enclose a volume, that is, they are manifold, watertight and with no self-intersections."* (`:235-237 R`)
- **Code (`crates/cherchi-rs/src/inputcheck.rs:116-247`; `crates/cherchi-rs/src/labeling/native.rs:109-130`):**
  - `check_input_mesh` (`inputcheck.rs:116-247`) implements a thorough port of upstream `inputcheck.cpp`: detects zero-area triangles (via N24-certified `orient2d`), boundary edges (`inc != 2`) or unoriented edges, vertex link multi-component fans (`non_manifold_vertices`), and exact non-adjacent triangle self-intersections (`self_intersecting_pairs` via `AabbTree` + `triangles_do_intersect`).
  - **Gap:** `native_labeled_arrangement` (`native.rs:109-130`) and `NativeBoolean::boolean` (`native.rs:530-565`) never call `check_input_mesh`. Self-intersecting or open shells passed to `NativeBoolean` proceed silently into arrangement and ray-casting unless a downstream step happens to hit `DegenerateOrientation` or `InnerLabelOutsideInputBounds`.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/stage0_operand_inputcheck.rs:51-125`; `crates/cherchi-rs/src/inputcheck.rs:365-690`.

#### `C22-07` — §3 Per-triangle origin provenance & multi-mesh coplanar ownership
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:241-245 R`):**
  > *"Mapping the result of a Boolean to the input shapes is useful in many applications, therefore for each output triangle we propagate information on its origin. Note that an output triangle can belong to many input meshes, for example in the case of meshes that overlap at a coplanar region."* (`:241-245 R`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:347-357, 864-867, 891-894, 1030-1039`; `crates/cherchi-rs/src/labeling/native.rs:163-184`; `crates/cherchi-rs/src/labeled_arrangement.rs:42-66`):**
  - `remove_degenerate_and_duplicated_triangles` (`soup.rs:347-357`) OR-merges `kept_labels[out_idx]` and appends duplicate base-triangle indices into `kept_source[out_idx]`.
  - Step 9 coplanar pocket deduplication (`soup.rs:1030-1039`) similarly calls `or_merge_label(&mut out_labels[p], &label)` and appends partner base-triangle provenance into `out_source[p]`:
    ```rust
    or_merge_label(&mut out_labels[p], &label);
    for &src in &kept_source[t as usize] {
        if !out_source[p].contains(&src) {
            out_source[p].push(src);
        }
    }
    ```
  - `native_labeled_arrangement` (`native.rs:163-184`) decodes flat soup indices into `LabeledArrangement::surface: Vec<Vec<InputId>>` and `LabeledArrangement::source: Vec<Vec<(InputId, u32)>>`.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/coplanar_dupl_restore.rs:214-231` (`output_surface_labels_keep_or_merged_overlap`); `crates/cherchi-rs/tests/coplanar_pocket_parity.rs:371-385`.

#### `C22-08` — §3 / §5 Arrangement partitioned into manifold surface patches bounded by non-manifold intersection loops
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:250-256 R, 386-388 L`):**
  > *"Throughout this paper, an arrangement is a set of surface patches bounded by intersection lines. When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."* (`:250-255 R`)
- **Code (`crates/cherchi-rs/src/labeling/patches.rs:72-207`):**
  - `compute_all_patches` (`patches.rs:83-89, 180-196`) builds edge-to-triangle incidence `e2t`, floods across edges with `inc.len() <= 2` (`patches.rs:183`), and stops at non-manifold edges `inc.len() > 2` while recording their endpoints in `border_verts` (`patches.rs:190-195`).
  - **Divergence (`N23`):** Where an ideal exact simplicial arrangement of closed shells has uniform `soup.labels` inside every manifold patch (asserted in debug C++ `booleans.cpp:449`), `patches.rs:126-178` implements `N23`:
    - **L2a (`patches.rs:128-129`):** allows flooding across a 2-incident edge when `cl ⊆ ref_l || ref_l ⊆ cl` (e.g., `[A, B]` ↔ `[A]` or `[A, B]` ↔ `[B]`), keeping `ref_l` (the seed's label). Because `patches.rs:97` visits `seed` in ascending `0..n` order, every unvisited triangle reached from `seed` has index `>= seed`, so after `patch.sort_unstable()` (`:198`), `patch[0]` **is** `seed` and `inside_out.rs:226` (`&soup.labels[patch[0] as usize]`) matches `ref_l` and C++ `booleans.cpp:629`.
    - **L2b (`patches.rs:130-175`):** returns loud `Err(PatchError::LabelMismatch)` when `cl` and `ref_l` are disjoint (`[A]` vs `[B]`), stricter than `NDEBUG` C++.
    - **Residual open question (`UNCERTAIN`):** Can an `[A, B]`-seeded L2a patch ever flood across a 2-incident manifold edge into a single-label (`[A]` or `[B]`) sub-region that is enclosed by the other solid (where `patch_surface_label = [A, B]` would skip both shells)? Uncertain whether any valid closed-shell input can realize that topology without a 3-incident intersection edge stopping the flood.
- **Status:** **`DEVIATES`**
- **Ledger:** `N23`
- **Test pin:** `crates/cherchi-rs/tests/r0046_patch_label_parity.rs:141-175`; `crates/cherchi-rs/src/labeling/patches.rs:222-805`.

---

### §4 Mesh Arrangement

#### `C22-09` — §4 Flatten input meshes into a single triangle soup tagged by input mesh ID
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:295-299 L`):**
  > *"From the perspective of the arrangement algorithm, the input meshes $M_1, M_2, \dots, M_n$ can be seen as a soup of possibly intersecting triangles. We therefore flatten all input triangles into a single array, associating to each triangle a tag that maps it to the input mesh it belongs to."* (`:295-299 L`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:115-134`; `crates/cherchi-rs/src/arrangements/soup.rs:712-724`):**
  - `native_labeled_arrangement` (`native.rs:115-134`) flattens `a` and `b` inline into contiguous `coords: Vec<f64>`, index-shifted `tris: Vec<[u32; 3]>`, and `labels: Vec<Label>`, then passes them to `mesh_arrangement`:
    ```rust
    let off = a.verts.len() as u32;
    let mut tris = a.tris.clone();
    tris.extend(b.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    let mut labels = vec![vec![InputId(0)]; a.tris.len()];
    labels.extend(std::iter::repeat_n(vec![InputId(1)], b.tris.len()));
    ...
    let soup =
        mesh_arrangement(&coords, &tris, &labels).map_err(NativeBooleanError::Arrangement)?;
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/arrangements/soup.rs:1156-1600`.

#### `C22-10` — §4 Preprocessing: degeneracy/duplicate removal & single-label input-shell restoration
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:342-345 L, 356-358 L`):**
  > *"Overall, we observe that the most costly operations of the arrangement algorithm are the removal of duplicate and degenerate elements, the construction of adjacencies, the intersection computations and the final triangulation. ... Note that we perform degeneracy removal and rebuilding of adjacencies each frame to ensure robustness while allowing any modeling operation to be performed by users."* (`:342-345 L, 356-358 L`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:203-261, 314-367, 742-758, 1101-1108`; `crates/cherchi-rs/src/processing/multiplier.rs:33-67`; `crates/cherchi-rs/src/predicates/collinearity.rs:25-34`):**
  - Steps 2–4 (`soup.rs:742-758`): `compute_multiplier` + `multiply_coordinates`, bit-exact `merge_duplicated_vertices` (`:203-261`), and `remove_degenerate_and_duplicated_triangles` (`:314-367`, dropping exact-collinear triangles via Shewchuk `points_are_collinear_3d`, deduplicating sorted `[v0, v1, v2]`, OR-merging `kept_labels`, and recording `DuplTriInfo { t_off, label, w }`).
  - Step 11 (`soup.rs:1101-1108`) ports `addDuplicateTrisInfoInStructures` (`booleans.cpp:358-393`), restoring `w`-oriented single-label copies into `arr_in_tris`/`arr_in_labels` so each solid's ray-cast shell is closed and outward-oriented:
    ```rust
    if points_are_collinear_3d(v0, v1, v2) {
        continue;
    }
    ...
    for d in &dupl_triangles {
        let [v0, v1, v2] = arr_in_tris[d.t_off];
        arr_in_tris.push(if d.w { [v0, v1, v2] } else { [v0, v2, v1] });
        arr_in_labels.push(d.label.clone());
        arr_in_labels[d.t_off].retain(|id| !d.label.contains(id));
    }
    ```
  - *(Note: `collinearity.rs:30-33` uses Shewchuk's `geometry_predicates::orient2d` directly, matching C++; see `C22-02` / `UL-04` for the latent `~1e-146` `N24` zero-certification unification.)*
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/coplanar_dupl_restore.rs:166-239`.

#### `C22-11` — §4 Fig. 4 Three-step arrangement refinement (1: detect intersections, 2: insert points, 3: insert segments)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:272-275, 300-303 L`):**
  > *"Fig. 4. First step of the Boolean pipeline. We start from an generic triangle soup (left) and detect intersection points and lines (1). We then split triangles to incorporate new points first (2), and then segments (3). The output is a well formed simplicial complex (right)."* (`:274-275`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:770-1082`; `intersection_detection.rs:85-170`; `intersection_points.rs:157-269`; `aux_structure.rs:195-495`; `coplanar_propagate.rs:70-480`; `retriangulate.rs:74-94`; `enforce.rs:138-266`):**
  - Step 1 (Detect): `detect_intersecting_pairs(&soup)` (`soup.rs:770`) queries `AabbTree` (`intersection_detection.rs:100`) and narrows non-adjacent pairs with exact `triangle_intersects_triangle_3d`; `classify_all(&soup, &pairs)` (`soup.rs:774`) constructs `Explicit` and `Lpi` points; `group_intersection_points`, `group_constraint_segments`, and `integrate_coplanar_into_arrangement` (`soup.rs:833-855`) intern points by exact `[RBig; 3]` coordinates (`AR3c` / `N18`).
  - Step 2 (Insert points): `split_single_triangle(&mut subm, &flat)` (`soup.rs:926`, `retriangulate.rs:74-94`) inserts every interior and on-edge point into a per-triangle `FastTrimesh`:
    ```rust
    split_single_triangle(&mut subm, &flat).map_err(|e| ...)?;
    enforce_constraints(&mut subm, &segments_per_tri[t as usize], &points).map_err(|detail| ...)?;
    ```
  - Step 3 (Insert segments + TPIs + coplanar pocket dedup): `enforce_constraints` (`soup.rs:940`, `enforce.rs:138-266`) realizes constraint segments, minting `VertexCoords::Tpi` where two constraints cross, and `VisitedPocketRegistry` (`soup.rs:987-1042`) deduplicates shared coplanar pockets across operands.
- **Status:** **`FAITHFUL`**
- **Ledger:** `N13`, `N14`, `N15`, `N16`, `N17`, `N18`, `N95`
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-1120`; `crates/cherchi-rs/tests/single_coplanar_edge_parity.rs:272-435`; `crates/cherchi-rs/tests/coplanar_pocket_parity.rs:371-385`.

#### `C22-12` — §4 Cached Predicates (4 cached $3\times 3$ plane minors per input triangle for `orient3D`)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:292-310 R`):**
  > *"We observe that in the arrangement algorithm planes are known a priori (they are the supporting planes of the input triangles) and are tested multiple times against several different points. ... We exploit this latter equation to cache, for each input triangle, the four $3 \times 3$ determinants, thus reducing each call to orient3D to a simple scalar product in 4D."* (`:292-295 R, 307-310 R`)
- **Code (`crates/cherchi-rs/src/predicates/orient.rs:58-95`; `crates/cherchi-rs/src/predicates/indirect/mod.rs:156-205`):**
  - **Absent.** `orient3d` (`orient.rs:58-95`) calls `geometry_predicates::orient3d` from scratch on the four 3D points at every invocation; neither `FastTrimesh` (`arrangements/fast_trimesh.rs`) nor `intersection_points.rs` precomputes or caches the four $3\times 3$ plane minors of input triangles. (Only implicit points cache their Attene 2020 homogeneous $\lambda$ vector via `OnceLock<LambdaIv>` / `OnceLock<LambdaF64>` in `predicates/indirect/mod.rs:163-179`.) Correctness-neutral; leaves the paper's ~40% predicate speedup on the table.
- **Status:** **`MISSING`**
- **Ledger:** —
- **Test pin:** — (performance optimization only).

#### `C22-13` — §4 Segment Insertion via Livesu et al. 2021 deterministic $O(n)$ earcut
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:323-336 R/L`):**
  > *"In [Cherchi et al. 2020] segment insertion was performed using the earcut algorithm, which in its best implementation achieves $O(n^2)$ worst case complexity [Eberly 2008], with $n$ being the number of polygon segments. We substituted earcut with a method recently introduced in [Livesu et al. 2021], which ensures optimal deterministic $O(n)$ complexity in all cases and is two orders of magnitude faster than the previously best performing existing method [Shewchuk and Brown 2015]."* (`:323-327 R, 332-336 L`)
- **Code (`crates/cherchi-rs/src/arrangements/enforce.rs:220-256, 670-745`):**
  - When `add_constraint_segment` (`enforce.rs:174-266`) walks the sequence of triangles conflicting with `(v_start, v_stop)`, `boundary_walker` (`enforce.rs:222-245`) extracts the two strictly star-shaped cavity chains `h0` and `h1`, and `earcut_linear` (`enforce.rs:670-745`) triangulates each in $O(n)$ time using a doubly-linked list (`prev`/`next`) and ear stack with local neighbor updates only (`pc` and `nc`), excluding endpoints `0` and `size - 1`:
    ```rust
    let mut new_tris: Vec<u32> = Vec::new();
    earcut_linear(subm, &h0, &mut new_tris, orientation);
    earcut_linear(subm, &h1, &mut new_tris, orientation);
    ```
  - *(Note: `crates/cherchi-rs/src/triangulation/mod.rs` exports `spade`-backed `cdt_polygon_with_holes*` for `yang-rs` Stage-1/Stage-4 face tessellation (`N9`), NOT for `cherchi-rs` §4 segment insertion.)*
- **Status:** **`FAITHFUL`**
- **Ledger:** `N16`
- **Test pin:** `crates/cherchi-rs/src/arrangements/enforce.rs:829-2156` (X-crossing, T-junction, multi-segment cavity earcut tests).

#### `C22-14` — §4 Data-parallel execution & work-stealing octree/arrangement pipeline
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:339-364 L`):**
  > *"Overall, we observe that the most costly operations of the arrangement algorithm are the removal of duplicate and degenerate elements, the construction of adjacencies, the intersection computations and the final triangulation. The main costs of these operations can be reduced by executing their core components in a data-parallel manner, giving us high speed up while maintaining the same algorithm pipeline. We also optimize octree construction by using nested parallel constructs. Parallel constructs are executed by a work-stealing scheduler that ensures good balanced workloads."* (`:341-356 L`)
- **Code (`crates/cherchi-rs/Cargo.toml`; `crates/cherchi-rs/CLAUDE.md`; `crates/cherchi-rs/src/arrangements/intersection_detection.rs:105-140`; `crates/cherchi-rs/src/arrangements/intersection_points.rs:53-55, 200-235`):**
  - Default build is **strictly single-threaded** (`cherchi-rs/CLAUDE.md` rule 5, WASM compatibility). An opt-in `parallel` cargo feature enables `rayon` work-stealing on two read-only stages (`detect_intersecting_pairs` and `classify_intersections`) with deterministic post-sort; preprocessing, `AabbTree::build`, per-triangle `split_single_triangle` / `enforce_constraints`, `compute_all_patches`, and `compute_inside_out` are always serial.
- **Status:** **`PARTIAL`**
- **Ledger:** `N95`
- **Test pin:** `crates/cherchi-rs/src/arrangements/intersection_detection.rs:180-395`.

#### `C22-15` — §4 Low-level data structures (swiss tables, arena allocators, small-array adjacency vectors)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:376-383 L`):**
  > *"We optimize these data structures with three techniques. First, we use hash tables based on the swiss table design for sets and dictionaries, to both save memory and improve cache coherency. Second, we use arena allocators to reduce the pressure on the memory allocator and reduce overall fragmentation. Third, we use dynamic arrays with small-array optimization for adjacency lists."* (`:376-382 L`)
- **Code (`crates/cherchi-rs/src/arrangements/fast_trimesh.rs:110-210`; `crates/cherchi-rs/src/arrangements/soup.rs:740-1130`):**
  - **Partially/mostly absent.** Rust's standard `std::collections::HashMap` uses `hashbrown` (swiss-table design, satisfying technique 1 where `HashMap` is used, though many maps use `BTreeMap`/`BTreeSet` for determinism), while **neither arena allocators** nor **`SmallVec`/inlined adjacency arrays** (`FastTrimesh` stores heap `Vec<Vec<u32>>` per vertex/edge) are implemented. Performance-only.
- **Status:** **`MISSING`**
- **Ledger:** —
- **Test pin:** — (memory/throughput optimization only).

---

### §5 Inside/Outside Classification

#### `C22-16` — §5 / Alg. 1 Per-patch ray-casting loop (scaling with patch count, initialized outside all $M_k$)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:332-351 R, 386-399 L/R`):**
  > *"for each patch $P$ do / Initialize $P$ as being outside of $M_1, \dots, M_n$; / Define a ray $r$ starting at point $p \in P$ towards point at infinite $p_\infty$; / for each input mesh $M$ do / compute and sort intersections between $r$ and $M$; ... end / end"* (`Alg. 1, :336-351 R`)
  > *"An important aspect of such an approach is that the algorithm scales with the number of patches in the arrangement and not with the number of triangles in the mesh."* (`:395-398 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:143-167, 220-279`; `crates/cherchi-rs/src/labeling/native.rs:169-173`):**
  - `compute_inside_out_with` (`inside_out.rs:220-279`) loops once per patch `(pi, patch)` (`:221`), resolves a single ray via `find_ray_endpoints` (`:228`, or Tier-3 `rational_ray_inner_label` at `:272`), runs `prune_intersections_and_sort_along_ray` + `analyze_sorted_intersections` once per patch (`:231-237`), and returns one containing-solid `Label` per patch (empty = outside all $M_k$), which `native_labeled_arrangement` (`native.rs:169-173`) maps to every triangle of the patch:
    ```rust
    let mut inner_labels: Vec<Label> = Vec::with_capacity(patches.patches.len());
    for (pi, patch) in patches.patches.iter().enumerate() {
        ...
        let patch_surface_label = &soup.labels[patch[0] as usize];
        match find_ray_endpoints(&search, patch, patch_surface_label, pi) {
            Ok(ray) => {
                let candidates = candidates_for(&ray);
                let sorted = prune_intersections_and_sort_along_ray(
                    soup, &ray, patch_surface_label, &candidates,
                )?;
                let label = analyze_sorted_intersections(soup, &ray, &sorted, pi)?;
    ```
    ```rust
    let pid = patches.tri_to_patch[t];
    patch.push(pid);
    let inn = &inner[pid as usize];
    inside.push((0..2u32).map(|k| inn.contains(&InputId(k))).collect());
    ```
  - *(Because `patches.rs:97` visits `seed` ascending, `patch[0]` after `sort_unstable` is always `seed`, matching `N23` and C++ `booleans.cpp:629`; see `C22-08` for the residual `UNCERTAIN` question on L2a mixed patches.)*
- **Status:** **`FAITHFUL`**
- **Ledger:** — (`N23` patch-flood policy tracked on `C22-08`)
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1421-2335`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:1-1215`.

#### `C22-17` — §5 / Alg. 1 / Fig. 5 Signed tetrahedron volume test $\text{orient3D}(t_i, p_\infty) < 0$ on first hit
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:342-348 R, 369-374 R, 389-395 R`):**
  > *"Given an intersection point $i$ and the triangle $t_i$ containing it, the ray traverses the mesh from inside to outside if the volume of the tetrahedron $(t_i, p_\infty)$ is negative, from outside to inside otherwise. This check can be performed exactly with arithmetic orientation predicates."* (`Fig. 5, :369-374 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1390-1415`):**
  - `analyze_sorted_intersections` (`inside_out.rs:1390-1415`) tracks `visited: BTreeSet<InputId>` along `sorted` (already filtered to exclude `patch_surface_label`), inspects **only the first hit** per foreign input mesh $M$, and evaluates `orient3d(tv[0], tv[1], tv[2], ray.v1)`:
    ```rust
    match orient3d(tv[0], tv[1], tv[2], ray.v1) {
        Sign::Negative => {
            inner.extend(label.iter().copied());
        }
        Sign::Positive => {}
        Sign::Zero => return Err(InsideOutError::DegenerateOrientation { patch: pi, tri: t }),
    }
    visited.extend(label.iter().copied());
    ```
  - Sign convention verified against `predicates/orient.rs:58-70`: `orient3d(a, b, c, d)` returns the sign of $\det[a-d, b-d, c-d] = (a-d) \cdot ((b-d)\times(c-d)) = -(d-a)\cdot((b-a)\times(c-a))$, which equals $+6\,\text{SignedVol}(a, b, c, d)$ under Cherchi/CinoLib's determinant in `:283-289 R`; when $p_\infty$ lies on the outward normal side of $(t_0, t_1, t_2)$, $(d - a)\cdot n_{\text{out}} > 0 \iff \text{orient3d}(t_0, t_1, t_2, p_\infty) = \text{Sign::Negative}$.
- **Status:** **`FAITHFUL`**
- **Ledger:** `N24`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1510-1620`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:150-310`.

#### `C22-18` — §5.1 Axis-aligned point at infinity $p_\infty$ strictly outside scene bounding box
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:421-429 L`):**
  > *"If $p$ is known, the infinite point $p_\infty$ can be easily defined by translating $p$ along one of the major axes by a quantity that is bigger than the extent of the bounding box of the input scene along the same axis (this guarantees that $p_\infty$ is outside all input meshes). Moreover, having an axis-aligned ray considerably simplifies the next ray-triangle intersection analysis as it always allows to drop one coordinate and operate on a 2D plane instead of the 3D space."* (`:422-429 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:190-202, 468-480, 659-662`):**
  - `compute_inside_out_with` (`inside_out.rs:190-202`) computes the coordinate-wise maximum `max_c` over all explicit vertices of `soup.in_tris` and sets `max_coords = [max_c[0] + 0.5, max_c[1] + 0.5, max_c[2] + 0.5]`, and `axis_ray` (`:468-480`, mirrored in the generated-ray branch at `:659-662`) places `v1` at `max_coords[dir]` while keeping both off-axis coordinates identical to `v0`:
    ```rust
    let max_coords = [max_c[0] + 0.5, max_c[1] + 0.5, max_c[2] + 0.5];
    ...
    fn axis_ray(p: Point3, dir: Axis, max_coords: [f64; 3]) -> Ray {
        let v1 = match dir {
            Axis::X => Point3::new(max_coords[0], p.y(), p.z()),
            Axis::Y => Point3::new(p.x(), max_coords[1], p.z()),
            Axis::Z => Point3::new(p.x(), p.y(), max_coords[2]),
        };
        Ray { v0: p, v1, dir, seed_tri: None }
    }
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1510-1620`.

#### `C22-19` — §5.1 Tier 1 ray definition from an interior explicit input vertex of $P$
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:430-433 L`):**
  > *"In the simplest case, the patch $P$ contains at least one input vertex with known ground-truth floating point coordinates in its interior, which can be used as a starting point for $r$."* (`:431-433 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:507-546, 585-627`):**
  - The explicit-vertex branch of `find_ray_endpoints` (`inside_out.rs:601-627`) iterates over vertices `v` of triangles in `patch`, skipping `border.contains(&v)` (`:604`) and matching `VertexCoords::Explicit(p)` (`:607`).
  - **Divergence (`N69`):** Whereas the paper and C++ `findRayEndpoints` (`booleans.cpp:504-524`) unconditionally take the **first** non-border explicit vertex along `+X`, `inside_out.rs:601-627` walks `RAY_AXES = [Axis::X, Axis::Y, Axis::Z]` (axis-major, origin-minor) and rejects any candidate `ray` for which `ray_grazes_foreign_input` (`:507-546`) finds a foreign input triangle in `tri_in_ray_aabb` whose plane contains the entire ray line (`orient3d(tv[0], tv[1], tv[2], ray.v0) == Sign::Zero && orient3d(tv[0], tv[1], tv[2], ray.v1) == Sign::Zero`, `:529-530`). If every explicit non-border candidate grazes on all three axes, execution falls through to the generated-ray branch (`:629`). This ladder is controlled by a release-active env var `CHERCHI_GRAZE_AWARE_RAY` (`:440-446`, **UL-07** / `N100` item 8).
- **Status:** **`DEVIATES`**
- **Ledger:** `N69`, `N100` item 8 (`UL-07`).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:2120-2335` (N69 / P0023 regression fixtures).

#### `C22-20` — §5.1 Tier 2 ray definition from approximate `f64` triangle barycenter pushed beneath $t$
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:445-460 L, 404-410 R`):**
  > *"Our main idea is that if we can define a ray that starts from beneath $P$ and is guaranteed to traverse the patch at some internal point $p \in P$, we can simply sort all the intersections we find, and perform the inside/outside classification by considering the first intersection that occurs after $p$. Our strategy is as follows: we pick a random triangle $t \in P$ and convert the coordinates of its implicit vertices into explicit floats, computing the approximate triangle barycenter $b_t$. To make sure that $b_t$ stays beneath triangle $t$ we push the point backwards along the same axis used to define $p_\infty$. ... Thanks to the axis alignment we can efficiently and reliably test that the wanted intersection exists with a simple 2D point in triangle test, performed considering the orthogonal projection of both $t$ and $b_t$ along the ray direction. If the test fails we attempt to produce the same construction with another triangle of $P$, until we find a valid one."* (`:445-457 L, 404-410 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:629-702, 933-987, 1222-1247`):**
  - The generated-ray branch of `find_ray_endpoints` (`inside_out.rs:629-702`) iterates deterministically `for &t in patch` (`:630`), materializes approximate `f64` coordinates via `approx_point` (`:632-636`, `:933-987`: `Explicit` verbatim, `Lpi` via `f64` line–plane evaluation, `Tpi` via `f64` Cramer's rule), skips `points_are_collinear_3d(a, b, c)` (`:640`), chooses `dir = max_component_in_triangle_normal(a, b, c)` (`:643`), computes `cen = [(a + b + c) / 3.0]` (`:644-651`), pushes `v0c[k] -= 0.1` (`:658`), sets `v1c[k] = max_coords[k]` (`:660`), validates with exact indirect `orient3d_gp3` straddle (`:677-683`) and 3-edge `orient3d_gp2_e2` strict interior (`:690-695`, using 3D indirect `orient3d_indirect` rather than 2D projection, matching C++ `checkIntersectionInsideTriangle3DImplPoints`), and returns `Ok(Ray { v0, v1, dir, seed_tri: Some(tri) })` (`:696-701`).
  - Downstream `sort_hits_along_ray` (`:1222-1247`) discards hits behind `seed_tri` via `behind_seed_plane` (`orient3d_gp3(&gps, &lpis[i])` strictly opposing `orient3d_gp3(&gps, &ray_v1)`), ensuring only intersections strictly after $p \in P$ are analyzed (`:448-450 L`).
  - **Gap (`UL-06`):** Unlike the explicit-vertex branch (`:614`), the generated-ray branch (`inside_out.rs:629-702`) **never calls `ray_grazes_foreign_input`**! On an originless patch (all vertices on `border_verts`, or all explicit candidates rejected by `ray_grazes_foreign_input` at `:614` so control falls through to `:629`), the generated-ray branch returns the first straddling synthetic ray unconditionally even if its supporting line lies in a foreign input triangle's plane, re-exposing the `N69` exterior-graze bug. *(Also note `max_component_in_triangle_normal` at `:643` carries `N99`, though `:677-695` re-validates straddle and strict interior with exact `orient3d_indirect`.)*
- **Status:** **`PARTIAL`**
- **Ledger:** `N99` (`UL-06` missing `ray_grazes_foreign_input` on generated-ray branch is unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1700-1820`. Unpinned for `UL-06` → see `TC-02`.

#### `C22-21` — §5.1 Tier 3 exact rational barycenter & exact rational ray casting fallback
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:409-412 R, 444-445 L`):**
  > *"instead, inspired by the cascaded approaches used by filtered predicates [Attene 2020; Lévy 2016; Shewchuk 1997], we define the ray $r$ by first attempting to find a satisfactory approximate floating point solution, while we resort to guaranteed exact rational numbers only as backup strategy."* (`:441-445 L`)
  > *"If no valid triangle can be found, we compute the exact barycenter with rational numbers and perform an exact ray casting."* (`:409-411 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:271-275, 784-926`):**
  - Whereas upstream C++ `booleans.cpp:578` left Tier 3 unimplemented (`exit`), `cherchi-rs` catches `Err(InsideOutError::NoExplicitRayOrigin { .. })` (`inside_out.rs:271`) and runs the paper's exact rational backup in `rational_ray_inner_label` (`inside_out.rs:784-926`):
    ```rust
    let three = RBig::from(3);
    let origin: Option<R3> = patch.iter().find_map(|&t| {
        let tri = soup.tris[t as usize];
        let (a, b, c) = (
            exact_coords(tri[0])?,
            exact_coords(tri[1])?,
            exact_coords(tri[2])?,
        );
        Some([
            (&a[0] + &b[0] + &c[0]) / &three,
            (&a[1] + &b[1] + &c[1]) / &three,
            (&a[2] + &b[2] + &c[2]) / &three,
        ])
    });
    ```
  - For `k in 0..3usize` (`+X, +Y, +Z`, `:845`): skips `patch_surface_label` (`:852`); skips parallel planes `n[k] == RBig::ZERO` (`:872`); computes exact ray parameter `t = &rhs / &n[k]` and discards `t <= RBig::ZERO` (`:879-885`, including `N20`'s `t == 0` exclusion); tests strict interior in `RBig` via `n`-aligned edge orientations `s1, s2, s3 > 0` (`:890-896`); retries the next axis on any boundary graze `!neg(&s1) && !neg(&s2) && !neg(&s3)` (`:897-901`); sorts and deduplicates equal `t` (`:907-908`); and classifies inside iff `n[k] > RBig::ZERO` (`:918-920`). Returns loud `Err(InsideOutError::RationalRayDegenerate { patch: pi })` (`:925`) if all three axes graze.
- **Status:** **`FAITHFUL`** *(faithful to the paper; diverges from C++'s `exit` stub, tracked as `N21`)*
- **Ledger:** `N21`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1950-2115` (`kv4_f1` sliver/implicit-patch rational-ray tests).

---

### §5.2 Intersection Detection & Along-Ray Sorting

#### `C22-22` — §5.2 Exact 2D projected point-in-triangle ray-triangle intersection (no Möller–Trumbore)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:424-429 R, 442-446 R`):**
  > *"In a typical ray casting implementation, ray-triangle intersections are computed using the efficient Möller–Trumbore algorithm [1997]. Unfortunately, our exactness requirements make it impossible to rely on such an algorithm, which involves floating point operations that accumulate error and it is also non stable in case of coplanarity. ... Once again, we exploit the axis aligned nature of our problem to recast the ray-triangle intersection as a point in triangle query in 2D, as previously described in Section 5.1."* (`:424-429 R, 442-445 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:63, 1016-1073`; `crates/cherchi-rs/src/predicates/mod.rs:26`; `crates/cherchi-rs/src/predicates/orient.rs:134-156`):**
  - `fast_2d_check_intersection_on_ray` (`inside_out.rs:1032-1073`) drops `ray.dir` via `project` (`Axis::X` → YZ, `Axis::Y` → ZX, `Axis::Z` → XY, `:1024-1030`) and evaluates three exact `crate::predicates::orient2d` calls (Shewchuk adaptive + `N24` `RBig` zero-certification):
    ```rust
    let or01 = orient2d(v[0], v[1], q);
    let or12 = orient2d(v[1], v[2], q);
    let or20 = orient2d(v[2], v[0], q);
    let nonneg = |s: Sign| s != Sign::Negative;
    let nonpos = |s: Sign| s != Sign::Positive;

    if (nonneg(or01) && nonneg(or12) && nonneg(or20))
        || (nonpos(or01) && nonpos(or12) && nonpos(or20))
    {
        for (k, vk) in v.iter().enumerate() {
            if vk.x() == q.x() && vk.y() == q.y() {
                return IntersInfo::IntInV(k as u8);
            }
        }
        let z = |s: Sign| s == Sign::Zero;
        if (z(or01) && z(or12)) || (z(or12) && z(or20)) || (z(or20) && z(or01)) {
            return IntersInfo::Discard;
        }
        if z(or01) { return IntersInfo::IntInEdge(0); }
        if z(or12) { return IntersInfo::IntInEdge(1); }
        if z(or20) { return IntersInfo::IntInEdge(2); }
        return IntersInfo::IntInTri;
    }
    IntersInfo::NoInt
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** `N24`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1421-1698`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:1-520`.

#### `C22-23` — §5.2 Shared axis-aligned octree acceleration structure across arrangement & ray-casting
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:432-436 R`):**
  > *"In our implementation, we use a plain octree as acceleration structure and perform ray casting by testing intersections between the ray bounding box and each octant. Note that, for efficiency, this is the same acceleration structure used in the arrangement part to detect triangle intersections."* (`:432-436 R`)
- **Code (`crates/cherchi-rs/src/aabb_tree.rs:33-40, 86-175`; `crates/cherchi-rs/src/arrangements/intersection_detection.rs:85-104`; `crates/cherchi-rs/src/labeling/octree.rs:60-82`; `crates/cherchi-rs/src/labeling/inside_out.rs:155, 412-423, 1297`):**
  - Both stages use the same 8-way axis-aligned octree implementation `AabbTree` (`src/aabb_tree.rs:86`: `MAX_DEPTH = 8`, `LEAF_SPLIT_THRESHOLD = 16`, `MAX_SPLIT_DUPLICATION = 2`, inclusive unpadded `Aabb::intersects`), and `prune_intersections_and_sort_along_ray` (`inside_out.rs:1297`) re-checks every candidate with exact `tri_in_ray_aabb` (`:412-423`, pure `min`/`max`/`<=`/`>=` comparisons, zero rounding).
  - **Gap:** Two `AabbTree` instances are **built separately** instead of sharing one structure: once inside `detect_intersecting_pairs` (`intersection_detection.rs:100`, over `kept_tris` before `dupl_triangles` restoration) and a second time in `TriOctree::build` (`inside_out.rs:155`, `octree.rs:67-74`, over `soup.in_tris` after step-11 `dupl_triangles` restoration). Performance-only; exact candidate superset invariant holds.
- **Status:** **`PARTIAL`**
- **Ledger:** `N95`
- **Test pin:** `crates/cherchi-rs/src/labeling/octree.rs:116-412` (`query_aabb` superset vs brute-force oracle).

#### `C22-24` — §5.2 Tight ray AABB & 4-float 2D octant overlap check
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:437-441 R`):**
  > *"Since both the octree and the ray are axis aligned we have two nice properties: the bounding box of the ray is tight (it’s the ray itself), and the intersection with the octant reduces to a 2D check which involves only four comparisons between floats."* (`:437-440 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:160-161, 386-399, 412-423`; `crates/cherchi-rs/src/labeling/octree.rs:79-81`; `crates/cherchi-rs/src/aabb_tree.rs:69-71`):**
  - `ray_aabb` (`inside_out.rs:386-399`) constructs the tight degenerate ray AABB `([v0.min(v1)], [v0.max(v1)])` (zero epsilon slab, zero rounding) and passes `(lo, hi)` to `octree.query_aabb(lo, hi)` (`octree.rs:79-81`).
  - **Minor difference:** Both `Aabb::intersects` (`aabb_tree.rs:69-71`) and `tri_in_ray_aabb` (`inside_out.rs:412-423`) perform 6 float comparisons across all 3 axes (`(0..3).all(|k| lo <= hi' && hi >= lo')`) rather than 4 comparisons on the 2 orthogonal axes. Keeping the ray-axis interval `[v0[dir], v1[dir]]` in fact **prunes** behind-origin triangles (`tri_max < v0[dir]`) before predicate evaluation, while remaining exact because `min`/`max`/`<=`/`>=` introduce zero rounding error.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/octree.rs:180-412`.

#### `C22-25` — §5.2 Implicit LPI representation of ray hits & exact along-ray sorting
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:447-453 R`):**
  > *"Since our classification is entirely based on the analysis of the first intersection between the ray and each input mesh, it is necessary to sort intersection points. Firstly, we represent intersections implicitly, using the LPI (Line-Plane Intersection) points described in [Attene 2020]. Then, we use the exact comparator introduced in [Cherchi et al. 2020] to sort them from the closest to the ray emanating point to the furthest."* (`:447-453 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1169-1263`):**
  - `sort_hits_along_ray` (`inside_out.rs:1169-1263`) represents each hit triangle `t` as `GenericPoint3D::lpi(ray.v0, ray.v1, tv[0], tv[1], tv[2])` (`:1181`), orders hits along `ray.dir` via `less_than_on_{x,y,z}_indirect` (`:1185-1206`), and collapses exact equal keys to the first inserted (`IpSign::Zero => continue 'insert` at `:1201`, matching C++ `btree_set` semantics).
  - **Divergence (`N20`):** On explicit-origin rays, `before_origin` (`inside_out.rs:1249-1256`) rejects `s == IpSign::Negative || s == IpSign::Zero` (discarding **both** behind-origin `< 0` hits and ray-parameter-zero $\lambda = 0$ hits), whereas upstream C++ `sortIntersectedTrisAlong*` (`booleans.cpp:898`) used `< 0` and kept $\lambda = 0$ hits (which mislabeled patches touching a foreign input at an interior point). On generated rays (`ray.seed_tri = Some(seed)`, `:1222-1247`), `behind_seed_plane` discards hits whose `orient3d_gp3(&gps, &lpis[i])` strictly opposes `orient3d_gp3(&gps, &ray_v1)`.
- **Status:** **`DEVIATES`** *(intentional correctness strengthening `N20` over C++)*
- **Ledger:** `N20`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1825-1945` (point-touch $\lambda = 0$ regression test).

---

### §5.3 Classification & Perturbation of Degenerate Hits

#### `C22-26` — §5.3 Direct local-winding classification when ray crosses strict triangle interior
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:457-460 R, 473-476 L`):**
  > *"The simplest configuration is when $r$ crosses $M$ at a point that is interior to a triangle (Figure 6, left). In this case determining if the ray is passing from inside to outside or vice-versa consists in simply analyzing the triangle orientation, which is encoded in the local winding (triangle vertex order) and can be tested exactly as shown in Figure 5 (left)."* (`:458-460 R, 473-476 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1325, 1383, 1390-1415`):**
  - In `prune_intersections_and_sort_along_ray` (`inside_out.rs:1325`):
    ```rust
    IntersInfo::IntInTri => inters.push(t),
    ```
    Strict interior hits bypass perturbation completely, are sorted along the ray by `sort_hits_along_ray(soup, ray, &inters)` (`:1383`), and are classified directly by `analyze_sorted_intersections` (`:1390-1415`) via `orient3d(tv[0], tv[1], tv[2], ray.v1)`.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1510-1620`.

#### `C22-27` — §5.3 Perturbation of $p_\infty$ via `nextafter` on vertex/edge ray hits until strict interior hit found
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:484-492 L`):**
  > *"Each time a ray does not intersect a patch triangle at an inner point, we perturb the coordinates of $p_\infty$ by $\epsilon$ (without moving its starting point) until the crossing happens at a point that is interior to a mesh triangle. Perturbation of point coordinates is performed using the next floating-point number representable starting from a given number (using std::nextafter). The perturbed ray is then tested again for intersection, and the operation is repeated until a valid intersection point is found."* (`:485-492 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1087-1153, 1326-1379`):**
  - On `IntInV(k)` or `IntInEdge(k)`, `prune_intersections_and_sort_along_ray` (`:1326-1379`) collects all same-label input triangles incident to `v_id` (`ring`, `:1328-1333`) or `(a, b)` (`edge_tris`, `:1351-1357`), marks them `visited`, and calls `perturb_ray_and_find_inters_tri`.
  - **Divergences (`UL-02` / `N101`, `N19`, and minor `UL-01`):**
    1. **Fixed 8-offset 1-ULP cap + silent tangential assumption (`UL-02` — CONFIRMED REAL by reviewer test, ledger `N101`, `inside_out.rs:1087-1153, 1337-1342, 1367-1378`):** Instead of repeating perturbation *"until a valid intersection point is found"* (`:491-492 L`), `perturb_ray` only tests 8 fixed single-ULP `STEPS` `(1,0), (1,1), (0,1), (-1,1), (-1,0), (-1,-1), (0,-1), (1,-1)` (`:1090-1099`). On an octagonal pyramid/bipyramid apex with 45° spoke planes (e.g. centered at `(y_c, z_c)` with symmetric ULPs), all 8 `next_up`/`next_down` offsets land in the 8 spoke planes (`orient3d == Sign::Zero`), `perturb_ray_and_find_inters_tri` returns `None`, and `:1340` skips the crossing as tangential — **silently mislabeling an enclosed patch as `outside`**. *(A separate agent is fixing this as ledger `N101`.)*
    2. **Coherent per-offset winner selection (`N19`, `inside_out.rs:1142-1150`):** Evaluates all ring triangles under ONE `offset` and picks the closest forward hit along `p_ray`, fixing C++'s mixed-ray `inters_tris` accumulation bug (`booleans.cpp:801-828`).
    3. **Early return before `before_origin` check (`UL-01`, LOW / theoretical ~1-ULP corner at `inside_out.rs:1147-1150`):** Checks `if !hits.is_empty()` before `sort_hits_along_ray` discards behind-origin hits (`sorted.first().copied()`, avoiding C++ `booleans.cpp:1016-1045` empty-vector `inters_tris[0]` UB). Because `N69` (`:507-546`) already rejects Tier-1 rays coplanar with any foreign candidate (`orient3d(tri, v0) == Zero && orient3d(tri, v1) == Zero`) and `N20` (`:1255`) drops `v == v0` by design, this can only drop a forward hit if a ring triangle is near-parallel to the ray within ~1 ULP (or via an un-gated Tier-2 ray under `UL-06`).
- **Status:** **`DEVIATES`**
- **Ledger:** `N19`, `N101` (`UL-02`, fix in progress; `UL-01` minor).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1625-1698`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:315-520`. Unpinned in `main` for `UL-02` → see `TC-01`.

#### `C22-28` — §5.3 Exact 3D ray-triangle interior test via three tetrahedron `orient3D` signs
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:492-500 L`):**
  > *"Note that perturbed rays are no longer axis aligned, therefore we cannot reduce the ray-triangle intersection to a 2D problem, and we need to perform this test in 3D. The full test consists in checking the sign of three tetrahedra, formed considering the two ray endpoints and the endpoints of each triangle edge. If all signs are strictly positive, or negative, there is an intersection inside the triangle. The computational overhead of this check is minimal (three orient2D for the 2D case and three orient3D for the 3D case)."* (`:492-500 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1077-1083`):**
  - `check_intersection_inside_triangle_3d` (`inside_out.rs:1077-1083`) evaluates the three edge-tetrahedron `orient3d` signs (with `N24` rational zero-certification) and returns `true` iff all three share one strict non-zero sign:
    ```rust
    fn check_intersection_inside_triangle_3d(ray: &Ray, tv: [Point3; 3]) -> bool {
        let or01 = orient3d(tv[0], tv[1], ray.v0, ray.v1);
        let or12 = orient3d(tv[1], tv[2], ray.v0, ray.v1);
        let or20 = orient3d(tv[2], tv[0], ray.v0, ray.v1);
        (or01 == Sign::Positive && or12 == Sign::Positive && or20 == Sign::Positive)
            || (or01 == Sign::Negative && or12 == Sign::Negative && or20 == Sign::Negative)
    }
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** `N24`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1625-1698`.

#### `C22-29` — §5.3 Coplanar / tangential ray-triangle graze handling
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:469-471, 480-488 L`):**
  > *"Note that the pathological cases depicted in the figure are not exhaustive. In fact, rays may also be tangent at a (coplanar) triangle."* (`Fig. 6, :469-471`)
  > *"Four of these cases are depicted in Figure 6, but there are also others (e.g. when a tangent ray is also coplanar to a triangle). ... Each time a ray does not intersect a patch triangle at an inner point, we perturb the coordinates of $p_\infty$ by $\epsilon$ (without moving its starting point) until the crossing happens at a point that is interior to a mesh triangle."* (`:480-488 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:347-381, 507-546, 601-702, 1055-1059`):**
  - On a triangle whose plane contains the axis-aligned ray, orthogonal 2D projection collapses the triangle to a collinear 2D segment (at least two of `or01, or12, or20` are `Sign::Zero`), which `fast_2d_check_intersection_on_ray` (`inside_out.rs:1057-1058`) maps to `IntersInfo::Discard` — meaning the coplanar triangle itself is discarded while its bounding edges/vertices on adjacent slanted triangles trigger `IntInEdge`/`IntInV`.
  - As documented in `N69` (`docs/yang_deviations.md:6456-6510`), when a ray **grazes** along a foreign solid's boundary (`orient3d(tv, v0) == Zero && orient3d(tv, v1) == Zero`), perturbing $p_\infty$ tilts the ray *into* the grazed solid while testing only the local incident ring, turning a 0-crossing tangent graze into a false 1-crossing winner!
  - `cherchi-rs` mitigates this on the explicit-vertex branch (`inside_out.rs:601-627`) via `ray_grazes_foreign_input` (`:507-546`) plus the post-condition `refute_inner_label_by_input_bounds` (`:347-381`, `Err(InnerLabelOutsideInputBounds)`).
  - **Gaps:** (1) The **generated-ray branch (`inside_out.rs:629-702`) never calls `ray_grazes_foreign_input` (`UL-06`)**, so both originless patches and patches whose explicit candidates all grazed (falling through `:627` into `:629`) can select a foreign-grazing synthetic ray; (2) `refute_inner_label_by_input_bounds` (`:347-381`) only catches false-positive `inside` labels when an explicit patch vertex lies strictly outside the foreign input's AABB — it cannot catch a false `inside` label on an all-implicit patch or inside a non-convex cavity of $B$'s AABB, nor any false-negative `outside` label.
- **Status:** **`PARTIAL`**
- **Ledger:** `N69` (`UL-06` unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:2120-2335`. Unpinned for generated-ray graze (`UL-06`) & non-convex AABB graze → see `TC-02`, `TC-03`.

---

### §1 / §3 / §5 Boolean Operator Filtering & Output Orientation

#### `C22-30` — §1 / §3 Boolean patch/triangle selection rules (`Union`, `Intersect`, `Subtract`, `Xor`)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:86-90 L, 280-290 L`):**
  > *"The output of the algorithm is eventually obtained by filtering the patches in the arrangement according to this information. For example, the union of two triangle meshes $M_1$ and $M_2$ ($M_1 \cup M_2$) is the set of patches of $M_1$ that are outside $M_2$ plus the patches of $M_2$ that are outside $M_1$."* (`:280-286 L`)
- **Code (`crates/cherchi-rs/src/labeled_arrangement.rs:113-157`):**
  - `LabeledArrangement::keep_tri` (`labeled_arrangement.rs:125-157`) ports C++ `booleans.cpp:1394-1485` bitset rules verbatim:
    - `BoolOp::Union`: `inside_count == 0`
    - `BoolOp::Intersect`: `xor_count == num` (every solid has `on_surface(k) != inside[k]`)
    - `BoolOp::Subtract`: `(on_surface(0) && inside_count == 0) || (!on_surface(0) && inside[0] && inside_count == 1)`
    - `BoolOp::Xor`: `inside_count == 0 || xor_count == num`
  - **Gap on coplanar overlaps:** Because `surface` is an unsigned set `{InputId(0), InputId(1)}` without normal-agreement (`opposite` vs flush), for any coplanar overlap triangle (`surface = [0, 1], inside = [false, false]`): `inside_count == 0` and `xor_count == 2`, so `keep_tri` returns `true` for **`Union`, `Intersect`, AND `Subtract`**, regardless of whether the two solids lie on the same side or opposite sides of the shared plane! In standalone `NativeBoolean::boolean` (`native.rs:536-565`), opposite-normal stacked cubes therefore retain an internal partition wall on `Union` and emit a zero-volume 2D sheet on `Intersect`, while same-normal flush pockets retain a cap over the carved opening on `Subtract` (verified in `coplanar_pocket_parity.rs:28-42`). (`yang-rs` overrides this in `boolean.rs:2273-2363`; see `YC22-04`.)
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeled_arrangement.rs:160-349`; `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776`.

#### `C22-31` — §3 / §5.3 Output triangle winding reversal on cavity / interior walls
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:238-241 R, 473-476 L`):**
  > *"The output is a mesh $B$ that contains the result of applying the Boolean operator to the input meshes. By definition of Boolean, $B$ is a sub-volume of the input, hence it is bounded by portions of input triangles."* (`:238-241 R`)
  > *"determining if the ray is passing from inside to outside or vice-versa consists in simply analyzing the triangle orientation, which is encoded in the local winding (triangle vertex order)"* (`:473-475 L`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:546-549, 583-589`):**
  - `flip_at_emission` (`native.rs:583-589`) reverses triangle winding (`tri.swap(1, 2)` at `:548`) for `BoolOp::Subtract` when `!la.surface[t].contains(&InputId(0))` (kept $B$-surface triangles forming the carved cavity wall) and for `BoolOp::Xor` when `la.inside[t].iter().any(|&b| b)`:
    ```rust
    fn flip_at_emission(la: &LabeledArrangement, op: BoolOp, t: usize) -> bool {
        match op {
            BoolOp::Union | BoolOp::Intersect => false,
            BoolOp::Subtract => !la.surface[t].contains(&InputId(0)),
            BoolOp::Xor => la.inside[t].iter().any(|&b| b),
        }
    }
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-790` (checks signed volume of `Subtract` and `Xor`).

---

### §6–§7 Applications, Benchmarks, Variadic Scaling & Limitations

#### `C22-32` — §6.1 Interactive rotation & ARAP deformation demos with double-buffered rendering
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:506-572, 623-631`):** Interactive GUI rotation and ARAP deformation demos.
- **Code:** Out of scope for the `cherchi-rs` kernel library crate.
- **Status:** **`N/A`**
- **Ledger:** —
- **Test pin:** —

#### `C22-33` — §6.2–§6.3 Thingi10K (3,814 pairs) & high-resolution (21.6M-tri) benchmark harnesses
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:579-620, 629-690`):** Paper evaluation benchmarks against `libigl` and Cherchi 2020.
- **Code:** Out of scope; `cherchi-rs` maintains its own parity and adversary suites under `crates/cherchi-rs/tests/`.
- **Status:** **`N/A`**
- **Ledger:** —
- **Test pin:** —

#### `C22-34` — §6.4 Variadic Boolean $O(1)$-ray-per-patch scaling across $n$ input meshes
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:696-698 L`):**
  > *"This is possible thanks to our inside/outside labeling based on ray casting, which shoots a ray for each surface patch regardless of the number of input shapes."* (`:696-698 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:221-278`; `crates/cherchi-rs/src/labeling/native.rs:109, 529`):**
  - `compute_inside_out` (`inside_out.rs:221-278`) casts one ray per patch and classifies all foreign input meshes from a single `TriOctree` traversal + single `sort_hits_along_ray` + single `analyze_sorted_intersections` pass, matching the paper's $O(1)$-ray-per-patch scaling.
  - **Gap:** Public `native_labeled_arrangement` (`native.rs:109`) and `NativeBoolean` (`native.rs:529`) only expose binary `(a: &Mesh, b: &Mesh)` entry points (`C22-05`).
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:640-725` (3-solid internal ray-cast tests; end-to-end 3-mesh arrangement+ray-cast+keep parity is `TC-06`).

#### `C22-35` — §7.1 Frame-to-frame temporal caching of octree and adjacencies
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:513-515 R, 740-752 L`):** Explicitly left as future work in the paper itself (*"Booleans are applied naively, meaning that each frame is computed separately, without propagating cached data"*).
- **Code:** Stateless per-call execution.
- **Status:** **`N/A`**
- **Ledger:** —
- **Test pin:** —

#### `C22-36` — §7.1 Cascaded Booleans & snap-rounding defect accumulation
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:755-765 L, 700-705 R`):**
  > *"We currently do not support cascaded Booleans, where the inputs are the result of a previous Boolean operation. While in our tool it is technically possible to cascade Booleans, no guarantees on the result can be given because after each frame we snap exact coordinates to floating points, possibly introducing small mesh defects that may spoil subsequent operations."* (`:757-765 L`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:515-521`; `crates/yang-rs/src/boolean.rs:1929-2159`):**
  - Standalone `NativeBoolean::boolean` (`native.rs:515-521`) snaps `VertexCoords` to `f64` via `RBig::to_f64().value() / m` with no guarantee under raw mesh cascading, matching §7.1.
  - In Waffle Iron's full CAD pipeline (`yang-rs`), mesh outputs are **never** fed raw into a subsequent mesh boolean: Stages 3–6 reconstruct an exact analytic `BRep` and re-tessellate cleanly at Stage 1, while `yang-rs/src/boolean.rs:1929-2159` reconciles single-boolean `f64` snap twins via KV10/KV15 planar near-weld, PR-6 cylinder-rim weld, and P0014 `weld_lpi_pencils`.
- **Status:** **`PARTIAL`**
- **Ledger:** `N36`
- **Test pin:** `crates/cherchi-rs/tests/r0046_patch_label_parity.rs` (post-Stage-0 chained CAD operands).

---

## 2. `yang-rs` Consumer Compliance Matrix (`LabeledArrangement` Contract, `YC22-01` .. `YC22-08`)

These rows audit where `crates/yang-rs` consumes `cherchi-rs`'s `LabeledArrangement` (`crates/cherchi-rs/src/labeled_arrangement.rs:34-103`) against both Cherchi 2022 and Yang et al. 2025 (`refs/text/yang2025_hybrid_boolean.txt`) + `docs/yang_faithfulness_roadmap.md` / `docs/yang_functional_roadmap.md`.

#### `YC22-01` — Stage-2 `MeshBoolean::labeled_arrangement` invocation
- **Paper (`refs/text/yang2025_hybrid_boolean.txt:297-302 L`; `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:246-256 R`):**
  > *"First, a bijective mapping between each B-Rep model and its triangular discretization is generated, and the intersection between the triangle meshes is performed, as described in Sec. 4.1 and Sec. 4.2. ... Then we leverage the fast mesh intersection methods described in [Cherchi et al. 2022]."* (`refs/text/yang2025_hybrid_boolean.txt:296-302 L`)
  > *"In the first step, intersecting mesh elements are split and intersection lines are incorporated in the elements connectivity. With slight abuse of notation we refer to this step as mesh arrangement [Cherchi et al. 2020]."* (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:247-250 R`)
- **Code (`crates/yang-rs/src/boolean.rs:1860-1863`; `crates/cherchi-rs/src/labeling/native.rs:567-573`):**
  ```rust
  let la = backend
      .labeled_arrangement(mesh_a, mesh_b)
      .map_err(YangError::MeshBooleanFailed)?;
  ```
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** Workspace `yang-rs` boolean suite (`crates/yang-rs/src/tests_unit/`).

#### `YC22-02` — Post-arrangement `f64` snap reconciliation (`lpi_through_vertex`, KV10, KV15, PR-6)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:238-243 L, 757-765 L`):** Cherchi 2022 snaps each distinct exact `VertexCoords` independently to `f64` and leaves snap-rounding reconciliation unsolved (§7.1).
- **Code (`crates/cherchi-rs/src/labeled_arrangement.rs:78-99`; `crates/cherchi-rs/src/labeling/native.rs:212-215, 234-269`; `crates/yang-rs/src/boolean.rs:524-575, 1929-2159`):**
  - `lpi_through_vertex_record` (`native.rs:234-269`) exports `lpi_through_vertex: Vec<(u32, u32)>` by indexing every emitted `VertexCoords::Explicit(p)` by bit-exact `[u64; 3]` coordinates (`:238-247`) and matching `VertexCoords::Lpi { line, .. }` endpoints (`line[0], line[1]`) against that explicit table (`:251-265`), recording `(lpi_idx, explicit_idx)` whenever an LPI's defining edge passes through an explicit vertex.
  - `yang-rs/src/boolean.rs:1929-2159` builds a `weld: Vec<u32>` map combining bit-exact deduplication (`:1990-1999`), all-planar / per-vertex-planar `TAU_WORK * (1 + scale)` near-weld (`:1929-1984`, `:2001-2041`), coincident-cylinder rim weld (`:2072-2144`), and `weld_lpi_pencils(&la.mesh.verts, &la.lpi_through_vertex, &mut weld)` (`:2158`), plus `#146` collapsed-wedge dedup (`:65-95, 2396-2435`).
- **Status:** **`DEVIATES`** *(intentional hybrid-pipeline snap-reconciliation layer compensating for `f64` emission between exact arrangement and B-Rep reconstruction)*
- **Ledger:** `N36`
- **Test pin:** `crates/yang-rs/src/tests_unit/` (`p0014`, `kv15`, `#146` wedge tests).

#### `YC22-03` — When `keep_set(op)` filters triangles: BEFORE vs. AFTER Stage-4 vertex relocation
- **Paper (`refs/text/yang2025_hybrid_boolean.txt:574-579 R`):**
  > *"4.4.2 Mesh and B-Rep Booleans. Mesh Booleans. After trimming the meshes using the intersection curves, we directly apply a standard inside/outside classification step [Cherchi et al. 2022] to identify the triangles that need to be retained, thus completing the mesh Boolean operation."* (`refs/text/yang2025_hybrid_boolean.txt:574-579 R`)
- **Code (`crates/yang-rs/src/boolean.rs:2162, 2248, 3049-3054`; `crates/yang-rs/src/brep.rs:406-510`; `crates/yang-rs/src/stage5_topology.rs:1109-1127`):**
  - `boolean.rs:2162` still executes `let kept = la.keep_set(op);` **before** Stage-3 SSI and Stage-4 relocation (`reconstruct_topology_stage4` at `:3063`), rather than after §4.4.1 mesh trimming/relocation.
  - Increment `F1-I1` has landed: `TriangleSideLabelMap` (`brep.rs:473-510`) carries `(cherchi_patch, inside, surface)` in 1:1 lockstep with `kept_submesh.tris` across Stage 4/5 and `debug_assert_eq!(side_labels.keep_set(op), (0..mesh.tris.len()).collect::<Vec<_>>())` at `stage5_topology.rs:1123-1127`, but increments `F1-I2..I4` (`docs/yang_faithfulness_roadmap.md`) have not yet moved filtering after Stage 4.
- **Status:** **`ORDER`**
- **Ledger:** `N80` (active Milestone `F1` in `docs/yang_faithfulness_roadmap.md`).
- **Test pin:** `crates/yang-rs/src/tests_unit/s442_side_labels.rs:51-130`.

#### `YC22-04` — Coplanar overlap-sheet (`la.surface[t].len() > 1`) keep semantics & `TriangleSideLabel` F1 hazard
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:243-245 R`; `refs/text/yang2025_hybrid_boolean.txt:696-706`):** Coplanar overlap triangles belong to multiple input meshes (`surface = {A, B}`); solid Boolean semantics retain the shared sheet iff exactly one side of the surface bounds the result (`!opposite` for `Union`/`Intersect`, `opposite` for `Subtract`).
- **Code (`crates/yang-rs/src/boolean.rs:2251-2363` vs. `crates/yang-rs/src/brep.rs:406-468`):**
  - At `boolean.rs:2273-2363`, `yang-rs` intercepts `la.surface[orig_t].len() > 1`, resolves `opposite` from Stage-0 `PairPlane` (using `la.source` → `(fa, fb)`) or `cyl_pairs`, and drops `!keep_sheet` triangles before building `compact_tris` and `side_labels`.
  - **Unledgered F1 hazard (`UL-08`):** `TriangleSideLabel` (`brep.rs:406-415`) stores **only** `cherchi_patch: u32`, `inside: [bool; 2]`, and unsigned `surface: [bool; 2]` — omitting `opposite` (or sheet drop/keep disposition)! Its `keep_for_op` method (`brep.rs:454-467`) copies Cherchi's unsigned `LabeledArrangement::keep_tri` verbatim, which evaluates to **`true` on every `[true, true], [false, false]` coplanar sheet for `Union`, `Intersect`, and `Subtract` alike**! Today's `stage5_topology.rs:1124` `debug_assert_eq!` passes **only** because `boolean.rs:2361` already discarded `!keep_sheet` triangles before `side_labels` was constructed. As soon as Milestone `F1-I2` (`N80`) passes the unfiltered arrangement through Stage 4 and calls `side_labels.keep_set(op)` afterward, every stacked-solid `Union`/`Intersect` (`opposite == true`) and every flush-pocket `Subtract` (`opposite == false`) will retain the wrong coplanar membrane!
- **Status:** **`PARTIAL`**
- **Ledger:** `N25`, `N80` (`UL-08` F1 hazard in `TriangleSideLabel::keep_for_op` is unledgered).
- **Test pin:** `crates/yang-rs/src/tests_unit/membrane.rs`; `crates/yang-rs/src/tests_unit/s442_side_labels.rs` (does not yet test `keep_for_op` on `!keep_sheet` coplanar membranes → see `TC-04`).

#### `YC22-05` — `BoolOp::Xor` handling at the `cherchi-rs` → `yang-rs` boundary
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:234-236 R`):** Paper specifies only `{union, intersection, subtraction}`.
- **Code (`crates/cherchi-rs/src/labeled_arrangement.rs:153-155`; `crates/yang-rs/src/boolean.rs:2225-2234`):** `cherchi-rs` implements `BoolOp::Xor` (following `booleans.cpp:1463-1510`); `yang-rs` rejects non-empty `BoolOp::Xor` loudly with `Err(YangError::UnsupportedOp(BoolOp::Xor))` (`boolean.rs:2232-2234`) because Stage-5/6 B-Rep topology reconstruction does not yet assemble multi-shell void topologies.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776` (tests `Xor` in `cherchi-rs`).

#### `YC22-06` — Per-op winding flip (`flip_for_op`) & B-Rep face sense propagation
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:238-241 R, 473-476 L`; `refs/text/yang2025_hybrid_boolean.txt:290-292 L`):**
  > *"The output is a mesh $B$ that contains the result of applying the Boolean operator to the input meshes. By definition of Boolean, $B$ is a sub-volume of the input, hence it is bounded by portions of input triangles."* (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:238-241 R`)
  > *"The results generated by our method are ensured to be watertight B-Rep models corresponding to the specified Boolean operations."* (`refs/text/yang2025_hybrid_boolean.txt:290-292 L`)
- **Code (`crates/yang-rs/src/boolean.rs:35-45, 2384-2386`; `crates/yang-rs/src/stage5_topology/emit.rs:213-218, 402-413`):**
  - `flip_for_op` (`boolean.rs:35-45`) flips kept triangles on `BoolOp::Subtract` iff `!on_a` (`!la.surface[t].iter().any(|&LaInputId(id)| id == 0)`):
    ```rust
    if flip_for_op(op, &la, orig_t) {
        tri.swap(1, 2);
    }
    ```
  - `stage5_topology/emit.rs:213-218` sets curved `BRepFace.reversed` from `op == Subtract && info.input == B`, and `:402-413` derives planar face outward normal sign from the largest-area boundary cycle's post-flip Newell vector.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/yang-rs/src/tests_unit/` Subtract volume & orientation tests.

#### `YC22-07` — Per-triangle provenance `LabeledArrangement::source` → direct `(InputId, face)` B-Rep attribution
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:241-245 R`):**
  > *"Mapping the result of a Boolean to the input shapes is useful in many applications, therefore for each output triangle we propagate information on its origin. Note that an output triangle can belong to many input meshes, for example in the case of meshes that overlap at a coplanar region."* (`:241-245 R`)
- **Code (`crates/cherchi-rs/src/labeled_arrangement.rs:52-66`; `crates/cherchi-rs/src/labeling/native.rs:195-220`; `crates/yang-rs/src/boolean.rs:65-95, 2304-2324, 2970-3042`):**
  - `LabeledArrangement::source` (`labeled_arrangement.rs:65`) carries `Vec<Vec<(InputId, u32)>>` mapping each arrangement triangle to the exact `(operand, input_tri_idx)` pair(s) it was split/merged from:
    ```rust
    pub source: Vec<Vec<(InputId, u32)>>,
    ```
  - `yang-rs/src/boolean.rs:2970-3042` maps `la.source[orig_t]` directly through `(tri_face_a, tri_face_b)` to attribute kept triangles to `(InputId, face)` without geometric proximity whenever Stage-1 `tri_face` lineage is present (falling back to geometric attribution only for `BRep::from_mesh` or chained boolean outputs whose `tri_face` is empty at `boolean.rs:3088`). Also used at `:2304-2324` to disambiguate n-ary Stage-0 coplanar `PairPlane` groups and at `:65-95` (`wedge_reject_reason`).
- **Status:** **`FAITHFUL`**
- **Ledger:** `N4` (resolved for `BRep::new` inputs; note `docs/yang_functional_roadmap.md:3931-3941` is stale — see §5).
- **Test pin:** `crates/cherchi-rs/src/labeling/native.rs:820-940` (`source` provenance unit tests).

#### `YC22-08` — Per-edge constrained submesh provenance `LabeledArrangement::intersection_edges`
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:250-255 R, 315-321 R`):**
  > *"Inserting a segment amounts to eliminating, from the current tessellation, all triangles that conflict with it, and then re-triangulate the so-generated polygonal pocket, while making sure that the wanted segment is part of the new tessellation."* (`:317-321 R`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:1072-1081`; `crates/cherchi-rs/src/labeled_arrangement.rs:68-76`; `crates/yang-rs/src/boolean.rs:1915-1923, 2171-2190`):**
  - After `enforce_constraints` in `soup.rs:1072-1081`, every `subm.edge_is_constr(e)` sub-edge is welded to global IDs and inserted into `ArrangementSoup::intersection_edges: BTreeSet<(u32, u32)>`:
    ```rust
    for e in 0..subm.num_edges() {
        if subm.edge_is_constr(e) {
            let (l0, l1) = subm.edge(e);
            let g0 = weld_local(l0, &mut globals);
            let g1 = weld_local(l1, &mut globals);
            if g0 != g1 {
                intersection_edges.insert((g0.min(g1), g0.max(g1)));
            }
        }
    }
    ```
  - `yang-rs/src/boolean.rs:2171-2190` maps `la.intersection_edges` through `weld` into `PosKeyedEdgeSet` for Stage-3 SSI curve assignment and `failures::record_missed_intersections` (`:1915-1923`).
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/native.rs:945-1060` (`intersection_edges` provenance tests).

---

## 3. Deep-Dive Check: Exactness of §5 Ray-Cast & Degenerate Hit Handling

The user prompt singles out **exactness of the §5 ray-cast (exact ray/triangle tests and handling of degenerate hits)** as the highest-value check of this audit. Walking the complete call graph in `crates/cherchi-rs/src/labeling/inside_out.rs` (`:1-1430`), `crates/cherchi-rs/src/labeling/octree.rs` (`:1-175`), `crates/cherchi-rs/src/aabb_tree.rs` (`:1-170`), and `crates/cherchi-rs/src/predicates/` reveals **what is genuinely exact** and **where degenerate-hit handling diverges from the paper**.

### 3.1 Step-by-Step Exactness Trace of `compute_inside_out`

1. **Input-shell substrate (`soup.in_tris` / `soup.in_labels`, `soup.rs:1089-1109`):**
   - Unlike output triangles (`soup.tris` / `soup.labels`), which carry subdivided implicit vertices and OR-merged coplanar labels `{A, B}`, the ray-cast tests **unsplit explicit input triangles** in `soup.in_tris` after step-11 `dupl_triangles` restoration (`coplanar_dupl_restore.rs`). Every vertex of `soup.in_tris` is `VertexCoords::Explicit(Point3)` (`inside_out.rs:161-167`).
   - **Exactness verdict:** **Exact** at CAD scales. *(Latent edge case `UL-04`: `points_are_collinear_3d` at `collinearity.rs:30-33` calls raw `geometry_predicates::orient2d == 0.0` without `N24` `RBig` zero-certification, which only underflows when coordinate differences are `~1e-146`; another agent is routing surviving raw `orient2d` calls through `crate::predicates::orient2d`.)*

2. **Broad-phase candidate filtering (`TriOctree::query_aabb`, `octree.rs:60-82`, + `ray_aabb` / `tri_in_ray_aabb`, `inside_out.rs:386-423`):**
   - `Aabb::from_points` (`aabb_tree.rs:35-48`) widens leaf/internal node boxes of the 8-way octree (`aabb_tree.rs:86`) by 1 ULP (`next_down` / `next_up`), and `prune_intersections_and_sort_along_ray` (`inside_out.rs:1300-1312`) queries `octree.query_aabb(&ray_box)` and post-filters leaf triangles with `tri_in_ray_aabb` (`inside_out.rs:412-423`) using exact unpadded `min`/`max` comparisons against `[min(v0, v1), max(v0, v1)]`.
   - **Exactness verdict:** **100% exact superset** for every axis-aligned ray (`min`/`max`/`<=`/`>=` incur zero rounding).

3. **Unperturbed 2D ray-triangle classification (`fast_2d_check_intersection_on_ray`, `inside_out.rs:1032-1073`):**
   - Projects `ray.v0` and `tv[0..3]` orthogonally onto YZ (`Axis::X`), ZX (`Axis::Y`), or XY (`Axis::Z`) via `drop_coord` (`:1022-1028`, zero coordinate rounding) and calls `crate::predicates::orient2d` (`predicates/mod.rs:26` → `orient.rs:134-156`) **with `N24` `RBig` zero-certification** on all three triangle edges (`:1040-1042`).
   - When the triangle is coplanar with the ray direction (`s0..s2` have two/three zeros with no strict interior sign, `:1057-1058`), returns `IntersInfo::Discard`, matching C++ `booleans.cpp:899-900`; the ray interacts only with the `IntInEdge` / `IntInV` boundary shared with adjacent non-parallel faces of the closed shell.
   - **Exactness verdict:** **100% exact** (all `Sign::Zero` decisions are certified in `RBig`).

4. **Perturbation of degenerate vertex (`IntInV`) & edge (`IntInEdge`) hits (`perturb_ray_and_find_inters_tri`, `inside_out.rs:1087-1152`):**
   - Perturbs `ray.v1` ($p_\infty$) on the two axes orthogonal to `ray.dir` by 8 fixed single-ULP steps (`next_up` / `next_down`):
     `offset 0..8 = [(+1,0), (-1,0), (0,+1), (0,-1), (+1,+1), (-1,-1), (+1,-1), (-1,+1)]`.
   - Tests each ring triangle in 3D via `check_intersection_inside_triangle_3d` (`inside_out.rs:1077-1083`), which calls `orient::orient3d` (**with** `N24` `RBig` zero-certification!).
   - **Crack #1 (`UL-02` / Ledger `N101` — CONFIRMED REAL — Fixed 8-offset 1-ULP table vs. paper's unbounded perturbation loop):**
     Cherchi 2022 §5.3 (`:485-492 L`) specifies: *"the operation is repeated until a valid intersection point is found."* Instead, `inside_out.rs:1095-1152` stops after 8 single-ULP offsets and `:1337-1342, 1367-1378` assumes `None` means *"the event is tangential/grazing — it contributes no parity crossing; skip it."*
     **Exact failure trace (confirmed by independent reviewer test; tracked for fix as `N101`):** Let solid $B$ be a closed **regular 8-sided pyramid / bipyramid** (standard $N = 8$ CAD cone/cylinder tessellation) centered on symmetric-ULP coordinates `(y_c, z_c) = (1.5, 1.5)` (where `1.5.next_up() - 1.5 == 1.5 - 1.5.next_down() == 2^-52`), with apex $V = (1.0, 1.5, 1.5)$ and 8 spoke planes at $0^\circ, 45^\circ, 90^\circ, 135^\circ$. Let solid $A$ sit strictly inside $B$ with interior vertex `v0 = (-0.5, 1.5, 1.5)`.
     - N69 `ray_grazes_foreign_input` (`:507-546`) returns **`false`** on `+X` (`v0` is strictly inside all 8 cone planes; `v1` is strictly outside all 8 cone planes).
     - The `+X` ray hits apex $V$ (`IntInV(V)`).
     - Every one of the 8 single-ULP offsets `(±2^-52, 0)`, `(0, ±2^-52)`, `(±2^-52, ±2^-52)` moves `p_ray` along one of the 4 planes $y = 1.5$, $z = 1.5$, $y - 1.5 = z - 1.5$, $y - 1.5 = -(z - 1.5)$ — **landing `orient3d == Sign::Zero` in one of the 45° spoke planes for all 8 offsets**!
     - `perturb_ray_and_find_inters_tri` exhausts `0..8` and returns `None`; `:1340` skips $V$ as "tangential"; `sorted` is empty; solid $A$ (100% inside $B$) is **silently misclassified as `outside` $B$**!
   - **Crack #2 (`N69` + `UL-06` — Generated-ray branch omits `ray_grazes_foreign_input`):**
     When an exterior ray lies in the plane of a foreign face (`orient3d(tv, v0) == Sign::Zero && orient3d(tv, v1) == Sign::Zero`) and overlaps it (`coplanar_ray_overlaps_tri`, `:554-600`), an `IntInEdge` or `IntInV` touch on solid $B$ is genuinely tangential (0 crossings), yet `perturb_ray_and_find_inters_tri` tilts $p_\infty$ into $B$'s interior while only testing the local 1-ring/2-ring of the grazed feature (`:1327, 1354`), recording a single uncompensated crossing.
     - In `find_ray_endpoints` (`:582-702`), the explicit interior-vertex branch (`:592-627`) filters candidate rays through `ray_grazes_foreign_input` (`:605`, `N69`), **but the generated-ray branch (`:629-702`) never calls `ray_grazes_foreign_input`** (`UL-06`), returning the first centroid ray (`:694, 698`) even when it grazes a foreign input triangle!
   - **Crack #3 (`UL-01` — Minor edge case in `:1147-1150` early return):**
     `check_intersection_inside_triangle_3d` (`:1077-1083`) tests the infinite line through `(p_ray.v0, p_ray.v1)`, whereas `sort_hits_along_ray` (`:1255`) filters out `before_origin` ($\lambda \le 0$). At `:1147-1150`, `if !hits.is_empty() { let sorted = sort_hits_along_ray(...); return sorted.first().copied(); }` returns `None` instead of continuing to the next offset if `hits` is non-empty and `sorted` is empty. *(Upstream C++ `booleans.cpp:1016-1045` had UB here, reading `inters_tris[0]` on an empty vector.)* Because `p_ray` passes through `v0` and a 1-ring triangle shares the forward hit vertex `v` ($\lambda > 0$), `p_ray` can pierce a 1-ring triangle at $\lambda \le 0$ only if (a) that triangle's plane contains the unperturbed ray line (`orient3d(tri, v0) == Zero && orient3d(tri, v1) == Zero` — already rejected on Tier 1 by `N69`'s `ray_grazes_foreign_input`), (b) `v == v0` (dropped by design under `N20`), or (c) the triangle is near-parallel to the ray within ~1 ULP (or reached via an un-gated Tier-2 ray under `UL-06`). Replacing `if !hits.is_empty()` with `if let Some(&first) = sorted.first() { return Some(first); }` closes this cleanly.

5. **Along-ray LPI sorting (`sort_hits_along_ray`, `inside_out.rs:1169-1263`) & first-hit orientation (`analyze_sorted_intersections`, `:1390-1415`):**
   - Exact indirect `less_than_on_{x,y,z}_indirect` (`predicates/indirect/mod.rs`) compares rational LPI coordinates along `ray.dir` via interval filter → `RBig`, with `N20` (`<= Ordering::Equal`, `:1255`) soundly rejecting $\lambda \le 0$ hits.
   - `analyze_sorted_intersections` (`:1390-1415`) evaluates `orient::orient3d(tv[0], tv[1], tv[2], ray.v1)` (`:1404`, with `N24` `RBig` zero-certification).
   - **Exactness verdict:** **100% exact** across the CAD coordinate domain. *(Algebraic check on `:1404` testing unperturbed `ray.v1` rather than `p_ray.v1`: any triangle `tv` containing both `ray.v0` and `ray.v1` in its plane is intersected by `p_ray` only at `ray.v0` ($\lambda = 0$), which `before_origin` (`N20`, `:1255`) discards; for any non-coplanar `tv` through the hit vertex/edge `q` ($\lambda > 0$), `ray.v0`, `q`, and `ray.v1` are collinear with `q` strictly between `ray.v0` and `ray.v1` (`max_c + 0.5 > max_c`), so `orient3d(tv, ray.v1)` has the exact opposite non-zero sign of `orient3d(tv, ray.v0)` and cannot be `Zero`.)*

---

## 4. Code With No Paper Row

Every mechanism, data channel, guard, helper, or environment knob in `crates/cherchi-rs/` (and the `yang-rs` Stage-2 consumer boundary) that has **no procedural counterpart in Cherchi et al. 2022**:

| Site (`path:line` at `a2058a01`) | Mechanism | Origin / Purpose | Ledger / Spec |
|---|---|---|---|
| `crates/cherchi-rs/src/labeled_arrangement.rs:153-155`; `crates/cherchi-rs/src/labeling/native.rs:587`; `crates/yang-rs/src/boolean.rs:43, 2232-2234` | `BoolOp::Xor` (symmetric difference) keep-rule & inside-flip | Ported from upstream C++ `booleans.cpp:1463-1510` (`boolXOR`), although Cherchi 2022 §3 (`:234-235 R`) lists only `{union, intersection, subtraction}`. Rejected loudly on non-empty kept sets in `yang-rs`. | — |
| `crates/cherchi-rs/src/labeled_arrangement.rs:68-99`; `crates/cherchi-rs/src/arrangements/soup.rs:1072-1081`; `crates/cherchi-rs/src/labeling/native.rs:212-215, 234-269` | `LabeledArrangement::intersection_edges` and `lpi_through_vertex` | Hybrid-kernel provenance channels added for Yang 2025 Stage-3 SSI curve gating and P0014 LPI-pencil near-weld (`yang-rs/src/boolean.rs:524-575, 2158, 2171-2190`). Absent from Cherchi 2022. | `specs/yang_s3_intersection_edge_provenance.md`, `specs/yang_p0014_arrangement_lpi_pencil_weld.md` |
| `crates/cherchi-rs/src/triangulation/mod.rs:1-2549` (`cdt_polygon_with_holes*`) | 2D Constrained Delaunay Triangulation (`spade` v2 + exact flood-fill / refinement variants) | Hosted in `cherchi-rs` for dependency curation; **never called by `cherchi-rs`'s arrangement or boolean pipeline** (used by `yang-rs` Stage-1/Stage-4 face tessellation and `kernel-v2`). | `N9` |
| `crates/cherchi-rs/src/labeling/inside_out.rs:265-275, 507-600, 605` | Foreign-graze avoidance (`ray_grazes_foreign_input`, `coplanar_ray_overlaps_tri`) + `refute_inner_label_by_input_bounds` (`InnerLabelOutsideInputBounds`) | P0023 mitigation for C++'s exterior coplanar/edge graze perturbation bug; walks interior vertices × `{X, Y, Z}` on Tier 1 and verifies `inner` against foreign input AABB overlap (`:717-780`). | `N69` |
| `crates/cherchi-rs/src/labeling/inside_out.rs:1249-1255` | Strict `<= Ordering::Equal` ray-origin filter (`before_origin`) | Discards $\lambda = 0$ ray-origin hits (C++ used `< 0`, misclassifying point-touch patches). | `N20` |
| `crates/cherchi-rs/src/labeling/patches.rs:126-178` | `PatchError::LabelMismatch` L2a/L2b subset-compatibility gate | Replaces C++ debug `assert(labels.surface[t_id] == ref_l)` with compatible-subset pass (`L2a`) + loud error on disjoint labels (`L2b`). | `N23` |
| `crates/cherchi-rs/src/predicates/orient.rs:79-94, 144-151, 156-175` | Exact `dashu::rational::RBig` zero-certification on `0.0` returns of `geometry_predicates::{orient2d, orient3d}` | Closes Shewchuk's subnormal underflow hole in `orient.rs` (3 sibling modules still call raw `geometry_predicates::orient2d` — latent `UL-04`). | `N24` |
| `crates/cherchi-rs/src/arrangements/soup.rs:64-135, 795-821, 926-972` | Typed `ArrangementError` loud-STOP variants (`CoplanarPairDeferred`, `DegeneratePairDeferred`, `TransversalEndpointOvercount`, `DeepRecursionRequired`) | Replaces C++ silent `assert` / undefined continuation with explicit `Result::Err` stops under P9 governance. | `N16`, `N17` |
| `crates/cherchi-rs/src/arrangements/coplanar_propagate.rs:70-81` (`build_coplanar_adjacency`) | Standalone `CoplanarAdjacency` builder | Dead production helper (`mesh_arrangement` calls `integrate_coplanar_into_arrangement` in `soup.rs:849`, which builds adjacency inline; module doc at `:12-16` still reflects pre-PR-4 standalone state). | — |
| `crates/cherchi-rs/src/labeling/inside_out.rs:440-446`; `crates/cherchi-rs/src/predicates/triangle_intersect.rs:35-44` | **Release-active behavior-altering env vars:** `CHERCHI_GRAZE_AWARE_RAY` and `CHERCHI_SCE_SHORTCIRCUIT` | Read via `std::env::var` **without `cfg!(debug_assertions)`** and **not registered** in `docs/kernel_probe_registry.md` (`UL-07`, tracked in `N100` item 8). | `N100` (`UL-07`) |
| `crates/cherchi-rs/src/` (13 sites across 4 files: `soup.rs:727,768,946,1112`; `patches.rs:131`; `native.rs:129,293`; `inside_out.rs:240,532,703,1133,1314,1368`) | Diagnostic env probes: `CHERCHI_TIME_PROBE`, `CHERCHI_ENFORCE_PROBE`, `CHERCHI_XEDGE_PROBE`, `CHERCHI_PATCH_PROBE`, `CHERCHI_VERT_PROVENANCE`, `CHERCHI_INOUT_PROBE`, `CHERCHI_GRAZE_PROBE`, `KV4F1_PROBE`, `CHERCHI_PERTURB_PROBE` | Print-only (`eprintln!`) or append-to-file (`CHERCHI_VERT_PROVENANCE`) diagnostics; output-neutral when set or unset. | `docs/kernel_probe_registry.md:76-80` |

---

## 5. Staleness of Prior Audits & Live Documentation

Both prior Cherchi audits — `docs/audits/yang_audit_c_cherchi2022.md` (2026-04-30) and `docs/audits/cherchi_port_audit.md` (2026-04-28) — audited **`crates/kernel/src/boolean/cherchi/*`**, an early prototype deleted on 2026-06-11 (`docs/yang_functional_roadmap.md` history) and replaced by the clean-room `crates/cherchi-rs/` crate. **100% of file paths in both prior audits no longer exist in the repository.**

### 5.1 Disposition of `docs/audits/yang_audit_c_cherchi2022.md` (`YC-01` .. `YC-24`)

| Prior ID | Legacy Finding (`crates/kernel/src/boolean/cherchi/*`) | Prior Verdict | Current `cherchi-rs` / `yang-rs` Site (`a2058a01`) | New Status (`Row`) | One-Line Disposition |
|---|---|---|---|---|---|
| `YC-01` | `findRayEndpoints` 3-tier cascade missing (centroid-only) | `MISSING` | `inside_out.rs:238-255, 582-702, 784-926` | **`PARTIAL` / `FAITHFUL`** (`C22-19..21`) | All 3 tiers now implemented (Tier 1 `:592-627` with `N69` graze avoidance, Tier 2 `:629-702` barycenter push with `UL-06` missing graze check, Tier 3 `:784-926` exact `RBig` `N21`). |
| `YC-02` | Backward push `0.1` along ray axis missing | `MISSING` | `inside_out.rs:650` | **`PARTIAL`** (`C22-20`) | `v0c[dir.idx()] -= 0.1` implemented in `find_ray_endpoints`; generated-ray branch lacks `ray_grazes_foreign_input` (`UL-06`). |
| `YC-03` | Axis selection cycles `0..2` instead of `maxComponentInTriangleNormal` | `PERF-DRIFT` | `inside_out.rs:601, 646`; `orientation.rs:59-108` | **`PARTIAL`** (`C22-19..20`, `C22-02`) | Arrangement uses filtered+exact `max_component_in_triangle_normal` (`N99` raw-coordinate filter tracked in ledger); ray-cast tries `{X, Y, Z}` cleanly. |
| `YC-04` | `f64` `t_hit` sort instead of exact implicit LPI sort | `CORRECTNESS-BUG` | `inside_out.rs:1169-1263` | **`DEVIATES`** (`C22-25`) | Fixed: `sort_hits_along_ray` uses exact indirect `less_than_on_{x,y,z}_indirect` on `GenericPoint3D::Lpi` + `N20` `<= Ordering::Equal` origin filter (`:1255`). |
| `YC-05` | Per-sub-triangle ray-cast instead of per-patch ray-cast | `DEVIATES` | `inside_out.rs:220-278`; `native.rs:169-173` | **`FAITHFUL`** (`C22-16`) | Fixed: `compute_inside_out` casts 1 ray per manifold patch and propagates `inner` uniformly to every triangle in the patch. |
| `YC-06` | `flood_fill_patches` used Yang intersection-edge barriers instead of manifold-edge barriers | `DEVIATES` | `labeling/patches.rs:72-207` | **`DEVIATES`** (`C22-08`) | Fixed: `cherchi-rs` `compute_all_patches` floods `inc.len() <= 2` manifold edges cleanly (separate from `yang-rs` Stage-5 `flood_fill_patches`), with `N23` L2a/L2b rule. |
| `YC-07` | `weld_mesh_vertices` quantized at `1e-9` nm grid | `CORRECTNESS-BUG` | `arrangements/soup.rs:285-320, 833-855, 1139-1153` | **`FAITHFUL`** (`C22-10`, `C22-11`) | Fixed: `cherchi-rs` uses bit-exact input vertex merge + exact `[RBig; 3]` geometric interning (`N18`), zero grid quantization. |
| `YC-08` | `vertInfo` patch-border vertex marker missing | `MISSING` | `labeling/patches.rs:47-50, 190-195`; `inside_out.rs:595` | **`PARTIAL`** (`C22-19`) | Fixed: `Patches::border_verts` marks all non-manifold edge endpoints and `find_ray_endpoints` (`:595`) excludes them. |
| `YC-09` | `IntersInfo` enum (`NoInt`/`IntInTri`/`IntInEdge`/`IntInV`) absent | `CORRECTNESS-BUG` | `inside_out.rs:1015-1073` | **`FAITHFUL`** (`C22-22`) | Fixed: `IntersInfo` enum + `fast_2d_check_intersection_on_ray` implemented using `N24`-certified `crate::predicates::orient2d` (`predicates/mod.rs:26`). |
| `YC-10` | Perturbed sub-tri normal (Hoffmann) instead of `nextafter` on $p_\infty$ | `CORRECTNESS-BUG` | `inside_out.rs:1087-1152` | **`DEVIATES`** (`C22-27`) | Replaced Hoffmann normal nudge with `next_up`/`next_down` on `ray.v1`, but capped at 8 1-ULP offsets (`UL-02` / `N101`, plus minor `:1147-1150` early return `UL-01`). |
| `YC-11` | `slab_eps = 1e-14` ray AABB expansion | `PERF-DRIFT` | `inside_out.rs:386-423`; `labeling/octree.rs:60-82` | **`PARTIAL`** (`C22-24`) | Fixed: zero-epsilon tight `ray_aabb` (`[min(v0,v1), max(v0,v1)]`) + exact `tri_in_ray_aabb` post-filter (6-float check). |
| `YC-12` | No octree shared between arrangement and ray-cast | `PERF-DRIFT` | `intersection_detection.rs:100`; `labeling/octree.rs:49-53` | **`PARTIAL`** (`C22-23`) | Both stages now use `AabbTree` (8-way octree, no longer $O(n^2)$), building two separate `AabbTree` instances instead of sharing one. |
| `YC-13` | §4 Cached 4-minor `orient3D` predicates not implemented | `PERF-DRIFT` | `predicates/orient.rs:61-95` | **`MISSING`** (`C22-12`) | Unchanged: per-triangle 4-minor `orient3d` plane cache is still not implemented. |
| `YC-14` | `analyzeSortedIntersections` first-per-label `visited` tracking absent | `DEVIATES` | `inside_out.rs:1390-1415` | **`FAITHFUL`** (`C22-17`) | Fixed: `analyze_sorted_intersections` tracks `visited: Vec<InputId>` across $N$ input solids and inspects only the first hit per solid. |
| `YC-15` | Tier-2 `seed_tri` pivot discard pass absent | `MISSING` | `inside_out.rs:1222-1247, 1300-1375` | **`DEVIATES`** (`C22-20`, `C22-25`) | Fixed: `behind_seed_plane` filters hits at/before `seed_tri` via exact `orient3d_gp3` against `seed_tri`'s plane. |
| `YC-16` | `checkTriangleOrientation` (`orient3d < 0` ⇒ inside) | `FAITHFUL` | `inside_out.rs:1404-1413` | **`FAITHFUL`** (`C22-17`) | Confirmed `FAITHFUL` in `cherchi-rs` (`Sign::Negative => inner_labels.push(id)`). |
| `YC-17` | Legacy GWN test `label_cells_raycast_matches_gwn_for_offset_boxes` | `TEST-NOTE` | `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs` | **`SUPERSEDED`** | Replaced by 18-fixture × 4-op native-vs-sidecar parity + adversary suites in `cherchi-rs`. |
| `YC-18` | Hoffmann perturbation fallback present without Cherchi citation | `DEVIATES` | Deleted (`crates/kernel/src/boolean/cherchi/*`) | **`SUPERSEDED`** | Hoffmann perturbation no longer exists anywhere in `cherchi-rs`. |
| `YC-19` | `RayHit::Degenerate` aborted axis loop on first `orient2d == 0` | `CORRECTNESS-BUG` | `inside_out.rs:1322-1374` | **`DEVIATES`** (`C22-27`) | Replaced by `IntInV`/`IntInEdge` perturbation in Tier 1/2 (and axis-retry in Tier 3 `:898-901`). |
| `YC-20` | `compute_global_max + 1.0` vs C++ `octree.bbox.max + 0.5` | `PERF-DRIFT` | `inside_out.rs:190-202, 468-480` | **`FAITHFUL`** (`C22-18`) | Uses `TriOctree::bbox()` max `+ 0.5`, matching paper §5.1 (`:423-426 L`) and C++ `booleans.cpp:645-649`. |
| `YC-21` | `addDuplicateTrisInfoInStructures` not called | `DEVIATES` | `arrangements/soup.rs:1089-1109` | **`FAITHFUL`** (`C22-10`) | Fixed: step 11 restores `dupl_triangles` with `w`-corrected winding into `in_tris`/`in_labels` (`coplanar_dupl_restore.rs`). |
| `YC-22` | `intersects_box` octree query absent | `PERF-DRIFT` | `labeling/octree.rs:60-82`; `inside_out.rs:386-423` | **`PARTIAL`** (`C22-23`, `C22-24`) | Fixed: `TriOctree::query_aabb` queries `AabbTree` with tight `ray_aabb` + `tri_in_ray_aabb`. |
| `YC-23` | Duplicate of `YC-14` (`first-on-each-label` semantics) | `DEVIATES` | `inside_out.rs:1390-1415` | **`FAITHFUL`** (`C22-17`) | Fixed alongside `YC-14`. |
| `YC-24` | `D-13` per-triangle `LocalMesh` marked out of scope | `OUT-OF-SCOPE` | `arrangements/soup.rs:897-972` | **`FAITHFUL`** (`C22-11`) | Per-base-triangle `FastTrimesh` + exact `[RBig; 3]` global weld (`soup.rs:1139-1153`) is verified conforming in `cherchi-rs`. |

### 5.2 Disposition of `docs/audits/cherchi_port_audit.md` (42 Legacy Items: `A-01..A-06`, `B-01..B-14`, `C-01..C-13`, `D-02..D-13`)

*(Note: original numbering in `cherchi_port_audit.md` skipped `C-04`, `C-12`, `D-01`, and folded `D-03` into `A-03`.)*

| Prior ID | Legacy Issue in Deleted `crates/kernel/src/boolean/cherchi/*` | Prior Severity | Current `cherchi-rs` Status & Site (`a2058a01`) |
|---|---|---|---|
| `A-01` | `points_are_collinear_3d` used inexact `f64` cross-product | `CORRECTNESS-BUG` | **Mostly fixed (`PARTIAL`, `UL-04`):** `predicates/collinearity.rs:30-33` uses Shewchuk `geometry_predicates::orient2d` on XY/YZ/ZX; raw `orient2d` still lacks `N24` `RBig` zero-certification at differences `~1e-146` (another agent is routing it through `orient::orient2d`). |
| `A-02` | `max_component_in_triangle_normal` used uncertified `f64` | `CORRECTNESS-BUG` | **Partially fixed (`PARTIAL`, `N99`):** `predicates/orientation.rs:59-108` uses a filtered `f64` bound + exact `RBig` fallback (`max_component_exact`); raw-coordinate filter + single-`eps` margin defect is tracked as **`N99`** (`cherchi2020_lit_check.md`). |
| `A-03` (`D-03`) | 5 jolly points appended eagerly at soup init instead of tail | `UNKNOWN` | **Fixed (`FAITHFUL`):** `arrangements/soup.rs:1085-1087` appends `jolly_points(m)` at step 10 (the tail of `globals.verts`), matching C++. |
| `A-04` (`D-04`) | Custom `Orientation` cosurface tracking replaced `dupl_triangles` | `DELIB-DIV` | **Fixed (`FAITHFUL`, `C22-10`):** `soup.rs:347-415, 1089-1109` ports C++ `DuplTriInfo` + step-11 shell restoration; `yang-rs/src/boolean.rs:2273-2363` handles solid `opposite` membrane semantics. |
| `A-05` | `compute_multiplier` silently fixed C++ UB | `PERF/CLEANUP` | **Tracked (`N98` in `cherchi2020_lit_check.md`):** `processing/multiplier.rs:33-48` returns `1.0` for `max_abs < 1.0` and `2^ceil(log2(max_abs))` for `max_abs >= 1.0`. |
| `A-06` | `compute_approximate_coordinates` off by 5 jolly slots | `BUG` | **Eliminated:** `cherchi-rs` stores `approx` inline on `IntersectionVertex::Lpi` / `FastTrimesh` vertices and appends jolly points at step 10 (`soup.rs:1086`). |
| `B-01` | $O(n^2)$ broad-phase in `detect_intersections` | `PERF-DRIFT` | **Fixed (`FAITHFUL` / `N95`):** `arrangements/intersection_detection.rs:85-125` uses `AabbTree` ($O(n \log n + k)$). |
| `B-02` | Yang §4.2.2 Gauss-map filter mixed into arrangement | `DELIB-DIV` | **Eliminated (`FAITHFUL`):** `cherchi-rs` is a pure exact mesh arrangement with no approximate Gauss-map pre-skip. |
| `B-03` | Missing `SIMPLICIAL_COMPLEX` adjacency skip in `triangles_intersect` | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `intersection_detection.rs:105-125` skips shared-vertex/shared-edge pairs per C++ soup rules or routes through exact `classify_intersections` (`intersection_points.rs`). |
| `B-04` | `check_single_no_coplanar_edge_intersection` missing guard | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `intersection_points.rs` + `aux_structure.rs` geometric `[RBig; 3]` interner (`N18`) deduplicate shared endpoints cleanly. |
| `B-05` | `SegmentIntersection` collapsed 4 states into 2 | `BUG` | **Fixed (`FAITHFUL`):** `predicates/segment_triangle.rs:15-50` & `indirect/mod.rs` preserve exact interior vs endpoint vs disjoint states. |
| `B-06` | `finalize_intersection` soft-asserts masked >2 endpoints | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `soup.rs:834-838` surfaces `ConstraintSegmentError::TooManyGeometricEndpoints` loudly as `Err(ArrangementError::TransversalEndpointOvercount)`. |
| `B-07` | `point_in_triangle_3d_classify` checked dominant axis only | `BUG` | **Mostly fixed (`PARTIAL`, `UL-04`):** `point_in_triangle.rs:45-145` checks all 3 projections XY/YZ/ZX (raw `orient2d` calls at `:97, 141-143` have latent subnormal `UL-04` gap). |
| `B-08` | `triangle_has_actual_intersection_data` workaround | `WORKAROUND` | **Eliminated:** Clean fast-path `if no_points && no_segments` in `soup.rs:886-895`. |
| `B-09` | Dropped 3 dead C++ fields | `CLEANUP` | **Eliminated:** Clean-room Rust structs in `arrangements/`. |
| `B-10` | `v_map` populate timing flag | `CLEANUP` | **Eliminated:** Replaced by `GlobalVerts` + `weld_vertex` (`soup.rs:859, 1139-1153`). |
| `B-11` | `propagate_coplanar_triangles_intersections` clone loop | `PERF` | **Cleanly ported (`FAITHFUL`):** `soup.rs:846-855` + `coplanar_propagate.rs`. |
| `B-12` | `init_from_triangle_soup` populated `v_map` with jolly entries | `BUG` | **Eliminated:** Jolly points appended only at step 10 (`soup.rs:1086-1087`). |
| `B-13` | Dead `compute_lpi_coords` with bad parallel-plane fallback | `DEAD-CODE` | **Eliminated:** Replaced by `predicates/indirect::approx_lpi` (`indirect/mod.rs:925-975`) and exact `RBig` `exact_point_coords` returning `None` on `den == 0`. |
| `B-14` | `segment_segment_intersect_3d` `1e-30` magnitude threshold | `CORRECTNESS-BUG` | **Eliminated (`FAITHFUL`):** In-plane crossings use exact indirect predicates (`inner_segments_cross_indirect`, `point_in_inner_segment_indirect` in `enforce.rs:36-38`). |
| `C-01` | `split_single_triangle_with_stack` skip-already-vertex defense | `UNKNOWN` | **Cleanly ported (`FAITHFUL`, `N14`):** `soup.rs:911-925` deduplicates `flat` by exact `VertexCoords` before `split_single_triangle` (`retriangulate.rs:74-94`). |
| `C-02` | `reposition_points_in_stack` `is_vertex` filter dropped points | `CORRECTNESS-BUG` | **Eliminated (`FAITHFUL`, `N14`):** `retriangulate.rs:74-94` locates each point in the live submesh via exact `find_containing_triangle` + uniform `fast_point_on_line`. |
| `C-03` | Manual descending-sort before `remove_tris` | `CLEANUP` | **Fixed:** `FastTrimesh::remove_tris` (`fast_trimesh.rs`) handles descending compaction internally (`enforce.rs:255`). |
| `C-05` | `find_intersecting_elements` silent skip on missing final tri | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `enforce.rs:315-369` returns loud `Err(EnforceError::SegmentNotLocatable)` on any broken walk. |
| `C-06` | `sort_edge_points` re-added vs commented out in C++ | `UNKNOWN` | **Eliminated (`N14`):** `retriangulate.rs:37-47` uses uniform `split_edge` / `split_tri` on live submesh triangles, needing no pre-sorted edge list. |
| `C-07` | `CustomStack::push` silent degenerate filter | `BUG` | **Eliminated:** No `CustomStack`; `retriangulate.rs` splits directly on `FastTrimesh`. |
| `C-08` | `split_edge` defensive guards removed tri without replacement | `CORRECTNESS-BUG` | **Eliminated:** `FastTrimesh::split_edge` (`fast_trimesh.rs`) splits incident triangles atomically and is verified by exact `RBig` area-covering oracles (`retriangulate.rs`, `enforce.rs`). |
| `C-09` | `add_tri` returned `0` on degenerate triangle (silent corruption) | `CRITICAL-BUG` | **Eliminated (`FAITHFUL`):** `FastTrimesh::add_tri` (`fast_trimesh.rs`) never silently drops triangles or aliases slot `0`. |
| `C-10` | `create_tpi` skipped dedup against existing TPI vertices | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`, `N18`):** `weld_vertex` (`soup.rs:1139-1153`) and `exact_point_coords` interning deduplicate coincident implicit points. |
| `C-11` | `tri_opp_to_edge` accepted non-manifold submesh edges | `BUG` | **Fixed:** Submesh is a strictly 2-manifold planar triangulation of 1 base triangle (`enforce.rs:332-337`). |
| `C-13` | `edge_id` softened `ev0 != ev1` assert to `None` | `MINOR` | **Verified safe:** Callers propagate `None` into loud `EnforceError::SegmentNotLocatable` (`enforce.rs:257-261, 346-361`). |
| `D-02` | `solve_intersections` ported only Cherchi 2020 Phase 1 | `ARCHITECTURAL` | **Superseded (`FAITHFUL`):** `cherchi-rs` implements both Stage 1 (`arrangements/`) and Stage 2 (`labeling/`). |
| `D-05` | Parity counting instead of signed-volume `orient3d(t, p_inf)` | `CRITICAL-BUG` | **Fixed (`FAITHFUL`, `C22-17`):** `inside_out.rs:1390-1415` uses exact first-hit `orient3d(tv[0], tv[1], tv[2], ray.v1)`. |
| `D-06` | Centroid-only emanating point (no 3-tier `findRayEndpoints`) | `UNKNOWN` | **Fixed (`C22-19..21`):** All 3 tiers implemented in `inside_out.rs:582-702, 784-926` (with `UL-06` graze-check gap on `:629-702`). |
| `D-07` | Hoffmann normal perturbation instead of `nextafter` on $p_\infty$ | `CORRECTNESS-BUG` | **Replaced (`DEVIATES`, `C22-27`):** `next_up`/`next_down` implemented, with confirmed `UL-02` (`N101`) 8-offset cap bug and minor `UL-01` early-return edge case. |
| `D-08` | Same-label skip implicit via API separation | `MINOR` | **Explicitly ported (`FAITHFUL`, `C22-16`):** `inside_out.rs:1304-1308` checks `patch_surface_label` explicitly. |
| `D-09` | `slab_eps` expansion on ray AABB | `PERF-DRIFT` | **Fixed (`PARTIAL`, `C22-24`):** Tight `ray_aabb` + `tri_in_ray_aabb` in `inside_out.rs:386-423` & `labeling/octree.rs:60-82`. |
| `D-10` | `weld_mesh_vertices` nanometer quantization | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`, `C22-10`):** Bit-exact vertex merge + exact `[RBig; 3]` implicit point interning. |
| `D-11` | `flood_fill_patches` intersection-edge barriers | `UNKNOWN` | **Fixed (`DEVIATES`, `C22-08`):** `labeling/patches.rs` floods manifold edges; `yang-rs` Stage-5 `flood_fill_patches` operates downstream on attributed B-Rep patches. |
| `D-12` | Interior vertex (`vertInfo`) seed preference missing | `MISSING` | **Fixed (`PARTIAL`, `C22-19`):** `Patches::border_verts` + Tier-1 explicit interior vertex scan implemented. |
| `D-13` | Per-triangle `LocalMesh` vs C++ global `FastTrimesh` | `ARCHITECTURAL` | **Verified (`FAITHFUL`, `C22-11`):** Per-triangle `FastTrimesh` + exact `GlobalVerts` weld (`soup.rs:897-1056`) achieves full sidecar parity. |

### 5.3 Stale Live Documentation, Ledger Entries & Comments Found at `a2058a01`

1. **`docs/yang_deviations.md:6749-6752` (N89 item 8):** Still states `"cherchi-rs's own pair scan is still O(n²)"`, contradicted by **N95** (`docs/yang_deviations.md:6998-7045`, marked `RESOLVED (2026-10-17)`) and `crates/cherchi-rs/src/arrangements/intersection_detection.rs:85-125` (`AabbTree::build`). *(The parallel stale text in `docs/audits/yang_2025_audit.md` has already been updated on `main`.)*
2. **`docs/kernel_probe_registry.md:76-80`:** Still states that all `cherchi-rs` `CHERCHI_*` env vars are print-only tracing probes, missing the two release-active output-changing switches `CHERCHI_GRAZE_AWARE_RAY` (`inside_out.rs:441`) and `CHERCHI_SCE_SHORTCIRCUIT` (`triangle_intersect.rs:40`), which are now tracked for removal/debug-gating under **`N100` item 8** (`docs/yang_deviations.md:7209-7212`).
3. **`docs/yang_functional_roadmap.md:3931-3941`:** Still states that `LabeledArrangement` drops input-triangle provenance (`"So the contract is solid-level; the triangle index is dropped"`), superseded by `LabeledArrangement::source: Vec<Vec<(InputId, u32)>>` (`crates/cherchi-rs/src/labeled_arrangement.rs:52-66`) and `yang-rs/src/boolean.rs:2970-3042`.
4. **`crates/cherchi-rs/src/boolean.rs:6`:** Module doc comment still describes `NativeBoolean` as `"(future; pure Rust; WASM-portable; gated on Stages 2-4 of the arrangement port ...)"`, although `NativeBoolean` (`labeling/native.rs:527-574`) is the live production backend.
5. **`crates/cherchi-rs/src/arrangements/coplanar_propagate.rs:12-16, 69`:** Module doc comment still claims *"nothing in [`super::soup::mesh_arrangement`] constructs or consumes it ... PR-4 will wire {bucket → propagate → pocket emit} together"*, whereas PR-4 wired `coplanar_propagate` into `soup.rs:846-855` via `integrate_coplanar_into_arrangement`.
6. **`crates/cherchi-rs/src/arrangements/enforce.rs:7-19` & `crates/cherchi-rs/src/arrangements/soup.rs:1157-1162`:** Doc comments still say *"No production code is authored here ... FAILS TO COMPILE"* from their initial TDD RED commits.

---

## 6. Divergences Found in This Check (Ranked by Severity)

| Rank & ID | Severity | Failure Mode | Code Site (`a2058a01`) | Violated Paper Statement / Ledger | Summary of Divergence |
|---|---|---|---|---|---|
| **`#1` — `UL-02`** | **CRITICAL (Confirmed Real → Ledger `N101`)** | **Silent wrong boolean output** (transversal crossing through 8-valent pyramid/cone apex skipped as "tangential", enclosed patch mislabelled `outside`) | `crates/cherchi-rs/src/labeling/inside_out.rs:1087-1152, 1337-1342, 1367-1378` | Cherchi 2022 §5.3 (`:485-492 L`: *"the operation is repeated until a valid intersection point is found"*) | Replaces the paper's unbounded perturbation loop with 8 fixed single-ULP offsets `(±1,0), (0,±1), (±1,±1)` and silently skips the intersection when all 8 return `None`. Confirmed by independent reviewer test on an octagonal pyramid apex: all 8 fixed `nextafter` offsets land in the 45° spoke planes (`orient3d == Sign::Zero`), dropping a transversal shell crossing. A separate agent is fixing this as ledger **`N101`**. |
| **`#2` — `UL-06`** | **MEDIUM (Real)** | **Silent wrong boolean output** on originless patches whose generated centroid ray grazes a foreign input | `crates/cherchi-rs/src/labeling/inside_out.rs:629-702` | Cherchi 2022 §5.1 & §5.3 (`:445-457 L, 480-492 L`) + Ledger `N69` | In `find_ray_endpoints` (`:582-702`), the explicit-vertex branch (`:592-627`) filters rays with `ray_grazes_foreign_input` (`:507-546`, `N69`), **but the generated-ray branch (`:629-702`) never calls `ray_grazes_foreign_input`**, accepting the first synthetic centroid ray (`:694, 698`) even when it lies in and overlaps a foreign triangle's plane. |
| **`#3` — `UL-08`** | **MEDIUM (Blocking for F1 / `N80`)** | **Latent wrong membrane retention** once `F1-I2` moves `keep_set` after Stage 4 | `crates/yang-rs/src/brep.rs:406-468` vs `crates/yang-rs/src/boolean.rs:2273-2363` | Yang 2025 §4.4.2 & §4.5.5 (`yang2025_hybrid_boolean.txt:572-578, 696-706`); Cherchi 2022 §3 (`:243-245 R`) | `TriangleSideLabel` (`brep.rs:406-415`) omits Stage-0's `opposite` normal-agreement flag, and `TriangleSideLabel::keep_for_op` (`:454-467`) returns `true` on every `[true, true], [false, false]` coplanar sheet for `Union`, `Intersect`, and `Subtract`. Hidden today only because `boolean.rs:2361` pre-filters `!keep_sheet` before Stage 4. |
| **`#4` — `UL-04` (+ `N99`)** | **LOW (Latent)** | **Latent subnormal `orient2d` underflow** in 3 non-ray-cast helpers (needs coordinate differences `~1e-146`) | `crates/cherchi-rs/src/predicates/simplex_location.rs:89-97`; `collinearity.rs:30-33`; `point_in_triangle.rs:97, 141-143` | Cherchi 2022 §2.1 (`:140-150 L`) + Ledger `N24`, `N99` | While `fast_2d_check_intersection_on_ray` (`inside_out.rs:1032-1073`) uses `N24`-certified `crate::predicates::orient2d` (`predicates/mod.rs:26`), these three modules still call raw `geometry_predicates::orient2d` directly; another agent is routing them through the certified predicate. *(Separately, the raw-coordinate filter + single-`eps` margin defect in `max_component_filtered` at `orientation.rs:82-108` is tracked as ledger **`N99`** from `cherchi2020_lit_check.md`.)* |
| **`#5` — `UL-07`** | **LOW (Governance → `N100.8`)** | **Release-active result-changing env vars** violating F12 probe policy | `crates/cherchi-rs/src/labeling/inside_out.rs:440-446`; `crates/cherchi-rs/src/predicates/triangle_intersect.rs:35-44`; `docs/kernel_probe_registry.md:76-80` | Governance F12 (`docs/kernel_probe_registry.md:17-20, 91-93`); `specs/yang_faithful_cleanup.md` §4; Ledger `N100` item 8 | `CHERCHI_GRAZE_AWARE_RAY` and `CHERCHI_SCE_SHORTCIRCUIT` alter kernel output in `--release` builds without `cfg!(debug_assertions)` and are omitted from `docs/kernel_probe_registry.md`. Now tracked for cleanup in `N100` item 8 (`docs/yang_deviations.md:7209-7212`). |
| **`#6` — `UL-01`** | **LOW (Minor edge case)** | **Premature `return None` at `offset = 0`** if a perturbed ray has only $\lambda \le 0$ 1-ring line hits | `crates/cherchi-rs/src/labeling/inside_out.rs:1147-1150` | Cherchi 2022 §5.3 (`:485-492 L`) | `if !hits.is_empty()` returns `sorted.first().copied()` (`None`) without trying `offset = 1..7` when `sort_hits_along_ray` strips all `hits` via `before_origin` (`N20`). Reachable on a real forward crossing only if a 1-ring triangle is near-parallel to the ray within ~1 ULP or via an un-gated Tier-2 grazing ray (`UL-06`), since Tier-1 coplanar rays are already rejected by `N69`'s `ray_grazes_foreign_input` (`:507-546`). *(C++ `booleans.cpp:1016-1045` had UB on empty `inters_tris[0]`.)* |

### 6.1 Residual Open Question (`UNCERTAIN`)

- **`UL-03` (`UNCERTAIN` — `N23` L2a `[A, B]`-seeded mixed patches):** Because `compute_all_patches` (`crates/cherchi-rs/src/labeling/patches.rs:97`) visits seeds in ascending index order, `patch.sort_unstable()` (`:198`) always preserves `patch[0] == seed`, matching `N23` and C++ `booleans.cpp:629`. The only remaining question is topological: if an `[A, B]` coplanar triangle has a lower output index than manifold-adjacent single-owner `[B]` triangles (`L2a` subset rule, `patches.rs:126-130`), `patch_surface_label = &soup.labels[patch[0]]` (`inside_out.rs:226`) is `[A, B]`, skipping ray-cast against both solids (`inside == [false, false]`). Whether a 2-manifold edge (incidence $\le 2$, no transversal `A`/`B` boundary sheet) can ever connect an `[A, B]` overlap triangle directly to a `[B]` triangle that lies *inside* $A$ (without $A$'s boundary creating a $\ge 3$-incident non-manifold barrier edge at the transition) is **`UNCERTAIN`**.

---

## 7. `TEST CANDIDATES` (Concrete Specifications for Unpinned Rows & Divergences)

Because this session is strictly read-only (`NO cargo`), none of the specs below were executed; each includes the exact `needs measurement:` command to run once CPU booking permits.

### `TC-01` — Enclosed Solid Inside Regular 8-Sided Pyramid / Bipyramid Apex (`UL-02` / `N101`, pins `C22-27`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs` *(note: an independent reviewer test on an octagonal pyramid apex already reproduced this failure; tracked as `N101`)*
- **Geometry:**
  - **Solid `B` (`InputId(1)`):** Closed outward-oriented regular 8-sided bipyramid centered on `(y_c, z_c) = (1.5, 1.5)`:
    - Right apex `V_R = (1.0, 1.5, 1.5)`, left apex `V_L = (-2.0, 1.5, 1.5)`.
    - 8 equatorial ring vertices at `x = 0.0` with `(y, z)` = `(2.5, 1.5)`, `(2.5, 2.5)`, `(1.5, 2.5)`, `(0.5, 2.5)`, `(0.5, 1.5)`, `(0.5, 0.5)`, `(1.5, 0.5)`, `(2.5, 0.5)` (16 triangles total: 8 `[E_k, E_{k+1}, V_R]`, 8 `[E_{k+1}, E_k, V_L]`).
  - **Solid `A` (`InputId(0)`):** Small closed tetrahedron strictly inside `B` with vertex 0 at `(-0.5, 1.5, 1.5)` and remaining 3 vertices at `(-0.6, 1.6, 1.5)`, `(-0.6, 1.45, 1.58)`, `(-0.6, 1.45, 1.42)` (Tier 1 picks vertex 0 along `Axis::X` because `ray_grazes_foreign_input` is `false`).
- **Expected behavior:** `A` is strictly inside `B` (`inside == [false, true]` on all 4 triangles of `A`; `NativeBoolean.boolean(&a, &b, BoolOp::Intersect)` has 4 triangles and positive volume equal to `vol(A)`).
- **Current bug (`UL-02` / `N101`):** All 8 single-ULP perturbations of `p_ray.v1` lie in the 45° spoke planes of `V_R`, returning `Sign::Zero`; `perturb_ray_and_find_inters_tri` returns `None`, dropping the crossing and mislabelling `A` as `outside` `B`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc01" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-02` — Originless Band Patch Whose Generated Centroid Ray Grazes a Foreign Input (`UL-06`, pins `C22-20` / `C22-29`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:**
  - **Solid `A`:** Thin rectangular prism `[-0.5, 0.5] × [-1.0, 1.0] × [-1.0, 1.0]` sliced by **two** slabs `B_top` (`[-2, 2] × [-2, 2] × [0.5, 2.0]`) and `B_bot` (`[-2, 2] × [-2, 2] × [-2.0, -0.5]`) joined outside `A` into a single C-shaped solid `B` whose inner vertical wall has a horizontal concave step at `z = 0.0` ahead of `A` (`x ∈ [1.0, 2.0]`).
  - Because `A`'s middle band `z ∈ [-0.5, 0.5]` has all vertices on the `z = ±0.5` intersection rings (`border_verts`), `A`'s band patch has **zero** interior explicit vertices → enters the generated-ray branch in `find_ray_endpoints` (`inside_out.rs:629-702`). Arrange one symmetric quad diagonal on `A`'s `-X` face so the first band triangle's centroid has `z = 0.0`, causing its `+X` synthetic ray to graze `B`'s concave horizontal face at `z = 0.0`.
- **Expected behavior:** The generated-ray branch (`:629-702`) skips the grazing `+X` ray via `ray_grazes_foreign_input` (`:507-546`) and classifies cleanly on `+Y`.
- **Predicted current bug (`UL-06`):** `find_ray_endpoints` (`:629-702`) never calls `ray_grazes_foreign_input`, accepts the `+X` grazing ray (`:694`), and perturbs into `B`'s concave step (inside `B`'s C-shaped AABB, so `refute_inner_label_by_input_bounds` at `:717` cannot refute a false `inside` label).
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc02" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-03` — Tier-1 All-Candidates-Graze Inside Non-Convex Foreign AABB (`C22-29` residual `N69` `fallback` gap)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:** Same C-shaped solid `B` (`AABB = [-2, 2]^3`) with a small cube `A` inside the open mouth of the `C` (strictly outside `B`, **inside** `AABB(B)`) whose interior explicit vertices on a patch align on `X`, `Y`, and `Z` with three exterior tangent shelves of `B`.
- **Expected behavior:** Rather than returning `fallback` (`inside_out.rs:624-627`, the first grazing ray), continue to a non-grazing generated centroid ray or Tier-3 `rational_ray_inner_label` (`:784-926`) and classify `outside` `B`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc03" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-04` — `TriangleSideLabel::keep_for_op` on Stacked (`opposite = true`) & Flush (`opposite = false`) Coplanar Sheets (`UL-08`, pins `YC22-04` for Milestone `F1`)
- **Target file:** `crates/yang-rs/src/tests_unit/s442_side_labels.rs`
- **Geometry:**
  1. **Stacked cubes (`opposite = true`):** `A = [0, 1]^3`, `B = [0, 1]^2 × [1, 2]` sharing `z = 1`. Construct `TriangleSideLabel` for the shared `z = 1` arrangement triangles (`surface = [true, true], inside = [false, false]`) **without** pre-stripping `!keep_sheet`.
  2. **Flush pocket (`opposite = false`):** `A = [0, 4]^3`, `B = [1, 3]^2 × [2, 4]` sharing `[1, 3]^2 × {4}`.
- **Expected behavior (required before `F1-I2` lands):** Post-Stage-4 `keep_for_op` drops the `z = 1` stacked sheet on `BoolOp::Union` and `BoolOp::Intersect`, keeps it on `BoolOp::Subtract`, and drops the `z = 4` flush pocket cap on `BoolOp::Subtract`.
- **Predicted current bug (`UL-08`):** `TriangleSideLabel::keep_for_op` (`crates/yang-rs/src/brep.rs:454-467`) returns **`true` in all four cases** because `TriangleSideLabel` lacks `opposite`!
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc04" -- cargo test -p yang-rs s442_side_labels`

### `TC-05` — Subnormal Zero-Certification in `simplex_location`, `collinearity`, and `point_in_triangle` (`UL-04`, pins `C22-02`)
- **Target file:** `crates/cherchi-rs/src/predicates/orient.rs` (or per-module `tests` in `simplex_location.rs`, `collinearity.rs`, `point_in_triangle.rs`)
- **Geometry:**
  - `a = Point3::new(0.0, 0.0, 0.0)`, `b = Point3::new(1e-162, 0.0, 0.0)`, `c = Point3::new(0.0, 1e-162, 0.0)` (exact XY area $= \frac{1}{2}\cdot 10^{-324} > 0$ in $\mathbb{Q}$, non-collinear).
  - Query points `p_in = Point3::new(0.25e-162, 0.25e-162, 0.0)` and `p_out = Point3::new(-0.25e-162, 0.25e-162, 0.0)`.
- **Expected behavior (once raw `geometry_predicates::orient2d` calls are routed through `crate::predicates::orient2d`):**
  - `points_are_collinear_3d(a, b, c) == false`;
  - `point_in_triangle_3d_loc(p_in, a, b, c) == TriangleLocation::StrictlyInside`;
  - `point_in_triangle_3d_loc(p_out, a, b, c) == TriangleLocation::Outside`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc05" -- cargo test -p cherchi-rs predicates`

### `TC-06` — Variadic 3-Mesh End-to-End Arrangement + Ray-Cast + Keep Parity (`C22-05`, `C22-34`)
- **Target file:** `crates/cherchi-rs/src/labeling/adversary_tests.rs`
- **Geometry:** Base cube `A = [0, 4]^3` minus two non-intersecting decorative cubes `B_1 = [0.5, 1.5]^3` (enclosed) and `B_2 = [2.5, 3.5] × [2.5, 3.5] × [3.0, 5.0]` (carving top face), passed as a 3-solid soup (`labels = [0], [1], [2]`, `num_inputs = 3`) vs pairwise `A \ (B_1 ∪ B_2)` (mirroring Cherchi 2022 §6.4 Fig. 9, `:673-680 R`).
- **Expected behavior:** Identical triangle set, watertightness, and signed volume ($64 - 1 - 1 = 62$) on `mesh_arrangement` + `compute_all_patches` + `compute_inside_out(..., 3)`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc06" -- cargo test -p cherchi-rs labeling::adversary_tests`
