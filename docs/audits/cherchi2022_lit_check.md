# Literature Check — `crates/cherchi-rs` & `yang-rs` Stage-2 Boundary vs. Cherchi et al. 2022

**Audited commit:** `b4bee54ee4104ef7cf23a29f6cf4c5845bfff0f4` (branch `agy/lit-cherchi2022`, 2026-10-27)
**Primary paper:** Gianmarco Cherchi, Fabio Pellacini, Marco Attene, and Marco Livesu. 2022. *Interactive and Robust Mesh Booleans.* ACM Trans. Graph. (`arXiv:2205.14151v1`, May 2022) — extracted text `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt` (854 lines; two-column `pdftotext` layout — line citations below note `L` / `R` where left and right columns share line numbers).
**Foundational companion papers invoked by Cherchi 2022:**
- Cherchi, Livesu, Scateni, Attene 2020 (*Fast and Robust Mesh Arrangements Using Floating-Point Arithmetic*) — `refs/text/mesh_arrangement.txt`
- Attene 2020 (*Indirect Predicates for Geometric Constructions*) — `refs/text/attene-predicates.txt`
- Yang, Jia, Wang, Yang, Xin, Yan 2025 (*Hybrid B-Rep and Mesh Booleans*, consumer spec for Stage 2) — `refs/text/yang2025_hybrid_boolean.txt`

**Prior audits superseded in full by this document:**
- `docs/audits/yang_audit_c_cherchi2022.md` (2026-04-30; audited deleted `crates/kernel/src/boolean/cherchi/*`)
- `docs/audits/cherchi_port_audit.md` (2026-04-28; audited deleted `crates/kernel/src/boolean/cherchi/*`)

**Method & rules:** Read-only static literature check (zero builds or test runs executed). Every `FAITHFUL` row quotes verbatim **both** the governing paper statement (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:<lines>`) and the satisfying code (`path:line` at `b4bee54e`). Runtime questions are marked `needs measurement:` with exact reproduction commands. No ledger entry is marked `PERMANENT`.

---

## Status Vocabulary & Summary Tally

| Status | Meaning | Paper rows (`C22-01..36`) | `yang-rs` consumer rows (`YC22-01..08`) | Combined (`44` rows) |
|---|---|---:|---:|---:|
| **`FAITHFUL`** | Implements the paper's procedural mechanism; verbatim paper + code quoted | 10 | 4 | **14** |
| **`PARTIAL`** | Implements core mechanism with an unclosed edge-case bug, missing sub-step, or scope restriction | 17 | 2 | **19** |
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
- **Code (`crates/cherchi-rs/src/labeling/native.rs:124-149, 536-564`):**
  - `native_labeled_arrangement` (`native.rs:109-247`) runs Stage 1 `let soup = mesh_arrangement(&coords, &tris, &labels)?;` (`native.rs:127`), partitions into manifold surface patches `let patches = compute_all_patches(&soup)?;` (`native.rs:133`), builds the input-shell `TriOctree` (`native.rs:138`), and computes Stage 2 inside/outside patch labels `let inside_labels = compute_inside_out(&soup, &patches, &octree, 2)?;` (`native.rs:144`).
  - `NativeBoolean::boolean` (`native.rs:530-565`) calls `let la = native_labeled_arrangement(a, b)?; let keep = la.keep_set(op);` (`native.rs:536-537`) and compacts the surviving surface triangles without building any tetrahedral/volumetric mesh.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776` (18-fixture × 4-op native-vs-sidecar parity suite).

#### `C22-02` — §2.1 Exact geometric predicates via arithmetic filtering + arbitrary-precision fallback
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:140-150 L, 174-186 L`):**
  > *"This is done by evaluating geometric predicates exactly and quickly through arithmetic filtering [Lévy 2016; Shewchuk 1997]. A typical predicate calculates the sign of a polynomial and, as long as the sign is correct, the program flow is guaranteed to be consistent. Arithmetic filtering [Devillers and Preparata 1998] makes it possible to evaluate a polynomial using floating point arithmetic but, along with it, an upper bound for the rounding error is computed. If the magnitude of the evaluated expression is larger than the error bound, its sign is guaranteed correct. If not, the filter fails, and the predicate is re-evaluated using arbitrary precision."* (`:140-149 L`)
- **Code (`crates/cherchi-rs/src/predicates/orient.rs:58-95, 134-156`; `crates/cherchi-rs/src/predicates/indirect/mod.rs:32-75, 495-680`; `collinearity.rs:30-33`; `point_in_triangle.rs:97, 141-143`; `simplex_location.rs:90`; `orientation.rs:80-109`):**
  - Explicit `orient3d` (`orient.rs:58-95`) and `orient2d` (`orient.rs:134-156`) call `geometry_predicates::{orient3d, orient2d}` (Shewchuk 1997 adaptive expansions) and re-certify any `0.0` return in `dashu::rational::RBig` (`orient3d_rational_sign` / `orient2d_rational_sign`) to close Shewchuk's subnormal underflow hole (`N24`).
  - Indirect predicates (`predicates/indirect/mod.rs:32-75`) implement a 2-stage cascade: directed-rounding IEEE-754 interval arithmetic (`indirect/interval.rs`) followed directly by exact `dashu::rational::RBig` evaluation (`exact_orient2d_indirect`, `exact_less_than_indirect`), omitting Attene 2020's intermediate expansion tier (performance-only difference; sign-identical).
  - **Gap (`UL-04`):** Three predicate modules bypass `orient::orient2d` and call raw `geometry_predicates::orient2d(...) == 0.0` directly without `N24` rational zero-certification: `collinearity.rs:30-33` (`points_are_collinear_3d`), `point_in_triangle.rs:97, 141-143`, and `simplex_location.rs:90` (used directly by §5.2 `fast_2d_check_intersection_on_ray` at `inside_out.rs:1022`). In addition, `orientation.rs:80-109` (`max_component_filtered`) uses `eps = 4.0 * f64::EPSILON * max_var * max_var`, which underflows to `0.0` in subnormal scales (`max_var <= 1e-162`) where product rounding error (`5e-324`) exceeds `eps`.
- **Status:** **`PARTIAL`**
- **Ledger:** `N24` (covers `orient.rs` only; `UL-04` bypasses are unledgered).
- **Test pin:** `crates/cherchi-rs/src/predicates/orient.rs:285-355`; `crates/cherchi-rs/tests/indirect_filter_soundness.rs:66-295`; `crates/cherchi-rs/tests/indirect_catalog_soundness.rs:189-1010`. Unpinned for `UL-04` → see `TC-04`.

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
- **Code (`crates/cherchi-rs/src/labeling/native.rs:483-521, 536-565`; `crates/cherchi-rs/src/processing/multiplier.rs:33-69`):**
  - All arrangement and ray-casting steps keep `VertexCoords` symbolic; conversion to `f64` happens only in `emit_vertex` (`native.rs:515-521`): `exact_point_coords(&soup.verts[v])` computes exact `[RBig; 3]`, then `xc[i].to_f64().value() / m` rounds once to `Point3`.
  - **Gaps:**
    1. `NativeBoolean::boolean` (`native.rs:536-565`) does not weld distinct `ArrangementSoup` vertex IDs that snap to bit-identical `f64` coordinates if they differed in exact rationals, and on tangential/coplanar contacts (`surface = {0, 1}`) emits internal opposite-normal walls on `Union` and open membranes on `Intersect` (consistent with the paper's *"that do not touch tangentially"* qualifier, noted in `coplanar_pocket_parity.rs:28-42`).
    2. Even on non-tangential watertight meshes, **UL-01** and **UL-02** (silent ray-perturbation drops in `inside_out.rs:1147-1150` and `:1087-1152`) can misclassify an enclosed patch, and **UL-05** (`multiplier.rs:33-48` scaling coordinates $\ge 1$ up by $\sim M$) triggers false `DegenerateOrientation` errors once `max_abs >= 2^26`.
- **Status:** **`PARTIAL`**
- **Ledger:** — (`UL-01`, `UL-02`, `UL-05` unledgered).
- **Test pin:** `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs:698-776` (asserts Euler characteristic & manifoldness on 18 clean fixtures). Unpinned for `UL-01`/`UL-02`/`UL-05` → see `TC-01`, `TC-02`, `TC-05`.

---

### §3 Overview — Inputs, Operators, Provenance & Patch Partition

#### `C22-05` — §3 Variadic input meshes $M_1, \dots, M_n$ and operators `{union, intersection, subtraction}`
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:234-236 R`):**
  > *"Our method takes as input a set of input meshes $M_1, M_2, \dots, M_n$, and a Boolean operator, namely union, intersection, subtraction."* (`:234-235 R`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:722-726`; `crates/cherchi-rs/src/labeling/inside_out.rs:212-220`; `crates/cherchi-rs/src/labeled_arrangement.rs:113-157`; `crates/cherchi-rs/src/labeling/native.rs:109-115, 529-574`; `crates/cherchi-rs/src/boolean.rs:31-58`):**
  - Internal stages (`mesh_arrangement`, `compute_all_patches`, `compute_inside_out(..., num_inputs)`, and `LabeledArrangement::keep_set`) accept arbitrary `Label = Vec<InputId>` and `num_inputs >= 2` (plus an extra `BoolOp::Xor` operator from upstream `booleans.cpp:1463-1510`).
  - **Gap:** The public entry points `native_labeled_arrangement(a: &Mesh, b: &Mesh)` (`native.rs:109`) and `MeshBoolean::{boolean, labeled_arrangement}` (`boolean.rs:34-57`) are hardcoded to binary `(a, b)` with `num_inputs = 2` (`native.rs:144, 244`). No public variadic `native_boolean_variadic(&[&Mesh], BoolOp)` API is exposed.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:640-725` (internal 3-solid `compute_inside_out` tests); binary public API pinned in `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs`. Unpinned for public variadic API → see `TC-09`.

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
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:384-415, 864-867, 891-894, 1018-1038`; `crates/cherchi-rs/src/labeled_arrangement.rs:42-66`):**
  - `remove_degenerate_and_duplicated_triangles` (`soup.rs:388-401`) OR-merges `kept_labels[survivor]` and appends duplicate base-triangle indices into `kept_source[survivor]`.
  - Step 9 coplanar pocket deduplication (`soup.rs:1030-1039`) similarly calls `or_merge_label(&mut out_labels[p], &label)` and appends partner base-triangle provenance into `out_source[p]`:
    ```rust
    or_merge_label(&mut out_labels[p], &label);
    for &src in &kept_source[t as usize] {
        if !out_source[p].contains(&src) {
            out_source[p].push(src);
        }
    }
    ```
  - `native_labeled_arrangement` (`native.rs:171-220`) decodes flat soup indices into `LabeledArrangement::surface: Vec<Vec<InputId>>` and `LabeledArrangement::source: Vec<Vec<(InputId, u32)>>`.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/tests/coplanar_dupl_restore.rs:214-231` (`output_surface_labels_keep_or_merged_overlap`); `crates/cherchi-rs/tests/coplanar_pocket_parity.rs:371-385`.

#### `C22-08` — §3 / §5 Arrangement partitioned into manifold surface patches bounded by non-manifold intersection loops
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:250-256 R, 386-388 L`):**
  > *"Throughout this paper, an arrangement is a set of surface patches bounded by intersection lines. When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."* (`:250-255 R`)
- **Code (`crates/cherchi-rs/src/labeling/patches.rs:72-207`):**
  - `compute_all_patches` (`patches.rs:83-89, 180-196`) builds edge-to-triangle incidence `e2t`, floods across edges with `inc.len() <= 2` (`patches.rs:183`), and stops at non-manifold edges `inc.len() > 2` while recording their endpoints in `border_verts` (`patches.rs:190-195`).
  - **Divergence (`N23` + `UL-03`):** Where a well-formed simplicial arrangement of closed shells has uniform `soup.labels` inside every manifold patch (asserted in debug C++ `booleans.cpp:449`), `patches.rs:126-178` implements `N23`:
    - **L2a (`patches.rs:128-129`):** allows flooding across a 2-incident edge when `cl ⊆ ref_l || ref_l ⊆ cl` (e.g., `[A, B]` ↔ `[A]` or `[A, B]` ↔ `[B]`), keeping `ref_l` (the seed's label).
    - **L2b (`patches.rs:130-175`):** returns loud `Err(PatchError::LabelMismatch)` when `cl` and `ref_l` are disjoint (`[A]` vs `[B]`), stricter than `NDEBUG` C++.
    - **Unledgered bug (`UL-03`):** Because `patches.rs:198` sorts `patch` ascending and `inside_out.rs:226` sets `patch_surface_label = &soup.labels[patch[0] as usize]`, whenever a mixed patch mixes `[A, B]` (lower `out_tris` index, e.g. emitted during solid $A$'s loop) with `[B]` (higher `out_tris` index, emitted during solid $B$'s loop), `patch_surface_label` becomes `[A, B]`, causing `compute_inside_out` (`inside_out.rs:1304-1308`) to skip **both** $A$ and $B$ and silently mislabel the `[B]` sub-region as outside $A$!
- **Status:** **`DEVIATES`**
- **Ledger:** `N23` (documents L2a/L2b on `[A]`-seeded R0046; misses the `[A, B]`-seeded blindness bug `UL-03`).
- **Test pin:** `crates/cherchi-rs/tests/r0046_patch_label_parity.rs:141-175`; `crates/cherchi-rs/src/labeling/patches.rs:222-805`. Unpinned for `UL-03` → see `TC-03`.

---

### §4 Mesh Arrangement

#### `C22-09` — §4 Flatten input meshes into a single triangle soup tagged by input mesh ID
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:295-299 L`):**
  > *"From the perspective of the arrangement algorithm, the input meshes $M_1, M_2, \dots, M_n$ can be seen as a soup of possibly intersecting triangles. We therefore flatten all input triangles into a single array, associating to each triangle a tag that maps it to the input mesh it belongs to."* (`:295-299 L`)
