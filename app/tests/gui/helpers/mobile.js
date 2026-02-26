/**
 * Mobile test helpers for viewport and overflow assertions.
 */

/**
 * Check if current viewport is mobile-sized.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<boolean>}
 */
export async function isMobileViewport(page) {
	return page.evaluate(() => window.innerWidth <= 768);
}

/**
 * Assert no element extends beyond window bounds.
 * @param {import('@playwright/test').Page} page
 * @param {string[]} selectors - CSS selectors to check
 * @param {import('@playwright/test').Expect} expect
 */
export async function assertNoOverflow(page, selectors, expect) {
	const viewport = page.viewportSize();
	for (const selector of selectors) {
		const el = page.locator(selector).first();
		if ((await el.count()) === 0) continue;
		const box = await el.boundingBox();
		if (!box) continue;
		expect(box.x, `${selector} left edge`).toBeGreaterThanOrEqual(-1);
		expect(box.y, `${selector} top edge`).toBeGreaterThanOrEqual(-1);
		expect(
			box.x + box.width,
			`${selector} right edge`
		).toBeLessThanOrEqual(viewport.width + 1);
		expect(
			box.y + box.height,
			`${selector} bottom edge`
		).toBeLessThanOrEqual(viewport.height + 1);
	}
}

/**
 * Assert a single element fits within viewport bounds.
 * @param {import('@playwright/test').Page} page
 * @param {string} selector
 * @param {import('@playwright/test').Expect} expect
 */
export async function assertElementWithinBounds(page, selector, expect) {
	await assertNoOverflow(page, [selector], expect);
}
