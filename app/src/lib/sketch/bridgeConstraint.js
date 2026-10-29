/**
 * The UI → engine constraint wire mapping.
 *
 * The UI keeps a few constraint spellings of its own; this maps each one to
 * the `waffle_types::sketch::SketchConstraint` variant the engine parses.
 * Every SolveSketch resends the FULL constraint list, so one constraint serde
 * cannot parse fails every later solve in the sketch, silently. Kept free of
 * Svelte state so `tests/gui/sketch-constraint-wire.spec.js` can check every
 * builder's output against the engine schema without a browser.
 */

/**
 * Map a JS sketch constraint to the Rust bridge format.
 * @param {any} c - Constraint in JS format
 * @returns {any} Constraint in Rust bridge format, or null to skip
 */
export function mapConstraintForBridge(c) {
	if (c.type === 'WhereDragged') {
		// Live drag pins keep Dragged semantics (weight 1/20, target = the
		// point's current position, refreshed per pointermove). Persistent
		// pins (origin/reference snaps, the Fix modal) lower to Pinned so the
		// stored (x, y) target survives every solve at full constraint weight
		// — Dragged would re-anchor to the drifted position and let the lock
		// walk away. See specs/pinned_constraint.md.
		if (c._isDrag || c.x == null || c.y == null) {
			return { type: 'Dragged', point: c.point };
		}
		return { type: 'Pinned', point: c.point, x: c.x, y: c.y };
	}
	// `EqualRadius` and `LengthRatio` are UI-side names with no matching
	// waffle-types variant — the Rust enum spells them `Equal` and `Ratio`.
	// Sent unmapped, serde rejects the whole message ("unknown variant
	// `EqualRadius`"), and because SolveSketch resends the FULL constraint
	// list every subsequent solve in the sketch fails to parse too — one
	// Equal-Radius click silently kills constraint solving for the rest of
	// the session. `Equal` dispatches on entity kind (circle/circle,
	// arc/arc, circle/arc), which covers every selection that produces
	// these. Emitted by the toolbar's Eq button, the right-click menu's
	// Equal Radius / Length Ratio items, and the sketch fillet tool.
	if (c.type === 'EqualRadius') {
		return { type: 'Equal', entity_a: c.entity_a, entity_b: c.entity_b };
	}
	if (c.type === 'LengthRatio') {
		return { type: 'Ratio', entity_a: c.entity_a, entity_b: c.entity_b, value: c.value };
	}
	// The right-click menu's Symmetric used to store its line as `entity_c`
	// (REVIEW.md §3.2-1); the engine field is `symmetry_line`. The builder now
	// emits `symmetry_line`; a constraint stored before the fix (an autosave,
	// an undo entry) is translated here rather than killing every solve.
	if (c.type === 'Symmetric' && c.symmetry_line == null && c.entity_c != null) {
		return { type: 'Symmetric', entity_a: c.entity_a, entity_b: c.entity_b, symmetry_line: c.entity_c };
	}
	return c;
}
