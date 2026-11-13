import { expect } from '@playwright/test';
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

/**
 * The computed `transform` of a mobile panel that has finished sliding in:
 * `.open` is `translateX(0)`; mid-slide it is a translation.
 */
export const PANEL_AT_REST = 'matrix(1, 0, 0, 1, 0, 0)';

/**
 * Tap the backdrop where a user can reach it: on the strip the open panel
 * leaves uncovered.
 *
 * The backdrop fills the window and the panel sits on top of it, 260 px of a
 * 440 px portrait window (`.mobile-panel { width: min(260px, 75vw) }`), so
 * the backdrop's CENTER — where a bare `backdrop.click()` aims — is under the
 * panel. Such a click can only land while the panel is still sliding in
 * (0.2 s) and short of the middle; once the slide has finished it waits on a
 * point that is covered by design until the test times out.
 *
 * So: let the slide come to rest, then aim at the middle of what is left.
 * Playwright still hit-tests that point, so a panel that grew to cover the
 * whole backdrop fails here rather than passing.
 *
 * @param {import('@playwright/test').Page} page
 * @param {import('@playwright/test').Locator} panel - the open `.mobile-panel`
 */
export async function tapBackdropBeside(page, panel) {
	const backdrop = page.locator('.mobile-backdrop');
	await expect(backdrop).toBeVisible();
	// `.open` is `transform: translateX(0)`; mid-slide it is a translation.
	await expect(panel).toHaveCSS('transform', PANEL_AT_REST);

	const view = page.viewportSize();
	const box = await panel.boundingBox();
	expect(box).not.toBeNull();
	// A left panel leaves the strip to its right, a right panel the one to
	// its left.
	const [from, to] = box.x <= 0 ? [box.x + box.width, view.width] : [0, box.x];
	expect(to - from, 'backdrop strip left uncovered by the open panel').toBeGreaterThan(0);
	await backdrop.click({ position: { x: (from + to) / 2, y: view.height / 2 } });
}
