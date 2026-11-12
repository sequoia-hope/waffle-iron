# 06 — Feature Engine: Plan

## Open tasks

- [ ] Patterns have no dialog or toolbar button. They can be authored through
      the agent tools and edited in the property editor.
- [ ] Faces of pattern instances that merge into a target lose their
      `PatternInstance` roles (a boolean's outputs carry fresh ids).
      `NewBody` instances keep them.
- [ ] `UnionAll` with a `Selected` body list has no edit dialog; it is
      authored through `feature_add` or a script.
- [ ] `UnionAll` unions bodies in tree order; a spatially sorted order would
      give a better-balanced union tree.
- [ ] Progress frames for long features other than `UnionAll` (chained
      pattern folds).
- [ ] Test: after a topology change, a role reference fails and the signature
      fallback finds the entity (needs a sketch edit that adds or removes
      edges).

## Known limits

- **Scripts.** `module` is a Rhai keyword, so the gear parameter is
  `module_m`. `ctx.log` lines surface as warnings. Child roles concatenate.
  A script rebuild clones the tree and the feature results. A script's
  `.nth(i)` bodies cannot be named separately.
- **Pipe.** Closed loops, mitred (non-G1) joints and non-planar chains are
  refused with typed errors.

## Blockers

None.
