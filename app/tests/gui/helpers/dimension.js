/**
 * Dimension tool interaction helpers.
 * Re-exports dimension-related helpers from constraint.js for convenience,
 * plus additional dimension-specific helpers.
 */

export {
	clickDimensionTool,
	getDimensionPopupState,
	waitForDimensionPopup,
	applyDimensionValue,
	dismissDimensionPopup,
} from './constraint.js';

/**
 * Apply a dimension value by filling the input and pressing Enter.
 * @param {import('@playwright/test').Page} page
 * @param {number} value
 */
export async function applyDimension(page, value) {
	const input = page.locator('.dimension-input');
	await input.waitFor({ state: 'visible', timeout: 3000 });
	await input.fill(String(value));
	await page.keyboard.press('Enter');
	await page.waitForTimeout(200);
}

/**
 * Cancel dimension by pressing Escape.
 * @param {import('@playwright/test').Page} page
 */
export async function cancelDimension(page) {
	await page.keyboard.press('Escape');
	await page.waitForTimeout(200);
}