- **Code (`crates/cherchi-rs/src/labeling/native.rs:116-127`; `crates/cherchi-rs/src/arrangements/soup.rs:722-755`):**
  - `concat_two_meshes` (`native.rs:437-466`) flattens `a` and `b` into contiguous `coords: Vec<f64>`, `tris: Vec<[u32; 3]>`, and `labels: Vec<Label>` (`vec![InputId(0)]` for `0..a.tris.len()`, `vec![InputId(1)]` for `b`), which `mesh_arrangement` (`soup.rs:722-726`) consumes directly:
    ```rust
    let (coords, tris, labels) = concat_two_meshes(a, b);
    let soup = mesh_arrangement(&coords, &tris, &labels)?;
    ```
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/arrangements/soup.rs:1156-1600`.

#### `C22-10` — §4 Preprocessing: degeneracy/duplicate removal & single-label input-shell restoration
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:342-345 L, 356-358 L`):**
  > *"Overall, we observe that the most costly operations of the arrangement algorithm are the removal of duplicate and degenerate elements, the construction of adjacencies, the intersection computations and the final triangulation. ... Note that we perform degeneracy removal and rebuilding of adjacencies each frame to ensure robustness while allowing any modeling operation to be performed by users."* (`:342-345 L, 356-358 L`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:285-425, 729-765, 1089-1109`; `crates/cherchi-rs/src/processing/multiplier.rs:33-69`; `crates/cherchi-rs/src/predicates/collinearity.rs:20-35`):**
  - Step 1 (`soup.rs:729-735`): `compute_multiplier` + `apply_multiplier`.
  - Step 2 (`soup.rs:285-320, 740`): `merge_duplicated_vertices` merges bit-identical `Point3` inputs (`[x.to_bits(), y.to_bits(), z.to_bits()]`).
  - Step 3 (`soup.rs:340-425, 744-753`): `remove_degenerate_and_duplicated_triangles` drops repeated-index or `points_are_collinear_3d` triangles, deduplicates sorted vertex triples, OR-merges `kept_labels`, and records `DuplTriInfo { t_off, label, w }` (`consistent_winding`).
  - Step 11 (`soup.rs:1089-1109`): ports `addDuplicateTrisInfoInStructures` (`booleans.cpp:358-393`), appending `w`-oriented single-label copies into `arr_in_tris`/`arr_in_labels` and stripping `d.label` from `arr_in_labels[d.t_off]` so each solid's ray-cast shell is closed and outward-oriented.
  - **Gaps (`UL-04`, `UL-05`):**
    1. `remove_degenerate_and_duplicated_triangles` (`soup.rs:372`) calls `points_are_collinear_3d` (`collinearity.rs:30-33`), which uses raw `geometry_predicates::orient2d(...) == 0.0` without `N24` rational zero-certification; valid triangles with coordinates around `1e-162` underflow `orient2d` to `0.0` and get deleted, breaking shell watertightness.
    2. `compute_multiplier` (`multiplier.rs:33-48`) multiplies coordinates with `max_abs >= 1.0` by `2^ceil(log2(max_abs))` (inflating `M` to `~M^2`) while leaving `max_abs < 1.0` at `1.0`.
- **Status:** **`PARTIAL`**
- **Ledger:** — (`UL-04` and `UL-05` unledgered).
- **Test pin:** `crates/cherchi-rs/tests/coplanar_dupl_restore.rs:166-239`. Unpinned for `UL-04`/`UL-05` → see `TC-04`, `TC-05`.

#### `C22-11` — §4 Fig. 4 Three-step arrangement refinement (1: detect intersections, 2: insert points, 3: insert segments)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:272-275, 300-303 L`):**
  > *"Fig. 4. First step of the Boolean pipeline. We start from an generic triangle soup (left) and detect intersection points and lines (1). We then split triangles to incorporate new points first (2), and then segments (3). The output is a well formed simplicial complex (right)."* (`:274-275`)
- **Code (`crates/cherchi-rs/src/arrangements/soup.rs:766-1082`; `intersection_detection.rs:77-145`; `intersection_points.rs:195-330`; `aux_structure.rs:195-495`; `coplanar_propagate.rs:70-480`; `retriangulate.rs:74-94`; `enforce.rs:104-266`):**
  - Step 1 (Detect): `detect_intersecting_pairs(&soup)` (`soup.rs:769`) queries an `AabbTree` (`intersection_detection.rs:100`) and filters non-adjacent pairs with exact `triangles_do_intersect`; `classify_intersections` (`soup.rs:775`) constructs `Explicit` and `Lpi` points; `group_intersection_points`, `group_constraint_segments`, and `integrate_coplanar_into_arrangement` (`soup.rs:833-855`) intern points by exact `[RBig; 3]` coordinates (`AR3c` / `N18`).
  - Step 2 (Insert points): `split_single_triangle(&mut subm, &flat)` (`soup.rs:926`, `retriangulate.rs:74-94`) inserts every interior and on-edge point into a per-triangle `FastTrimesh`:
    ```rust
    split_single_triangle(&mut subm, &flat).map_err(|e| ...)?;
    enforce_constraints(&mut subm, &segments_per_tri[t as usize], &points).map_err(|detail| ...)?;
    ```
  - Step 3 (Insert segments + TPIs + coplanar pocket dedup): `enforce_constraints` (`soup.rs:940`, `enforce.rs:138-161`) realizes constraint segments, minting `VertexCoords::Tpi` where two constraints cross (`enforce.rs:373-435`), and `VisitedPocketRegistry` (`soup.rs:987-1042`) deduplicates shared coplanar pockets across operands.
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
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:212-360, 1295-1348`):**
  - `compute_inside_out` (`inside_out.rs:212-360`) initializes `let mut inner = vec![vec![false; num_inputs]; soup.tris.len()];` (`:218`, all outside) and loops once per patch `(p_id, patch)` (`:225`), casting a single ray `ray` (`:234-282`, or Tier-3 rational ray `:284-345`), running `prune_intersections_and_sort_along_ray` + `analyze_sorted_intersections` once, and propagating `patch_inner` to all `t_id in patch` (`:358`).
  - **Gap (`UL-03`):** Line `226` sets `let patch_surface_label = &soup.labels[patch[0] as usize];` and `prune_intersections_and_sort_along_ray` (`:1304-1308`), `ray_grazes_foreign_input` (`:519-523`), and `rational_ray_inner_label` (`:852-854`) skip any input triangle whose `in_labels` intersects `patch_surface_label`. Under `N23` (`patches.rs:128-129`), a patch can mix `[A, B]` coplanar triangles and `[B]` single-solid triangles. Whenever `patch[0]` (the lowest output triangle index in the patch) is an `[A, B]` triangle, `patch_surface_label` is `[A, B]`, causing `compute_inside_out` to skip **both** $A$ and $B$ and leave the `[B]` triangles unconditionally `outside` $A$ even when enclosed in $A$!
- **Status:** **`PARTIAL`**
- **Ledger:** `N23` (interaction `UL-03` is unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1410-2335`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:1-1215`. Unpinned for `UL-03` → see `TC-03`.

#### `C22-17` — §5 / Alg. 1 / Fig. 5 Signed tetrahedron volume test $\text{orient3D}(t_i, p_\infty) < 0$ on first hit
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:342-348 R, 369-374 R, 389-395 R`):**
  > *"Given an intersection point $i$ and the triangle $t_i$ containing it, the ray traverses the mesh from inside to outside if the volume of the tetrahedron $(t_i, p_\infty)$ is negative, from outside to inside otherwise. This check can be performed exactly with arithmetic orientation predicates."* (`Fig. 5, :369-374 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1273-1293`):**
  - `analyze_sorted_intersections` (`inside_out.rs:1273-1293`) tracks a `visited` set of input-mesh labels along `sorted_inters` (already filtered to exclude `patch_surface_label`), inspects **only the first hit** per foreign input mesh $M$, and evaluates `orient3d(tv[0], tv[1], tv[2], ray.v1)`:
    ```rust
    match orient3d(tv[0], tv[1], tv[2], ray.v1) {
        Sign::Negative => inner_labels.push(id),
        Sign::Positive => {}
        Sign::Zero => {
            return Err(InsideOutError::DegenerateOrientation { tri: t_id });
        }
    }
    ```
  - Sign convention verified against `predicates/orient.rs:58-70`: `orient3d(a, b, c, d)` returns the sign of $\det[a-d, b-d, c-d] = (a-d) \cdot ((b-d)\times(c-d)) = -(d-a)\cdot((b-a)\times(c-a))$, which equals $+6\,\text{SignedVol}(a, b, c, d)$ under Cherchi/CinoLib's determinant in `:283-289 R`; when $p_\infty$ lies on the outward normal side of $(t_0, t_1, t_2)$, $(d - a)\cdot n_{\text{out}} > 0 \iff \text{orient3d}(t_0, t_1, t_2, p_\infty) = \text{Sign::Negative}$.
- **Status:** **`FAITHFUL`**
- **Ledger:** `N24`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1510-1620`; `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:150-310`.

#### `C22-18` — §5.1 Axis-aligned point at infinity $p_\infty$ strictly outside scene bounding box
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:421-429 L`):**
  > *"If $p$ is known, the infinite point $p_\infty$ can be easily defined by translating $p$ along one of the major axes by a quantity that is bigger than the extent of the bounding box of the input scene along the same axis (this guarantees that $p_\infty$ is outside all input meshes). Moreover, having an axis-aligned ray considerably simplifies the next ray-triangle intersection analysis as it always allows to drop one coordinate and operate on a 2D plane instead of the 3D space."* (`:422-429 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:193-203, 591-595, 660-664`):**
  - `bbox_max_plus_half` (`inside_out.rs:193-203`) computes the coordinate-wise max over all explicit vertices in `soup.in_tris` and adds a literal `+ 0.5`: `[max_c[0] + 0.5, max_c[1] + 0.5, max_c[2] + 0.5]`. Ray construction copies `v0` into `v1` and overwrites `v1[axis] = max_coords[axis]` (`:594, :663`).
  - **Gap (`UL-05`):** `soup.verts` are multiplied by `compute_multiplier` (`multiplier.rs:33-48`), which scales coordinates of magnitude $M \ge 1$ up to $\sim M^2$. Whenever unscaled `max_abs >= 2^26 = 67_108_864.0`, multiplied `max_c[k] >= 2^52`, where `ulp(max_c[k]) >= 1.0 > 0.5`, so `max_c[k] + 0.5 == max_c[k]` in IEEE-754! $p_\infty$ then lies **on** the scene bounding box face rather than outside all input meshes, violating `:425-426 L` and causing `orient3d(tv, ray.v1) == Sign::Zero` (`Err(DegenerateOrientation)`) whenever the first hit lies on that bbox plane.
- **Status:** **`PARTIAL`**
- **Ledger:** — (`UL-05` unledgered).
- **Test pin:** Unpinned at large coordinates → see `TC-05`.

#### `C22-19` — §5.1 Tier 1 ray definition from an interior explicit input vertex of $P$
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:430-433 L`):**
  > *"In the simplest case, the patch $P$ contains at least one input vertex with known ground-truth floating point coordinates in its interior, which can be used as a starting point for $r$."* (`:431-433 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:582-625, 507-557`):**
  - `try_patch_ray_from_verts` (`inside_out.rs:582-625`) collects vertices of `patch` that are not in `patches.border_verts` (`!border.contains(&v_id)`) and match `VertexCoords::Explicit(p)`.
  - **Divergence (`N69`):** Whereas the paper and C++ `findRayEndpoints` (`booleans.cpp:544-567`) unconditionally take the **first** non-border explicit vertex along `+X`, `inside_out.rs:590-623` scans all `(interior_vertex, dir ∈ {X, Y, Z})` candidates and skips any candidate for which `ray_grazes_foreign_input` (`:507-557`) detects a foreign input triangle in the ray AABB with `orient3d(tv, v0) == Zero || orient3d(tv, v1) == Zero` whose vertex or edge is grazed (`fast_2d_check_intersection_on_ray` returning `IntInV`/`IntInEdge` or coplanar segment overlap `coplanar_ray_touches_tri`), falling back to `first_grazing` only if every candidate grazes. This is gated by an un-debug-gated env var `CHERCHI_GRAZE_AWARE_RAY` (`:440-446`, **UL-07**).
- **Status:** **`DEVIATES`**
- **Ledger:** `N69` (`CHERCHI_GRAZE_AWARE_RAY` release read is `UL-07`).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:2120-2335` (N69 / P0023 regression fixtures).

