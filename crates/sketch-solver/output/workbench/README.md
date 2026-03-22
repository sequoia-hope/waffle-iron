# Sketch Solver Workbench

Step-by-step demonstrations of the pure-Rust constraint solver on real CAD sketching scenarios. Each report shows progressive constraint application — from free geometry to fully constrained — with rendered output at every step.

## Scenarios

### [01 — Parametric Rectangle](01_parametric_rectangle/REPORT.md)
Four points connected into a closed loop, then constrained with horizontal/vertical, a pinned origin, and distance dimensions. Walks from 8 DOF down to 0.

### [02 — Bolt Circle](02_bolt_circle/REPORT.md)
Six circles arranged on a bolt circle pattern with equal-radius constraints. Demonstrates circle entities, radial placement, and equal-radius solving.

### [03 — Tangent Arc Transition](03_tangent_arc_transition/REPORT.md)
Two lines joined by a tangent arc. Covers arc entities, tangent-line-arc constraints, pinned base points, arc radius dimensioning, and symmetric-V layout.

### [04 — Symmetric Bracket](04_symmetric_bracket/REPORT.md)
A bracket profile with vertical symmetry. Uses horizontal/vertical constraints, symmetry about a centerline, and distance dimensions.

### [05 — Hex Bolt Head](05_hex_bolt_head/REPORT.md)
Regular hexagon constructed from six points with equal-length and angle constraints. Demonstrates rotational geometry and multi-constraint convergence.

### [06 — Slotted Plate](06_slotted_plate/REPORT.md)
Rectangular plate with slotted holes. Combines lines, arcs, tangent constraints, equal radii, and distance dimensions in a single sketch.
