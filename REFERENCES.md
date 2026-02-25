# Waffle Iron — Reference Materials

Technical references for B-rep kernel development, boolean operations, and computational geometry.

## Primary References

### 1. Patrikalakis, Maekawa & Cho — "Shape Interrogation for Computer Aided Design and Manufacturing"

**Access**: Free HTML hyperbook at MIT:
https://web.mit.edu/hyperbook/Patrikalakis-Maekawa-Cho/

**Key chapters for our work**:
- **Chapter 5: Intersection Problems** — Surface-surface intersection algorithms (lattice, marching, subdivision methods). This is our #1 reference for improving truck's intersection curve computation. Covers all cases: parametric/parametric, parametric/implicit, implicit/implicit.
- **Chapter 6: Differential Geometry of Intersection Curves** — Properties of curves formed by intersecting surfaces. Needed for accurate IC edge representation (our NURBS arc healing work extends from this).
- **Chapter 4: Nonlinear Polynomial Solvers and Robustness Issues** — Numerical robustness for geometric computation, interval arithmetic, the Projected Polyhedron algorithm.
- **Chapter 11: Offset Curves and Surfaces** — Relevant if we ever revisit shell operations.

**How to use**: `WebFetch` works directly on the HTML pages. The chapter/section URLs follow the pattern `https://web.mit.edu/hyperbook/Patrikalakis-Maekawa-Cho/nodeNNN.html`. Navigate from the table of contents to find specific sections.

### 2. Hoffmann — "Geometric and Solid Modeling: An Introduction"

**Access**: PDF at http://lib.ysu.am/open_books/416499.pdf (SSL cert expired; use `curl -k` to download, then `pdftotext` to extract). The Read tool can read the PDF if `poppler-utils` is installed (`sudo apt-get install poppler-utils`).

**Key chapters for our work**:
- **Chapter 3: Boolean Operations on Boundary Representation** (pp. 67-109) — The complete algorithmic pipeline for B-rep booleans: shell intersection, face classification via neighborhood analysis, face subdivision, adjacency computation, and result assembly. This is the architectural blueprint for redesigning truck's boolean engine.
  - 3.3: Geometric operations (face direction, splitting, line/solid classification)
  - 3.4: Intersection of two shells (face intersection, neighborhood analysis, face subdivision)
  - 3.5: Multishell objects
  - 3.6: Complement, union, and difference
  - 3.7: Face-boxing / spatial indexing techniques
- **Chapter 2: Basic Concepts** (pp. 13-65) — B-rep foundations: Euler operators, winged-edge representation, topological validity, manifold vs nonmanifold. Useful reference for understanding truck's topology layer.
- **Chapter 4: Robust and Error-Free Geometric Operations** (pp. 111-153) — Floating-point arithmetic pitfalls, geometric failures, conditioning. Directly relevant to our tolerance management.
- **Chapter 5: Representation of Curved Edges and Faces** (pp. 155+) — Parametric curves/surfaces, NURBS, trimmed patches.

**How to use**: Download the PDF once, extract text with `pdftotext`. For targeted reading, use page ranges: `pdftotext input.pdf output.txt -f 67 -l 110` for Chapter 3.

### 3. OpenCascade Boolean Operations Documentation

**Access**: Free online documentation:
https://dev.opencascade.org/doc/overview/html/specification__boolean_operations.html

**Source code**: https://github.com/Open-Cascade-SAS/OCCT (LGPL-2.1)

**Why this is critical**: OCCT's General Fuse Algorithm (GFA) is the most mature open-source B-rep boolean implementation. It handles ALL the degenerate configurations that cause our perturbation cascade to trigger — coplanar faces, coincident edges, vertex-on-face, tangential contact. The source code is available to study.

