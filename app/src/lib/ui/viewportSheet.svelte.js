/**
 * Feature dialogs that cover the 3D view on phones and tablets.
 *
 * On a narrow screen the feature dialogs (Extrude, Revolve, …) are fixed
 * bottom sheets up to 60vh tall; on a phone held sideways they are side
 * panels over a canvas only ~500px wide. Either way they are drawn OVER the
 * canvas, and the model the user just framed ends up half hidden. Each
 * dialog root takes `use:viewportSheet`; it reports its client rect here,
 * and Viewport ends the canvas box at the dialog's edge. The camera keeps
 * its target and vertical extent, so what was centered in the view stays
 * centered (and whole) in the space still visible.
 *
 * A desktop panel floating beside the model on a mouse-driven screen is
 * left alone: the action reports nothing outside the touch/narrow query.
 */

const SHEET_QUERY = '(max-width: 768px), (pointer: coarse)';

/** @typedef {{ top: number, left: number, right: number, bottom: number }} SheetRect */

/** Client rect of each open dialog. */
let sheetRects = $state(/** @type {Map<object, SheetRect>} */ (new Map()));

/**
 * How far each edge of `view` (a client rect) is covered by an open dialog.
 * A dialog spanning most of the view's width covers it from the bottom;
 * one hugging the right edge covers it from the right.
 * @param {SheetRect} view
 * @returns {{ bottom: number, right: number }}
 */
export function getViewportSheetInsets(view) {
	let bottom = 0;
	let right = 0;
	const width = view.right - view.left;
	for (const r of sheetRects.values()) {
		if (r.bottom <= view.top || r.top >= view.bottom || r.right <= view.left || r.left >= view.right) continue;
		const spanW = Math.min(r.right, view.right) - Math.max(r.left, view.left);
		if (spanW >= 0.8 * width) {
			bottom = Math.max(bottom, view.bottom - r.top);
		} else if (r.right >= view.right - 40) {
			right = Math.max(right, view.right - r.left);
		}
	}
	return { bottom: Math.max(0, Math.round(bottom)), right: Math.max(0, Math.round(right)) };
}

/** @param {object} key @param {SheetRect | null} rect */
function report(key, rect) {
	const next = new Map(sheetRects);
	if (rect == null) next.delete(key);
	else next.set(key, rect);
	sheetRects = next;
}

/**
 * Svelte action for a feature dialog root.
 * @param {HTMLElement} node
 */
export function viewportSheet(node) {
	const key = {};
	const mq = typeof window !== 'undefined' && window.matchMedia ? window.matchMedia(SHEET_QUERY) : null;
	let last = '';

	const measure = () => {
		let rect = null;
		if (mq?.matches) {
			const r = node.getBoundingClientRect();
			rect = { top: Math.round(r.top), left: Math.round(r.left), right: Math.round(r.right), bottom: Math.round(r.bottom) };
		}
		const sig = JSON.stringify(rect);
		if (sig === last) return;
		last = sig;
		report(key, rect);
	};

	const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(measure) : null;
	ro?.observe(node);
	mq?.addEventListener?.('change', measure);
	window.addEventListener('resize', measure);
	measure();

	return {
		destroy() {
			ro?.disconnect();
			mq?.removeEventListener?.('change', measure);
			window.removeEventListener('resize', measure);
			report(key, null);
		}
	};
}
