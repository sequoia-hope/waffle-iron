You are running as part of auto-waffle, doing a Yang 2025 pipeline review
pass on the Waffle Iron kernel codebase. Your job is to audit every step of
the Yang hybrid boolean pipeline against the paper and produce an honest
assessment of implementation completeness.

Read the governance model (Constitution, FIP, DoD, Architectural Invariants).
Read the Yang 2025 paper: `docs/references/yang2025_hybrid_boolean.txt`.

## Agent Teams (REQUIRED)

You are the Manager. You MUST use the TeamCreate tool to create an agent
team with 5 auditors. Do NOT use the Agent tool as a substitute.

Create teammates:
- **auditor-1**: Steps 1-4 (Discretization + Mesh Intersection, Yang 4.1-4.2)
- **auditor-2**: Steps 5-8 (Gauss Map + Optimization, Yang 4.2.2-4.3.3)
- **auditor-3**: Steps 9-12 (Refinement + Classification + Segmentation, Yang 4.3.4-4.4.2)
- **auditor-4**: Steps 13-16 (B-Rep Construction + Failure Recovery, Yang 4.4.1-4.5.1)
- **auditor-5**: Steps 17-20 (Mesh Refinement + Reversal + Coplanar, Yang 4.5.2-4.5.5)

All 5 auditors run IN PARALLEL. Each auditor:
1. Reads the Yang 2025 paper text for its assigned sections
2. Reads EVERY LINE of implementation code for its section
3. Checks the interface between its section and neighbors
4. Produces per-step verdicts: CORRECT / INCOMPLETE / WRONG / STUB

## Steps to Audit

| # | Yang Step | Section | Key Files |
|---|-----------|---------|-----------|
| 1 | Error-bounded discretization | 4.1.1 | tessellation/mod.rs, yang_integration.rs |
| 2 | Bijective parametric mapping | 4.1.1 | tessellation/bijective.rs |
| 3 | CDT boundary re-triangulation | 4.1.2 | tessellation/mod.rs |
| 4 | Mesh intersection (Cherchi) | 4.2 | cherchi/, exact_mesh.rs |
| 5 | Conservative 2*d_epsilon detection | 4.2.1 | cherchi/intersection_class.rs |
| 6 | Gauss map normal cone filtering | 4.2.2 | geometry/surface.rs, cherchi/intersection_class.rs |
| 7 | Newton/geometric optimization | 4.3.1-2 | intersection_opt.rs |
| 8 | Method selection by case | 4.3.3 | intersection_opt.rs |
| 9 | Curvature-based refinement | 4.3.4 | ssi_refinement.rs |
| 10 | Inside/outside classification | 4.4.2 | exact_mesh.rs |
| 11 | Cell selection per boolean op | 4.4.2 | topology_extract.rs, exact_mesh.rs |
| 12 | Flood-fill patch segmentation | 4.4.2 | topology_extract.rs |
| 13 | B-Rep construction from patches | 4.4.2 | topology_extract.rs |
| 14 | CDT mesh updating | 4.4.1 | ssi_refinement.rs |
| 15 | Topology validation | 4.4.3 | yang_integration.rs |
| 16 | Optimize across boundaries | 4.5.1 | intersection_opt.rs |
| 17 | Local mesh refinement | 4.5.2 | yang_integration.rs |
| 18 | Reversed curve correction | 4.5.3 | intersection_opt.rs |
| 19 | Self-intersection removal | 4.5.4 | yang_integration.rs |
| 20 | Coplanar preprocessing | 4.5.5 | coplanar_preprocess.rs, euler_ops.rs |

## Output

After all 5 auditors complete, compile their findings into a single report
and OVERWRITE the file `docs/audits/yang_2025_audit.md` (always the same
file, not dated — each review overwrites the previous).

The report must include:
1. Summary table with per-step verdicts
2. Detailed findings per agent
3. Critical issues in priority order
4. Verdict counts (CORRECT / INCOMPLETE / WRONG / STUB)

Commit the updated audit file. Do NOT push to remote.
