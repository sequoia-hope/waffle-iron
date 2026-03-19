# R6: SVG Rendering Conventions for 2D Constraint Systems

**Feeds into**: Wave 2 / Fork C (render)
**Priority**: Medium

## What We Know

We're building an SVG renderer for solved sketches. The spec doesn't
prescribe visual conventions — that's our design choice.

## What We Need

Visual conventions that make constraint system diagrams immediately
readable, both for humans and for LLM visual review (Gemini).

## Specific Questions

### Q1: How do production CAD sketchers render constraints?
- FreeCAD Sketcher: what do constraint annotations look like?
- Onshape: how do they show H/V, perpendicular, parallel, coincident?
- SolidWorks: constraint visualization conventions?
- Are there ISO/ASME standards for constraint visualization in 2D sketches?

### Q2: Color conventions
- What colors do CAD tools use for:
  - Fully constrained geometry (green? black? blue?)
  - Under-constrained geometry (blue? white?)
  - Over-constrained geometry (red? orange?)
  - Construction geometry (dashed? different color?)
  - Selected/highlighted geometry?
- Is there a de facto standard across CAD tools?

### Q3: Constraint symbol conventions
For each constraint type, what's the standard visual indicator?

| Constraint | Visual |
|-----------|--------|
| Horizontal | "H" or horizontal bar icon near line? |
| Vertical | "V" or vertical bar icon? |
| Parallel | "//" between the two lines? |
| Perpendicular | "⊥" at intersection? |
| Coincident | concentric circles at point? |
| Tangent | "T" or tangent symbol? |
| Equal | "=" between entities? |
| Symmetric | centerline marks? |
| Distance | dimension line with arrows and value? |
| Angle | arc with degree value? |
| Radius | "R" + value? |

### Q4: SVG best practices for geometric diagrams
- viewBox conventions for auto-sizing to content
- Recommended stroke widths, font sizes, marker sizes
- How to make SVGs that render well at different zoom levels
- Anti-aliasing considerations for thin lines
- Should we use SVG `<marker>` elements for arrowheads on dimension lines?

### Q5: LLM-readable diagrams
Since one purpose is Gemini visual review:
- What makes a diagram easy for an LLM to parse?
- High contrast? Large text labels? Entity ID annotations?
- Should we label every point with its ID?
- Should we include a legend?

## Desired Output

1. Color palette recommendation (hex values) for constraint status
2. Symbol/annotation convention for each constraint type
3. SVG template/structure recommendation
4. Example of what a rendered rectangle with H/V/Distance constraints
   should look like (ASCII art or description is fine)

## References

- FreeCAD Sketcher UI screenshots
- Onshape sketch mode screenshots
- ISO 128 (technical drawing conventions)
- SVG spec for `<marker>`, `<text>`, best practices