**Key concepts for our work**:
- **General Fuse Algorithm (GFA)**: Operates on n arguments simultaneously, producing a complete decomposition of space. Boolean operations (union, intersection, difference) are extractions from the GFA result. `RGF = Sp1 + Sp2 + Sp12` for two arguments.
- **Staged Interference Computation**: Processes interferences in strict order: V/V → V/E → E/E → V/F → E/F → F/F → Solid. Each interference type is handled with appropriate algorithms. This prevents the "surface-surface intersection through an existing vertex" problem (our D3 root cause).
- **Pave Blocks**: Edge segments between neighboring intersection points (paves). Central data structure for tracking how edges get subdivided by intersections. Similar to our IC vertex insertion but more principled.
- **FaceInfo / In-On-Sc States**: Classifies pave blocks and vertices as geometrically inside a face (In), lying on its boundary (On), or created from intersection curves (Sc). This three-state model is more nuanced than our binary And/Or classification.
- **Same-Domain Analysis**: Detects coplanar face pairs and computes connexity chains. This is OCCT's solution to our D1 (coplanar faces) problem.
- **Fuzzy Boolean Operations**: `SetFuzzyValue()` expands tolerance spheres, allowing nearby-but-not-touching geometry to be treated as intersecting. More principled than our perturbation cascade.
- **Shrunk Ranges**: Narrowed parameter intervals on edges/faces to prevent intersection artifacts at boundaries.

**How to use**: Read the documentation page for the algorithmic overview. For implementation details, study the OCCT source code — the key classes are `BOPAlgo_PaveFiller` (intersection part), `BOPAlgo_Builder` (building part), `BOPDS_DS` (data structure), and `BOPDS_FaceInfo`.

### 4. Shewchuk — "Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates" (1997)

**Access**: Free PDF:
https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf

