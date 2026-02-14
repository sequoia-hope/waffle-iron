/**
 * Toolbar interaction helpers — click buttons by data-testid.
 */

/**
 * Click the Sketch toolbar button and wait for sketch mode to activate.
 * @param {import('@playwright/test').Page} page
 */
export async function clickSketch(page, plane = 'xy') {
	await page.locator('[data-testid="toolbar-btn-sketch"]').click();
	// Handle the sketch plane dialog if it appears
	const dialog = page.locator('[data-testid="sketch-plane-dialog"]');
	try {
		await dialog.waitFor({ state: 'visible', timeout: 2000 });
		// Select the requested plane
		await page.locator(`[data-testid="plane-btn-${plane}-plane"]`).click();
		await page.locator('[data-testid="sketch-plane-ok"]').click();
	} catch {
		// Dialog may not appear (e.g., sketch-on-face bypasses it)
	}
	// Wait for sketch mode to be active (toolbar switches to sketch tools)
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	// Allow Svelte reactivity to settle
	await page.waitForTimeout(200);
}

/**
 * Click the Line sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickLine(page) {
	await page.locator('[data-testid="toolbar-btn-line"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'line',
		{ timeout: 3000 }
	);
}

/**
 * Click the Rectangle sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickRectangle(page) {
	await page.locator('[data-testid="toolbar-btn-rectangle"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'rectangle',
		{ timeout: 3000 }
	);
}

/**
 * Click the Circle sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickCircle(page) {
	await page.locator('[data-testid="toolbar-btn-circle"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'circle',
		{ timeout: 3000 }
	);
}

/**
 * Click the Arc sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickArc(page) {
	await page.locator('[data-testid="toolbar-btn-arc"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'arc',
		{ timeout: 3000 }
	);
}

/**
 * Click the Select sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickSelect(page) {
	await page.locator('[data-testid="toolbar-btn-select"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'select',
		{ timeout: 3000 }
	);
}

/**
 * Click the Dimension sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickDimension(page) {
	await page.locator('[data-testid="toolbar-btn-dimension"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'dimension',
		{ timeout: 3000 }
	);
}

/**
 * Click the Finish Sketch button and wait for sketch mode to deactivate.
 * @param {import('@playwright/test').Page} page
 */
export async function clickFinishSketch(page) {
	await page.locator('[data-testid="toolbar-btn-finish-sketch"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === false,
		{ timeout: 10000 }
	);
	// Allow Svelte reactivity and engine processing to settle
	await page.waitForTimeout(300);
}

/**
 * Click the Extrude toolbar button and wait for the dialog.
 * @param {import('@playwright/test').Page} page
 */
export async function clickExtrude(page) {
	await page.locator('[data-testid="toolbar-btn-extrude"]').click();
	await page.locator('[data-testid="extrude-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/**
 * Click the Revolve toolbar button and wait for the dialog.
 * @param {import('@playwright/test').Page} page
 */
export async function clickRevolve(page) {
	await page.locator('[data-testid="toolbar-btn-revolve"]').click();
	await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/**
 * Press a keyboard shortcut key.
 * @param {import('@playwright/test').Page} page
 * @param {string} key
 */
export async function pressKey(page, key) {
	await page.keyboard.press(key);
	await page.waitForTimeout(100);
}

/**
 * Check if a specific toolbar button is visible.
 * @param {import('@playwright/test').Page} page
 * @param {string} buttonId
 * @returns {Promise<boolean>}
 */
export async function isToolbarButtonVisible(page, buttonId) {
	return page.locator(`[data-testid="toolbar-btn-${buttonId}"]`).isVisible();
}
