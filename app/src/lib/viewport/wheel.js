/**
 * Wheel delta normalisation (REVIEW.md §3.4).
 *
 * `WheelEvent.deltaY` is in the unit `deltaMode` names: pixels (0), lines
 * (1) or pages (2). Chromium sends pixels (~100 per notch); Firefox sends
 * lines for a mouse wheel (3 per notch) and pages for paged scrolling. Read
 * raw, a Firefox notch zoomed about 30x less than a Chromium one.
 */

/**
 * Pixels per line. 40 is the common convention (Facebook's normalize-wheel,
 * and what makes Firefox's 3-line notch ≈ Chromium's 100–120 px notch).
 */
export const WHEEL_LINE_PX = 40;

/**
 * The vertical wheel delta in pixels.
 * @param {{ deltaY?: number, deltaMode?: number }} e - a WheelEvent (or its shape)
 * @param {number} pageHeightPx - one "page", e.g. the viewport's height in px
 * @returns {number}
 */
export function wheelDeltaPixels(e, pageHeightPx) {
	const dy = e.deltaY ?? 0;
	if (e.deltaMode === 1) return dy * WHEEL_LINE_PX;
	if (e.deltaMode === 2) return dy * pageHeightPx;
	return dy;
}