#### `C22-20` — §5.1 Tier 2 ray definition from approximate `f64` triangle barycenter pushed beneath $t$
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:445-460 L, 404-410 R`):**
  > *"Our main idea is that if we can define a ray that starts from beneath $P$ and is guaranteed to traverse the patch at some internal point $p \in P$, we can simply sort all the intersections we find, and perform the inside/outside classification by considering the first intersection that occurs after $p$. Our strategy is as follows: we pick a random triangle $t \in P$ and convert the coordinates of its implicit vertices into explicit floats, computing the approximate triangle barycenter $b_t$. To make sure that $b_t$ stays beneath triangle $t$ we push the point backwards along the same axis used to define $p_\infty$. ... Thanks to the axis alignment we can efficiently and reliably test that the wanted intersection exists with a simple 2D point in triangle test, performed considering the orthogonal projection of both $t$ and $b_t$ along the ray direction. If the test fails we attempt to produce the same construction with another triangle of $P$, until we find a valid one."* (`:445-457 L, 404-410 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:629-702, 742-787, 1353-1384`):**
  - `try_patch_ray_from_tris` (`inside_out.rs:629-702`) iterates deterministically over `&t_id in patch` (`:649`, deterministic scan instead of random pick), materializes approximate vertex coordinates via `approx_vert` (`:742-765`: `Explicit` verbatim, `Lpi` via `approx_lpi` interval midpoint, `Tpi` via exact `RBig::to_f64`), averages `(p0 + p1 + p2) / 3.0`, pushes `v0c[dir.idx()] -= 0.1` (`:658`), tries axes `{X, Y, Z}`, and validates via `find_first_inters_into_patch` (`:682, :1353-1384`) that the ray intersects `P` in the forward direction, returning `RayChoice::Generated { ray, seed_tri: first_t }`.
  - Downstream `prune_intersections_and_sort_along_ray` (`:1300-1348`) passes `seed_tri` into `sort_hits_along_ray` (`:1214-1261`), which filters hits behind `seed_tri` via `behind_seed_plane` (`less_than_on_{x,y,z}_indirect(&hit_lpi, &seed_lpi) != Greater`), ensuring only intersections strictly after $p \in P$ are analyzed (`:448-450 L`).
  - **Gaps (`UL-05`, `UL-06`):**
    1. **Missing N69 graze check (`UL-06`):** Unlike Tier 1 (`:596-604`), `try_patch_ray_from_tris` (`:682-699`) never calls `ray_grazes_foreign_input`! On an originless patch (all vertices on patch borders) whose approximate centroid lies on a foreign face/edge plane (common on symmetric CAD cuts centered at `0.0`), Tier 2 accepts a grazing ray on `Axis::X` instead of trying `Axis::Y`/`Axis::Z` or falling through to Tier 3, re-exposing the N69 silent-wrong parity bug.
    2. **Scale-dependent `- 0.1` (`UL-05`):** At unscaled `max_abs >= 2^26` (multiplied `cen >= 2^52`), `v0c[k] -= 0.1` is a floating-point no-op (`v0 == cen`), so `before_origin` (`N20`) discards the $\lambda = 0$ crossing of `seed_tri` and Tier 2 fails on every triangle.
- **Status:** **`PARTIAL`**
- **Ledger:** — (`UL-05`, `UL-06` unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1700-1820`. Unpinned for `UL-06` → see `TC-06`.

#### `C22-21` — §5.1 Tier 3 exact rational barycenter & exact rational ray casting fallback
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:409-412 R, 444-445 L`):**
  > *"instead, inspired by the cascaded approaches used by filtered predicates [Attene 2020; Lévy 2016; Shewchuk 1997], we define the ray $r$ by first attempting to find a satisfactory approximate floating point solution, while we resort to guaranteed exact rational numbers only as backup strategy."* (`:441-445 L`)
  > *"If no valid triangle can be found, we compute the exact barycenter with rational numbers and perform an exact ray casting."* (`:409-411 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:284-345, 795-961`):**
  - Whereas upstream C++ `booleans.cpp:616` left Tier 3 unimplemented (`std::cerr << ...; exit(1)`), `cherchi-rs` implements the paper's exact rational backup in `rational_ray_inner_label` (`inside_out.rs:795-961`):
    ```rust
    let origin = [
        (&(&c0[0] + &c1[0]) + &c2[0]) / RBig::from(3),
        (&(&c0[1] + &c1[1]) + &c2[1]) / RBig::from(3),
        (&(&c0[2] + &c1[2]) + &c2[2]) / RBig::from(3),
    ];
    ```
  - For `axis` in `[0, 1, 2]` (`+X, +Y, +Z`): tests every foreign `in_tris` triangle in exact `RBig` 2D edge-function arithmetic (`s1, s2, s3` at `:885-895`); if any projected ray hits a triangle boundary (`(s1==0 || s2==0 || s3==0)` with consistent sign, `:898-901`), retries the next axis (`continue 'axis`); computes exact plane normal `n` and ray parameter `t = rhs / n[k]`; keeps strict forward hits `t > 0` (`rhs` and `n[k]` same strict sign, `:916-924`); selects the minimum exact rational `t` per foreign solid (`:928-940`); and classifies inside iff `n[k] > 0` (ray exits along outward normal, `:950-954`). If all three axes hit an exact vertex/edge boundary, returns loud `Err(InsideOutError::NoValidPatchRay)` (`:959`).
- **Status:** **`FAITHFUL`** *(faithful to the paper; diverges from C++'s `exit(1)` stub, tracked as `N21`)*
- **Ledger:** `N21`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1950-2115` (`kv4_f1` sliver/implicit-patch rational-ray tests).

---

### §5.2 Intersection Detection & Along-Ray Sorting

#### `C22-22` — §5.2 Exact 2D projected point-in-triangle ray-triangle intersection (no Möller–Trumbore)
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:424-429 R, 442-446 R`):**
  > *"In a typical ray casting implementation, ray-triangle intersections are computed using the efficient Möller–Trumbore algorithm [1997]. Unfortunately, our exactness requirements make it impossible to rely on such an algorithm, which involves floating point operations that accumulate error and it is also non stable in case of coplanarity. ... Once again, we exploit the axis aligned nature of our problem to recast the ray-triangle intersection as a point in triangle query in 2D, as previously described in Section 5.1."* (`:424-429 R, 442-445 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1013-1057`; `crates/cherchi-rs/src/predicates/simplex_location.rs:219-280, 89-97`):**
  - `fast_2d_check_intersection_on_ray` (`inside_out.rs:1013-1057`) drops `ray.dir` (`Axis::X` → YZ, `Axis::Y` → ZX, `Axis::Z` → XY) and delegates to `point_in_triangle_3d_loc` (`simplex_location.rs:219-280`), mapping `StrictlyInside → IntInTri`, `OnEdge(k) → IntInEdge(v0, v1)`, `OnVertex(c) → IntInV(v)`, and `Outside | Degenerate → NoInt`.
  - **Gap (`UL-04`):** `simplex_location.rs:89-97` defines its own private `orient2d_sign` calling raw `geometry_predicates::orient2d(pa, pb, pc)` and mapping `0.0 => Sign::Zero` **without** `N24` rational zero-certification (`orient::orient2d`). At subnormal determinant scales, a ray strictly outside a triangle (`true det < 0`) whose third edge determinant underflows to `0.0` while the other two are `< 0` is falsely classified as `OnEdge` (`IntInEdge`) instead of `Outside` (`NoInt`).
- **Status:** **`PARTIAL`**
- **Ledger:** `N24` (not applied in `simplex_location.rs` — `UL-04`).
- **Test pin:** `crates/cherchi-rs/src/predicates/simplex_location.rs:290-549`. Unpinned for subnormal `simplex_location` underflow → see `TC-04`.

#### `C22-23` — §5.2 Shared axis-aligned octree acceleration structure across arrangement & ray-casting
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:432-436 R`):**
  > *"In our implementation, we use a plain octree as acceleration structure and perform ray casting by testing intersections between the ray bounding box and each octant. Note that, for efficiency, this is the same acceleration structure used in the arrangement part to detect triangle intersections."* (`:432-436 R`)
- **Code (`crates/cherchi-rs/src/aabb_tree.rs:1-165`; `crates/cherchi-rs/src/arrangements/intersection_detection.rs:85-104`; `crates/cherchi-rs/src/labeling/octree.rs:34-101`; `crates/cherchi-rs/src/labeling/native.rs:127, 138`):**
  - Both stages use the axis-aligned `AabbTree` (`src/aabb_tree.rs`; median-split BVH with 1-ULP box padding in `Aabb::from_points`), and `TriOctree::intersects_ray` (`labeling/octree.rs:76-101`) applies an exact unpadded `tri_in_ray_aabb` post-filter so candidate inclusion uses zero floating-point arithmetic (pure `min`/`max`/`<=`/`>=` comparisons, preserving exactness).
  - **Gap:** The acceleration structure is **built twice** instead of reused: once inside `detect_intersecting_pairs` (`intersection_detection.rs:100`, over `kept_tris` before `dupl_triangles` restoration) and a second time in `TriOctree::build` (`native.rs:138`, `octree.rs:67-73`, over `soup.in_tris` after step-11 `dupl_triangles` restoration). Performance-only; exact candidate superset invariant holds.
- **Status:** **`PARTIAL`**
- **Ledger:** `N95`
- **Test pin:** `crates/cherchi-rs/src/labeling/octree.rs:180-412` (`query_is_superset_of_brute`).

#### `C22-24` — §5.2 Tight ray AABB & 4-float 2D octant overlap check
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:437-441 R`):**
  > *"Since both the octree and the ray are axis aligned we have two nice properties: the bounding box of the ray is tight (it’s the ray itself), and the intersection with the octant reduces to a 2D check which involves only four comparisons between floats."* (`:437-440 R`)
- **Code (`crates/cherchi-rs/src/labeling/octree.rs:76-118`; `crates/cherchi-rs/src/aabb_tree.rs:133-155`):**
  - `TriOctree::intersects_ray` (`octree.rs:76-101`) constructs the tight degenerate ray AABB `lo = [v0[k].min(v1[k])]`, `hi = [v0[k].max(v1[k])]` (zero epsilon slab, zero rounding).
  - **Minor difference:** Both `AabbTree::query` (`aabb_tree.rs:133-155`) and `tri_in_ray_aabb` (`octree.rs:105-118`) perform 6 float comparisons across all 3 axes (`[x_min, x_max] × [y_0, y_0] × [z_0, z_0]`) rather than 4 comparisons on the 2 orthogonal axes. In fact, keeping the ray-axis interval `[v0[dir], v1[dir]]` in the query **prunes** triangles strictly behind `v0` (`tri_max < v0[dir]`) before predicate evaluation, while remaining exact because `min`/`max`/`<=` on IEEE-754 floats introduce zero rounding error.
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/octree.rs:210-340`.

#### `C22-25` — §5.2 Implicit LPI representation of ray hits & exact along-ray sorting
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:447-453 R`):**
  > *"Since our classification is entirely based on the analysis of the first intersection between the ray and each input mesh, it is necessary to sort intersection points. Firstly, we represent intersections implicitly, using the LPI (Line-Plane Intersection) points described in [Attene 2020]. Then, we use the exact comparator introduced in [Cherchi et al. 2020] to sort them from the closest to the ray emanating point to the furthest."* (`:447-453 R`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1154-1269`):**
  - `sort_hits_along_ray` (`inside_out.rs:1185-1269`) represents every candidate hit triangle `t_id` as `GenericPoint3D::Lpi { line: [ray.v0, ray.v1], plane: tv }` (`:1197-1201`), compares hits along `ray.dir` via `less_than_on_{x,y,z}_indirect` (`:1169-1175`), and breaks exact ties deterministically by `a.0.cmp(&b.0)` (`:1264`).
  - **Divergence (`N20`):** `before_origin` (`inside_out.rs:1249-1255`) filters with `axis_cmp(ray.dir, &hit.1, &origin) != Ordering::Greater` (rejecting **both** `<` behind-origin hits and `==` ray-parameter-zero hits $\lambda = 0$), whereas upstream C++ `sortIntersectedTrisAlong*` (`booleans.cpp:898`) used `< 0` and kept $\lambda = 0$ hits (which mislabeled patches touching a foreign input at an interior point). For Tier-2 rays (`seed_tri = Some(st)`), `behind_seed_plane` (`:1214-1231`) likewise requires `axis_cmp(ray.dir, &hit.1, &seed_lpi) == Ordering::Greater`.
- **Status:** **`DEVIATES`** *(intentional correctness strengthening `N20` over C++)*
- **Ledger:** `N20`
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1825-1945` (point-touch $\lambda = 0$ regression test).

---

### §5.3 Classification & Perturbation of Degenerate Hits

#### `C22-26` — §5.3 Direct local-winding classification when ray crosses strict triangle interior
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:457-460 R, 473-476 L`):**
  > *"The simplest configuration is when $r$ crosses $M$ at a point that is interior to a triangle (Figure 6, left). In this case determining if the ray is passing from inside to outside or vice-versa consists in simply analyzing the triangle orientation, which is encoded in the local winding (triangle vertex order) and can be tested exactly as shown in Figure 5 (left)."* (`:458-460 R, 473-476 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1322-1324, 1346-1347, 1273-1293`):**
  - In `prune_intersections_and_sort_along_ray` (`inside_out.rs:1322-1324`):
    ```rust
    IntersInfo::IntInTri => {
        hits.push(t_id);
    }
    ```
    Strict interior hits bypass perturbation completely, are sorted along the ray by `sort_hits_along_ray`, and are classified directly by `analyze_sorted_intersections` via `orient3d(tv[0], tv[1], tv[2], ray.v1)`.
- **Status:** **`FAITHFUL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1510-1620`.

#### `C22-27` — §5.3 Perturbation of $p_\infty$ via `nextafter` on vertex/edge ray hits until strict interior hit found
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:484-492 L`):**
  > *"Each time a ray does not intersect a patch triangle at an inner point, we perturb the coordinates of $p_\infty$ by $\epsilon$ (without moving its starting point) until the crossing happens at a point that is interior to a mesh triangle. Perturbation of point coordinates is performed using the next floating-point number representable starting from a given number (using std::nextafter). The perturbed ray is then tested again for intersection, and the operation is repeated until a valid intersection point is found."* (`:485-492 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1087-1152, 1325-1345, 1364-1379`):**
  - On `IntInV(v_id)` or `IntInEdge(v0, v1)`, `prune_intersections_and_sort_along_ray` (`:1325-1345`) gathers the local 1-ring (`adj_in_v2t` filtered to `tested_label`) or 2-triangle edge link (`adj_in_e2t` filtered to `tested_label`), deduplicates via `visited_verts`/`visited_edges`, and calls `perturb_ray_and_find_inters_tri`.
  - **Three substantive divergences (`N19`, `UL-01`, `UL-02`):**
    1. **Premature `return None` on behind-origin line hits (`UL-01`, CRITICAL BUG at `inside_out.rs:1147-1150`):** `check_intersection_inside_triangle_3d(&p_ray, &tv)` tests the **infinite line** through `(v0, v1)` (`\lambda \in \mathbb{R}`). When `offset = 0` (`da = +1, db = 0`) hits a 1-ring triangle behind `v0` (`\lambda < 0`, so `!hits.is_empty()` is `true`) while grazing an edge on the forward side (`\lambda > 0`, `orient3d == Zero`), `sort_hits_along_ray` discards the `\lambda < 0` hit and returns `sorted = []`, and `return sorted.first().copied()` **immediately returns `None` on `offset = 0` without testing `offset = 1..7`**, silently dropping a real transversal crossing!
    2. **Fixed 8-offset 1-ULP cap + silent tangential assumption (`UL-02`, CRITICAL PAPER DIVERGENCE at `inside_out.rs:1095-1152, 1337-1342`):** Instead of repeating perturbation *"until a valid intersection point is found"* (`:491-492 L`), `perturb_ray` only tests 8 single-ULP offsets `(±1, 0), (0, ±1), (±1, ±1)`. If all 8 offsets land on spoke edges (`orient3d == Zero`) — which occurs deterministically whenever a ray hits the apex of a regular 8-sided cone/bipyramid centered at `(y_c, z_c)` with symmetric ULPs (e.g. `y_c = z_c = 1.5`) — `perturb_ray_and_find_inters_tri` returns `None`, and `:1337-1340` assumes *"no clean perturbed crossing means the event is tangential/grazing"* and skips it, silently misclassifying an enclosed solid as outside!
    3. **Coherent per-offset winner selection (`N19`, `inside_out.rs:1138-1150`):** Evaluates all ring triangles under ONE `offset` and picks the closest forward hit along `p_ray`, fixing C++'s mixed-ray `inters_tris` accumulation bug (`booleans.cpp:801-828`).
