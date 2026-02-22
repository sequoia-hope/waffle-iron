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

## How to Reference During Development

When working on boolean reliability or kernel improvements:

1. **Start with Hoffmann Ch. 3** for the algorithmic framework — understand the overall pipeline before diving into specifics
2. **Use Patrikalakis Ch. 5** for intersection algorithm details — when you need to understand or improve how two surfaces find their intersection curve
3. **Use Patrikalakis Ch. 4** for robustness — when debugging numerical issues or tolerance problems
4. **Use Hoffmann Ch. 4** for error analysis — when trying to understand why geometric operations fail

## Future References (if needed)

- **Mäntylä — "An Introduction to Solid Modeling"** — Deep dive on Euler operators and half-edge data structures. Buy if we need to restructure truck's topology layer.
- **Granados et al. — "Boolean Operations on 3D Selective Nef Complexes"** — Alternative boolean approach using Nef polyhedra. Relevant if we consider a fundamentally different boolean algorithm.
