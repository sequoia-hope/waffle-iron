# 09 — File Format: Plan

## Open tasks

- [ ] Load-time hardening for shared files: bounds-check counts
      (`tooth_count`, entity and array lengths, `active_index`).
- [ ] Deduplicate `PreviewMesh`, which is defined in both `file-format` and
      `feature-engine`.
- [ ] Region size: a region's `outer` and `outer_edges` store the same
      boundary twice.

## Blockers

None.