- **Status:** **`DEVIATES`**
- **Ledger:** `N19` (`UL-01` and `UL-02` are unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1625-1698` (translation-along-X perturbation test); `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:315-520`. Unpinned for `UL-01` & `UL-02` → see `TC-01`, `TC-02`.

#### `C22-28` — §5.3 Exact 3D ray-triangle interior test via three tetrahedron `orient3D` signs
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:492-500 L`):**
  > *"Note that perturbed rays are no longer axis aligned, therefore we cannot reduce the ray-triangle intersection to a 2D problem, and we need to perform this test in 3D. The full test consists in checking the sign of three tetrahedra, formed considering the two ray endpoints and the endpoints of each triangle edge. If all signs are strictly positive, or negative, there is an intersection inside the triangle. The computational overhead of this check is minimal (three orient2D for the 2D case and three orient3D for the 3D case)."* (`:492-500 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:1077-1083`):**
  - `check_intersection_inside_triangle_3d` (`inside_out.rs:1077-1083`) evaluates `orient3d(ray.v0, ray.v1, tv[0], tv[1])`, `orient3d(ray.v0, ray.v1, tv[1], tv[2])`, and `orient3d(ray.v0, ray.v1, tv[2], tv[0])` (with `N24` rational zero-certification) and returns `all_strict_same_sign(v0, v1, v2)`.
  - **Gap (`UL-01` enabler):** Three edge-tetrahedron signs test whether the **infinite line** through `(ray.v0, ray.v1)` pierces the strict triangle interior, NOT whether the **forward ray** `[ray.v0, +∞)` pierces it (no `orient3d(tv[0], tv[1], tv[2], ray.v0) != orient3d(tv[0], tv[1], tv[2], ray.v1)` straddle check). Deferring forward-ray filtering to `sort_hits_along_ray` is sound *only* if `perturb_ray_and_find_inters_tri` checks `!sorted.is_empty()` rather than `!hits.is_empty()` (`inside_out.rs:1147`).
- **Status:** **`PARTIAL`**
- **Ledger:** `N24` (`UL-01` unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:1625-1698`. Unpinned for behind-origin line pierce → see `TC-01`.

#### `C22-29` — §5.3 Coplanar / tangential ray-triangle graze handling
- **Paper (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:469-471, 480-488 L`):**
  > *"Note that the pathological cases depicted in the figure are not exhaustive. In fact, rays may also be tangent at a (coplanar) triangle."* (`Fig. 6, :469-471`)
  > *"Four of these cases are depicted in Figure 6, but there are also others (e.g. when a tangent ray is also coplanar to a triangle). ... Each time a ray does not intersect a patch triangle at an inner point, we perturb the coordinates of $p_\infty$ by $\epsilon$ (without moving its starting point) until the crossing happens at a point that is interior to a mesh triangle."* (`:480-488 L`)
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:347-357, 507-579, 1013-1057`):**
  - On a triangle whose plane contains the axis-aligned ray, orthogonal 2D projection collapses the triangle to a collinear 2D segment (`simplex_location.rs:230-241` returns `TriangleLocation::Degenerate`), which `fast_2d_check_intersection_on_ray` (`inside_out.rs:1032`) maps to `IntersInfo::NoInt` — meaning the coplanar triangle itself is invisible in 2D while its bounding edges/vertices on adjacent slanted triangles trigger `IntInEdge`/`IntInV`.
  - As documented in `N69` (`docs/yang_deviations.md:6456-6510`), when a ray **grazes** an exterior coplanar face/edge/vertex of a foreign solid (e.g. P0023), perturbing $p_\infty$ tilts the ray *into* the grazed solid, turning a 0-crossing tangent graze into a false 1-crossing entry with inward-facing first hit!
  - `cherchi-rs` mitigates this at Tier 1 via `ray_grazes_foreign_input` (`inside_out.rs:507-557`, including `coplanar_ray_touches_tri` `:559-579`) plus the post-condition `refute_inner_label_by_input_bounds` (`:347-357`, `Err(InnerLabelOutsideInputBounds)`).
  - **Gaps:** (1) Tier 2 (`try_patch_ray_from_tris`, `:629-702`) omits `ray_grazes_foreign_input` (**UL-06**); (2) if all Tier-1 candidates graze (`first_grazing` fallback at `:622`), `refute_inner_label_by_input_bounds` only catches false-positive `inside` labels on patches disjoint from the foreign AABB — it cannot catch a false-positive `inside` label on a patch inside a non-convex cavity of $B$'s AABB, nor any false-negative `outside` label!
- **Status:** **`PARTIAL`**
- **Ledger:** `N69` (`UL-06` unledgered).
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out.rs:2120-2335`. Unpinned for Tier-2 graze (`UL-06`) & non-convex AABB graze → see `TC-06`, `TC-07`.

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
- **Code (`crates/cherchi-rs/src/labeling/inside_out.rs:225-360`; `crates/cherchi-rs/src/labeling/native.rs:109, 529`):**
  - `compute_inside_out` (`inside_out.rs:225-360`) casts one ray per patch and classifies all foreign input meshes from a single `TriOctree` traversal + single `sort_hits_along_ray` + single `analyze_sorted_intersections` pass, matching the paper's $O(1)$-ray-per-patch scaling.
  - **Gap:** Public `native_labeled_arrangement` (`native.rs:109`) and `NativeBoolean` (`native.rs:529`) only expose binary `(a: &Mesh, b: &Mesh)` entry points (`C22-05`).
