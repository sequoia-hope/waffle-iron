/**
 * Screenshot baseline helper wrapping Playwright's toHaveScreenshot.
 *
 * Provides sensible defaults for the Waffle Iron UI:
 *   - Masks the status message (timestamps change between runs)
 *   - Configurable pixel diff threshold
 */
import { expect } from '@playwright/test';

/**
 * Assert that the current page screenshot matches a stored baseline.
 * Creates the baseline on first run.
 *
 * @param {import('@playwright/test').Page} page
 * @param {string} name - screenshot baseline name (e.g. 'initial-viewport.png')
 * @param {object} [options] - optional overrides
 * @param {number} [options.threshold] - per-pixel color diff threshold (0-1, default 0.2)
 * @param {number} [options.maxDiffPixelRatio] - max ratio of diff pixels (default 0.01)
 * @param {Array<import('@playwright/test').Locator>} [options.mask] - additional locators to mask
 */
export async function assertScreenshot(page, name, options = {}) {
	const {
		threshold = 0.2,
		maxDiffPixelRatio = 0.01,
		mask: userMasks = [],
	} = options;

	// Default masks — elements whose content changes between runs
	const defaultMasks = [
		page.locator('[data-testid="status-message"]'),
	];

	const allMasks = [...defaultMasks, ...userMasks];

	await expect(page).toHaveScreenshot(name, {
		threshold,
		maxDiffPixelRatio,
		mask: allMasks,
	});
}
