/**
 * Revolve test helpers.
 *
 * Since the March 2026 revolve UX overhaul the dialog requires an explicit
 * axis pick (Apply is disabled until one is set). Specs that test the
 * APPLY path set the axis through the test API — SETUP only; the pick-mode
 * interaction itself is covered by the dialog-lifecycle specs.
 */

/**
 * Set a revolve axis parallel to the sketch X axis, offset BELOW the
 * sketched profile (kernel-v2 requires the profile strictly on one side
 * of the axis — an axis through or touching the profile is invalid input).
 * Reads the finished sketch's solved positions from the feature tree.
 */
export async function pickOffsetRevolveAxis(page) {
	await page.evaluate(() => {
		const tree = window.__waffle.getFeatureTree();
		const sketch = [...tree.features]
			.reverse()
			.find((f) => f.operation?.type === 'Sketch')?.operation?.sketch;
		if (!sketch) throw new Error('pickOffsetRevolveAxis: no sketch feature');
		const ys = Object.values(sketch.solved_positions || {}).map((p) =>
			Array.isArray(p) ? p[1] : p.y
		);
		if (!ys.length) throw new Error('pickOffsetRevolveAxis: no solved positions');
		const minY = Math.min(...ys);
		const maxY = Math.max(...ys);
		const margin = Math.max((maxY - minY) * 0.5, 1e-3);
		const axisY = minY - margin;
		// Map (sketch x-direction line at sketch-y = axisY) to world with the
		// ENGINE's plane basis (tangent_x_from_normal in rebuild.rs — same
		// math as axisUtils.computePlaneBasis, inlined because page context
		// cannot import modules).
		const o = sketch.plane_origin || [0, 0, 0];
		const pn = sketch.plane_normal || [0, 0, 1];
		const ref = Math.abs(pn[2]) < 0.99 ? [0, 0, 1] : [1, 0, 0];
		let r = [
			ref[1] * pn[2] - ref[2] * pn[1],
			ref[2] * pn[0] - ref[0] * pn[2],
			ref[0] * pn[1] - ref[1] * pn[0]
		];
		const rlen = Math.hypot(r[0], r[1], r[2]);
		r = rlen > 1e-10 ? [r[0] / rlen, r[1] / rlen, r[2] / rlen] : [1, 0, 0];
		const up = [
			pn[1] * r[2] - pn[2] * r[1],
			pn[2] * r[0] - pn[0] * r[2],
			pn[0] * r[1] - pn[1] * r[0]
		];
		window.__waffle.setRevolveAxis(
			[o[0] + up[0] * axisY, o[1] + up[1] * axisY, o[2] + up[2] * axisY],
			r,
			'Test axis'
		);
	});
}
