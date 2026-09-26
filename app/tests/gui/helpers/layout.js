/**
 * Layout-overflow oracle.
 *
 * The page never scrolls (`html, body { overflow: hidden }`), so any chrome
 * that outgrows its box is CLIPPED and unreachable — invisible to the
 * container-level bounds checks in mobile.js, which measure the clipped
 * container, not the children that fell out of it. This helper measures the
 * children: every interactive element must lie inside the window unless a
 * scrollable ancestor makes it reachable along that axis.
 */

/**
 * Every visible interactive element that lies outside the window (or would
 * need the document itself to scroll), with where it landed.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<{id: string, rect: number[], axis: string}[]>}
 */
export async function findOutOfBounds(page) {
	return page.evaluate(() => {
		const W = window.innerWidth;
		const H = window.innerHeight;
		const EPS = 1;
		const out = [];
		// The document itself must never have been scrolled to reach something:
		// with overflow hidden that shifts the whole shell out of the window.
		if (window.scrollX > 0 || window.scrollY > 0) {
			out.push({ id: 'document scrolled', rect: [window.scrollX, window.scrollY, 0, 0], axis: 'doc' });
		}
		const label = (el) =>
			el.dataset.testid ||
			(typeof el.className === 'string' && el.className.split(' ')[0]) ||
			el.tagName.toLowerCase();
		const nodes = document.querySelectorAll('button, input, select, textarea, a, [data-testid]');
		for (const el of nodes) {
			// Off-canvas by design: a closed mobile slide-in panel.
			if (el.closest('.mobile-panel:not(.open)')) continue;
			const cs = getComputedStyle(el);
			if (cs.display === 'none' || cs.visibility === 'hidden' || cs.opacity === '0') continue;
			const r = el.getBoundingClientRect();
			if (r.width === 0 && r.height === 0) continue;
			// Reachable by scrolling an ancestor along that axis is fine.
			let scrollX = false;
			let scrollY = false;
			for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
				const acs = getComputedStyle(a);
				if (/auto|scroll/.test(acs.overflowX)) scrollX = true;
				if (/auto|scroll/.test(acs.overflowY)) scrollY = true;
			}
			const badX = !scrollX && (r.left < -EPS || r.right > W + EPS);
			const badY = !scrollY && (r.top < -EPS || r.bottom > H + EPS);
			if (badX || badY) {
				out.push({
					id: label(el),
					rect: [r.left, r.top, r.right, r.bottom].map((v) => Math.round(v)),
					axis: badX && badY ? 'xy' : badX ? 'x' : 'y',
				});
			}
		}
		return out;
	});
}

/**
 * Assert nothing interactive is outside the window. `where` names the UI
 * state under test so a failure says which state overflowed.
 * @param {import('@playwright/test').Page} page
 * @param {import('@playwright/test').Expect} expect
 * @param {string} where
 */
export async function expectNothingOffscreen(page, expect, where) {
	const offenders = await findOutOfBounds(page);
	expect(offenders, `${where}: elements outside the window`).toEqual([]);
}

/**
 * The 3D view keeps a usable size no matter what the chrome does.
 * @param {import('@playwright/test').Page} page
 * @param {import('@playwright/test').Expect} expect
 * @param {string} where
 * @param {{width?: number, height?: number}} [min]
 */
export async function expectViewportUsable(page, expect, where, min = {}) {
	const minW = min.width ?? 320;
	const minH = min.height ?? 200;
	const box = await page.locator('.viewport-area canvas').first().boundingBox();
	expect(box, `${where}: canvas present`).not.toBeNull();
	expect(box.width, `${where}: canvas width`).toBeGreaterThanOrEqual(minW);
	expect(box.height, `${where}: canvas height`).toBeGreaterThanOrEqual(minH);
}

/**
 * Drag a side-panel divider to an absolute window x.
 * @param {import('@playwright/test').Page} page
 * @param {'left' | 'right'} side
 * @param {number} toX
 */
export async function dragDivider(page, side, toX) {
	const divider = page.locator('.app-shell .divider').nth(side === 'left' ? 0 : 1);
	const box = await divider.boundingBox();
	if (!box) throw new Error(`${side} divider not found`);
	const y = box.y + Math.min(100, box.height / 2);
	await page.mouse.move(box.x + box.width / 2, y);
	await page.mouse.down();
	await page.mouse.move(toX, y, { steps: 6 });
	await page.mouse.up();
	await page.waitForTimeout(50);
}
