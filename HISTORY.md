# History

Waffle Iron started on 2026-02-08 as an experiment by Sequoia Alexander: can coding agents, directed by one person, build a parametric CAD system good enough for everyday work?

The first versions wrapped an existing open-source B-Rep library. Its booleans were not reliable enough to build on, so the project wrote its own kernel. After a first attempt based on clipping and tolerances, the kernel was rebuilt around one published algorithm, Yang et al. 2025 (*Boolean Operation for CAD Models Using a Hybrid Representation*), with the rule that the paper is the specification and every departure from it is recorded. That kernel is the only one in the tree today.

Everything else about how the system got here is in the git history. The rest of the documentation describes how it works now.