**Rust implementation**: `robust` crate (https://github.com/georust/robust) — already used in our `robust_classify.rs`.

**Relevance**: The foundation for exact geometric predicates (orient2d, orient3d, incircle) using adaptive-precision arithmetic. Only does as much extra work as needed — fast for easy cases, exact for degenerate cases. We already use this via the `robust` crate, but only for ray-triangle tests. Should be extended to coplanar detection, edge-edge intersection, and all load-bearing geometric decisions.

### 5. Edelsbrunner & Mucke — "Simulation of Simplicity" (1990)

**Access**: Free PDF:
https://arxiv.org/abs/math/9410209

Also at: https://www.sandia.gov/files/samitch/unm_math_579/p66_edelsbrunner_simulation_of_simplicity.pdf

**Citation**: Edelsbrunner, H. and Mucke, E.P. "Simulation of simplicity: a technique to cope with degenerate cases in geometric algorithms." ACM Transactions on Graphics 9(1):66–104, 1990.

**Relevance**: General-purpose technique for handling degenerate input. Adds symbolic infinitesimal perturbations to input data so that no predicate ever evaluates to zero — algorithms can pretend degeneracies never occur. Unlike our perturbation cascade (which modifies actual geometry), SoS perturbations are virtual and never applied to the data. We have partial SoS in `robust_classify.rs` (`sos_orient2d_tiebreak`); full SoS would cover orient3d, incircle, and all geometric decisions, eliminating degenerate-case special handling entirely.

### 6. Sugihara & Iri — "Topology-Oriented Implementation — An Approach to Robust Geometric Algorithms" (2000)

**Access**: Springer (may need institutional access):
https://link.springer.com/article/10.1007/s004530010002

**Citation**: Sugihara, K., Iri, M., Inagaki, H., Imai, T. Algorithmica 27:5–20, 2000.

**Relevance**: The philosophical framework for replacing our perturbation cascade. Key insight: describe the algorithm in terms of *topological* operations first (combinatorial computation that is never contaminated by numerical errors), then use numerical computation only to choose among topologically valid branches. The resulting software is "completely robust in the sense that no matter how large numerical errors arise, the algorithm never fails." Our wire splitting, biangle filtering, and shell closure recovery are ad-hoc versions of what this paper systematizes. Applying this framework would mean: maintain manifoldness at every step, use geometry only to select between topologically consistent alternatives.

## Mesh Boolean References (Exact Computation Approaches)

These references operate on triangle meshes rather than NURBS B-rep, but their classification and robustness techniques are directly applicable to our ray-cast classification and shell assembly stages.

### 7. Jacobson, Kavan & Sorkine-Hornung — "Robust Inside-Outside Segmentation Using Generalized Winding Numbers" (2013)

**Access**: Free PDF:
https://igl.ethz.ch/projects/winding-number/robust-inside-outside-segmentation-using-generalized-winding-numbers-siggraph-2013-jacobson-et-al.pdf

**Citation**: ACM Transactions on Graphics 32(4):33:1–33:12, 2013 (SIGGRAPH).

**Relevance**: Generalized winding numbers provide robust inside/outside classification that works even for non-manifold, self-intersecting, and open meshes. The winding number is a sum over faces giving a continuous function: 1 inside, 0 outside, degrading gracefully near boundaries. Would replace our 8-ray majority-voting classifier, eliminating ray-edge grazing issues, edge-neighbor propagation fallback, and Unknown face classification entirely.

### 8. Zhou, Grinspun, Zorin & Jacobson — "Mesh Arrangements for Solid Geometry" (2016)

**Access**: Free PDF:
https://cims.nyu.edu/gcl/papers/zhou2016mas.pdf

**Project page**: https://www.cs.columbia.edu/cg/mesh-arrangements/

**Citation**: ACM Transactions on Graphics 35(4), 2016 (SIGGRAPH).

**Relevance**: Complete boolean pipeline using mesh arrangements + winding number vectors. Two stages: (1) resolve all intersections to create an arrangement (space partition into cells), (2) assign winding number vectors to each cell and extract based on boolean operation. Makes NO general position assumptions, does NOT use perturbation, is variadic (n-ary). Tested on 10,000+ real-world meshes. Used as the basis for Blender's boolean redesign. Our pipeline has equivalent stages (`create_loops_stores` ≈ arrangement, `classify_faces` ≈ winding number extraction) but uses fragile heuristics where they use exact computation.

### 9. Cherchi, Livesu, Scateni & Attene — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" (2020)

**Access**: Free PDF:
https://www.gianmarcocherchi.com/pdf/mesh_arrangement.pdf

**Code**: https://github.com/gcherchi/FastAndRobustMeshArrangements (header-only C++)

**Citation**: ACM TOG 39(6), 2020 (SIGGRAPH Asia).

**Relevance**: Indirect predicates — exact geometric tests using only floating-point hardware. As fast as non-robust float implementations but provably correct. Practical realization of Shewchuk's vision applied to mesh arrangements.

### 10. Cherchi, Livesu, Scateni & Attene — "Exact Predicates, Exact Constructions and Combinatorics for Mesh CSG" (2025)

**Access**: https://dl.acm.org/doi/10.1145/3744642

**Citation**: ACM Transactions on Graphics, 2025.

**Relevance**: State-of-the-art. Computes the full Weiler model using exact arithmetic, with radial sort for non-manifold intersection edges using specialized exact predicates. Proposes two geometric kernels (arithmetic expansions and multi-precision floating-point). Directly relevant to our edge welding (radial sort replaces our tolerance-based weld_coincident_edges) and shell assembly problems.

### 11. Trettner, Nehring-Wirxel & Kobbelt — "EMBER: Exact Mesh Booleans via Efficient & Robust Local Arrangements" (2022)

**Access**: Free PDF:
https://www.graphics.rwth-aachen.de/media/papers/339/ember_exact_mesh_booleans_via_efficient_and_robust_local_arrangements.pdf

**Citation**: ACM TOG 41(4):39, 2022 (SIGGRAPH).

**Relevance**: Plane-based representation + homogeneous integer coordinates for exactness. Adaptive recursive subdivision instead of global acceleration structure. Shows exact booleans can avoid building a complete global arrangement (early-out termination).

### 12. Barki, Liris et al. — "Exact, Robust, and Efficient Regularized Booleans on General 3D Meshes" (2015)

**Access**: Free PDF:
https://hal.science/hal-01203173/

**Citation**: Computers & Mathematics with Applications, 2015.

**Relevance**: Efficient exact booleans without the overhead of Nef polyhedra. Uses co-refinement + classification. 3x faster than Maya, 5x faster than CGAL Nef. Could inform a mesh-level boolean fallback path.

## Curved B-rep Boolean References

### 13. ESOLID — Keyser, Culver, Manocha et al.

**Access**: Free PDF:
http://gamma.cs.unc.edu/ESOLID/keyser02.pdf

**Project page**: http://www.cs.unc.edu/~geom/ESOLID/

**Citation**: Keyser, J., Culver, T., Foskey, M., Krishnan, S., Manocha, D. "ESOLID—A System for Exact Boundary Evaluation." Computer-Aided Design 36(2):175–193, 2004.

**Relevance**: Exact boolean operations on LOW-DEGREE CURVED solids (quadrics, tori, cylinders — the primitives in mechanical CAD). Uses lazy evaluation, floating-point filters, and arbitrary-precision arithmetic. The lazy evaluation strategy (try cheap float first, escalate to exact only when needed) is the correct alternative to our perturbation cascade. This is the only reference that does exact booleans on curved B-rep (not tessellated meshes).

### 14. Barton, Hanniel, Elber & Zayer — "Accelerated Robust Boolean Operations Based on Hybrid Representations" (2018)

**Access**: https://www.sciencedirect.com/science/article/abs/pii/S0167839618300359

**Citation**: Computer Aided Geometric Design 64:36–49, 2018.

**Relevance**: Hybrid approach combining NURBS (for precision) with mesh (for robustness). Uses exact predicates on mesh representation to guide decisions on the NURBS representation. This is the architecture truck should aspire to — maintain exact topological decisions while using NURBS geometry.

## Foundational References

### 15. Granados, Hachenberger, Kettner, Mehlhorn et al. — "Boolean Operations on 3D Selective Nef Complexes" (2003/2007)

**Access**: Free PDF (Hachenberger PhD thesis):
https://publikationen.sulb.uni-saarland.de/bitstream/20.500.11880/25961/1/Dissertation_1778_Hach_Pete_2006.pdf

**CGAL docs**: https://doc.cgal.org/latest/Nef_3/index.html

**Citation**: Computational Geometry 38(1-2):64–99, 2007.

**Relevance**: Nef polyhedra (closure of half-spaces under boolean ops) handle non-manifold, open boundaries, and mixed-dimensional complexes — exactly the cases where our `finalize_boolean_shell` struggles. Exact arithmetic throughout. The CGAL implementation is production-proven. Operates on polyhedral (flat face) geometry, not NURBS.

### 16. Mäntylä — "An Introduction to Solid Modeling" (1988)

**Access**: https://archive.org/details/introductiontoso0000mant (Internet Archive, may need borrow)

**Relevance**: Deep dive on Euler operators and half-edge data structures. Proves that Euler operators form a complete set of modeling primitives for manifold solids. Relevant if we restructure truck's topology layer. The Euler operator approach ensures that every topological operation preserves manifoldness — which is what our `finalize_boolean_shell` struggles with.

### 17. Requicha & Voelcker — "Boolean Operations in Solid Modeling" (1985)

**Citation**: Proceedings of the IEEE 73(1):30–44, 1985.

**Relevance**: The theoretical framework for regularized boolean operations on solids. Defines regularized union/intersection/difference as closure-of-interior operations, guaranteeing the result is always a regular closed set (no dangling faces/edges). This is the mathematical reason WHY boolean results should be manifold. When `finalize_boolean_shell` produces shells with singular vertices, it's because the regularization step is failing.

### 18. Bernstein & Fussell — "Fast, Exact, Linear Booleans" (2009)

**Access**: Free PDF:
http://www.gilbertbernstein.org/resources/booleans2009.pdf

**Citation**: Computer Graphics Forum 28(5):1269–1278, 2009 (SGP).

**Relevance**: BSP-tree based exact booleans using plane-based representations. Only 4 geometric predicates needed. 16-28x faster than CGAL Nef polyhedra. Relevant if we consider a BSP-based approach for the planar-face portions of our boolean pipeline.

## How to Reference During Development

When working on boolean reliability or kernel improvements:

1. **Start with Hoffmann Ch. 3** for the algorithmic framework — understand the overall pipeline before diving into specifics
2. **Study OpenCascade docs (#3)** for production-validated degenerate handling — the staged interference approach (VV→VE→EE→VF→EF→FF) and pave block data structures
3. **Use Patrikalakis Ch. 5** for intersection algorithm details — when you need to understand or improve how two surfaces find their intersection curve
4. **Use Patrikalakis Ch. 4** for robustness — when debugging numerical issues or tolerance problems
5. **Use Hoffmann Ch. 4** for error analysis — when trying to understand why geometric operations fail
6. **Use Jacobson (#7) + Zhou (#8)** for classification — winding numbers are the correct replacement for our ray-cast majority voting
7. **Use Sugihara & Iri (#6)** for architectural guidance — topology-first algorithm design eliminates the need for perturbation
8. **Use Edelsbrunner & Mucke (#5)** for degenerate handling — Simulation of Simplicity eliminates all degenerate-case special handling
9. **Use ESOLID (#13) or Barton (#14)** for curved surface intersection — lazy exact evaluation for NURBS booleans
