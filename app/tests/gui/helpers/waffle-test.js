/**
 * Core test fixture for GUI-first Playwright tests.
 *
 * Provides a WafflePage class and an extended `test` fixture that
 * auto-navigates to the app and waits for engine readiness.
 */
import { test as base, expect } from '@playwright/test';

/**
 * WafflePage wraps a Playwright Page with Waffle Iron-specific helpers.
 */
export class WafflePage {
	/** @param {import('@playwright/test').Page} page */
	constructor(page) {
		this.page = page;
	}

	/**
	 * Navigate to the app root.
	 */
	async goto() {
		await this.page.goto('/');
	}

	/**
	 * Wait for the engine to fully initialize (status dot turns green).
	 * Falls back to checking __waffle API if status dot never appears.
	 */
	async waitForReady() {
		// First wait for __waffle API to be defined
		await this.page.waitForFunction(
			() => typeof window.__waffle !== 'undefined',
			{ timeout: 30000 }
		);

		// Then wait for engine to be ready
		await this.page.waitForFunction(
			() => window.__waffle?.getState()?.engineReady === true,
			{ timeout: 30000 }
		);

		// Wait for the green status dot to confirm UI is synced
		try {
			await this.page.locator('[data-testid="status-dot"].ready').waitFor({
				state: 'visible',
				timeout: 5000,
			});
		} catch {
			// Status dot may not have the class if CSS doesn't match — engine state is enough
		}

		// Allow a frame for Svelte reactivity to settle
		await this.page.waitForTimeout(200);
	}

	/**
	 * Capture a screenshot and attach it to the test report.
	 * @param {string} name - descriptive name for the screenshot
	 * @returns {Promise<Buffer>}
	 */
	async screenshot(name) {
		const buffer = await this.page.screenshot();
		await base.info().attach(name, { body: buffer, contentType: 'image/png' });
		return buffer;
	}

	/**
	 * Capture a screenshot + JSON state dump for debugging.
	 * @param {string} label
	 */
	async dumpState(label) {
		const state = await this.page.evaluate(() => {
			const w = window.__waffle;
			if (!w) return { error: '__waffle not available' };
			return {
				state: w.getState(),
				entityCount: w.getEntities()?.length ?? 0,
				featureTree: w.getFeatureTree(),
				meshes: w.getMeshes(),
			};
		});

		const buffer = await this.page.screenshot();
		await base.info().attach(`${label} - screenshot`, { body: buffer, contentType: 'image/png' });
		await base.info().attach(`${label} - state`, {
			body: JSON.stringify(state, null, 2),
			contentType: 'application/json',
		});
	}
}

/**
 * Extended Playwright test fixture that provides a `waffle` WafflePage
 * which auto-navigates and waits for engine readiness.
 */
export const test = base.extend({
	waffle: async ({ page }, use) => {
		const waffle = new WafflePage(page);
		await waffle.goto();
		await waffle.waitForReady();
		await use(waffle);
	},
});

export { expect };
