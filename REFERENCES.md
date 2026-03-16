# Waffle Iron — Reference Materials

Technical references for B-rep kernel development, boolean operations, and computational geometry.

## Keyword Index

Find references by topic. Numbers refer to reference entries below.

**Boolean operations (general)** → #2 Ch.3, #3, #17, #20, #24 (hybrid B-Rep/mesh), #33 §6.1 (stepwise with Euler ops)
**Boolean pipeline architecture** → #2 Ch.3, #3 (GFA), #8 (mesh arrangements), #17 (PADL), #24 (6-stage hybrid), #33 §6.1 (SSI→sew→merge→separate)
**BSP trees** → #11 (EMBER), #18 (Bernstein)
**B-rep foundations** → #2 Ch.2, #16 (Euler ops), #17 (set membership classification), #33 Ch.3 (datastructures/traversals), #33 Ch.4 (Euler ops spanning sets)
**CDT / constrained Delaunay** → #10 (optimized CDT), #12 (per-triangle CDT)
**Chained booleans** → #24 (bijective re-mapping preserves topology across chains)
**Classification (face/in-out)** → #7 (winding number), #8 (winding number vectors), #12 (radial sort), #17 (set membership), #20 (4-way/8-way), #30 (GWN on trimmed NURBS, no tessellation)
**Convergent modeling** → #36 (Parasolid v26+, mixed mesh+analytic bodies, boolean on faceted+parametric)
**Coplanar / overlap regions** → #3 (same-domain analysis), #8 (coplanar CDT clustering), #10 (coplanar-heavy perf), #11 (plane-based repr), #26 (overlap as 2D phenomenon, bilevel optimization), #33 §6.1 (coincident surface imprinting)
**Curve-face intersection classification** → #33 §6.1.1 (ENTERS/LEAVES/INOSCUL/OUTOSCUL/WIRE/ONEDGE, sequencing rules)
**CSG / constructive solid geometry** → #14 (hybrid CSG), #17 (CSG→BRep), #2 Ch.3
**Curvature (curves)** → #22 (finite total curvature, discrete/smooth bridge)
**Curved surface booleans** → #13 (ESOLID, exact on quadrics), #14 (hybrid NURBS/mesh), #24 (hybrid B-Rep/mesh, SIGGRAPH 2025), #28 (watertight NURBS reparameterization)
**Datastructure design (B-rep)** → #33 Ch.3 (entity hierarchy, traversals, Appendix A specs), #16 (Mäntylä)
**DCEL / half-edge** → #16 (Mäntylä), #20 (Tekla/DCEL hierarchy)
**Deflation / singularity regulation** → #29 (over-determined system transforms singular zeros to regular)
**Degeneracy handling** → #5 (SoS), #6 (topology-oriented), #8 (no general position), #12 (two-case reduction), #25 (characteristic points determine topology), #29 (deflation for tangent points/tiny loops)
**Delaunay complexes** → #23 Ch.III (alpha complexes)
**Dixon resultant / implicitization** → #25 (Dixon matrix for SSI topology), #27 (algebraic methods survey)
**Edge splitting / pave blocks** → #3 (OCCT pave blocks, shrunk ranges)
**Euler operators** → #2 Ch.2, #16 (completeness proof), #20 (MEV/MEF/MEKL), #33 Ch.4 (spanning sets, matrix decomposition, 99 operators)
**Exact arithmetic** → #4 (adaptive expansions), #9 (indirect predicates), #10 (exact constructions), #13 (lazy exact), #15 (Nef, exact throughout), #19 (filter thresholds)
**Floating-point robustness** → #1 Ch.4, #2 Ch.4, #4, #19 (filter failure probabilities)
**Homology / topological invariants** → #23 Ch.IV–V (Euler characteristic, Betti numbers)
**Hybrid B-Rep/mesh boolean** → #14 (Sheng 2018), #24 (Yang 2025, bijective mapping)
**Intersection curves** → #1 Ch.5–6, #3 (FF interference), #13 (algebraic curves on quadrics), #25 (topology-guaranteed tracing), #27 (comprehensive survey)
**Feature modelling** → #33 Ch.9 (facesets, frames, design-by-features, recognition, verification)
**Gluing / face joining** → #33 §6.4 (identical topology), #33 §6.13 (non-matching faces via local boolean)
**Manifoldness** → #6 (topology-first guarantees), #16 (Euler ops preserve), #17 (regularization)
**Mesh arrangements** → #8 (Zhou), #9 (Cherchi), #10 (Levy), #12 (Barki)
**Mesh booleans (exact)** → #8, #9, #10, #11, #12, #18
**Morse theory** → #23 Ch.VI
**Nef polyhedra** → #15 (CGAL Nef 3D, sphere maps, non-manifold)
**Knot insertion / refinement / degree elevation** → #32 Ch.5 (algorithms A5.1-A5.9)
**Lofting / skinning** → #33 Ch.13 (multi-section lofting), #32 Ch.10 (surface skinning algorithms)
**NURBS / parametric surfaces** → #1 Ch.5, #2 Ch.5, #13 (ESOLID), #14 (hybrid), #28 (watertight reparameterization), #32 (comprehensive NURBS algorithms)
**NURBS evaluation and derivatives** → #32 Ch.2-4 (basis functions, curves, surfaces, rational forms)
**NURBS implementation** → #32 Ch.13 (data structures, memory, error control, programming concepts)
**Numerical robustness** → #1 Ch.4, #2 Ch.4, #4, #6, #19
**Point inversion / projection** → #32 Ch.6 (A6.1-A6.5, Newton iteration with convergence criteria)
**Offset curves/surfaces** → #1 Ch.11
**orient2d / orient3d** → #4 (Shewchuk), #5 (SoS perturbation), #9 (indirect variants), #19 (failure prob)
**Overlap region extraction** → #26 (Yang 2025, bilevel optimization, 2D overlap boundary)
**Perturbation (SoS)** → #5 (Edelsbrunner-Mucke), #8 (uses SoS for triangle sort)
**Perturbation cascade (ours, alternatives)** → #5 (SoS replacement), #6 (topology-oriented replacement), #3 (fuzzy booleans)
**Persistent homology** → #23 Ch.VII
**Plane-based representation** → #11 (EMBER, integer coords), #18 (BSP + planes)
**Radial sort** → #10 (Levy, non-manifold edges), #12 (Barki, classification)
**Regularized booleans** → #12 (explicit regularization), #17 (closure-of-interior definition)
**Robustness comparison** → #15 (Nef vs ACIS), #20 (Tekla vs CGAL vs EMBER), #27 (SSI method comparison across OCCT/ACIS/SolidWorks)
**Rust implementations** → #21 (kigumi mesh booleans)
**Scale normalization** → #24 (unit-cube normalization before mesh boolean), #27 (all production systems normalize), #31 (unit-cube + ε=10⁻⁶ after normalization)
**Set membership classification** → #17 (PADL), #20 (in/out/on)
**Model verification / healing** → #33 §14.1 (ACIS body checker: edge convexity, containment, self-intersection), #33 §14.2 (topological/geometric healing)
**Non-manifold models** → #33 Ch.5 (partial/degenerate/non-manifold, compound models, wedge structures)
**Shell assembly / closure** → #3 (building part stages), #15 (Nef), #16 (Euler ops)
**Simplicial complexes** → #23 Ch.III
**Spatial indexing / AABB** → #2 Ch.3.7, #12 (AABB-accelerated)
**SSI topology determination** → #25 (Dixon matrix, characteristic points), #27 (algebraic vs numerical survey), #29 (IATA, interval algebraic analysis in 4D)
**Surface-surface intersection** → #1 Ch.5 (lattice/marching/subdivision), #3 (FF interference), #25 (topology-guaranteed tracing), #27 (comprehensive survey), #28 (isocurve reparameterization), #29 (IATA hybrid symbolic-numeric)
**Surface type taxonomy** → #36 (Parasolid 3-tier: analytic/procedural/NURBS), #33 Appendix A (geometry format specs), #32 Ch.2-4 (NURBS evaluation)
**Tessellation / mesh conversion** → #22 (curvature across discrete/smooth boundary), #34 (curvature-adapted remeshing via R^6 embedding), #35 (conforming NURBS tessellation with anisotropic metrics)
**Tolerance / fuzzy** → #3 (fuzzy booleans, SetFuzzyValue), #13 (lazy exact vs tolerance), #28 (gap-free via reparameterization), #33 Ch.16 (6 tolerance types, consistency rules)
**Sweeping / extrude / revolve** → #33 §6.2 (edge-based sweep/swing with Euler ops, movable-edge detection), #37 (swept volume B-Rep construction: profile + spine + orientation law)
**Topological validity** → #6 (topology-oriented), #16 (Euler ops), #23 (homology invariants), #33 Ch.4 (Euler ops preserve Euler-Poincaré)
**Trim testing (2D)** → #30 (2D GWN for parametric trim containment)
**Watertightness / gap-free geometry** → #28 (isocurve reparameterization), #24 (mesh boolean guarantees watertight), #30 (GWN robust to non-watertight geometry)
**Self-intersection detection** → #31 (algebraic signature, NURBS control-point-based)
**Winding numbers** → #7 (generalized), #8 (winding number vectors), #11 (EMBER WNV), #30 (GWN on trimmed NURBS via Stokes' theorem)

## Primary References

### 1. Patrikalakis, Maekawa & Cho — "Shape Interrogation for Computer Aided Design and Manufacturing"

**Access**: Free HTML hyperbook at MIT:
https://web.mit.edu/hyperbook/Patrikalakis-Maekawa-Cho/

**Local copy**: `docs/references/patrikalakis-shape-interrogation.txt` (10,940 lines, full text extraction from 246 HTML pages)

**Fetch script**: `scripts/fetch-patrikalakis.sh` — re-run to update

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

**Local copy**: `docs/references/hoffmann-geometric-solid-modeling.txt` (15,554 lines, full text extraction)

**How to use**: For targeted reading, use page ranges: `pdftotext input.pdf output.txt -f 67 -l 110` for Chapter 3.

### 3. OpenCascade Boolean Operations Documentation

**Access**: Free online documentation:
https://dev.opencascade.org/doc/overview/html/specification__boolean_operations.html

**Local copy**: `docs/references/opencascade-boolean-operations.md` (1,074 lines, full specification)

**Source code**: https://github.com/Open-Cascade-SAS/OCCT (LGPL-2.1)

**Why this is critical**: OCCT's General Fuse Algorithm (GFA) is the most mature open-source B-rep boolean implementation. It handles ALL the degenerate configurations that cause our perturbation cascade to trigger — coplanar faces, coincident edges, vertex-on-face, tangential contact. The source code is available to study.

