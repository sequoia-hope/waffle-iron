## Recommendations

### 1. Replace the burndown with a single `BACKLOG.md`

A "burndown" implies tracking velocity/dates, which is misleading for AI-assisted work where session length varies wildly. Instead, keep a flat prioritized list:

```markdown
# BACKLOG.md

## Active (pick from top)
- [ ] Boolean: box-cylinder at face boundaries (#truck)
- [ ] GUI: constraint tool tests (distance, angle, parallel)
- [ ] Test harness: revolve → boolean → tessellation scenarios

## Parked (intentionally deferred)
- [ ] Assemblies (Phase 7 — blocked on everything else)
- [ ] Fillet/chamfer/shell (DEFERRED INDEFINITELY)

## Done (move here, don't delete)
- [x] Fix orbit on empty space — gate BoxSelect on Shift key
- [x] Sprint 22: wire splitting + vertex dedup
```

**Why this works better:**
- No milestone numbers or dates to go stale
- "Pick from top" gives me clear priority without needing to parse a complex document
- You can reorder items between sessions to steer direction
- Done items accumulate as a record of progress

### 2. Your session workflow

**Normal session:** "Check the backlog and get to work"
- I read `BACKLOG.md`, pick the top unchecked item, enter plan mode, execute, mark done

**Spec expansion session:** "Analyze [spec/doc/idea] and expand the backlog"
- I read the spec, propose new items with rationale, you approve, I update `BACKLOG.md`

**Off-backlog session:** "Do [specific thing]" (no mention of backlog)
- I do the thing. At the end, you can say "add this to the backlog as done" or not — your call

### 3. Keep `CLAUDE.md` as the authority

`CLAUDE.md` already has the right structure — priorities, conventions, constraints. The backlog is just the *task queue*, not a second source of truth about what matters. If they ever conflict, `CLAUDE.md` wins.

### 4. Practical tips

- **Keep items small.** "Improve boolean reliability" is too big. "Fix box-cylinder boolean when cylinder touches face boundary" is one session.
- **Don't track sub-steps in the backlog.** That's what plan mode is for. The backlog item is the *goal*, the plan is the *how*.
- **Review the backlog periodically.** Ask me "review the backlog for stale/completed items" and I'll clean it up.
