/**
 * Constraint interaction helpers — toolbar constraint buttons and dimension tool.
 */

/**
 * Click a constraint toolbar button by constraint ID.
 * @param {import('@playwright/test').Page} page
 * @param {string} constraintId - e.g. 'horizontal', 'vertical', 'coincident', etc.
 */
export async function clickConstraintButton(page, constraintId) {
	await page.locator(`[data-testid="toolbar-constraint-${constraintId}"]`).click();
	await page.waitForTimeout(200);
}

/**
 * Check if a constraint button is enabled (not disabled).
 * @param {import('@playwright/test').Page} page
 * @param {string} constraintId
 * @returns {Promise<boolean>}
 */
export async function isConstraintEnabled(page, constraintId) {
	const btn = page.locator(`[data-testid="toolbar-constraint-${constraintId}"]`);
	const disabled = await btn.isDisabled();
	return !disabled;
}

/**
 * Check if a constraint button is visible.
 * @param {import('@playwright/test').Page} page
 * @param {string} constraintId
 * @returns {Promise<boolean>}
 */
export async function isConstraintVisible(page, constraintId) {
	return page.locator(`[data-testid="toolbar-constraint-${constraintId}"]`).isVisible();
}

/**
 * Get all sketch constraints via __waffle API.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<Array<object>>}
 */
export async function getConstraints(page) {
	return page.evaluate(() => window.__waffle?.getConstraints() ?? []);
}

/**
 * Get constraint count.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<number>}
 */
export async function getConstraintCount(page) {
	return page.evaluate(() => (window.__waffle?.getConstraints() ?? []).length);
}

/**
 * Count constraints of a specific type.
 * @param {import('@playwright/test').Page} page
 * @param {string} type - e.g. 'Horizontal', 'Vertical', 'Coincident', 'Distance', 'Radius'
 * @returns {Promise<number>}
 */
export async function getConstraintCountByType(page, type) {
	return page.evaluate((t) => {
		const constraints = window.__waffle?.getConstraints() ?? [];
		return constraints.filter(c => c.type === t).length;
	}, type);
}

/**
 * Wait until constraint count reaches at least n.
 * @param {import('@playwright/test').Page} page
 * @param {number} n
 * @param {number} timeout
 */
export async function waitForConstraintCount(page, n, timeout = 5000) {
	await page.waitForFunction(
		(expected) => (window.__waffle?.getConstraints() ?? []).length >= expected,
		n,
		{ timeout }
	);
}

/**
 * Set the sketch selection to specific entity IDs.
 * Used to programmatically select entities before testing constraint buttons.
 * @param {import('@playwright/test').Page} page
 * @param {number[]} entityIds
 */
export async function setSketchSelection(page, entityIds) {
	await page.evaluate((ids) => {
		// Access the store's setSketchSelection function via internal API
		// Since __waffle doesn't expose this directly, we use the module's exported function
		// The Svelte store module exposes setSketchSelection
		const { setSketchSelection } = window.__waffleInternal ?? {};
		if (setSketchSelection) {
			setSketchSelection(new Set(ids));
		}
	}, entityIds);
	await page.waitForTimeout(100);
}

/**
 * Click the Dimension tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickDimensionTool(page) {
	await page.locator('[data-testid="toolbar-btn-dimension"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'dimension',
		{ timeout: 3000 }
	);
}

/**
 * Get the current dimension popup state.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<object | null>}
 */
export async function getDimensionPopupState(page) {
	return page.evaluate(() => window.__waffle?.getDimensionPopup() ?? null);
}

/**
 * Wait for the dimension popup to appear.
 * @param {import('@playwright/test').Page} page
 * @param {number} timeout
 */
export async function waitForDimensionPopup(page, timeout = 5000) {
	await page.waitForFunction(
		() => window.__waffle?.getDimensionPopup() != null,
		{ timeout }
	);
}

/**
 * Type a value into the dimension input popup and press Enter to confirm.
 * @param {import('@playwright/test').Page} page
 * @param {number} value
 */
export async function applyDimensionValue(page, value) {
	const input = page.locator('.dimension-input');
	await input.waitFor({ state: 'visible', timeout: 3000 });
	await input.fill(String(value));
	await page.keyboard.press('Enter');
	await page.waitForTimeout(200);
}

/**
 * Dismiss the dimension popup by pressing Escape.
 * @param {import('@playwright/test').Page} page
 */
export async function dismissDimensionPopup(page) {
	const input = page.locator('.dimension-input');
	if (await input.isVisible()) {
		await input.focus();
		await page.keyboard.press('Escape');
		await page.waitForTimeout(200);
	}
}