**Key concepts for our work**:
- **General Fuse Algorithm (GFA)**: Operates on n arguments simultaneously, producing a complete decomposition of space. Boolean operations (union, intersection, difference) are extractions from the GFA result. `RGF = Sp1 + Sp2 + Sp12` for two arguments. Section Operator extracts intersection geometry (vertices + edges). Splitter Operator divides Objects using Tools.
- **Staged Interference Computation**: Processes interferences in strict order: `VV → VE → EE → VF → EF → FF → VZ → EZ → FZ → ZZ`. Each interference type is handled with appropriate algorithms. This prevents the "surface-surface intersection through an existing vertex" problem (our D3 root cause). The ordering allows avoiding redundant interferences between upper-level shapes when there are interferences between lower sub-shapes.
- **Interference types**: VV (vertex/vertex — distance < sum of tolerances, new vertex at enclosing sphere center), VE (vertex/edge — projection within tolerance, vertex tolerance adjusted), EE (edge/edge — common curve segments OR common points), VF (vertex/face — projection within tolerance), EF (edge/face — parametric ranges or new vertices), FF (face/face — intersection curves Cijk and isolated points). Non-BRep: VZ/EZ/FZ/ZZ (shape entirely inside solid, no boundary contact).
- **Pave Blocks**: Edge segments between neighboring intersection points (paves). Each pave stores vertex index + parameter value on curve. `BOPDS_PaveBlock` stores: `myEdge` (produced edge), `myOriginalEdge`, `myPave1`/`myPave2` (boundary paves), `myExtPaves` (internal splitting paves), `myCommonBlock`, `myShrunkData`. Pave blocks are the fundamental unit for edge splitting.
- **Common Blocks**: Form when pave blocks have same bounding vertices and geometrically coincide (from EE or EF interference). Contain multiple pave blocks + associated face indices.
- **FaceInfo / In-On-Sc States**: `BOPDS_FaceInfo` classifies pave blocks and vertices into 6 categories: PaveBlocksIn/VerticesIn (fully interior to face), PaveBlocksOn/VerticesOn (on face boundary), PaveBlocksSc/VerticesSc (created from intersection curves/points). This three-state model is more nuanced than our binary And/Or classification.
- **Same-Domain Analysis**: Detects coplanar face pairs and computes connexity chains. This is OCCT's solution to our D1 (coplanar faces) problem.
- **Fuzzy Boolean Operations**: `SetFuzzyValue()` expands tolerance spheres, allowing nearby-but-not-touching geometry to be treated as intersecting. More principled than our perturbation cascade.
- **Shrunk Ranges**: Narrowed parameter intervals `[t1S, t2S]` ⊂ `[t1C, t2C]` on edges accounting for vertex tolerances. Prevents redundant interference calculations at boundaries.
- **Shape requirements**: Arguments must be valid per `BRepCheck_Analyzer`, not self-interfered, with C1-continuous underlying geometry.
- **Building Part stages**: Split vertices → split edges → split faces → same-domain faces → shells → solids → containers. Each stage builds up from the interference results.
- **Key classes**: `BOPAlgo_PaveFiller` (intersection part), `BOPAlgo_Builder` (building part), `BOPDS_DS` (central data structure), `BOPDS_FaceInfo`, `BOPDS_PaveBlock`, `BOPDS_CommonBlock`.

**How to use**: Read the local copy for the algorithmic overview. For implementation details, study the OCCT source code.

### 4. Shewchuk — "Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates" (1997)

**Access**: Free PDF:
https://people.eecs.berkeley.edu/~jrs/papers/robustr.pdf

**Local copy**: `docs/references/shewchuk-adaptive-predicates-1997.txt` (4,727 lines, full text extraction)