- **Status:** **`PARTIAL`**
- **Ledger:** —
- **Test pin:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs:640-725` (3-solid internal ray-cast tests).

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
- **Code (`crates/cherchi-rs/src/labeled_arrangement.rs:78-99`; `crates/cherchi-rs/src/labeling/native.rs:222-246`; `crates/yang-rs/src/boolean.rs:526-575, 1929-2159`):**
  - `native_labeled_arrangement` (`native.rs:222-246`) exports `lpi_through_vertex: Vec<(u32, u32)>` naming every `(Lpi, Explicit)` vertex pair where exact `point_in_triangle_3d` (`orient2d == 0` on all three projections) certifies the plane passes through the explicit vertex.
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
- **Test pin:** `crates/yang-rs/src/tests_unit/membrane.rs`; `crates/yang-rs/src/tests_unit/s442_side_labels.rs` (does not yet test `keep_for_op` on `!keep_sheet` coplanar membranes → see `TC-08`).

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

The user prompt singles out **exactness of the §5 ray-cast (exact ray/triangle tests and handling of degenerate hits)** as the highest-value check of this audit. Walking the complete call graph in `crates/cherchi-rs/src/labeling/inside_out.rs` (`:1-1408`), `crates/cherchi-rs/src/labeling/octree.rs` (`:1-175`), and `crates/cherchi-rs/src/predicates/` reveals **what is genuinely exact** and **five concrete cracks where exactness or degenerate-hit handling breaks**.

### 3.1 Step-by-Step Exactness Trace of `compute_inside_out`

1. **Input-shell substrate (`soup.in_tris` / `soup.in_labels`, `soup.rs:1089-1109`):**
   - Unlike output triangles (`soup.tris` / `soup.labels`), which carry subdivided implicit vertices and OR-merged coplanar labels `{A, B}`, the ray-cast tests **unsplit explicit input triangles** in `soup.in_tris` after step-11 `dupl_triangles` restoration. Every vertex of `soup.in_tris` is `VertexCoords::Explicit(Point3)` (`inside_out.rs:163-169`).
   - **Exactness verdict:** Sound *provided* no valid input triangle was deleted at step 3 (`soup.rs:372`). **Crack #1 (`UL-04`):** `points_are_collinear_3d` (`collinearity.rs:30-33`) uses uncertified `geometry_predicates::orient2d == 0.0`, which underflows non-collinear triangles at scale `~1e-162` to `0.0` and deletes them from `kept_tris`, punching holes in `soup.in_tris`.

2. **Broad-phase candidate filtering (`TriOctree::intersects_ray`, `octree.rs:76-118`):**
   - `Aabb::from_points` (`aabb_tree.rs:35-48`) widens leaf/internal node boxes by 1 ULP (`next_down` / `next_up`), and `TriOctree::intersects_ray` post-filters leaf triangles with `tri_in_ray_aabb` (`octree.rs:105-118`) using exact unpadded `min`/`max` comparisons against `[min(v0, v1), max(v0, v1)]`.
   - **Exactness verdict:** **100% exact superset** for every axis-aligned AND perturbed ray (`p_ray.v1` moves by 1 ULP, so `[min(v0, p_ray.v1), max(v0, p_ray.v1)]` stays tight). Zero arithmetic rounding occurs in `min`/`max`/`<=`/`>=`.

3. **Unperturbed 2D ray-triangle classification (`fast_2d_check_intersection_on_ray`, `inside_out.rs:1013-1057`):**
   - Projects `ray.v0` and `tv[0..3]` orthogonally onto YZ (`Axis::X`), ZX (`Axis::Y`), or XY (`Axis::Z`) — zero coordinate rounding — and calls `point_in_triangle_3d_loc` (`simplex_location.rs:219-280`).
   - **Crack #2 (`UL-04`):** `simplex_location::orient2d_sign` (`simplex_location.rs:90`) calls `geometry_predicates::orient2d` directly without `N24` `RBig` zero-certification! Under subnormal determinant underflow (`|det| <= 5e-324`), a non-zero negative sign collapses to `Sign::Zero`, turning `Outside` into a false `OnEdge` (`IntInEdge`) or `StrictlyInside` into a false `OnEdge`.
   - **Coplanar ray-in-plane behavior:** If `tv` is parallel to `ray.dir`, all three projected 2D edge determinants sum to zero area; `point_in_triangle_3d_loc` (`simplex_location.rs:230-241`) detects two/three `Sign::Zero` values with no strict interior and returns `TriangleLocation::Degenerate` → `IntersInfo::NoInt`. Thus a ray-parallel face is never directly added to `hits`; the ray interacts only with the `IntInEdge` / `IntInV` boundary shared with adjacent non-parallel faces of the closed shell.

4. **Perturbation of degenerate vertex (`IntInV`) & edge (`IntInEdge`) hits (`perturb_ray_and_find_inters_tri`, `inside_out.rs:1087-1152`):**
   - Perturbs `ray.v1` ($p_\infty$) on the two axes orthogonal to `ray.dir` by 8 fixed single-ULP steps (`next_up` / `next_down`):
     `offset 0..8 = [(+1,0), (-1,0), (0,+1), (0,-1), (+1,+1), (-1,-1), (+1,-1), (-1,+1)]`.
   - Tests each ring triangle in 3D via `check_intersection_inside_triangle_3d` (`inside_out.rs:1077-1083`), which calls `orient::orient3d` (**with** `N24` `RBig` zero-certification!).
   - **Crack #3 (`UL-01` — Premature `return None` on behind-origin line hits):**
     Look at `inside_out.rs:1139-1151`:
     ```rust
     let mut hits = Vec::new();
     for &t_id in tris_to_test {
         let tv = in_tri_verts(soup, t_id);
         if check_intersection_inside_triangle_3d(&p_ray, &tv) {
             hits.push(t_id);
         }
     }
     if !hits.is_empty() {
         let sorted = sort_hits_along_ray(soup, &p_ray, &hits);
         return sorted.first().copied();
     }
     ```
     `check_intersection_inside_triangle_3d` tests the **infinite line** through `(p_ray.v0, p_ray.v1)` (`\lambda \in (-\infty, +\infty)`), whereas `sort_hits_along_ray` filters out behind-origin hits (`\lambda \le 0`, `before_origin` at `:1249`).
     **Exact failure trace:** Suppose `p_ray.v0 = (0.0, 1.0, 1.0)` (inside solid $B$), `ray.v1 = (2.5, 1.0, 1.0)`, and the ray exits $B$ through vertex $V = (0.5, 1.0, 1.0)$ ahead of `v0`. At `offset = 0` (`da = +1, db = 0`), `p_ray.v1 = (2.5, 1.0 + 2^-52, 1.0)` stays in the plane $z = 1.0$:
     - On the forward $+Y$ side ($\lambda > 0$), `p_ray` crosses a spoke edge $V \to E_+ = (1.5, 2.0, 1.0)$ lying in $z = 1.0$, so `orient3d == Sign::Zero` on both $+Y$ triangles (neither enters `hits`).
     - On the backward $-Y$ side ($\lambda < 0$, behind `v0`!), if $V$ has a 1-ring triangle $\triangle(V, B_1, B_2)$ with $B_1 = (-2.0, 1.0 - 0.5\cdot 2^{-52}, 0.0)$ and $B_2 = (-2.0, 1.0 - 0.5\cdot 2^{-52}, 2.0)$, the backward extension of `p_ray` pierces the **strict interior** of $\triangle(V, B_1, B_2)$ at $x = -0.5 < 0$ (barycentric $(0.6, 0.2, 0.2)$)!
     - Result: `hits = [tri(V, B_1, B_2)]` (`!hits.is_empty()` is **`true`**), `sort_hits_along_ray` rejects $x = -0.5 < 0$ via `before_origin` and returns `sorted = []`, and `return sorted.first().copied()` **immediately returns `None` at `offset = 0`** without ever testing `offset = 1..7`! `prune_intersections_and_sort_along_ray` (`:1340`) treats `None` as tangential and **silently drops a genuine exit crossing**, misclassifying enclosed solid $A$ as outside $B$!

   - **Crack #4 (`UL-02` — Fixed 8-offset 1-ULP table vs. paper's unbounded perturbation loop):**
     Cherchi 2022 §5.3 (`:485-492 L`) specifies: *"the operation is repeated until a valid intersection point is found."* Instead, `inside_out.rs:1095-1152` stops after 8 single-ULP offsets and `:1337-1340` assumes `None` means *"the event is tangential/grazing — it contributes no parity crossing; skip it."*
     **Exact failure trace:** Let solid $B$ be a closed **regular 8-sided bipyramid** (standard $N = 8$ CAD cone/cylinder tessellation!) centered on $(y_c, z_c) = (1.5, 1.5)$ (where `1.5.next_up() - 1.5 == 1.5 - 1.5.next_down() == 2^-52`), with apex $V = (1.0, 1.5, 1.5)$ and 8 equatorial vertices at $x = 0.0$, $(y - 1.5, z - 1.5) \in \{(\pm 1, 0), (0, \pm 1), (\pm 1, \pm 1)\}$. Let solid $A$ sit strictly inside $B$ with interior vertex `v0 = (-0.5, 1.5, 1.5)`.
     - N69 `ray_grazes_foreign_input` returns **`false`** on `+X` (`v0` is strictly inside all 8 cone planes; `v1 = (1.5, 1.5, 1.5)` is strictly outside all 8 cone planes).
     - The `+X` ray hits apex $V$ (`IntInV(V)`).
     - Every one of the 8 single-ULP offsets `(±2^-52, 0)`, `(0, ±2^-52)`, `(±2^-52, ±2^-52)` moves `p_ray` along one of the 4 planes $y = 1.5$, $z = 1.5$, $y - 1.5 = z - 1.5$, $y - 1.5 = -(z - 1.5)$ — **landing `orient3d == Sign::Zero` on one of the 8 spoke edges $V E_k$ for all 8 offsets**!
     - `perturb_ray_and_find_inters_tri` exhausts `0..8` and returns `None`; `:1340` skips $V$ as "tangential"; `sorted` is empty; solid $A$ (100% inside $B$) is **silently misclassified as outside $B$**!

   - **Crack #5 (`N69` + `UL-06` — Exterior grazing ray tilted INTO foreign solid by `perturb_ray`):**
     When a ray originates outside solid $B$ on a plane/line tangent to $B$'s boundary (`orient3d(tv, v0) == 0` and `orient3d(tv, v1) == 0`, e.g., $A$ and $B$ share a flush plane $z = 0$ outside their contact patch, P0023), an `IntInEdge` or `IntInV` graze on $B$ is **genuinely tangential (0 crossings)**, yet `perturb_ray` tilts $p_\infty$ toward $-Z$ (into $B$'s interior!), producing **2** real crossings of $B$ (entry at the grazed feature + exit at the far wall) while **only testing the local 1-ring/2-ring of the grazed feature** (`tris_to_test` at `:1327, :1344`)! That records **1** inward-going crossing (false `outside`) or — when the ray grazes a concave/back-facing edge first — **1** outward-going crossing (`orient3d(tv, v1) == Negative` → false `inside`!).
     - Tier 1 (`try_patch_ray_from_verts`, `:596-604`) avoids such rays via `ray_grazes_foreign_input`, **but Tier 2 (`try_patch_ray_from_tris`, `:629-702`) omits `ray_grazes_foreign_input` completely (`UL-06`)**, leaving originless patches exposed!

5. **Along-ray LPI sorting (`sort_hits_along_ray`, `inside_out.rs:1185-1269`) & first-hit orientation (`analyze_sorted_intersections`, `:1273-1293`):**
   - Exact indirect `less_than_on_{x,y,z}_indirect` (`predicates/indirect/mod.rs:580-680`) compares rational LPI coordinates along `ray.dir` via interval filter → `RBig`, with `N20` (`<= Ordering::Equal`) soundly rejecting $\lambda \le 0$ hits.
   - `analyze_sorted_intersections` evaluates `orient::orient3d(tv[0], tv[1], tv[2], ray.v1)` (with `N24` `RBig` zero-certification).
   - **Exactness verdict:** **100% exact** *given* that (a) `ray.v1` actually moved past the bbox (`max_c + 0.5 > max_c`, violated at `max_abs >= 2^26` by **UL-05**) and (b) `sorted_inters[0]` came from the same perturbed ray `p_ray` whose `p_ray.v1` is tested. *(Subtlety check on `analyze_sorted_intersections` at `:1287`: `analyze_sorted_intersections` tests `orient3d(tv[0], tv[1], tv[2], ray.v1)` using the **unperturbed** `ray.v1`, NOT `p_ray.v1`! Could `orient3d(tv, ray.v1)` be `Sign::Zero` when `tv` was selected via `p_ray` because `tv` contains the unperturbed ray `ray` in its plane? Look: if `tv` contains both `ray.v0` and `ray.v1` in its plane, then `p_ray` passes through `ray.v0 ∈ plane(tv)` and `p_ray.v1 ∉ plane(tv)`, so `p_ray` intersects `plane(tv)` ONLY at `ray.v0` ($\lambda = 0$)! And `before_origin` (`N20`, `:1249`) discards $\lambda = 0$! Therefore no triangle coplanar with the unperturbed ray can ever survive `before_origin` as a `p_ray` winner! And for any non-coplanar `tv` through the hit vertex/edge `q` ($\lambda > 0$), `ray.v0`, `q`, and `ray.v1` are collinear with `q` strictly between `ray.v0` and `ray.v1` (since `q_k <= max_c < max_c + 0.5 = ray.v1_k` when `UL-05` does not fire), so `orient3d(tv, ray.v1)` has the exact opposite non-zero sign of `orient3d(tv, ray.v0)` and cannot be `Zero`! That proves testing unperturbed `ray.v1` in `:1287` is algebraically sound **iff** `max_c + 0.5 > max_c` (`UL-05`) and `N20` holds!)*

---

## 4. Code With No Paper Row

Every mechanism, data channel, guard, helper, or environment knob in `crates/cherchi-rs/` (and the `yang-rs` Stage-2 consumer boundary) that has **no procedural counterpart in Cherchi et al. 2022**:

| Site (`path:line` at `b4bee54e`) | Mechanism | Origin / Purpose | Ledger / Spec |
|---|---|---|---|
| `crates/cherchi-rs/src/labeled_arrangement.rs:153-155`; `crates/cherchi-rs/src/labeling/native.rs:587`; `crates/yang-rs/src/boolean.rs:43, 2232-2234` | `BoolOp::Xor` (symmetric difference) keep-rule & inside-flip | Ported from upstream C++ `booleans.cpp:1463-1510` (`boolXOR`), although Cherchi 2022 §3 (`:234-235 R`) lists only `{union, intersection, subtraction}`. Rejected loudly on non-empty kept sets in `yang-rs`. | — |
| `crates/cherchi-rs/src/labeled_arrangement.rs:68-99`; `crates/cherchi-rs/src/arrangements/soup.rs:1072-1081`; `crates/cherchi-rs/src/labeling/native.rs:222-246` | `LabeledArrangement::intersection_edges` and `lpi_through_vertex` | Hybrid-kernel provenance channels added for Yang 2025 Stage-3 SSI curve gating and P0014 LPI-pencil near-weld (`yang-rs/src/boolean.rs:526-575, 2158, 2171-2190`). Absent from Cherchi 2022. | `specs/yang_s3_intersection_edge_provenance.md`, `specs/yang_p0014_arrangement_lpi_pencil_weld.md` |
| `crates/cherchi-rs/src/triangulation/mod.rs:1-2549` (`cdt_polygon_with_holes*`) | 2D Constrained Delaunay Triangulation (`spade` v2 + exact flood-fill / refinement variants) | Hosted in `cherchi-rs` for dependency curation; **never called by `cherchi-rs`'s arrangement or boolean pipeline** (used by `yang-rs` Stage-1/Stage-4 face tessellation and `kernel-v2`). | `N9` |
| `crates/cherchi-rs/src/labeling/inside_out.rs:347-357, 463-579, 596-604` | Foreign-graze avoidance (`ray_grazes_foreign_input`, `coplanar_ray_touches_tri`) + `refute_inner_label_by_input_bounds` (`InnerLabelOutsideInputBounds`) | P0023 mitigation for C++'s exterior coplanar/edge graze perturbation bug; walks interior vertices × `{X, Y, Z}` and verifies `inner` against foreign input AABB overlap. | `N69` |
| `crates/cherchi-rs/src/labeling/inside_out.rs:1249-1255` | Strict `<= Ordering::Equal` ray-origin filter (`before_origin`) | Discards $\lambda = 0$ ray-origin hits (C++ used `< 0`, misclassifying point-touch patches). | `N20` |
| `crates/cherchi-rs/src/labeling/patches.rs:126-178` | `PatchError::LabelMismatch` L2a/L2b subset-compatibility gate | Replaces C++ debug `assert(labels.surface[t_id] == ref_l)` with compatible-subset pass (`L2a`) + loud error on disjoint labels (`L2b`). | `N23` |
| `crates/cherchi-rs/src/predicates/orient.rs:79-94, 145-150, 165-275` | Exact `dashu::rational::RBig` zero-certification on `0.0` returns of `geometry_predicates::{orient2d, orient3d}` | Closes Shewchuk's subnormal underflow hole in `orient.rs` (though bypassed in 3 sibling modules — `UL-04`). | `N24` |
| `crates/cherchi-rs/src/arrangements/soup.rs:64-135, 795-821, 926-972` | Typed `ArrangementError`loud-STOP variants (`CoplanarPairDeferred`, `DegeneratePairDeferred`, `TransversalEndpointOvercount`, `DeepRecursionRequired`) | Replaces C++ silent `assert` / undefined continuation with explicit `Result::Err` stops under P9 governance. | `N16`, `N17` |
| `crates/cherchi-rs/src/arrangements/coplanar_propagate.rs:70-81` (`build_coplanar_adjacency`) | Standalone `CoplanarAdjacency` builder | Dead production helper (`mesh_arrangement` calls `integrate_coplanar_into_arrangement` in `soup.rs:849`, which builds adjacency inline; module doc at `:12-16` still reflects pre-PR-4 standalone state). | — |
| `crates/cherchi-rs/src/labeling/inside_out.rs:440-446`; `crates/cherchi-rs/src/predicates/triangle_intersect.rs:35-44` | **Release-active behavior-altering env vars:** `CHERCHI_GRAZE_AWARE_RAY` and `CHERCHI_SCE_SHORTCIRCUIT` | Read via `std::env::var` **without `cfg!(debug_assertions)`** and **not registered** in `docs/kernel_probe_registry.md` (`UL-07`). | `UL-07` |
| `crates/cherchi-rs/src/` (8 sites: `soup.rs:727,768,946,1112`; `patches.rs:131`; `native.rs:129,293`; `inside_out.rs:240,532,703,1133,1314,1368`) | Diagnostic env probes: `CHERCHI_TIME_PROBE`, `CHERCHI_ENFORCE_PROBE`, `CHERCHI_XEDGE_PROBE`, `CHERCHI_PATCH_PROBE`, `CHERCHI_VERT_PROVENANCE`, `CHERCHI_INOUT_PROBE`, `CHERCHI_GRAZE_PROBE`, `KV4F1_PROBE`, `CHERCHI_PERTURB_PROBE` | Print-only (`eprintln!`) or append-to-file (`CHERCHI_VERT_PROVENANCE`) diagnostics; output-neutral when set or unset. | `docs/kernel_probe_registry.md:76-80` |

---

## 5. Staleness of Prior Audits & Live Documentation

Both prior Cherchi audits — `docs/audits/yang_audit_c_cherchi2022.md` (2026-04-30) and `docs/audits/cherchi_port_audit.md` (2026-04-28) — audited **`crates/kernel/src/boolean/cherchi/*`**, an early prototype deleted on 2026-06-11 (`docs/yang_functional_roadmap.md` history) and replaced by the clean-room `crates/cherchi-rs/` crate. **100% of file paths in both prior audits no longer exist in the repository.**

### 5.1 Disposition of `docs/audits/yang_audit_c_cherchi2022.md` (`YC-01` .. `YC-24`)

| Prior ID | Legacy Finding (`crates/kernel/src/boolean/cherchi/*`) | Prior Verdict | Current `cherchi-rs` / `yang-rs` Site (`b4bee54e`) | New Status (`Row`) | One-Line Disposition |
|---|---|---|---|---|---|
| `YC-01` | `findRayEndpoints` 3-tier cascade missing (centroid-only) | `MISSING` | `inside_out.rs:234-345, 582-702, 795-961` | **`PARTIAL` / `DEVIATES` / `FAITHFUL`** (`C22-19..21`) | All 3 tiers now implemented (Tier 1 `N69` graze-aware, Tier 2 barycenter push with `UL-06` gap, Tier 3 exact `RBig` `N21`). |
| `YC-02` | Backward push `0.1` along ray axis missing | `MISSING` | `inside_out.rs:658` | **`PARTIAL`** (`C22-20`) | `v0c[dir.idx()] -= 0.1` implemented; suffers scale collapse at `max_abs >= 2^26` (`UL-05`). |
| `YC-03` | Axis selection cycles `0..2` instead of `maxComponentInTriangleNormal` | `PERF-DRIFT` | `inside_out.rs:590, 650`; `orientation.rs:55-60` | **`FAITHFUL` / `PARTIAL`** (`C22-19..20`, `C22-02`) | Arrangement uses filtered+exact `max_component_in_triangle_normal`; ray-cast tries `{X, Y, Z}` cleanly (`orientation.rs` has subnormal `UL-04` gap). |
| `YC-04` | `f64` `t_hit` sort instead of exact implicit LPI sort | `CORRECTNESS-BUG` | `inside_out.rs:1185-1269` | **`DEVIATES`** (`C22-25`) | Fixed: `sort_hits_along_ray` uses exact indirect `less_than_on_{x,y,z}_indirect` on `GenericPoint3D::Lpi` + `N20` `<= Equal` origin filter. |
| `YC-05` | Per-sub-triangle ray-cast instead of per-patch ray-cast | `DEVIATES` | `inside_out.rs:212-360` | **`PARTIAL`** (`C22-16`) | Fixed structurally: `compute_inside_out` casts 1 ray per manifold patch; has mixed-patch `patch[0]` skip bug (`UL-03`). |
| `YC-06` | `flood_fill_patches` used Yang intersection-edge barriers instead of manifold-edge barriers | `DEVIATES` | `labeling/patches.rs:72-207` | **`DEVIATES`** (`C22-08`) | Fixed: `cherchi-rs` `compute_all_patches` floods `inc.len() <= 2` manifold edges cleanly (separate from `yang-rs` Stage-5 `flood_fill_patches`), with `N23` L2a/L2b rule. |
| `YC-07` | `weld_mesh_vertices` quantized at `1e-9` nm grid | `CORRECTNESS-BUG` | `arrangements/soup.rs:285-320, 833-855, 1139-1153` | **`FAITHFUL`** (`C22-10`, `C22-11`) | Fixed: `cherchi-rs` uses bit-exact input vertex merge + exact `[RBig; 3]` geometric interning (`N18`), zero grid quantization. |
| `YC-08` | `vertInfo` patch-border vertex marker missing | `MISSING` | `labeling/patches.rs:47-50, 190-195`; `inside_out.rs:585` | **`FAITHFUL`** (`C22-19`) | Fixed: `Patches::border_verts` marks all non-manifold edge endpoints and Tier 1 excludes them. |
| `YC-09` | `IntersInfo` enum (`NoInt`/`IntInTri`/`IntInEdge`/`IntInV`) absent | `CORRECTNESS-BUG` | `inside_out.rs:970-1057` | **`PARTIAL`** (`C22-22`) | Fixed: `IntersInfo` enum + 1-ring/2-ring dispatch implemented; underlying `simplex_location.rs:90` lacks `N24` zero-certification (`UL-04`). |
| `YC-10` | Perturbed sub-tri normal (Hoffmann) instead of `nextafter` on $p_\infty$ | `CORRECTNESS-BUG` | `inside_out.rs:1087-1152` | **`DEVIATES`** (`C22-27`) | Replaced Hoffmann normal nudge with `next_up`/`next_down` on `ray.v1`, but capped at 8 1-ULP offsets (`UL-02`) with premature `return None` (`UL-01`). |
| `YC-11` | `slab_eps = 1e-14` ray AABB expansion | `PERF-DRIFT` | `labeling/octree.rs:76-118` | **`PARTIAL`** (`C22-24`) | Fixed: zero-epsilon tight ray AABB `[min(v0,v1), max(v0,v1)]` + exact `tri_in_ray_aabb` post-filter (6-float check). |
| `YC-12` | No octree shared between arrangement and ray-cast | `PERF-DRIFT` | `intersection_detection.rs:100`; `labeling/octree.rs:67-73` | **`PARTIAL`** (`C22-23`) | Both stages now use `AabbTree` (no longer $O(n^2)$), but build two separate `AabbTree` instances instead of sharing one. |
| `YC-13` | §4 Cached 4-minor `orient3D` predicates not implemented | `PERF-DRIFT` | `predicates/orient.rs:58-95` | **`MISSING`** (`C22-12`) | Unchanged: per-triangle 4-minor `orient3d` plane cache is still not implemented. |
| `YC-14` | `analyzeSortedIntersections` first-per-label `visited` tracking absent | `DEVIATES` | `inside_out.rs:1273-1293` | **`FAITHFUL`** (`C22-17`) | Fixed: `analyze_sorted_intersections` tracks `visited: Vec<InputId>` across $N$ input solids and inspects only the first hit per solid. |
| `YC-15` | Tier-2 `seed_tri` pivot discard pass absent | `MISSING` | `inside_out.rs:1214-1231, 1300-1348` | **`DEVIATES`** (`C22-20`, `C22-25`) | Fixed: `behind_seed_plane` filters hits at/before `seed_tri` via exact indirect `less_than_on_{x,y,z}_indirect`. |
| `YC-16` | `checkTriangleOrientation` (`orient3d < 0` ⇒ inside) | `FAITHFUL` | `inside_out.rs:1287-1292` | **`FAITHFUL`** (`C22-17`) | Confirmed `FAITHFUL` in `cherchi-rs` (`Sign::Negative => inner_labels.push(id)`). |
| `YC-17` | Legacy GWN test `label_cells_raycast_matches_gwn_for_offset_boxes` | `TEST-NOTE` | `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs` | **`SUPERSEDED`** | Replaced by 18-fixture × 4-op native-vs-sidecar parity + adversary suites in `cherchi-rs`. |
| `YC-18` | Hoffmann perturbation fallback present without Cherchi citation | `DEVIATES` |Deleted (`crates/kernel/src/boolean/cherchi/*`) | **`SUPERSEDED`** | Hoffmann perturbation no longer exists anywhere in `cherchi-rs`. |
| `YC-19` | `RayHit::Degenerate` aborted axis loop on first `orient2d == 0` | `CORRECTNESS-BUG` | `inside_out.rs:1318-1345` | **`DEVIATES`** (`C22-27`) | Replaced by `IntInV`/`IntInEdge` perturbation in Tier 1/2 (and axis-retry in Tier 3 `:898-901`). |
| `YC-20` | `compute_global_max + 1.0` vs C++ `octree.bbox.max + 0.5` | `PERF-DRIFT` | `inside_out.rs:193-203` | **`PARTIAL`** (`C22-18`) | Uses `+ 0.5` matching C++, which reveals scale bug `UL-05` when multiplied `max_c >= 2^52`. |
| `YC-21` | `addDuplicateTrisInfoInStructures` not called | `DEVIATES` | `arrangements/soup.rs:1089-1109` | **`PARTIAL`** (`C22-10`) | Fixed: step 11 restores `dupl_triangles` with `w`-corrected winding into `in_tris`/`in_labels` (`coplanar_dupl_restore.rs`). |
| `YC-22` | `intersects_box` octree query absent | `PERF-DRIFT` | `labeling/octree.rs:76-101` | **`PARTIAL`** (`C22-23`, `C22-24`) | Fixed: `TriOctree::intersects_ray` queries `AabbTree` with tight ray box + `tri_in_ray_aabb`. |
| `YC-23` | Duplicate of `YC-14` (`first-on-each-label` semantics) | `DEVIATES` | `inside_out.rs:1273-1293` | **`FAITHFUL`** (`C22-17`) | Fixed alongside `YC-14`. |
| `YC-24` | `D-13` per-triangle `LocalMesh` marked out of scope | `OUT-OF-SCOPE` | `arrangements/soup.rs:897-972` | **`FAITHFUL`** (`C22-11`) | Per-base-triangle `FastTrimesh` + exact `[RBig; 3]` global weld (`soup.rs:1139-1153`) is verified conforming in `cherchi-rs`. |

### 5.2 Disposition of `docs/audits/cherchi_port_audit.md` (42 Legacy Items: `A-01..A-06`, `B-01..B-14`, `C-01..C-13`, `D-02..D-13`)

*(Note: original numbering in `cherchi_port_audit.md` skipped `C-04`, `C-12`, `D-01`, and folded `D-03` into `A-03`.)*

| Prior ID | Legacy Issue in Deleted `crates/kernel/src/boolean/cherchi/*` | Prior Severity | Current `cherchi-rs` Status & Site (`b4bee54e`) |
|---|---|---|---|
| `A-01` | `points_are_collinear_3d` used inexact `f64` cross-product | `CORRECTNESS-BUG` | **Partially fixed (`PARTIAL`, `UL-04`):** `predicates/collinearity.rs:20-35` now uses Shewchuk `geometry_predicates::orient2d` on XY/YZ/ZX (fixing `O(1)` rounding), **but** omits `N24` `RBig` zero-certification on subnormal underflow! |
| `A-02` | `max_component_in_triangle_normal` used uncertified `f64` | `CORRECTNESS-BUG` | **Partially fixed (`PARTIAL`, `UL-04`):** `predicates/orientation.rs:55-120` uses a Shewchuk-filtered `f64` bound + exact `RBig` fallback, **except** `eps = 4*EPSILON*max_var^2` underflows to `0.0` at `max_var <= 1e-162`. |
| `A-03` (`D-03`) | 5 jolly points appended eagerly at soup init instead of tail | `UNKNOWN` | **Fixed (`FAITHFUL`):** `arrangements/soup.rs:1085-1087` appends `jolly_points(m)` at step 10 (the tail of `globals.verts`), matching C++. |
| `A-04` (`D-04`) | Custom `Orientation` cosurface tracking replaced `dupl_triangles` | `DELIB-DIV` | **Superseded (`PARTIAL`, `C22-10`):** `soup.rs:340-415, 1089-1109` ports C++ `DuplTriInfo` + step-11 shell restoration; `yang-rs/src/boolean.rs:2273-2363` handles solid `opposite` membrane semantics. |
| `A-05` | `compute_multiplier` silently fixed C++ UB | `PERF/CLEANUP` | **Open bug (`PARTIAL`, `UL-05`):** `processing/multiplier.rs:33-48` returns `>= 1.0` for `max_abs >= 1.0` (squaring large coords and breaking `+ 0.5`/`- 0.1` offsets at `max_abs >= 2^26`) and `1.0` for `max_abs < 1.0`. |
| `A-06` | `compute_approximate_coordinates` off by 5 jolly slots | `BUG` | **Eliminated:** `cherchi-rs` stores `approx` inline on `IntersectionVertex::Lpi` / `FastTrimesh` vertices and appends jolly points at step 10 (`soup.rs:1086`). |
| `B-01` | $O(n^2)$ broad-phase in `detect_intersections` | `PERF-DRIFT` | **Fixed (`FAITHFUL` / `N95`):** `arrangements/intersection_detection.rs:85-104` uses `AabbTree` ($O(n \log n + k)$). |
| `B-02` | Yang §4.2.2 Gauss-map filter mixed into arrangement | `DELIB-DIV` | **Eliminated (`FAITHFUL`):** `cherchi-rs` is a pure exact mesh arrangement with no approximate Gauss-map pre-skip. |
| `B-03` | Missing `SIMPLICIAL_COMPLEX` adjacency skip in `triangles_intersect` | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `intersection_detection.rs:110-125` skips shared-vertex/shared-edge pairs per C++ soup rules or routes through exact `classify_intersections` (`intersection_points.rs`). |
| `B-04` | `check_single_no_coplanar_edge_intersection` missing guard | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `intersection_points.rs` + `aux_structure.rs` geometric `[RBig; 3]` interner (`N18`) deduplicate shared endpoints cleanly. |
| `B-05` | `SegmentIntersection` collapsed 4 states into 2 | `BUG` | **Fixed (`FAITHFUL`):** `predicates/segment_triangle.rs:15-50` & `indirect/mod.rs` preserve exact interior vs endpoint vs disjoint states. |
| `B-06` | `finalize_intersection` soft-asserts masked >2 endpoints | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`):** `soup.rs:834-838` surfaces `ConstraintSegmentError::TooManyGeometricEndpoints` loudly as `Err(ArrangementError::TransversalEndpointOvercount)`. |
| `B-07` | `point_in_triangle_3d_classify` checked dominant axis only | `BUG` | **Fixed (`PARTIAL`):** `point_in_triangle.rs:45-90` checks all 3 projections XY/YZ/ZX (subject to `UL-04` subnormal `orient2d` bypass). |
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
| `D-05` | Parity counting instead of signed-volume `orient3d(t, p_inf)` | `CRITICAL-BUG` | **Fixed (`FAITHFUL`, `C22-17`):** `inside_out.rs:1273-1293` uses exact first-hit `orient3d(tv[0], tv[1], tv[2], ray.v1)`. |
| `D-06` | Centroid-only emanating point (no 3-tier `findRayEndpoints`) | `UNKNOWN` | **Fixed (`C22-19..21`):** All 3 tiers implemented in `inside_out.rs` (with `UL-05`/`UL-06` gaps noted). |
| `D-07` | Hoffmann normal perturbation instead of `nextafter` on $p_\infty$ | `CORRECTNESS-BUG` | **Replaced (`DEVIATES`, `C22-27`):** `next_up`/`next_down` implemented, with newly found `UL-01` & `UL-02` bugs. |
| `D-08` | Same-label skip implicit via API separation | `MINOR` | **Explicitly ported (`PARTIAL`, `C22-16`):** `inside_out.rs:1304-1308` checks `patch_surface_label` explicitly (exposing `UL-03` on mixed patches). |
| `D-09` | `slab_eps` expansion on ray AABB | `PERF-DRIFT` | **Fixed (`PARTIAL`, `C22-24`):** Tight ray AABB in `labeling/octree.rs:76-118`. |
| `D-10` | `weld_mesh_vertices` nanometer quantization | `CORRECTNESS-BUG` | **Fixed (`FAITHFUL`, `C22-10`):** Bit-exact vertex merge + exact `[RBig; 3]` implicit point interning. |
| `D-11` | `flood_fill_patches` intersection-edge barriers | `UNKNOWN` | **Fixed (`DEVIATES`, `C22-08`):** `labeling/patches.rs` floods manifold edges; `yang-rs` Stage-5 `flood_fill_patches` operates downstream on attributed B-Rep patches. |
| `D-12` | Interior vertex (`vertInfo`) seed preference missing | `MISSING` | **Fixed (`DEVIATES`, `C22-19`):** `Patches::border_verts` + Tier-1 explicit interior vertex scan implemented. |
| `D-13` | Per-triangle `LocalMesh` vs C++ global `FastTrimesh` | `ARCHITECTURAL` | **Verified (`FAITHFUL`, `C22-11`):** Per-triangle `FastTrimesh` + exact `GlobalVerts` weld (`soup.rs:897-1056`) achieves full sidecar parity. |

### 5.3 Stale Live Documentation, Ledger Entries & Comments Found at `b4bee54e`

1. **`docs/yang_deviations.md:6726-6729` (N89 item 8) & `docs/audits/yang_2025_audit.md:230-231, 486, 523`:** Still state that `cherchi-rs` `intersection_detection.rs:77` is an **$O(n^2)$ pairwise scan with no spatial index**, directly contradicted by **N95** (`docs/yang_deviations.md:6916-6981`, marked `RESOLVED (2026-10-17)`) and `crates/cherchi-rs/src/arrangements/intersection_detection.rs:85-104` (`AabbTree` broad-phase).
2. **`docs/yang_deviations.md:171, 354-357` (N69 summary/open-items):** Still lists the `full_corpus_categorized` confirmation run and deletion of `CHERCHI_GRAZE_AWARE_RAY` (`inside_out.rs:434-456`) as owed.
3. **`docs/yang_functional_roadmap.md:3931-3941`:** Still states that `LabeledArrangement` drops input-triangle provenance (`"the interface carries Mesh + surface: Vec<Vec<InputId>>, not input triangle indices"`), superseded by `LabeledArrangement::source: Vec<Vec<(InputId, u32)>>` (`crates/cherchi-rs/src/labeled_arrangement.rs:52-66`) and `yang-rs/src/boolean.rs:2970-3042`.
4. **`docs/kernel_probe_registry.md:76-80`:** Claims all `cherchi-rs` `CHERCHI_*` env vars are print-only tracing probes, missing the two release-active output-changing switches `CHERCHI_GRAZE_AWARE_RAY` (`inside_out.rs:441`) and `CHERCHI_SCE_SHORTCIRCUIT` (`triangle_intersect.rs:40`).
5. **`crates/cherchi-rs/src/boolean.rs:6`:** Module doc comment still describes `NativeBoolean` as `"(future, M3+)"`, although `NativeBoolean` (`labeling/native.rs:527-574`) is the live production backend.
6. **`crates/cherchi-rs/src/arrangements/coplanar_propagate.rs:12-16, 69`:** Module doc comment still claims *"nothing in `super::soup::mesh_arrangement` constructs or consumes it ... PR-4 will wire {bucket → propagate → pocket emit} together"*, whereas PR-4 wired `coplanar_propagate` into `soup.rs:846-855` via `integrate_coplanar_into_arrangement`.
7. **`crates/cherchi-rs/src/arrangements/enforce.rs:7-19` & `crates/cherchi-rs/src/arrangements/soup.rs:1157-1162`:** Doc comments still say *"No production code is authored here ... FAILS TO COMPILE"* from their initial TDD RED commits.

---

## 6. Unledgered Divergences (Ranked by Severity)

| Rank & ID | Severity | Failure Mode | Code Site (`b4bee54e`) | Violated Paper Statement | Summary of Divergence |
|---|---|---|---|---|---|
| **`#1` — `UL-01`** | **CRITICAL** | **Silent wrong boolean output** (drops real shell crossing → enclosed patch labeled `outside`, unrefuted by `InnerLabelOutsideInputBounds`) | `crates/cherchi-rs/src/labeling/inside_out.rs:1077-1083, 1147-1150` | Cherchi 2022 §5.2–§5.3 (`:447-453 R, 485-500 L`) | `check_intersection_inside_triangle_3d` tests the infinite line ($\lambda \in \mathbb{R}$), while `perturb_ray_and_find_inters_tri` checks `if !hits.is_empty()` **before** `sort_hits_along_ray` strips $\lambda \le 0$ (`before_origin`). A $\lambda < 0$ 1-ring pierce paired with a $\lambda > 0$ edge graze at `offset = 0` makes `hits` non-empty and `sorted` empty, immediately returning `None` at `offset = 0` without trying `offset = 1..7`. |
| **`#2` — `UL-02`** | **CRITICAL** | **Silent wrong boolean output** (transversal crossing through 8-valent cone/cylinder apex skipped as "tangential") | `crates/cherchi-rs/src/labeling/inside_out.rs:1087-1152, 1337-1342, 1367-1378` | Cherchi 2022 §5.3 (`:485-492 L`: *"the operation is repeated until a valid intersection point is found"*) | Replaces the paper's unbounded perturbation loop with 8 single-ULP offsets `(±1,0), (0,±1), (±1,±1)` and silently skips the intersection when all 8 return `None`. On a regular 8-sided cone/bipyramid centered at symmetric-ULP coords (e.g. `(1.5, 1.5)`), all 8 offsets hit spoke edges (`orient3d == Zero`), silently dropping a transversal shell crossing. |
| **`#3` — `UL-03`** | **HIGH** | **Silent wrong boolean output** on mixed `[A,B] + [B]` patches (operand-order asymmetric!) | `crates/cherchi-rs/src/labeling/patches.rs:126-130, 198` + `crates/cherchi-rs/src/labeling/inside_out.rs:226, 519-523, 852-854, 1304-1309` | Cherchi 2022 §5 Alg. 1 (`:339-350 R, 386-395`) | `N23` (`L2a`) merges `[A, B]` coplanar triangles and manifold-adjacent single-owner `[B]` triangles into one patch, and `inside_out.rs:226` sets `patch_surface_label = &soup.labels[patch[0]]`. Because `[A, B]` triangles emitted during $A$'s loop have lower `out_tris` indices than `[B]` triangles emitted during $B$'s loop, `patch[0]` has label `[A, B]`, causing `compute_inside_out` to skip **both** $A$'s and $B$'s shells and mislabel `[B]` as outside $A$. |
| **`#4` — `UL-04`** | **HIGH** | **Silent deletion of valid tiny triangles / false `OnEdge` ray hits** at subnormal scales | `crates/cherchi-rs/src/predicates/collinearity.rs:30-33`; `point_in_triangle.rs:97, 141-143`; `simplex_location.rs:90`; `orientation.rs:80-109` | Cherchi 2022 §2.1 & §5.2 (`:140-150 L, 442-446 R`) + Ledger `N24` | Three predicate modules call raw `geometry_predicates::orient2d(...) == 0.0` directly, bypassing `orient::orient2d`'s `N24` rational zero-certification, and `max_component_filtered` underflows `eps` to `0.0` at subnormal scales (`~1e-162`), causing `remove_degenerate_and_duplicated_triangles` (`soup.rs:372`) to delete valid triangles and `fast_2d_check_intersection_on_ray` to misclassify `Outside` as `OnEdge`. |
| **`#5` — `UL-05`** | **HIGH** | **False `Err(DegenerateOrientation)` & Tier-2 ray failure** for unscaled coordinates `>= 2^26` (`~6.7e7`) | `crates/cherchi-rs/src/processing/multiplier.rs:33-48` + `crates/cherchi-rs/src/labeling/inside_out.rs:193-203, 658` | Cherchi 2022 §5.1 (`:422-426 L, 453-455 L`) | `compute_multiplier` multiplies coordinates with `max_abs = M >= 1` by `2^ceil(log2(M)) >= M`, inflating multiplied coords to `~M^2`. Once unscaled `max_abs >= 2^26`, multiplied `max_c >= 2^52` (`ulp >= 1.0`), so `max_c + 0.5 == max_c` ($p_\infty$ lands **on** the scene bbox instead of outside it!) and `v0c -= 0.1` becomes a no-op (`v0` stays on `seed_tri`). |
| **`#6` — `UL-06`** | **MEDIUM** | **Silent wrong boolean output** on originless patches whose Tier-2 centroid ray grazes a foreign input | `crates/cherchi-rs/src/labeling/inside_out.rs:629-702` | Cherchi 2022 §5.1 & §5.3 (`:445-457 L, 480-492 L`) + Ledger `N69` | `try_patch_ray_from_verts` (Tier 1, `:596-604`) checks `ray_grazes_foreign_input` (`N69`), **but `try_patch_ray_from_tris` (Tier 2, `:629-702`) omits `ray_grazes_foreign_input`**, accepting the first synthetic centroid ray even when it grazes a foreign face/edge plane. |
| **`#7` — `UL-08`** | **MEDIUM (Blocking for F1 / `N80`)** | **Latent wrong membrane retention** once `F1-I2` moves `keep_set` after Stage 4 | `crates/yang-rs/src/brep.rs:406-468` vs `crates/yang-rs/src/boolean.rs:2273-2363` | Yang 2025 §4.4.2 & §4.5.5 (`yang2025_hybrid_boolean.txt:572-578, 696-706`); Cherchi 2022 §3 (`:243-245 R`) | `TriangleSideLabel` (`brep.rs:406-415`) omits Stage-0's `opposite` normal-agreement flag, and `TriangleSideLabel::keep_for_op` (`:454-467`) returns `true` on every `[true, true], [false, false]` coplanar sheet for `Union`, `Intersect`, and `Subtract`. Hidden today only because `boolean.rs:2361` pre-filters `!keep_sheet` before Stage 4. |
| **`#8` — `UL-07`** | **LOW (Governance)** | **Release-active result-changing env vars** violating F12 probe policy | `crates/cherchi-rs/src/labeling/inside_out.rs:440-446`; `crates/cherchi-rs/src/predicates/triangle_intersect.rs:35-44`; `docs/kernel_probe_registry.md:76-80` | Governance F12 (`docs/kernel_probe_registry.md:17-20, 91-93`); `specs/yang_faithful_cleanup.md` §4 | `CHERCHI_GRAZE_AWARE_RAY` and `CHERCHI_SCE_SHORTCIRCUIT` alter kernel output in `--release` builds without `cfg!(debug_assertions)` and are omitted from `docs/kernel_probe_registry.md`. |

---

## 7. `TEST CANDIDATES` (Concrete Specifications for Unpinned Rows & Divergences)

Because this session is strictly read-only (`NO cargo`), none of the specs below were executed; each includes the exact `needs measurement:` command to run once CPU booking permits.

### `TC-01` — Behind-Origin 1-Ring Pierce + Forward Edge Graze (`UL-01`, pins `C22-27` / `C22-28`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:**
  - **Solid `A` (`InputId(0)`):** Small closed outward-oriented tetrahedron strictly inside `B`, with interior vertex `v0 = (0.0, 1.0, 1.0)` (achieved by subdividing one face of `A` at `(0.0, 1.0, 1.0)` with corners `(-0.1, 0.9, 0.9)`, `(-0.1, 1.1, 0.9)`, `(-0.1, 1.0, 1.1)`, and apex `(-0.2, 1.0, 1.0)` so `(0.0, 1.0, 1.0)` is the earliest non-border explicit vertex of `A`'s patch).
  - **Solid `B` (`InputId(1)`):** Closed manifold polyhedron enclosing `A` (`[-2.0, 1.5] × [0.0, 2.0] × [0.0, 2.0]` deformed so `max_x = 2.0` ⇒ `ray.v1.x = 2.5`), with vertex `V = (0.5, 1.0, 1.0)` whose 1-ring includes:
    - Forward $+Y$ ridge edge `V = (0.5, 1.0, 1.0)` → `E_+ = (1.5, 2.0, 1.0)` in plane $z = 1.0$ shared by $\triangle(V, E_+, (1.5, 1.5, 2.0))$ and $\triangle(V, (1.5, 1.5, 0.0), E_+)$;
    - Backward $-Y$ face $\triangle(V, B_1, B_2)$ with `B_1 = (-2.0, 1.0 - 0.5 * f64::EPSILON, 0.0)` and `B_2 = (-2.0, 1.0 - 0.5 * f64::EPSILON, 2.0)` (closed cleanly around $x = -2.0$).
- **Expected behavior:** `BoolOp::Intersect` of `(A, B)` equals `A` (`inside[t_A] == [false, true]`); `BoolOp::Union` equals `B`.
- **Predicted current bug (`UL-01`):** At `offset = 0`, `p_ray` pierces $\triangle(V, B_1, B_2)$ behind `v0` ($x = -0.5 < 0$) and grazes $V E_+$ ahead of `v0` ($z = 1.0$); `perturb_ray_and_find_inters_tri` (`inside_out.rs:1147-1150`) sees `!hits.is_empty()`, gets `sorted = []`, returns `None` immediately, and mislabels `A` as `outside` `B` (`Intersect` returns empty mesh!).
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc01" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-02` — Enclosed Solid Inside Regular 8-Sided Bipyramid Apex (`UL-02`, pins `C22-27`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:**
  - **Solid `B` (`InputId(1)`):** Closed outward-oriented regular 8-sided bipyramid centered on `(y_c, z_c) = (1.5, 1.5)`:
    - Right apex `V_R = (1.0, 1.5, 1.5)`, left apex `V_L = (-2.0, 1.5, 1.5)`.
    - 8 equatorial ring vertices at `x = 0.0` with `(y, z)` = `(2.5, 1.5)`, `(2.5, 2.5)`, `(1.5, 2.5)`, `(0.5, 2.5)`, `(0.5, 1.5)`, `(0.5, 0.5)`, `(1.5, 0.5)`, `(2.5, 0.5)` (16 triangles total: 8 `[E_k, E_{k+1}, V_R]`, 8 `[E_{k+1}, E_k, V_L]`).
  - **Solid `A` (`InputId(0)`):** Small closed tetrahedron strictly inside `B` around `(-0.5, 1.5, 1.5)` whose lowest-index vertex is `(-0.5, 1.5, 1.5)` (disjoint from `B`, so all 4 vertices of `A` are interior non-border explicit vertices; place all 4 vertices on the axis `y = 1.5, z = 1.5`? No, 4 collinear vertices would be degenerate — place 1 vertex at `(-0.5, 1.5, 1.5)` as vertex 0, and the other 3 at `(-0.6, 1.6, 1.5)`, `(-0.6, 1.45, 1.58)`, `(-0.6, 1.45, 1.42)`: Tier 1 picks vertex 0 along `Axis::X` because `ray_grazes_foreign_input` is `false`!).
- **Expected behavior:** `A` is strictly inside `B` (`inside == [false, true]` on all 4 triangles of `A`; `NativeBoolean.boolean(&a, &b, BoolOp::Intersect)` has 4 triangles and positive volume equal to `vol(A)`).
- **Predicted current bug (`UL-02`):** All 8 single-ULP perturbations of `p_ray.v1 = (1.5, 1.5 ± 2^-52, 1.5 ± 2^-52)` lie on the 8 spoke planes of `V_R`, returning `Sign::Zero`; `perturb_ray_and_find_inters_tri` returns `None`, dropping the crossing and returning **0 triangles** for `A ∩ B`!
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc02" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-03` — Operand-Swap `(B, A)` Asymmetry on Mixed-Label Coplanar Patch (`UL-03`, pins `C22-08` / `C22-16`)
- **Target file:** `crates/cherchi-rs/tests/r0046_patch_label_parity.rs` (plus unit fixture in `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`)
- **Geometry:**
  1. **Direct R0046 swap check:** Load `r0046_stage0_a.obj` and `r0046_stage0_b.obj` (from `r0046_patch_label_parity.rs:40`) and run `native_labeled_arrangement(&b, &a)` (operands swapped so the mixed patch's single-solid region belongs to `InputId(1)` while the coplanar `[0, 1]` triangles take the lowest `out_tris` indices!). Compare `NativeBoolean.boolean(&b, &a, BoolOp::Union)` and `BoolOp::Intersect` against `NativeBoolean.boolean(&a, &b, BoolOp::Union)` / `BoolOp::Intersect` (union and intersection must be commutative in volume and Euler characteristic!).
  2. **Minimal synthetic `ArrangementSoup` unit test:** Construct a 2-patch `ArrangementSoup` where patch 0 has `tris[0]` with `labels[0] = vec![InputId(0), InputId(1)]` manifold-adjacent across a 2-incident edge to `tris[1..4]` with `labels = vec![InputId(1)]` lying strictly inside closed `in_tris` shell `InputId(0)`. Assert `compute_inside_out` marks `tris[1..4]` with `inside[t][0] == true`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc03" -- cargo test -p cherchi-rs --test r0046_patch_label_parity -- --ignored`

### `TC-04` — Subnormal Underflow Zero-Certification in `collinearity`, `point_in_triangle`, and `simplex_location` (`UL-04`, pins `C22-02` / `C22-10` / `C22-22`)
- **Target file:** `crates/cherchi-rs/src/predicates/orient.rs` (or per-module `tests` in `collinearity.rs`, `point_in_triangle.rs`, `simplex_location.rs`)
- **Geometry:**
  - `a = Point3::new(0.0, 0.0, 0.0)`, `b = Point3::new(1e-162, 0.0, 0.0)`, `c = Point3::new(0.0, 1e-162, 0.0)` (exact XY area $= \frac{1}{2}\cdot 10^{-324} > 0$ in $\mathbb{Q}$, non-collinear!).
  - Query point `p_in = Point3::new(0.25e-162, 0.25e-162, 0.0)` (strictly inside) and `p_out = Point3::new(-0.25e-162, 0.25e-162, 0.0)` (strictly outside).
- **Expected behavior:**
  - `points_are_collinear_3d(a, b, c) == false`;
  - `point_in_triangle_3d_loc(p_in, a, b, c) == TriangleLocation::StrictlyInside`;
  - `point_in_triangle_3d_loc(p_out, a, b, c) == TriangleLocation::Outside`;
  - Two overlapping closed boxes scaled by `1e-162` (`[0, 2e-162]^3` and `[1e-162, 3e-162]^3`) retain all 12 faces per box through `remove_degenerate_and_duplicated_triangles` and succeed in `NativeBoolean::boolean`.
- **Predicted current bug (`UL-04`):** `geometry_predicates::orient2d` underflows `(1e-162)^2` to `0.0`, so `points_are_collinear_3d` returns `true` and `remove_degenerate_and_duplicated_triangles` **deletes all 24 triangles**, returning an empty arrangement!
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc04" -- cargo test -p cherchi-rs predicates`

### `TC-05` — Large-Coordinate Scene (`max_abs = 1e8 > 2^26`) Ray Offset Collapse (`UL-05`, pins `C22-04` / `C22-18` / `C22-20`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:**
  - **Solid `A`:** Axis-aligned cube `[0.0, 1.0e8]^3`.
  - **Solid `B`:** Axis-aligned cube `[0.5e8, 1.5e8]^3` (its $+X$ face lies at $x = 1.5 \times 10^8$).
- **Expected behavior:** `NativeBoolean.boolean(&a, &b, BoolOp::Union)` succeeds with signed volume $1.875 \times 10^{24}$.
- **Predicted current bug (`UL-05`):** `compute_multiplier` (`multiplier.rs:33-48`) computes `m = 2^28 = 268_435_456.0`, scaling $x_{\max} = 1.5\times 10^8$ to $4.0265\times 10^{16} > 2^{55}$, where `ulp = 8.0 >> 0.5`. `bbox_max_plus_half` computes `max_c[0] + 0.5 == max_c[0]`, placing `ray.v1` **on** $B$'s $+X$ face; when `A`'s interior patch casts a $+X$ ray through $B$'s $+X$ face (or when `B` encloses `A` up to `x_max`), `analyze_sorted_intersections` evaluates `orient3d(tv, ray.v1) == Sign::Zero` and fails with `Err(InsideOutError::DegenerateOrientation)`!
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc05" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-06` — Originless Band Patch Whose Tier-2 Centroid Ray Grazes a Foreign Input (`UL-06`, pins `C22-20` / `C22-29`)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:**
  - **Solid `A`:** Thin rectangular prism `[-0.5, 0.5] × [-1.0, 1.0] × [-1.0, 1.0]` sliced by **two** disjoint slabs `B_top` (`[-2, 2] × [-2, 2] × [0.5, 2.0]`) and `B_bot` (`[-2, 2] × [-2, 2] × [-2.0, -0.5]`) joined outside `A` into a single C-shaped solid `B` whose inner vertical wall has a horizontal concave step at `z = 0.0` ahead of `A` (`x ∈ [1.0, 2.0]`).
  - Because `A`'s middle band `z ∈ [-0.5, 0.5]` has all vertices on the `z = ±0.5` intersection rings (`border_verts`), `A`'s band patch has **zero** interior explicit vertices → enters **Tier 2** (`try_patch_ray_from_tris`). Arrange one symmetric quad diagonal on `A`'s `-X` face so the first band triangle's centroid has `z = 0.0`, causing its `+X` synthetic ray to graze `B`'s concave horizontal face at `z = 0.0`!
- **Expected behavior:** Tier 2 skips the grazing `+X` ray via `ray_grazes_foreign_input` and classifies cleanly on `+Y`.
- **Predicted current bug (`UL-06`):** `try_patch_ray_from_tris` (`inside_out.rs:682-699`) omits `ray_grazes_foreign_input`, accepts the `+X` grazing ray, and triggers `N69` perturbation into `B`'s concave step (moreover, because `A`'s band IS inside `B`'s C-shaped AABB, `refute_inner_label_by_input_bounds` does NOT catch the false `inside` label!).
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc06" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-07` — Tier-1 All-Candidates-Graze Inside Non-Convex Foreign AABB (`C22-29` residual `N69` gap)
- **Target file:** `crates/cherchi-rs/src/labeling/inside_out_adversary_tests.rs`
- **Geometry:** Same C-shaped solid `B` (`AABB = [-2, 2]^3`) with a small cube `A` inside the open mouth of the `C` (strictly outside `B`, **inside** `AABB(B)`) whose single interior explicit vertex on a patch aligns on `X`, `Y`, and `Z` with three exterior tangent shelves of `B`.
- **Expected behavior:** Rather than falling back to `first_grazing` (`inside_out.rs:622`), escalate to Tier-3 `rational_ray_inner_label` (whose triangle centroid `(c0+c1+c2)/3` is off the integer shelf planes!) and classify `outside` `B`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc07" -- cargo test -p cherchi-rs inside_out_adversary`

### `TC-08` — `TriangleSideLabel::keep_for_op` on Stacked (`opposite = true`) & Flush (`opposite = false`) Coplanar Sheets (`UL-08`, pins `YC22-04` for Milestone `F1`)
- **Target file:** `crates/yang-rs/src/tests_unit/s442_side_labels.rs`
- **Geometry:**
  1. **Stacked cubes (`opposite = true`):** `A = [0, 1]^3`, `B = [0, 1]^2 × [1, 2]` sharing `z = 1`. Construct `TriangleSideLabel` for the shared `z = 1` arrangement triangles (`surface = [true, true], inside = [false, false]`) **without** pre-stripping `!keep_sheet`.
  2. **Flush pocket (`opposite = false`):** `A = [0, 4]^3`, `B = [1, 3]^2 × [2, 4]` sharing `[1, 3]^2 × {4}`.
- **Expected behavior (required before `F1-I2` lands):** Post-Stage-4 `keep_for_op` drops the `z = 1` stacked sheet on `BoolOp::Union` and `BoolOp::Intersect`, keeps it on `BoolOp::Subtract`, and drops the `z = 4` flush pocket cap on `BoolOp::Subtract`.
- **Predicted current bug (`UL-08`):** `TriangleSideLabel::keep_for_op` (`crates/yang-rs/src/brep.rs:454-467`) returns **`true` in all four cases** because `TriangleSideLabel` lacks `opposite`!
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc08" -- cargo test -p yang-rs s442_side_labels`

### `TC-09` — Variadic 3-Mesh End-to-End Arrangement + Ray-Cast + Keep Parity (`C22-05`, `C22-34`)
- **Target file:** `crates/cherchi-rs/src/labeling/adversary_tests.rs`
- **Geometry:** Base cube `A = [0, 4]^3` minus two non-intersecting decorative cubes `B_1 = [0.5, 1.5]^3` (enclosed) and `B_2 = [2.5, 3.5] × [2.5, 3.5] × [3.0, 5.0]` (carving top face), passed as a 3-solid soup (`labels = [0], [1], [2]`, `num_inputs = 3`) vs pairwise `A \ (B_1 ∪ B_2)` (mirroring Cherchi 2022 §6.4 Fig. 9, `:673-680 R`).
- **Expected behavior:** Identical triangle set, watertightness, and signed volume ($64 - 1 - 1 = 62$) on `mesh_arrangement` + `compute_all_patches` + `compute_inside_out(..., 3)`.
- **Verification command:** `needs measurement: ~/.claude/bin/reserve -c 4 -l "tc09" -- cargo test -p cherchi-rs labeling::adversary_tests`
