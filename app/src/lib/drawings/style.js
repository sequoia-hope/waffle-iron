/**
 * Drawing style — line weights, text heights, arrowheads and gaps
 * (`specs/drawings_and_mbd.md` §7: "Line weights and text heights follow
 * ISO 128 / ASME Y14.2 defaults scaled by the view scale, and are document
 * settings").
 *
 * Every length here is in **paper millimetres**, because that is what the
 * standards specify and what a plotted sheet actually measures. The renderer
 * emits SVG whose user unit IS one paper millimetre, so these numbers reach
 * the output unscaled — a 0.5 mm outline is 0.5 user units wide however the
 * view is scaled. That is the whole point of a view scale: the part shrinks,
 * the pen does not.
 *
 * ## Where the numbers come from
 *
 * - **Line widths** are the ISO 128-20 line-width group `0.5`: wide 0.5 mm
 *   for visible outlines, narrow 0.25 mm for dimension, extension, leader and
 *   centre lines. Hidden lines are 0.35 mm dashed — the middle width, so they
 *   read as lighter than an outline and heavier than a dimension.
 * - **Text height** 3.5 mm is the ISO 3098 / ISO 129-1 default for a sheet of
 *   A3 or smaller.
 * - **Arrowheads** are ISO 129-1 §6.2: a closed filled head whose length is
 *   the text height and whose width is a third of its length.
 * - **Extension-line gap and overshoot** are ISO 129-1 §5.4: a visible gap
 *   between the feature and the start of its extension line, and an
 *   extension past the dimension line. The standard states both as multiples
 *   of the line width (≈ 8×); at 0.25 mm that is 2 mm, which is what is used.
 * - **Dash patterns** are ISO 128-2's type 02 (dashed, hidden detail) and
 *   type 04 (long-dash dotted, centre lines).
 *
 * ## Colours are CSS variables, never literals
 *
 * The renderer emits `stroke="var(--drawing-ink)"` and friends, so a drawing
 * follows the document theme the same way the rest of the chrome does. The
 * tokens are defined once in `app/src/app.css` in terms of the theme's own
 * (`--text-primary`, `--bg-primary`, …), which is why a new theme needs no
 * drawing block of its own.
 */

/** The CSS custom properties a rendered sheet paints with. */
export const DRAWING_TOKENS = {
	paper: 'var(--drawing-paper)',
	visible: 'var(--drawing-ink)',
	hidden: 'var(--drawing-hidden)',
	section: 'var(--drawing-ink)',
	annotation: 'var(--drawing-annotation)',
	text: 'var(--drawing-annotation)',
	/** D4b: a section cap's hatching, and the title block's own rules. */
	hatch: 'var(--drawing-hatch)',
	frame: 'var(--drawing-ink)'
};

/**
 * ISO 128 / ASME Y14.2 defaults. Lengths in paper mm.
 *
 * @typedef {object} DrawingStyle
 * @property {number} visibleWidth    - wide line, visible outlines
 * @property {number} hiddenWidth     - hidden detail
 * @property {number} thinWidth       - dimension / extension / leader / centre
 * @property {number} textHeight      - nominal capital height
 * @property {number} arrowLength     - closed-filled head length
 * @property {number} arrowWidthRatio - head width ÷ head length
 * @property {number} extensionGap    - gap between the feature and its extension line
 * @property {number} extensionOvershoot - how far an extension line passes the dimension line
 * @property {number} dimensionOffset - default clearance from the feature to the dimension line
 * @property {number} textGap         - halo gap between the text and the dimension line
 * @property {number} leaderShoulder  - the horizontal landing a leader ends with
 * @property {number} leaderAngleDeg  - default leader direction, from +u, counter-clockwise
 * @property {number} centreMarkOvershoot - how far a centre mark's arms pass the circle
 * @property {number} dotRadius       - a leader's dot terminator
 * @property {number[]} hiddenDash    - ISO 128-2 type 02
 * @property {number[]} centreDash    - ISO 128-2 type 04
 * @property {boolean} architecturalTicks - draw 45° ticks instead of arrowheads (§7's option)
 * @property {number} hatchSpacing   - D4b: paper mm between section hatch lines
 * @property {number} hatchAngleDeg  - D4b: hatch direction, from +u counter-clockwise
 * @property {number} cutLineWidth   - D4b: the cutting-plane line on a parent view
 * @property {number} titleBlockWidth  - D4b: the title block's paper width
 * @property {number} titleBlockRowHeight - D4b: one row's height
 * @property {number} titleBlockLabelWidth - D4b: the label column's width
 * @property {string} fontFamily
 */

/** @type {DrawingStyle} */
export const DEFAULT_STYLE = {
	visibleWidth: 0.5,
	hiddenWidth: 0.35,
	thinWidth: 0.25,
	textHeight: 3.5,
	arrowLength: 3.5,
	arrowWidthRatio: 1 / 3,
	extensionGap: 2,
	extensionOvershoot: 2,
	dimensionOffset: 10,
	textGap: 1,
	leaderShoulder: 6,
	leaderAngleDeg: 45,
	centreMarkOvershoot: 2,
	dotRadius: 0.75,
	hiddenDash: [4, 2],
	centreDash: [12, 2, 2, 2],
	architecturalTicks: false,
	// D4b. ISO 128-50 specifies section hatching as continuous NARROW lines
	// at a uniform spacing and (for a single material) 45°, and leaves the
	// spacing to the drawing's scale and size; 3 mm is the middle of the
	// 2–4 mm range general-purpose practice uses on A4–A2 and is coarse
	// enough that a 10 mm cap reads as hatched rather than as solid. It is a
	// PAPER quantity like the line widths, so it does not scale with the
	// view.
	hatchSpacing: 3,
	hatchAngleDeg: 45,
	// The cutting-plane line (ISO 128-2 type 04, long-dash dotted) is drawn
	// at the WIDE width at its ends and narrow between; drawn at the hidden
	// width throughout here, which is the middle one — heavier than a
	// dimension line, lighter than an outline, so it reads as a construction
	// of the drawing rather than as an edge of the part.
	cutLineWidth: 0.35,
	// The title block. ISO 7200 fixes the data-field block at 180 mm wide,
	// which fits inside the frame of every sheet from A4 portrait up; it is
	// clamped to the frame's width for a smaller custom sheet rather than
	// hanging off the paper.
	titleBlockWidth: 180,
	titleBlockRowHeight: 8,
	titleBlockLabelWidth: 38,
	fontFamily: 'var(--font-ui)'
};

/**
 * `DEFAULT_STYLE` with `overrides` applied — the document-settings seam.
 * Unknown keys are kept, so a setting this version does not know about
 * survives to the one that does.
 *
 * @param {Partial<DrawingStyle>} [overrides]
 * @returns {DrawingStyle}
 */
export function drawingStyle(overrides) {
	return { ...DEFAULT_STYLE, ...(overrides ?? {}) };
}