**Rust implementation**: `robust` crate (https://github.com/georust/robust) — already used in our `robust_classify.rs`.

**Relevance**: The foundation for exact geometric predicates (orient2d, orient3d, incircle) using adaptive-precision arithmetic. Only does as much extra work as needed — fast for easy cases, exact for degenerate cases. We already use this via the `robust` crate, but only for ray-triangle tests. Should be extended to coplanar detection, edge-edge intersection, and all load-bearing geometric decisions.

### 5. Edelsbrunner & Mucke — "Simulation of Simplicity" (1990)

**Access**: Free PDF:
https://arxiv.org/abs/math/9410209

Also at: https://www.sandia.gov/files/samitch/unm_math_579/p66_edelsbrunner_simulation_of_simplicity.pdf

**Local copy**: `docs/references/edelsbrunner-mucke-sos.md` (682 lines, full equation transcription)

**Citation**: Edelsbrunner, H. and Mucke, E.P. "Simulation of simplicity: a technique to cope with degenerate cases in geometric algorithms." ACM Transactions on Graphics 9(1):66–104, 1990.

**Relevance**: General-purpose technique for handling degenerate input. Adds symbolic infinitesimal perturbations to input data so that no predicate ever evaluates to zero — algorithms can pretend degeneracies never occur. Unlike our perturbation cascade (which modifies actual geometry), SoS perturbations are virtual and never applied to the data. We have partial SoS in `robust_classify.rs` (`sos_orient2d_tiebreak`); full SoS would cover orient3d, incircle, and all geometric decisions, eliminating degenerate-case special handling entirely.

**The SoS perturbation scheme**:
- Each coordinate xi,j is replaced by xi,j + ε^{2^λ(i,j)} where λ(i,j) = i·δ + (δ+1-j) and δ ≥ d (Cartesian) or δ ≥ d+1 (homogeneous). For fixed point i, coordinate j=d (last spatial column before the "1") gets the most perturbation; j=1 gets the least. Earlier points (smaller i) get more perturbation than later points.
- **Domination property (Eq. 3-c)**: ε^{2^a} > ∏_{b>a} ε^{2^b} for 0 < ε < 1. This ensures no cancellation — each term in the determinant expansion dominates the sum of all less-significant terms.
- **Non-degeneracy guarantee (Lemma 3.3)**: All determinants of the perturbed system are nonzero for sufficiently small ε > 0. The last coefficient in the determinant's ε-polynomial is always ±1, so the determinant never vanishes.

**The peeling/evaluation process (SignDet)**:
- To evaluate sign(det Δ(ε)): scan cofactors of increasing depth t=0,1,2,...
- At depth 0: evaluate det Δ (the unperturbed determinant). If nonzero, return its sign.
- At depth t>0: evaluate a subdeterminant (cofactor) of the unperturbed matrix. If nonzero, return its sign (adjusted by ±1 from the encoding).
- The encoding vector v = [v₁,...,v_D,D+1] tracks which ε(i,j) terms are "active." Next_v generates the successor. The Matrix procedure extracts the submatrix and computes the sign adjustment.
- **Termination guarantee**: The final cofactor (0×0 determinant = 1) is always nonzero. The process always terminates.

**Term counts for relevant cofactors**: Delta (Cartesian): D=2→5, D=3→15, D=4→50. Omega (homogeneous): D=2→2, D=3→5, D=4→15. Non-degenerate input has zero overhead (depth 0 suffices).

**Six predicates defined**: (1) Smaller — perturbed coordinate comparison. (2) Positive_d — orientation of d+1 points. (3) IntersectHalfLine — point-in-polygon test. (4) OnPositiveSide — hyperplane sidedness via duality. (5) Above_d — nonvertical hyperplanes. (6) InSphere_d — in-sphere test via lifting transform to paraboloid x_{d+1} = Σx_μ².

**Practical note**: Exact integer arithmetic required. Long-integer → built-in integer arithmetic speedup ≈ 10×. Hadamard bound: |det| ≤ D^{D/2}·M^D. Floating-point as a filter: if result is far from zero, sign is correct; otherwise fall back to exact arithmetic (cf. Devillers-Preparata #19 for filter failure probabilities).

### 6. Sugihara & Iri — "Topology-Oriented Implementation — An Approach to Robust Geometric Algorithms" (2000)

**Access**: Free PDF:
https://ftp.cs.wisc.edu/pub/users/prem/for-prem/sugihara-algorithmica-2000.pdf

Also: Springer (institutional access): https://link.springer.com/article/10.1007/s004530010002

**Local copy**: `docs/references/sugihara-iri-topology-oriented.md` (551 lines, full transcription)

**Citation**: Sugihara, K., Iri, M., Inagaki, H., Imai, T. Algorithmica 27:5–20, 2000.

**Relevance**: The philosophical framework for replacing our perturbation cascade. Key insight: describe the algorithm in terms of *topological* operations first (combinatorial computation that is never contaminated by numerical errors), then use numerical computation only to choose among topologically valid branches. The resulting software is "completely robust in the sense that no matter how large numerical errors arise, the algorithm never fails." Our wire splitting, biangle filtering, and shell closure recovery are ad-hoc versions of what this paper systematizes. Applying this framework would mean: maintain manifoldness at every step, use geometry only to select between topologically consistent alternatives.

**Two foundational assumptions**: (1) Logical/combinatorial computation is correct; numerical computation contains errors. (2) No a priori bound on numerical error magnitude. These deliberately separate robustness from error analysis.

**The three-step method**:
- **Step I**: Collect purely topological properties Q that valid solutions must satisfy and that can be checked efficiently. "Purely topological" = stated without reference to numerical values.
- **Step II**: Construct the **topological skeleton** — the algorithm described entirely in combinatorial/topological terms, guaranteeing properties in Q. This skeleton is nondeterministic (multiple valid branches). *Any* path through it yields a topologically valid output.
- **Step III**: Use numerical computation at each branch point to choose the path most likely leading to the correct answer. This makes the algorithm deterministic.

**Key definitions**:
- **Numerically robust**: f̃(X) is defined for any input X ∈ I(P) — the program always terminates with some output, never crashes or loops forever.
- **Topologically consistent**: f̃ is robust AND f̃_T(X) ∈ O_T(P) — the output topology is a valid answer to *some* instance of the problem (not necessarily the given input).

**Two worked examples with full pseudocode**:
- **Algorithm 1 (Clipping convex polyhedron by plane)**: Properties Q = {planarity of vertex-edge graph (P1.1), connectivity of both sides (P1.2)}. Step 1 (nondeterministic): divide vertices into inside/outside sets maintaining connectivity. Steps 2-4 (deterministic): generate new vertices on crossing edges, create separating circuit, remove outside vertices. Topological consistency proven via **Steinitz's theorem** (3-connected planar graph ↔ convex polyhedron).
- **Algorithm 2 (Voronoi diagram for line segments)**: Properties Q = {deleted subgraph is a tree (P2.1), tree connects the two endpoint regions (P2.2)}. Incremental addition of open line segments to existing Voronoi diagram.

**Guarantees**:
- **Robustness**: Absolute — works even if all numerical results are replaced by random numbers.
- **Topological consistency**: Depends on Q. Proven for convex polyhedra (via Steinitz). Not provable for line-segment Voronoi diagrams (no known topological characterization).
- **Convergence**: For non-degenerate input, output converges to correct answer as precision increases. For degenerate input with consistency, output converges to infinitesimally perturbed version (microstructures of length → 0).
- **Time complexity**: O(g(n) + h(n)) where g(n) is original algorithm cost and h(n) is property-checking cost.

**How this differs from SoS (#5)** (our synthesis, not from the paper):
| SoS | Topology-Oriented |
|-----|-------------------|
| Requires exact arithmetic | No precision requirements |
| Explicit perturbation for degeneracy | Degeneracy ignored (undetectable under error) |
| Correct answer for perturbed input | Topologically valid answer; correct if precision sufficient |
| Category 2: complete reliance on exact numerics | Category 3: least reliance on numerical values |
| Make numerics exact so topology follows | Make topology correct so numerics become advisory |

**Limitations**: (1) Finding Q and the topological skeleton is non-trivial — problem-specific work required. (2) Output is generally an approximation — not suitable when strictly correct answers are needed. (3) Microstructures near degenerate configurations can increase time complexity.

## Mesh Boolean References (Exact Computation Approaches)

These references operate on triangle meshes rather than NURBS B-rep, but their classification and robustness techniques are directly applicable to our ray-cast classification and shell assembly stages.

### 7. Jacobson, Kavan & Sorkine-Hornung — "Robust Inside-Outside Segmentation Using Generalized Winding Numbers" (2013)

**Access**: Free PDF:
https://igl.ethz.ch/projects/winding-number/robust-inside-outside-segmentation-using-generalized-winding-numbers-siggraph-2013-jacobson-et-al.pdf

**Local copy**: `docs/references/jacobson-winding-numbers-2013.txt` (836 lines, text extraction)

**Citation**: ACM Transactions on Graphics 32(4):33:1–33:12, 2013 (SIGGRAPH).

**Relevance**: Generalized winding numbers provide robust inside/outside classification that works even for non-manifold, self-intersecting, and open meshes. The winding number is a sum over faces giving a continuous function: 1 inside, 0 outside, degrading gracefully near boundaries. Would replace our 8-ray majority-voting classifier, eliminating ray-edge grazing issues, edge-neighbor propagation fallback, and Unknown face classification entirely.

### 8. Zhou, Grinspun, Zorin & Jacobson — "Mesh Arrangements for Solid Geometry" (2016)

**Access**: Free PDF:
https://cims.nyu.edu/gcl/papers/zhou2016mas.pdf

**Project page**: https://www.cs.columbia.edu/cg/mesh-arrangements/

**Implementation**: libigl (https://github.com/libigl/libigl), deployed in Blender

**Local copy**: `docs/references/zhou-mesh-arrangements-2016.md` (557 lines, full equation transcription)

**Citation**: ACM Transactions on Graphics 35(4), 2016 (SIGGRAPH).

**Relevance**: Complete boolean pipeline using mesh arrangements + winding number vectors. Two stages: (1) resolve all intersections to create an arrangement (space partition into cells), (2) assign winding number vectors to each cell and extract based on boolean operation. Makes NO general position assumptions, does NOT use perturbation, is variadic (n-ary). Tested on 10,000+ real-world meshes. Used as the basis for Blender's boolean redesign. Our pipeline has equivalent stages (`create_loops_stores` ≈ arrangement, `classify_faces` ≈ winding number extraction) but uses fragile heuristics where they use exact computation.

**Winding number classification (Eqs. 1-8)**:
- **Eq. 2**: w_i(p) = (1/4π) Σ_{t∈A_i} Ω_t(p), where Ω_t(p) is signed solid angle subtended by triangle t at point p.
- **Eq. 3 (Union)**: f(w) = true if ∃i: w_i ≠ 0 (inside any). When n=1, this is self-union (mesh repair).
- **Eq. 4 (Intersection)**: f(w) = true if w_i ≠ 0 for all i (inside all).
- **Eq. 5 (Difference)**: f(w) = (w₁ ≠ 0) ∧ (w₂ = 0).
- **Eq. 7 (Propagation)**: w_n ← w_c + s_p · [δ_{i1},...,δ_{in}], where s_p = ±1 based on patch orientation, i = originating mesh index. BFS from ambient cell (w = [0,...,0]).
- **Eq. 8 (Complement)**: w_j = 1−|w_i| (orientation-insensitive) or w_j = 1−w_i (orientation-sensitive).

**Four-stage pipeline**: (1a) Intersection resolution — discard zero-area, pairwise exact triangle-triangle intersection, cluster CDT for coplanar groups, extract/replicate subtriangles. (1b) Cell partitioning — patches (maximal manifold-connected triangle sets), bipartite cell-patch graph, cyclical triangle sort around non-manifold edges. (1c) Winding number labeling — BFS from ambient cell using Eq. 7. (2) Result extraction — flag cells by f(w), collect boundary patches, flip orientations, purge zero-volume symbolic cells.

**Critical subroutine — cyclical triangle sort**: Divide-and-conquer sort of triangles about a common edge. Uses SoS (Edelsbrunner-Mucke) for consistent ordering of duplicate/coplanar triangles. Four groups: same-side coplanar, opposite-side coplanar, below, above.

**Performance**: 1.7s geometric mean on 8616 PWN meshes (100% success rate). Variadic 10-tet union: 4.5s vs 8.9s cascading binary. "Inside at least 5 of 10": same 4.5s vs weeks cascading. Uses CGAL exact arithmetic kernel.

**Thingi10K statistics**: 86.2% of 10K meshes are PWN; 45.3% have self-intersections; 30.8% have coplanar self-intersections. This quantifies how common "degenerate" configurations are in real-world meshes.

### 9. Cherchi, Livesu, Scateni & Attene — "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" (2020)

**Access**: Free PDF:
https://www.gianmarcocherchi.com/pdf/mesh_arrangement.pdf

**Code**: https://github.com/gcherchi/FastAndRobustMeshArrangements (header-only C++)

**Local copy**: `docs/references/cherchi-indirect-predicates-2020.md` (542 lines, full transcription)

**Citation**: ACM TOG 39(6), 2020 (SIGGRAPH Asia).

**Relevance**: Indirect predicates — exact geometric tests using only floating-point hardware. As fast as non-robust float implementations but provably correct. Practical realization of Shewchuk's vision applied to mesh arrangements.

**Key innovation — indirect predicates**: Intersection points are never computed explicitly. Instead, they are stored as references to the input primitives that generated them (5 floats for Line-Plane intersections, 9 for Three-Plane). Predicates (orient2d, etc.) are reformulated to take these implicit point representations as input, evaluating the predicate on the exact intersection point without ever materializing its coordinates.

**Three-stage filtering**: (1) Floating-point with semi-static error bound → succeeds >99.99% of calls. (2) Interval arithmetic. (3) Shewchuk-style arithmetic expansions. Stage 1 makes performance near-identical to non-robust code.

**10 indirect orient2d variants**: For all combinations of Explicit (E), Line-plane (L), and Three-plane (T) point types. Filter constants range from δ² (EEE) to δ²⁶ (TTT).

**Performance**: 99.3% of models faster than libigl/CGAL in serial. Top 10 hardest models: 18.1% of libigl's time, 38.6% of libigl's memory. Extreme model (40M intersections): libigl runs out of memory, Cherchi completes in <1h.

### 10. Levy — "Exact Predicates, Exact Constructions and Combinatorics for Mesh CSG" (2025)

**Access**: Free PDF:
https://hal.science/hal-05251901v1/file/CSG.pdf

TeX source: https://arxiv.org/src/2405.12949

ACM: https://dl.acm.org/doi/10.1145/3744642

**Local copy**: `docs/references/levy-exact-constructions-2025.md` + `docs/references/levy-exact-constructions-2025-tex.md`

**Citation**: Levy, B. ACM Transactions on Graphics 44(5):1–27, 2025. DOI: 10.1145/3744642.

**Note**: This paper is by Bruno Levy (Inria Saclay), not Cherchi et al. as previously attributed. The HAL filename and our original plan entry were misleading. Levy cites and compares against Cherchi et al. (#9) as prior work.

**Relevance**: State-of-the-art. Computes the full Weiler model (3D arrangement) using exact arithmetic, with radial sort for non-manifold intersection edges. Proposes two geometric kernels (arithmetic expansions and multi-precision floating-point). Directly relevant to our edge welding (radial sort replaces our tolerance-based weld_coincident_edges) and shell assembly problems.

**Two exact arithmetic kernels**:
1. **Arithmetic expansions** (Shewchuk-style): Extended from predicates to **exact constructions** — storing intersection point coordinates as expansion arrays, not just evaluating signs. Limited by exponent overflow at ~65,000-component expansions. Includes a compression algorithm (with a correction to Shewchuk's original).
2. **Multi-precision floating-point** (GMP-based, CGAL-style): No exponent limitations, handles all test cases. Used in production.

**Intersection points in homogeneous coordinates**: Three construction cases: edge-plane intersection ([a·q₂ + (b−a)·q₁ ; b]_h), edge-edge intersection in 2D (2×2 Cramer), three-triangle intersection (3×3 Cramer on plane normals). Homogeneous representation avoids division.

**Optimized CDT**: 75% fewer orient_2d calls during constraint enforcement by exploiting combinatorial information about which triangle vertex is opposite the edge being flipped.

**Radial sort for non-manifold edges**: Uses two predicates (`orient` and `Norient`) to define four quadrants around each radial edge. Propagation along radial polylines minimizes expensive predicate evaluations.

**Performance**: Up to 6× faster than Cherchi et al. (#9) and Zhou et al. (#8) on coplanar-heavy configurations. EMBER (#11) remains 50-100× faster but requires integer coordinates. Demonstrates exact constructions without CGAL's heavy infrastructure.

### 11. Trettner, Nehring-Wirxel & Kobbelt — "EMBER: Exact Mesh Booleans via Efficient & Robust Local Arrangements" (2022)

**Access**: Free PDF:
https://www.graphics.rwth-aachen.de/media/papers/339/ember_exact_mesh_booleans_via_efficient_and_robust_local_arrangements.pdf

**Local copy**: `docs/references/trettner-ember-2022.txt` (1,110 lines, text extraction)

**Citation**: ACM TOG 41(4):39, 2022 (SIGGRAPH).

**Relevance**: Plane-based representation + homogeneous integer coordinates for exactness. Adaptive recursive subdivision instead of global acceleration structure. Shows exact booleans can avoid building a complete global arrangement (early-out termination).

### 12. Barki, Guennebaud & Foufou — "Exact, Robust, and Efficient Regularized Booleans on General 3D Meshes" (2015)

**Access**: Free PDF:
https://inria.hal.science/hal-01203173/file/RobustBooleans_2015.pdf

Also: https://hal.science/hal-01203173/

**Local copy**: `docs/references/barki-robust-booleans-2015.md` (554 lines, full transcription)

**Citation**: Barki, H., Guennebaud, G., Foufou, S. Computers & Mathematics with Applications 70(6):1235–1254, 2015.

**Relevance**: Efficient exact booleans without the overhead of Nef polyhedra. Uses co-refinement + classification. 3.35× faster than Maya, 5.06× faster than CGAL Nef. Could inform a mesh-level boolean fallback path.

**Three-phase algorithm**: (1) Pre-processing — triangulate, remove degenerate facets. (2) Intersection + representation — AABB-accelerated triangle-triangle intersection, 2D arrangement construction per triangle (bijective projection to axis-aligned plane), constrained Delaunay triangulation of arrangements. (3) Classification + browse — local radial-sort classification around intersection segments, linear-time mesh browse propagation, ray-shooting for disjoint/tangential components.

**Key insight — two unambiguous cases**: Reduces the explosion of degenerate intersection configurations to exactly two: (1) non-orientable pair → one is union, other is intersection result. (2) orientable pair → both contribute same result. Classification via the meets-and-joins set theory property.

**Degenerate handling**: Coplanar overlapping (explicit in 2D arrangements), opposite overlapping (discarded by regularization), equivalent/complementary operands (early termination), non-manifold geometry (custom simplicial-complex data structure).

**Performance**: Succeeded on ALL test cases where Maya, CGAL Nef, and PGBT each failed on multiple cases. 19.5× faster than Maya for equivalent operands.

## Curved B-rep Boolean References

### 13. ESOLID — Keyser, Culver, Manocha et al.

**Access**: Free PDF:
http://gamma.cs.unc.edu/ESOLID/keyser02.pdf

**Project page**: http://www.cs.unc.edu/~geom/ESOLID/

**Local copy**: `docs/references/keyser-esolid-2004.txt` (832 lines, text extraction)

**Citation**: Keyser, J., Culver, T., Foskey, M., Krishnan, S., Manocha, D. "ESOLID—A System for Exact Boundary Evaluation." Computer-Aided Design 36(2):175–193, 2004.

**Relevance**: Exact boolean operations on LOW-DEGREE CURVED solids (quadrics, tori, cylinders — the primitives in mechanical CAD). Uses lazy evaluation, floating-point filters, and arbitrary-precision arithmetic. The lazy evaluation strategy (try cheap float first, escalate to exact only when needed) is the correct alternative to our perturbation cascade. This is the only reference that does exact booleans on curved B-rep (not tessellated meshes).

### 14. Sheng, Liu, Li, Fu, Ma & Wu — "Accelerated Robust Boolean Operations Based on Hybrid Representations" (2018)

**Access**: Free PDF:
https://hongbofu.people.ust.hk/doc/Accelerated_robust_Boolean_operations_CAGD18.pdf

Also: https://www.sciencedirect.com/science/article/abs/pii/S0167839618300359

**Local copy**: `docs/references/sheng-accelerated-robust-booleans-2018.txt` (1,148 lines, text extraction)

**Citation**: Sheng, B., Liu, B., Li, P., Fu, H., Ma, L., Wu, E. "Accelerated Robust Boolean Operations Based on Hybrid Representations." Computer Aided Geometric Design 64:36–49, 2018.

**Relevance**: Hybrid approach combining NURBS (for precision) with mesh (for robustness). Uses exact predicates on mesh representation to guide decisions on the NURBS representation. This is the architecture truck should aspire to — maintain exact topological decisions while using NURBS geometry.

## Foundational References

### 15. Granados, Hachenberger, Kettner, Mehlhorn et al. — "Boolean Operations on 3D Selective Nef Complexes" (2003/2007)

**Access**: Free PDF (Hachenberger PhD thesis):
https://publikationen.sulb.uni-saarland.de/bitstream/20.500.11880/25961/1/Dissertation_1778_Hach_Pete_2006.pdf

EWCG 2005 implementation paper (free PDF):
https://ewcg2005.win.tue.nl/Proceedings/36.pdf

**Local copy**: `docs/references/hachenberger-nef-ewcg2005.md` (EWCG 2005 paper, full transcription)

**CGAL docs**: https://doc.cgal.org/latest/Nef_3/index.html

**Citation**: Computational Geometry 38(1-2):64–99, 2007. Also: Hachenberger, P. and Kettner, L. "Boolean Operations on 3D Selective Nef Complexes: Optimized Implementation and Experiments." EWCG 2005, pp. 139–142.

**Relevance**: Nef polyhedra (closure of half-spaces under boolean ops) handle non-manifold, open boundaries, and mixed-dimensional complexes — exactly the cases where our `finalize_boolean_shell` struggles. Exact arithmetic throughout. The CGAL implementation is production-proven. Operates on polyhedral (flat face) geometry, not NURBS.

**Nef polyhedra definition**: A point set P ⊂ ℝ^d generated from a finite number of open halfspaces by set complement and set intersection. Closed under ALL boolean and topological operations by construction.

**Three-step algorithm** (EWCG 2005): (1) Find candidate vertices — original vertices of both inputs plus all edge-edge and edge-face intersection points. (2) Compute local sphere maps at each candidate vertex (known for existing vertices, constructed for new intersections). (3) Overlay sphere maps using hemisphere sweep (extension of planar sweep to spherical geometry) to produce result sphere maps. Full SNC synthesized from result sphere maps using Plücker coordinates for edges.

**Three key optimizations** (tested on TetGrid N=16, 480s→164s = 65.9% reduction): (i) Specialized overlay algorithms for simple cases — 93.8% reduction in sweeps (240K→15K). (ii) Single half-sphere sweep when possible — 9% improvement. (iii) Vertex absorption — skip overlays for interior vertices.

**Robustness comparison with ACIS R13**: ACIS is 4-6× faster on balanced operations but runs out of memory at N=14 where Nef 3D completes in 317s. On the RotCylinder stress test: ACIS fails for rotation angle α < 10⁻³; Nef 3D handles α = 10⁻⁷ with 10K polygon sides. Performance gap narrows with scale: 5× at n=100 → 1.2× at n=2000.

### 16. Mäntylä — "An Introduction to Solid Modeling" (1988)

**Access**: https://archive.org/details/introductiontoso0000mant (Internet Archive, may need borrow)

**Relevance**: Deep dive on Euler operators and half-edge data structures. Proves that Euler operators form a complete set of modeling primitives for manifold solids. Relevant if we restructure truck's topology layer. The Euler operator approach ensures that every topological operation preserves manifoldness — which is what our `finalize_boolean_shell` struggles with.

### 17. Requicha & Voelcker — "Boolean Operations in Solid Modelling: Boundary Evaluation and Merging Algorithms" (1985)

**Access**: SciSpace (may require manual download in a browser):
https://scispace.com/pdf/boolean-operations-in-solid-modeling-boundary-evaluation-and-9zz6lpq7h1.pdf

Fallback (scanned copy, no text layer): University of Rochester institutional repository:
https://urresearch.rochester.edu/institutionalPublicationPublicView.action?institutionalItemId=990

Also: https://ieeexplore.ieee.org/document/1457376/

**Local copy**: `docs/references/requicha-voelcker-boolean-ops-1985.txt` (1,454 lines, text extraction from SciSpace PDF)

**Citation**: Requicha, A.A.G. and Voelcker, H.B. "Boolean Operations in Solid Modelling: Boundary Evaluation and Merging Algorithms." Technical Memorandum TM-26, Production Automation Project, University of Rochester, January 1984. Published in Proceedings of the IEEE 73(1):30–44, 1985.

**Relevance**: The theoretical framework for regularized boolean operations on solids. Describes boundary evaluation algorithms used by the PADL solid modelling systems. Introduces the concepts of set membership classification and neighborhood manipulation. Defines regularized union/intersection/difference as closure-of-interior operations, guaranteeing the result is always a regular closed set (no dangling faces/edges). This is the mathematical reason WHY boolean results should be manifold. When `finalize_boolean_shell` produces shells with singular vertices, it's because the regularization step is failing.

**Key chapters**:
- **Ch. 1**: Introduction — solid modelling, boolean operations, computational problems
- **Ch. 2**: Set Membership Classification — definitions, combining classifications, representing/combining neighborhoods, divide-and-conquer paradigm for CSG, classification algorithms for BReps
- **Ch. 3**: Boundary Evaluation and Merging — generate-and-test paradigm for faces and edges, efficiency improvements, survey of known approaches
- **Ch. 4**: Summary and Concluding Remarks
- **Ch. 5**: Acknowledgements and Historical Notes

### 18. Bernstein & Fussell — "Fast, Exact, Linear Booleans" (2009)

**Access**: Free PDF:
http://www.gilbertbernstein.org/resources/booleans2009.pdf

**Local copy**: `docs/references/bernstein-fast-exact-booleans-2009.txt` (748 lines, text extraction)

**Citation**: Computer Graphics Forum 28(5):1269–1278, 2009 (SGP).

**Relevance**: BSP-tree based exact booleans using plane-based representations. Only 4 geometric predicates needed. 16-28x faster than CGAL Nef polyhedra. Relevant if we consider a BSP-based approach for the planar-face portions of our boolean pipeline.

### 19. Devillers & Preparata — "A Probabilistic Analysis of the Power of Arithmetic Filters" (1998)

**Access**: Free PDF (via Springer):
https://link.springer.com/content/pdf/10.1007/PL00009400.pdf

**Local copy**: `docs/references/devillers-preparata-arithmetic-filters-1998.txt` (1,315 lines, text extraction)

**Citation**: Devillers, O. and Preparata, F.P. Discrete & Computational Geometry 20:523–547, 1998.

**Core concept**: An **arithmetic filter** is a pair (evaluator, certifier). The evaluator computes an approximate value μ(E) of an expression μ using floating-point. The certifier compares |μ(E)| against a threshold ε(E). If |μ(E)| ≥ ε(E), the computed sign is reliable. Otherwise, the filter *fails* and a more expensive evaluator (eventually exact arithmetic) is needed. This paper computes both the threshold ε and the probability of filter failure for the two core geometric predicates.

**Key results for our work**:

- **Which-side predicate** (orient2d/orient3d — δ×δ determinant with independent entries): Filter failure probability is *linear* in the threshold: `Prob(|det| ≤ V) ≤ ψ_δ · V`, where ψ₁=1, ψ₂=π, ψ₃=27π⁴/128≈21, ψ₄=32π⁶/81≈380, ψ₅≈23,000, ψ₆≈4.5×10⁶. General formula: `ψ_δ = δ · v_δ(1) · v_{δ-1}(1)^δ · δ^{δ(δ-1)/2} / 2^{δ²}`.

- **Insphere predicate** (determinant with dependent x² entries): Failure probability is *worse* due to dependencies. For 1-insphere: `≤ 5.355·A^{2/3}`. For 2-insphere: `≤ π√(2V) ≈ 4.44√V`. For δ≥3: `≤ ϕ_δ·√W·ln(1/W) + χ_δ·√W`. Constants: ϕ₃≈70 (χ₃=-100), ϕ₄≈408 (χ₄=350), ϕ₅≈3970 (χ₅=18,000), ϕ₆≈68,500 (χ₆=640,000).

- **Error thresholds for recursive determinant evaluation** (b-bit mantissa floating-point): ε₂=2·2⁻ᵇ, ε₃=13·2⁻ᵇ, ε₄=76·2⁻ᵇ, ε₅=576·2⁻ᵇ, ε₆=3672·2⁻ᵇ, ε₇=27,304·2⁻ᵇ, ε₈=226,624·2⁻ᵇ. These derive from error propagation rules: `E[M₁,m₁]+E[M₂,m₂] = E[M₁+M₂, 2⁻ᵇ⁻¹·M̄₁₊M₂+m₁+m₂]` and `E[M₁,m₁]·E[M₂,m₂] = E[M₁·M₂, 2⁻ᵇ⁻¹·M̄₁·M̄₂+m₁·M̄₂+m₂·M̄₁]`, where E[M,m] means absolute value ≤ M, error ≤ m, and M̄ = 2^⌈log₂M⌉ (ceiling to next power of 2). Hadamard bounds used: D₃≤4, D₄≤16, D₅≤48, D₆≤160, D₇≤576, D₈≤4096.

- **Practical IEEE 754 double (b=53) failure probabilities**: orient2d (2×2, δ=2): ρ₂≈1.2×10⁻¹⁵. orient3d (3×3, δ=3): ρ₃≈4.8×10⁻¹⁴. δ=4: ρ₄≈5.9×10⁻¹². δ=5: ρ₅≈3.0×10⁻⁹. δ=6: ρ₆≈8.7×10⁻⁶. **Conclusion: for δ≤4, floating-point filters fail less than 1 in a billion times; for δ=5, ~3 per billion. Exact arithmetic is almost never needed.**

- **Operation counts** for dynamic-programming recursive evaluation: r_δ = (δ-1)(2^δ-1). r₃=14, r₄=45, r₅=124, r₆=315, r₇=762, r₈=1785.

**Relevance**: Directly validates the `robust` crate's adaptive-precision strategy (try float first, escalate to exact when needed). For orient3d (our most-used predicate, 3×3 determinant, δ=3), the filter fails ~10⁻¹⁴ of the time — essentially never. The E[M,m] error propagation rules give us a recipe for computing static filter thresholds for *any* custom determinantal predicate we might add (e.g., coplanar distance, point-on-ray). The insphere analysis shows that predicates with dependent entries (like our lifted-coordinate winding number computations, if we implement Phase 2A) need wider thresholds — the √W·ln(1/W) bound means the filter fails more often, but still rarely enough for practical use.

### 20. Astarlioglu — "Comparing Boolean Operation Methods on 3D Solids" (2023)

**Access**: Free PDF:
https://aaltodoc.aalto.fi/server/api/core/bitstreams/03d44db1-43a8-458d-8e13-731a9a7f9736/content

**Local copy**: `docs/references/astarlioglu-comparing-booleans-2023.md` (375 lines)

**Citation**: Astarlioglu, E. MSc thesis, Aalto University, 2023. Supervised by Prof. Sandor Kisfaludi-Bak, industry partner Trimble/Tekla Structures.

**Relevance**: Comparative study of three boolean implementations — Tekla Structures (vertex-based, derived from Mäntylä's method), CGAL (exact computation), and EMBER (plane-based, exact integer). Tests on random convex polyhedra, rotated cubes, Menger sponges, and 1019 special robustness cases.

**Key findings for our pipeline**:
- **Boundary classification scheme**: 4-way (in/out) and 8-way (in/out/on+/on−) classification, with reclassification tables for collapsing ON cases into IN/OUT. Maps directly to our `classify_faces`.
- **Vertex neighborhood reduction**: Boolean operations reduce to vertex neighborhood classification (edge-edge, edge-face, vertex-face, vertex-vertex intersections). Maps to our IC computation + face division pipeline.
- **Half-edge (DCEL) data structure**: Complete hierarchy Solid→Face→Loop→HalfEdge→Edge→Vertex with validity requirements. Euler operators MEV, MEF, MEKL, KEV and the completeness theorem (Mäntylä 1984).
- **Failure modes**: Intersection works well everywhere; union and difference fail in vertex-based methods on near-degenerate inputs; chained operations are the real stress test; internal validity checks produce false positives.
- **EMBER's winding number approach**: WNV/WNTV for face classification, BSP-based recursive subdivision.

### 21. kigumi — Rust Mesh Boolean Library

**Access**: https://github.com/unageek/kigumi

**Relevance**: A **Rust** implementation of mesh boolean operations. Potentially more directly useful for integration with our Rust codebase than C++/CGAL implementations. Discovered via survey of mesh boolean implementations.

### 22. Sullivan — "Curves of Finite Total Curvature" (2006/2008)

**Access**: Free PDF:
https://arxiv.org/abs/math/0606007

**Local copy**: `docs/references/sullivan-finite-total-curvature-2008.txt` (1,112 lines, text extraction)

**Citation**: Sullivan, J.M. "Curves of Finite Total Curvature." In *Discrete Differential Geometry*, Oberwolfach Seminars 38, Birkhauser, 2008. 25 pages, 4 figures.

**MSC**: 53A04 (Primary); 57M25, 53C65, 26A45 (Secondary).

**Relevance**: Develops Milnor's framework for curves of finite total curvature, a class that encompasses both smooth and polygonal curves and bridges discrete and differential geometry. Covers theorems by Fary/Milnor, Schur, Chakerian, and Wienholtz. Natural for variational problems and geometric knot theory. Relevant to our tessellation and edge representation — understanding how curvature behaves across the smooth/discrete boundary informs NURBS-to-polyline approximation quality and error bounds for edge tessellation.

### 23. Edelsbrunner & Harer — "Computational Topology: An Introduction" (2010)

**Access**: Free PDF:
https://webhomes.maths.ed.ac.uk/~v1ranick/papers/edelcomp.pdf

**Local copy**: `docs/references/edelsbrunner-harer-computational-topology.txt` (11,647 lines, full text extraction, 294 pages)

**Citation**: Edelsbrunner, H. and Harer, J. *Computational Topology: An Introduction*. American Mathematical Society, 2010. Departments of Computer Science and Mathematics, Duke University.

**Relevance**: Comprehensive textbook covering the mathematical foundations of computational topology — simplicial complexes, homology, Morse theory, persistent homology, and Reeb graphs. Directly relevant to our boolean pipeline's topological validity: simplicial complex theory (Ch. III) underpins mesh arrangement data structures; homology (Ch. IV) provides invariants for verifying shell closure (Euler characteristic, Betti numbers); Morse theory (Ch. VI) relates to critical point analysis on surfaces; persistent homology (Ch. VII) offers tools for robustness analysis of geometric features across scales.

**Key chapters**:
- **Part A** (Geometry): Graphs, Surfaces, Complexes (simplicial complexes, Delaunay, alpha complexes)
- **Part B** (Topology): Homology, Duality (Poincaré, Alexander), Morse Theory (smooth/piecewise-linear)
- **Part C** (Algorithms): Persistence, Reeb Graphs

### 32. Piegl & Tiller — "The NURBS Book" (2nd Edition, 1997)

**Access**: Springer (purchased). ISBN 978-3-540-61545-3.

**Local copy**: `refs/piegl_tiller1997_nurbs_book.pdf` (646 pages, full text extraction: 32,703 lines)

**Citation**: Piegl, L. and Tiller, W. *The NURBS Book*. 2nd ed. Springer, Monographs in Visual Communications, 1997.

**Relevance**: **The standard reference for NURBS implementation.** Every algorithm needed for the geometry layer of a CAD kernel — evaluation, knot insertion, degree elevation, point inversion, surface construction, fitting — is presented with C-like pseudocode, geometric interpretation, and tested implementations. This is the implementation manual for our kernel's geometry layer.

**Chapter map for kernel development**:

- **Ch. 1: Curve and Surface Basics** (pp. 1-43) — Implicit vs parametric forms, Bezier curves, rational Bezier, tensor product surfaces. Foundation for understanding the representation.
- **Ch. 2: B-Spline Basis Functions** (pp. 47-78) — Definition, properties, derivatives, computational algorithms. The Cox-de Boor recursion and its stable evaluation.
- **Ch. 3: B-spline Curves and Surfaces** (pp. 81-116) — Definition, properties, derivatives for both curves and surfaces. The core data structures.
- **Ch. 4: Rational B-spline (NURBS) Curves and Surfaces** (pp. 117-138) — NURBS definition, properties, derivatives. The generalization from B-splines to rational forms. Homogeneous coordinate representation.
- **Ch. 5: Fundamental Geometric Algorithms** (pp. 141-227) — **Critical chapter.** Knot insertion (Algorithm A5.1), knot refinement, knot removal, degree elevation (A5.9), degree reduction. These are the building blocks for subdivision, Bezier extraction, and surface splitting — all needed for SSI and boolean face division.
- **Ch. 6: Advanced Geometric Algorithms** (pp. 229-279) — **Critical chapter.** Point inversion and projection (A6.1-A6.5), surface tangent vector inversion, reparameterization, curve/surface reversal. Point inversion is the operation truck's `search_parameter` implements — this chapter provides the correct algorithms with convergence analysis.
- **Ch. 7: Conics and Circles** (pp. 281-330) — Conic arcs as rational quadratics, circle construction. Relevant for representing cylinder/cone/sphere edges exactly.
- **Ch. 8: Construction of Common Surfaces** (pp. 333-351) — Bilinear, cylinders, ruled surfaces, surfaces of revolution, scaling. Direct construction of the primitive surfaces in mechanical CAD (cylinder, cone, sphere, torus).
- **Ch. 9: Curve and Surface Fitting** (pp. 361-452) — Global/local interpolation, least squares approximation, constrained fitting. Needed for fitting NURBS curves to intersection curve sample points (IC edge healing).
- **Ch. 10: Advanced Surface Construction** (pp. 455-496) — Skinning, sweeping, Coons surfaces, curve network interpolation. Feature creation operations.
- **Ch. 11: Shape Modification Tools** (pp. 509-562) — Control point repositioning, weight modification, warping, bending, constraint-based shaping. Parametric editing.
- **Ch. 12: Standards and Data Exchange** (pp. 571-586) — IGES, STEP, PHIGS. File format interoperability.
- **Ch. 13: B-spline Programming Concepts** (pp. 593-616) — Data types, data structures, memory allocation, error control, utility routines. Implementation architecture.

**Key algorithms for our kernel** (by algorithm number):
- **A2.1**: FindSpan — locate knot span for parameter value
- **A2.2/A2.3**: BasisFuns / DersBasisFuns — evaluate basis functions and derivatives
- **A3.1/A3.5**: CurvePoint / SurfacePoint — evaluate curve/surface at parameter
- **A3.2/A3.6**: CurveDerivsAlg1 / SurfaceDerivsAlg1 — compute derivatives
- **A5.1**: CurveKnotIns — knot insertion (Oslo algorithm)
- **A5.3**: SurfaceKnotIns — surface knot insertion
- **A5.4**: RefineKnotVectCurve — knot refinement
- **A5.9**: DegreeElevateCurve — degree elevation
- **A6.1**: CurvePointInversion — point inversion on curves (Newton iteration with convergence criteria)
- **A6.2**: SurfacePointInversion — point inversion on surfaces (Newton iteration in 2D parameter space)
- **A9.1**: GlobalCurveInterp — global curve interpolation through points

## Recent Advances in Curved Boolean Operations (2019–2026)

### 24. Yang, Jia & Yan — "Boolean Operations on NURBS B-Rep Models via a Hybrid Approach" (2025)

**Access**: SIGGRAPH 2025. PDF available at author's page:
https://yangjieyin.github.io/homepage/

**Local copy**: `refs/yang2025_hybrid_boolean.pdf`

**Citation**: Yang, J., Jia, X. and Yan, D.-M. "Boolean Operations for Solids Represented by NURBS B-Rep via a Hybrid Approach." ACM Transactions on Graphics (SIGGRAPH 2025).

**Relevance**: **The most important reference for our kernel redesign.** Solves the fundamental problem that has plagued our pipeline: performing reliable boolean operations on NURBS B-rep without degrading to mesh approximations. Uses bijective mesh↔B-Rep mapping to get the best of both worlds — exact mesh boolean for topological decisions, NURBS surfaces for geometric accuracy. Zero failures on their test suite, 17× faster than OCCT, supports chained booleans via re-mapping.

**Six-stage pipeline**:
1. **Tessellation**: Convert B-Rep faces to triangulated mesh with bijective mapping (each mesh triangle maps to exactly one B-Rep face, preserving parametric coordinates).
2. **Mesh Boolean**: Apply Cherchi et al. 2022 exact mesh boolean (indirect predicates, no perturbation, no general position assumptions). This produces the topologically correct result mesh.
3. **Topology Extraction**: From the result mesh, extract the topological structure — which original faces survive, how they are trimmed, which new edges are created. The bijective mapping makes this unambiguous.
4. **SSI Refinement**: For intersection edges that lie on curved surfaces, refine the mesh-derived approximation to true surface-surface intersection curves using tangent-plane optimization. Key insight: the mesh boolean gives the correct topology (which faces intersect, the combinatorial structure of the result), and this stage only improves geometric accuracy.
5. **Trim Curve Construction**: Build NURBS trim curves on the original surfaces from the refined intersection curves. The original surfaces are never re-approximated — only re-trimmed.
6. **B-Rep Assembly**: Assemble the final B-Rep from trimmed faces, inheriting the topology from the mesh boolean and geometry from the original NURBS surfaces.

**Key insights for our work**:
- **Bijective mapping is the key enabler**: Every mesh vertex/edge/triangle maps to exactly one B-Rep vertex/edge/face and vice versa. This eliminates the "which face does this intersection point belong to?" ambiguity that causes our `add_polygon_vertex_capturing` failures.
- **Topology from mesh, geometry from NURBS**: The mesh boolean is guaranteed correct (exact predicates). The NURBS geometry is preserved (never tessellated-and-reconstructed). This separates the hard problem (topological correctness) from the tractable problem (geometric accuracy).
- **Chained booleans work**: After each boolean, re-tessellate the result and rebuild the bijective mapping. No accumulated error, no degraded face geometry.
- **Scale-independent**: Mesh boolean operates after unit-cube normalization. No absolute tolerances.
- **17× faster than OCCT** on their benchmark suite, with zero failures vs OCCT's non-trivial failure rate.

### 25. Yang, Jia & Yan — "Topology Guaranteed B-Rep Surface-Surface Intersection" (2023)

**Access**: ACM Digital Library (open access):
https://dl.acm.org/doi/abs/10.1145/3618349

**Local copy**: `refs/yang2023_topology_guaranteed_ssi.pdf`

**Citation**: Yang, J., Jia, X. and Yan, D.-M. "Topology Guaranteed B-Rep Surface-Surface Intersection." ACM Transactions on Graphics 42(6):240, 2023 (SIGGRAPH Asia).

**Relevance**: Solves the SSI topology determination problem — knowing the correct topology (number of branches, connectivity, singularities) of the intersection curve BEFORE tracing it. This eliminates the class of failures where our pipeline traces an intersection curve with the wrong topology (missing branches, incorrect connectivity at singular points).

**Five-stage algorithm**:
1. **Bounding box test**: Quick rejection for non-intersecting surface pairs.
2. **Implicitization via Dixon matrix**: Convert one parametric surface to implicit form using the Dixon resultant matrix. The Dixon matrix is more numerically stable than Sylvester/Bezout resultants for NURBS surfaces.
3. **Characteristic point computation**: Find singular points of the implicit curve in parameter space. These are the points where the intersection curve topology changes — branch points, cusps, self-intersections. Uses the GCD of the Dixon resultant and its partial derivatives.
4. **Topology determination**: From the characteristic points, determine the exact topology of each intersection branch — is it a loop? does it connect to the surface boundary? how many branches meet at each singular point?
5. **Curve tracing with topology guarantee**: Trace the intersection curve using the determined topology as a constraint. If tracing produces a result inconsistent with the determined topology, it is an error to be corrected, not accepted.

**Also introduces**: Redundant curve clipping — when the Dixon resultant introduces spurious algebraic curves (from the implicitization process), these are detected and removed before topology determination.

**Performance**: Successfully handles cases where OCCT, ACIS, and SolidWorks fail, including high-degree NURBS with multiple tangential contacts.

### 26. Yang & Jia — "Overlap Region Extraction for Two B-Rep Models" (2025)

**Access**: SIGGRAPH Asia 2025. PDF available at author's page:
https://yangjieyin.github.io/homepage/

**Local copy**: `refs/yang2025_overlap_region_extraction.pdf`

**Citation**: Yang, J. and Jia, X. "Overlap Region Extraction for Two B-Rep Models." ACM Transactions on Graphics (SIGGRAPH Asia 2025).

**Relevance**: **Directly solves our D1 (coplanar faces) problem.** Our entire D1 patch cascade (B14 through B26, ~3000 lines of code) was trying to force 1D intersection curve semantics onto what is fundamentally a 2D phenomenon. When two surfaces are coplanar or near-coplanar, their "intersection" is not a curve but a 2D overlap region bounded by a 1D curve. This paper provides the correct framework.

**Key insight — overlap is 2D with 1D boundary**: When two NURBS surfaces are coplanar (or nearly so), standard SSI algorithms fail because they search for a 1D intersection curve, but the intersection is a 2D surface region. The correct approach is:
1. **Detect** that the surfaces overlap (not just intersect along a curve).
2. **Compute the overlap boundary** — the 1D curve where one surface's trim boundary crosses the other surface. This is a 2D problem in the shared parameter space, not a 3D SSI problem.
3. **Extract the overlap region** as a bounded 2D domain.

**Bilevel optimization**: The overlap boundary is computed via bilevel optimization. The outer level finds points on surface A's trim boundary that lie on surface B (a constrained optimization in A's parameter space). The inner level projects these points onto surface B (closest-point projection). The boundary curve is then traced in both parameter spaces simultaneously.

**Why our D1 approach was wrong**: We tried to compute coplanar "intersection curves" by projecting trim boundaries, computing 2D polygon booleans (iOverlay), and injecting overlay fragments. This failed because:
- The projection step loses the parametric connection between surfaces
- `add_independent_loop` creates topologically isolated wires (no vertex sharing)
- The boundary curve needs to be a shared topological entity, not independent geometry
Yang's bilevel approach computes the boundary curve as a shared entity from the start.

**Implications for kernel design**: Overlap detection should be a **preprocessing step** before SSI, not a special case within it. The overlap boundary is computed with different algorithms than transversal intersection curves.

### 27. Li, Yang & Jia — "Surface-Surface Intersection Computation: A Survey with New Results" (2026)

**Access**: Computer-Aided Design (Elsevier), 2026.

**Local copy**: `refs/li2026_ssi_survey.pdf`

**Citation**: Li, J., Yang, J. and Jia, X. "Surface-Surface Intersection Computation: A Survey with New Results." Computer-Aided Design, 2026.

**Relevance**: The most comprehensive and current survey of SSI methods. Covers taxonomy, algorithms, open problems, and comparative benchmarks. Validates the hybrid approach (#24) as the state of the art.

**SSI method taxonomy**:
- **Lattice methods**: Subdivide parameter domains into grids, find approximate intersection points, connect into curves. Simple but resolution-dependent. OCCT's IntPatch uses a variant.
- **Marching methods**: Start from a seed point, march along the intersection curve using differential geometry (tangent vector = n₁ × n₂). Fast but can miss disconnected branches and has trouble at singular points.
- **Subdivision methods**: Recursively subdivide both surfaces until patches are sufficiently flat, then intersect the flat approximations. Robust but slow. Bezier clipping is a refined variant.
- **Algebraic methods**: Implicitize one surface (Dixon/Sylvester resultant), reduce to curve tracing in 2D. Exact topology but high algebraic degree. Yang 2023 (#25) is the state of the art here.
- **Hybrid methods**: Combine two or more approaches. The recommended architecture: fast numerical method for easy cases, algebraic fallback for degenerate cases.

**Comparative benchmarks** (Table 5 in the paper):
- OCCT fails on 4/8 test cases (high-degree tangential, self-intersecting, near-degenerate)
- ACIS fails on 2/8 test cases
- SolidWorks fails on 3/8 test cases
- Yang 2023 (#25) succeeds on all 8/8

**Open problems identified**:
1. Robust singular point handling (cusps, branch points)
2. Near-tangential intersection (the hardest case numerically)
3. Self-intersecting surfaces
4. High-degree NURBS (degree > 6)
5. Performance at interactive rates for complex models
6. Integration of SSI with boolean pipeline (topology preservation across the full pipeline)

**Key recommendations from the survey**:
- No single SSI method handles all cases — hybrid architecture is necessary
- Algebraic topology determination (#25) should be used as preprocessing for numerical tracing
- Scale normalization to unit cube is essential before any numerical SSI
- Adaptive precision (exact predicates with floating-point filters) is the correct robustness strategy

### 28. Urick, Marussig, Hughes, Riesenfeld & Cohen — "Watertight Boolean Operations: A Framework for Creating CAD-compatible Gap-free Editable Solid Models" (2019)

**Access**: Computer-Aided Design (Elsevier), 2019.

**Local copy**: `refs/urick2019_watertight_booleans.pdf`

**Citation**: Urick, B., Marussig, B., Hughes, T.J.R., Riesenfeld, R.F. and Cohen, E. "Watertight Boolean Operations: A Framework for Creating CAD-compatible Gap-free Editable Solid Models." Computer-Aided Design 115:80–98, 2019.

**Relevance**: Addresses the gap problem in NURBS boolean results — after trimming, adjacent faces don't share exact edge geometry, creating gaps that cause problems for downstream operations (meshing, analysis, chained booleans). Their solution: reparameterize trimmed faces so that intersection curves become isoparametric lines (knot lines) of untrimmed patches.

**Three-stage process**:
1. **SSI computation**: Standard surface-surface intersection to find intersection curves.
2. **Isocurve reparameterization**: For each trimmed face adjacent to an intersection curve, reparameterize the surface so that the intersection curve becomes an isoparametric curve (constant u or constant v). The reparameterized surface is untrimmed along that edge — no trim curve needed, so no gap.
3. **Result assembly**: Assemble the reparameterized patches into a watertight B-Rep. Adjacent faces share exact edge geometry by construction (both use the same isoparametric curve as their common edge).

**Key insight for our work**: Traditional NURBS booleans produce trimmed surfaces with approximate trim curves. Two adjacent trimmed faces approximate the intersection curve independently — their trim curves are close but not identical, creating gaps. The reparameterization approach eliminates this by making the intersection curve part of the surface parameterization itself.

**Limitations**: Reparameterization can distort the surface parameterization, affecting surface quality. Works best for simple intersection topologies. Complex intersection patterns (many branches, tangential contacts) make reparameterization impractical. The hybrid approach (#24) handles these cases better by keeping original surface parameterizations and relying on exact mesh topology.

**Relevance to our kernel**: Useful for simple-topology intersections (planar cuts, cylinder-plane) where reparameterization is straightforward. For complex cases, the hybrid approach (#24) is more appropriate. Could be a post-processing step to improve geometric quality of boolean results.

### 29. Cheng, Zhang, Xiao & Li — "Topology-Driven Approximation to Rational Surface-Surface Intersection via Interval Algebraic Topology Analysis" (2023)

**Access**: ACM open access:
https://dl.acm.org/doi/10.1145/3592452

**Local copy**: `refs/cheng2023_topology_driven_ssi.pdf`

**Citation**: Cheng, J.-S., Zhang, B., Xiao, Y. and Li, M. "Topology-driven approximation to rational surface-surface intersection via interval algebraic topology analysis." ACM Transactions on Graphics 42(4):38, 2023 (SIGGRAPH).

**Relevance**: A hybrid symbolic-numeric SSI framework that determines intersection topology BEFORE tracing, using interval algebraic topology analysis (IATA) in the 4D parameter space. Handles all the hard cases: tangent points (isolated or not), tangent curves, tiny loops, self-intersections, partial surface coincidences, and mixed combinations. Compared favorably against IRIT, SISL, and Parasolid — correct topology on 15/16 benchmark cases vs. partial failures in all three competitors.

**Key innovation — IATA (Interval Algebraic Topology Analysis)**: Maps SSI singularities from 3D physical space to algebraic system solutions in 4D parameter space (s,t,u,v). The SSI curve C is viewed as the image of a 4D implicit curve Ψ = {R₁(s,t) - R₂(u,v) = 0} under projection. Singularities of C correspond to singularities of Ψ, which can be classified and computed algebraically.

**Three fundamental singular cases + regular**:
- **C₀ (regular)**: Non-zero tangent vector TΨ in at least one component. Simple connected curve segments. Monotonicity analysis suffices.
- **C₁ (surface singularity)**: Cusp points/curves on one surface. Detected via Σ₁ = ∂R₁/∂s × ∂R₁/∂t = 0. The 4D preimage is regular on Ψ even though the 3D projection is singular — so C₁ singularities can be computed as regular zeros.
- **C₂ (tangency)**: Two surfaces tangent — normals are collinear. TΨ = 0 at these points. The hardest case. Sub-cases: isolated tangent points, tangent curves, tiny loops, partial coincidence, and mixtures. Solved via **deflation** — transform singular zeros into regular zeros of an over-determined system Φ = Ψ ∩ TΨ.
- **C₃ (self-intersection)**: Two distinct 4D points map to the same 3D point. 6D algebraic system. Theorem 3 provides injectivity conditions to rule out self-intersection within a box.

**Deflation technique for C₂**: The key challenge is that tangent points are singular zeros of both Ψ and its tangent vector. Deflation adds the tangent vector equations TΨᵢ = 0 to form an over-determined system where the singular zero becomes regular. For isolated tangent points and tiny loops (which co-exist with other singularities), Lemma 2 constructs a regulated system Φ whose regular curve passes through both isolated singularities and tiny loops, enabling their robust detection.

**Overall algorithm (Algorithm 1)**:
1. Construct BVHs for both surfaces, find candidate box pairs
2. Classify each 4D box as C₀/C₁/C₂ by checking function system zeros
3. Resolve C₂ boxes first (most complex) — compute singularity sub-cases via decision trees
4. Trace regular branches through C₀ boxes from boundary points and C₂-derived starting points
5. Resolve C₁ singularities globally
6. Resolve C₃ self-intersections
7. Link traced point sequences, optionally fit B-splines

**Performance**: Implemented in C++ with GPU for initial box generation. Slower than competitors on easy cases (6s vs 0.01-0.8s for regular multi-loop) but correct on hard cases where others fail. Currently limited to rational Bézier surfaces (degree 3-7); extension to general NURBS noted as future work.

**Relevance to our kernel**: Complementary to Yang 2023 (#25) — both determine SSI topology before tracing, but via different approaches. Cheng uses interval algebraic analysis in the 4D parameter box; Yang uses Dixon matrix implicitization. Cheng handles self-intersecting surfaces (C₃) which Yang does not address. The deflation technique for detecting isolated tangent points and tiny loops is novel and directly applicable to our tangential intersection failures (D4). The interval Newton certification of solution existence/uniqueness provides rigorous topology guarantees. Could serve as the algebraic fallback in a hybrid SSI architecture.

### 30. Spainhour & Weiss — "Robust Containment Queries over Collections of Trimmed NURBS Surfaces via Generalized Winding Numbers" (2026)

**Access**: ACM open access:
https://dl.acm.org/doi/10.1145/3797957

**Implementation**: BSD-licensed, in LLNL's Axom library:
https://github.com/llnl/axom

**Local copy**: `refs/spainhour2026_gwn_trimmed_nurbs.pdf`

**Citation**: Spainhour, J. and Weiss, K. "Robust Containment Queries over Collections of Trimmed NURBS Surfaces via Generalized Winding Numbers." ACM Transactions on Graphics, 2026. DOI: 10.1145/3797957.

**Relevance**: **Critical reference for our kernel's face classification.** The first algorithm for evaluating generalized winding numbers (GWN) directly on trimmed NURBS surfaces — no tessellation, no point clouds, no mesh approximation. This directly solves the inside/outside classification problem that has driven the majority of our boolean pipeline bugs (B14 through B27). The GWN degrades gracefully for non-watertight geometry, eliminating the class of misclassifications that arise from tessellation-dependent evaluation.

**Core technique — Stokes' theorem dimension reduction**: The 2D surface integral for solid angle (GWN definition) is reduced to a 1D line integral along the trimming curve boundaries of each NURBS patch. This provides both efficiency (O(n) vs O(n²) quadrature nodes) and stability (near-singular behavior localized to boundary curves, not entire surface). A half-integer correction term accounts for the GWN field's jump discontinuity across surfaces.

**Three-case evaluation**:
- **Far-field**: Query point outside surface AABB → direct boundary integral only. Cheap. >90% of point-surface pairs in practice.
- **Near-field**: Query inside AABB → boundary integral + correction via line-surface intersection test. Each signed intersection adds ±0.5.
- **Edge cases**: Intersection near boundary curves, cusps, or tangencies → extract parametric disk, reprocess recursively.

**Key techniques for our kernel**:
1. **GWN-based face classification**: Replace tessellation-based winding number + ray-cast voting with direct NURBS GWN evaluation. Eliminates discretization-dependent misclassification. For each face fragment, pick interior point, sum GWN against other solid's patches. GWN > 0.5 → inside.
2. **2D GWN for trim testing**: Uses 2D generalized winding numbers (from Spainhour et al. 2024) to determine whether parametric points lie within trimmed face regions. More robust than edge-crossing parity tests when trimming curves have gaps.
3. **GARP for line-surface intersection**: Recursive NURBS subdivision to approximately-bilinear sub-patches, solved with Reshetov 2019 closed-form bilinear intersection. Avoids Newton convergence failures near tangencies.
4. **Parametric disk extraction**: Isolates problematic regions (cusps, boundary intersections) by splitting along small parametric circles. General-purpose degenerate-case handler.
5. **Tolerance architecture**: Three independent, relative/user-controlled tolerances (quadrature εq, intersection εlsi, disk radius) — superior to truck's single absolute 1e-6.
6. **Memoization**: Quadrature nodes cached per trimming curve, reused across all classification queries for the same solid. Natural for boolean pipeline where many fragments classify against same patches.

**Coincident point handling**: For points ON the surface, GWN is defined as average of two one-sided limits → 0.5 at single-layer surfaces. Correction terms cancel. Provides principled coplanar face classification without heuristics.

**Performance**: 0.004ms (far-field) to 94ms (edge cases) per query. Feasible for boolean pipeline classifying ~10-100 face fragments.

### 31. Li, Jia & Chen — "Fast Determination and Computation of Self-intersections for NURBS Surfaces" (2025)

**Access**: ACM open access (CC-BY 4.0):
https://dl.acm.org/doi/10.1145/3727620

**Local copy**: `refs/li2025_nurbs_self_intersection.pdf`

**Citation**: Li, K., Jia, X. and Chen, F. "Fast Determination and Computation of Self-intersections for NURBS Surfaces." ACM Transactions on Graphics 44(2):18, 2025. DOI: 10.1145/3727620.

**Relevance**: Fast algebraic self-intersection detection for NURBS surfaces. The algebraic signature — computed purely from control points — certifies self-intersection-freedom in microseconds. Useful as pre-boolean validation and for detecting problematic faces before SSI. Also validates unit-cube normalization as standard practice.

**Key innovation — algebraic signature**: For a NURBS patch, the determination function f(u,v) = (Sᵤ × Sᵥ) · N(u₀,v₀) is expressed in B-spline basis. If all coefficients cₖₗ ≥ 0, the surface normal stays within a half-sphere → no self-intersection possible. Computed from control point differences and cross products — no surface evaluation needed.

**Recursive subdivision algorithm**:
1. Compute algebraic signature. If all non-negative → certified self-intersection-free.
2. Subdivide at two nearby interior knots (avoids shared-boundary ambiguity).
3. Cross-check complementary sub-patches for mutual intersection (OBB → control net → Newton).
4. Recurse on each sub-patch. Terminate at tolerance 10⁻⁶ after unit-cube normalization.

**Useful techniques for our kernel**:
1. **Pre-boolean validation**: Check input faces for self-intersection before entering SSI pipeline.
2. **Unit-cube normalization**: Confirms our B27 `compute_scale_normalization` approach. Paper uses ε=10⁻⁶ geometric, ε₁=10⁻³ flatness after normalization.
3. **Loop detection via collinear normals**: 4-equation system (Eq. 8) detects small intersection loops that marching methods miss. Applicable to tangential IC detection (D4).
4. **Flatness bound (Theorem C.3)**: Tight upper bound on patch-to-plane distance from second-order control point differences. Useful for adaptive tessellation/subdivision criteria.
5. **Subdivision avoiding shared boundaries**: Cutting at two nearby knots instead of midpoint prevents degenerate shared-boundary configurations.

**Comparison**: 2-4 orders of magnitude faster than OCCT and ACIS for simple surfaces. More robust on degenerate cases (near-tangential self-intersections, small loops, closed surfaces).

### 33. Stroud — "Boundary Representation Modelling Techniques"

**Local copy**: `refs/stroud2006_brep_modelling_techniques.pdf`

**Citation**: Stroud, I. "Boundary Representation Modelling Techniques." Springer-Verlag, London, 2006. ISBN: 978-1-84628-312-3.

**Relevance**: **HIGH**. The comprehensive textbook on B-rep solid modelling implementation. Written by a practitioner who worked on BUILD (Cambridge), GPM, and ACIS-era systems. Covers datastructures, Euler operators, boolean operations, sweeping, feature modelling, tolerances, and debugging — with algorithms given as stepwise sequences of Euler operators that preserve topological validity throughout. Uniquely practical: describes not just what algorithms do, but implementation pitfalls, special cases, error recovery, and design decisions.

**Chapter map (key chapters for our kernel)**:

- **Ch. 3 — Datastructures and tools**: B-rep entity hierarchy (vertex, edge, loop-edge link, face, loop, facegroup, shell, body). Topological connections, traversal algorithms (all faces at vertex, all edges in face, etc.), geometric interrogations (point-in-face, curve-face intersection classification). Assembly structures with instances and constraints. **Key for designing our kernel's data structures.**
- **Ch. 4 — Euler operators**: The 99 possible Euler operators, spanning sets (MEV, MEF, MBFV, MGB, MEKH + inverses), and decomposition of any topological change into Euler operator sequences via matrix inversion. Euler-Poincaré formula: v-e+f-h=2(b-g). **Key for implementing topology-preserving kernel operations.**
- **Ch. 5 — Special representations**: Non-manifold, degenerate, and partial (open) models. Conversion between representations. Boolean operations on non-manifold models (ambiguity issues). Compound models with wedge-based structures.
- **Ch. 6 — Stepwise modelling algorithms**: The core chapter.
  - §6.1 Boolean operations (pp. 126–141): Face-face comparison with AABB hierarchy, SSI→curve-face intersection with ENTERS/LEAVES/INOSCUL/OUTOSCUL/WIRE/ONEDGE classification, wire-edge sewing, coincident surface handling (imprinting), merging via A IN B / A OUT B / A ON B decomposition, shell separation, point-in-body containment test. Assembly booleans. Local (progressive) booleans.
  - §6.2 Sweeping/swinging (extrude/revolve): Edge-based improved sweep with movable-edge detection, special cases (axis edges, wire edges, faces swept past neighbors).
  - §6.4 Gluing coincident topology: Identical face gluing, edge gluing, vertex gluing.
  - §6.8 Planar sectioning.
  - §6.9 Imprinting.
  - §6.11 SETSURF (surface replacement in a face with edge/vertex recalculation).
  - §6.13 Gluing non-matching faces (local boolean with imprinting).
- **Ch. 7 — Modelling operator definition**: Task decomposition methodology for designing new operations. Pathological conditions analysis. Detailed worked examples: slot/pocket addition, edge rebating. **Key for our feature engine design.**
- **Ch. 9 — Feature modelling**: Feature datastructures (facesets, frames), design-by-features, feature recognition, feature verification. Relevant to our parametric feature engine.
- **Ch. 13 — Free-form geometry**: Bézier, B-spline, NURBS curves and surfaces. Interpolation, lofting, multi-patch SETSURF, surface sculpting. Complements Piegl & Tiller (#32) with modelling-oriented perspective.
- **Ch. 14 — Applications**: Model verification (§14.1, ACIS body checker approach: edge convexity, face containment, dangling vertices, self-intersection), model healing (§14.2: topological, geometric, combined), STL→B-rep recreation. **Key for our model validation layer.**
- **Ch. 16 — Tolerances and debugging**: Six tolerance types (TOLABS machine, TOLEPS length, TOLANG angle, TOLGEO geometric, TOLREL relative, TOLPOL polynomial). Consistency rules for tolerance usage. Error handling with return codes and tracebacks.
- **Appendix A — Data definitions**: Complete datastructure specifications for all B-rep entities (vertex, edge, loop-edge link, face, loop, facegroup, shell, wireframe/sheet/volume objects, point, curve, surface, instance, feature, etc.). Geometry format specifications (plane, sphere, cylinder, cone, quadric, toroid, free-form).

**Key insights for our kernel**:

1. **Euler operators as building blocks**: All complex operations decomposed into Euler operator sequences. Any valid B-rep change can be expressed as a sequence of MEV, MEF, MBFV, MGB, MEKH + inverses. This ensures topological validity is maintained incrementally — never create an invalid intermediate state. The matrix decomposition (§4.1) provides a systematic way to verify operator sequences.
2. **Stepwise boolean approach**: Wire edges are added to faces during SSI (maintaining validity at each step), then sewn into the boundary. This is different from truck's approach (which builds topology all at once). The incremental approach makes error recovery possible — if a face-face comparison fails, the model is still valid up to that point.
3. **Curve-face intersection classification** (§6.1.1): The 6-type classification (ENTERS/LEAVES/INOSCUL/OUTOSCUL/WIRE/ONEDGE) with sequencing rules provides a complete framework for intersection boundary creation. ONEDGE handling (partial edge coincidence) directly addresses our D2 coincident-edge degeneracy.
4. **A ON B handling**: Coincident surfaces are resolved by imprinting each face on the other to produce matching sub-faces. This is the classical approach to our D1 coplanar degeneracy.
5. **Bulletin Board technique**: ACIS-style change recording for undo/recovery. Copy-before-modify for operation rollback. Important for our feature engine's rebuild error recovery.
6. **Local booleans**: Progressive face-pair traversal (follow intersection boundaries to neighboring faces) instead of all-pairs comparison. More efficient and naturally handles the locality of typical CAD operations.
7. **Model verification** (§14.1): Systematic checks — edge convexity, face containment, dangling elements, self-intersection. ACIS uses resabs distance tolerance for all checks. Comprehensive validation is essential for kernel reliability.

## Tessellation References

### 34. Dassi, Mola & Si — "Curvature-Adapted Remeshing of CAD Surfaces" (2014/2018)

**Access**: Free PDF (conference version):
https://www.sandia.gov/imr/Papers/20_IMR23_Dassi.pdf

Also: https://files01.core.ac.uk/download/pdf/198237553.pdf

**Citation**: Dassi, F., Mola, A. and Si, H. "Curvature-adapted remeshing of CAD surfaces." *Procedia Engineering* 82:253–265, 2014 (23rd International Meshing Roundtable). Journal version: *Engineering with Computers* 34:565–576, 2018.

**Relevance**: Curvature-adaptive surface remeshing via R^6 embedding. Each surface point p with unit normal n is lifted to (p, lambda*n) in R^6, where lambda controls curvature sensitivity. A mesh that is "uniform" in R^6 is automatically curvature-adapted in R^3 — flat regions get large triangles, curved regions get small ones. Local mesh modifications (split/collapse/flip/smooth) are evaluated against the R^6 metric. Simpler than metric-tensor approaches and directly applicable to truck's existing `PolygonMesh`.

### 35. Aubry, Dey, Mestreau, Karamete & Gayman — "A Robust Conforming NURBS Tessellation for Industrial Applications Based on a Mesh Generation Approach" (2015)

**Access**: Elsevier (paywall):
https://www.sciencedirect.com/science/article/abs/pii/S0010448515000032

**Citation**: Aubry, R., Dey, S., Mestreau, E.L., Karamete, B.K. and Gayman, D. "A robust conforming NURBS tessellation for industrial applications based on a mesh generation approach." *Computer-Aided Design* 63:26–38, 2015.

**Relevance**: Industrial-grade NURBS tessellation framed as mesh generation. Uses anisotropic metric tensors from principal curvatures to drive triangle aspect ratios. Guarantees watertight conformity between adjacent NURBS patches by meshing shared boundary curves once in 3D. Handles NURBS degeneracies (collapsed edges, seam lines, singular poles) common in naval and aerospace CAD. More rigorous than #34 for production tessellation, but requires curvature tensor computation infrastructure.

## Surface Architecture References

### 36. Parasolid — XT Format Reference & Surface Type Architecture

**Access**: Siemens Parasolid documentation (various sources):
- XT Format Reference (v35): http://www.q-solid.com/Parasolid_Docs_V35/pdf/xt.pdf
- Format overview: https://cadexchanger.com/blog/3d-formats-overview-parasolid/
- Faceting API: http://www.q-solid.com/Parasolid_Docs/chapters/fd_chap.55.html
- Convergent Modeling: https://news.siemens.com/en-us/parasolid-convergent-modeling-mixed-models/

**Citation**: Siemens Digital Industries Software. "Parasolid XT Format Reference." Version 35, 2023.

**Relevance**: Parasolid's 3-tier surface hierarchy is the industry standard that informs our SurfaceGeom design. Tier 1: five analytic surfaces (plane, cylinder, cone, sphere, torus) stored as compact parameter sets. Tier 2: procedural surfaces (swept, spun, offset) stored as construction recipes (profile + spine + orientation law). Tier 3: B-surface (NURBS) as universal fallback. Procedural definitions are preserved natively — never eagerly converted to NURBS — enabling exact reconstruction during editing and maintaining analytical SSI eligibility. Convergent Modeling (v26+) extends the architecture to mixed mesh+analytic bodies, supporting boolean operations between faceted and parametric geometry. The faceting API (PK_TOPOL_facet) provides chordal tolerance, angular tolerance, and surface-type-aware tessellation strategies. This architecture directly validates our ADR-11 decision for lazy upward conversion.

### 37. Mistry, Sarkar & Ranjan — "Swept Volume B-Rep Computation" (2015)

**Access**: Open access (CAD Journal):
https://www.cad-journal.net/files/vol_12/CAD_12(2)_2015_181-191.pdf

**Citation**: Mistry, J.B., Sarkar, A.K. and Ranjan, R. "Swept Volume B-Rep Computation." Computer-Aided Design and Applications 12(2):181–191, 2015.

**Relevance**: Formalizes swept solid construction as profile + spine + orientation law, producing B-Rep boundaries directly without tessellation intermediate. The construction recipe consists of: a 2D cross-section profile (wire or face), a 3D spine curve, and an orientation law (Frenet, fixed, or user-specified frame propagation). The resulting surface is computed by transporting the profile along the spine, with the orientation law controlling twist. Self-intersection detection at concave spine regions prevents invalid sweeps. Directly applicable to our Tier 2 procedural surface representation — the (profile, spine, orientation) triple is the stored definition for swept surfaces.

## How to Reference During Development

When working on boolean reliability or kernel improvements:

1. **Start with Yang (#24)** for the target architecture — the hybrid B-Rep/mesh pipeline is the state of the art for NURBS boolean operations. This is the blueprint for our kernel redesign.
2. **Start with Hoffmann Ch. 3** for the algorithmic framework — understand the overall pipeline before diving into specifics
3. **Study OpenCascade docs (#3)** for production-validated degenerate handling — the staged interference approach (VV→VE→EE→VF→EF→FF) and pave block data structures
4. **Use Patrikalakis Ch. 5** for intersection algorithm details — when you need to understand or improve how two surfaces find their intersection curve
5. **Use Patrikalakis Ch. 4** for robustness — when debugging numerical issues or tolerance problems
6. **Use Hoffmann Ch. 4** for error analysis — when trying to understand why geometric operations fail
7. **Use Jacobson (#7) + Zhou (#8)** for classification — winding numbers are the correct replacement for our ray-cast majority voting
8. **Use Sugihara & Iri (#6)** for architectural guidance — topology-first algorithm design eliminates the need for perturbation
9. **Use Edelsbrunner & Mucke (#5)** for degenerate handling — Simulation of Simplicity eliminates all degenerate-case special handling
10. **Use ESOLID (#13) or Sheng (#14)** for curved surface intersection — hybrid NURBS/mesh approach for robust booleans
11. **Use Devillers & Preparata (#19)** for filter threshold computation — when adding new predicates or assessing whether floating-point is sufficient for a geometric decision
12. **Use Levy (#10) or Cherchi (#9)** for exact constructions without CGAL — arithmetic expansions for both predicates and constructed intersection points
13. **Use Barki (#12)** for co-refinement + radial-sort classification — the two-case reduction (orientable/non-orientable) simplifies classification logic
14. **Use Hachenberger (#15 EWCG)** for sphere-map overlay and Nef polyhedra — when investigating alternatives to our shell closure recovery
15. **Use Astarlioglu (#20)** for comparative analysis — quantitative robustness data across Tekla/CGAL/EMBER on standard test cases
16. **Use Yang (#25)** for SSI topology determination — when intersection curve tracing fails due to missing branches or incorrect connectivity at singular points
17. **Use Yang (#26)** for coplanar/overlap handling — when two surfaces overlap rather than intersect along a curve. Overlap detection should be preprocessing, not a special case.
18. **Use Li (#27)** for SSI method selection — comprehensive comparison of all SSI approaches with failure modes and benchmarks
19. **Use Urick (#28)** for watertight edge geometry — isocurve reparameterization for gap-free results on simple intersection topologies
20. **Use Cheng (#29)** for tangential/degenerate SSI — IATA framework with deflation handles isolated tangent points, tiny loops, and mixed singular cases that other methods miss
21. **Use Spainhour (#30)** for face classification — GWN directly on trimmed NURBS without tessellation. The correct replacement for ray-cast voting and mesh-based winding numbers. BSD-licensed Axom implementation available.
22. **Use Li (#31)** for pre-boolean validation — algebraic signature certifies self-intersection-freedom from control points in microseconds. Also: loop detection via collinear normals, flatness bounds, subdivision-avoiding-shared-boundaries
23. **Use Piegl & Tiller (#32)** for ALL NURBS geometry implementation — evaluation (A3.1/A3.5), knot insertion (A5.1/A5.3), point inversion (A6.1/A6.2), degree elevation (A5.9), curve fitting (A9.1). The implementation manual for the geometry layer.
24. **Use Stroud (#33)** for B-rep kernel architecture — datastructure design (Ch.3), Euler operator framework (Ch.4), stepwise boolean with wire-edge sewing (§6.1), sweep/revolve (§6.2), model verification (§14.1), tolerance design (Ch.16). The practitioner's guide to building a modelling system from scratch.
25. **Use Dassi (#34)** for curvature-adaptive tessellation — R^6 embedding (p, lambda*n) makes uniform meshes automatically curvature-adapted. Simpler than metric tensors, directly applicable to truck's PolygonMesh.
26. **Use Aubry (#35)** for industrial-grade NURBS tessellation — anisotropic metric tensors from principal curvatures, watertight conformity between adjacent patches, degeneracy handling for production CAD models.
27. **Use Parasolid (#36)** for surface type taxonomy — the 3-tier hierarchy (analytic, procedural, NURBS) is the industry-standard architecture. Informs SurfaceGeom enum design, lazy conversion strategy, and surface-type-aware tessellation.
28. **Use Mistry (#37)** for swept volume construction — profile + spine + orientation law formalization. Directly applicable to Tier 2 procedural surface representation.
