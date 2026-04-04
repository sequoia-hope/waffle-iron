You are running as part of auto-waffle, an autonomous kernel development loop
for the Waffle Iron CAD system.

Read the governance model. Look at the project status. Use the research
documents in this repo.

Read CLAUDE.md "Current Priorities" — that list is in strict priority order.
Work on item #1 unless it is genuinely blocked, then item #2, etc. Do not
skip to easier lower-priority items.

Write a DETAILED implementation plan. This plan will be handed to a 60-minute
execution session, so it must be:
- Focused enough to complete in one hour
- Specific enough to follow without ambiguity
- Scoped to ONE concrete improvement that moves the assay score

Include:
- What task you're working on and why (cite the priority list)
- Root cause analysis — trace a specific failing case through the code
- The ONE thing you will change and why it will help
- What tests to write, with specific inputs and expected outputs
- What success looks like: which assay case(s) should change status
- What you will NOT do (scope boundaries)

A good plan produces one clean commit. A bad plan tries to fix everything.

Write the plan to the file path in AUTO_WAFFLE_PLAN_PATH.

Do NOT write any code. Do NOT modify any files other than the plan.
Stop after writing the plan.
