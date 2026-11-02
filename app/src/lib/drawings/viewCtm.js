/**
 * The screen matrix of a NESTED `<svg>`'s own user space — for a drawing
 * view on the sheet, paper millimetres → client pixels.
 *
 * Not `svg.getScreenCTM()`, because the browsers disagree about what that
 * means for a nested `<svg>`: Chromium and WebKit include the element's own
 * viewport (its `x`/`y` and `viewBox`), Firefox does not — it answers the
 * PARENT's matrix, so every view on the sheet reads as if it sat at the
 * sheet's top-left corner. Measured on CI (Firefox 1.64 build, 2026-11-02):
 * a view at `x="170" y="116"` with `viewBox="-20 -20 80 65"` reported the
 * sheet's own `e, f` while its bounding rect sat 190 × 136 mm further in,
 * so a click on a drawn edge hit-tested 190 mm away from it.
 *
 * So the matrix is COMPOSED from what every browser agrees on: the parent's
 * screen matrix (the `<g>` the view is wrapped in, i.e. sheet paper space),
 * then the nested viewport's transform as SVG 1.1 §7.7 defines it —
 * translate to `x, y`, then `viewBox` → viewport with `preserveAspectRatio`.
 *
 * @param {SVGSVGElement | null | undefined} svg
 * @returns {DOMMatrix | null} null when there is no parent matrix (a
 *   detached or `display: none` element)
 */
export function viewScreenCTM(svg) {
	const parent = /** @type {SVGGraphicsElement | null} */ (
		svg?.parentNode ?? null
	);
	const pm = parent?.getScreenCTM?.();
	if (!svg || !pm) return null;
	let m = DOMMatrix.fromMatrix(pm).translate(
		svg.x.baseVal.value,
		svg.y.baseVal.value,
	);
	const vb = svg.viewBox?.baseVal;
	if (!vb || !(vb.width > 0) || !(vb.height > 0)) return m;
	const w = svg.width.baseVal.value;
	const h = svg.height.baseVal.value;
	let sx = w / vb.width;
	let sy = h / vb.height;
	let tx = 0;
	let ty = 0;
	const par = svg.preserveAspectRatio.baseVal;
	if (par.align !== SVGPreserveAspectRatio.SVG_PRESERVEASPECTRATIO_NONE) {
		const s =
			par.meetOrSlice === SVGPreserveAspectRatio.SVG_MEETORSLICE_SLICE
				? Math.max(sx, sy)
				: Math.min(sx, sy);
		sx = sy = s;
		// The nine alignments run xMinYMin … xMaxYMax, x fastest: 0/1/2 is
		// min/mid/max on each axis, and mid/max take half/all of the slack.
		const k =
			par.align - SVGPreserveAspectRatio.SVG_PRESERVEASPECTRATIO_XMINYMIN;
		tx = ((w - vb.width * s) * (k % 3)) / 2;
		ty = ((h - vb.height * s) * Math.floor(k / 3)) / 2;
	}
	m = m.translate(tx, ty).scale(sx, sy).translate(-vb.x, -vb.y);
	return m;
}
