/**
 * Core test fixture for GUI-first Playwright tests.
 *
 * Provides a WafflePage class and an extended `test` fixture that
 * auto-navigates to the app and waits for engine readiness.
 * Also provides helpers for document model testing (seedDocument, makeTestDocument, etc.).
 */
import { test as base, expect } from '@playwright/test';
import crypto from 'crypto';

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

/**
 * Extended fixture that provides a `homePage` for home-page tests
 * (no engine wait needed).
 */
export const homeTest = base.extend({
	homePage: async ({ page }, use) => {
		await page.goto('/home');
		await use(page);
	},
});

/**
 * Seed a document directly into IndexedDB.
 * @param {import('@playwright/test').Page} page
 * @param {object} doc - document object with { id, json, created, modified }
 */
export async function seedDocument(page, doc) {
	await page.evaluate(async (d) => {
		return new Promise((resolve, reject) => {
			const req = indexedDB.open('waffle-iron', 1);
			req.onupgradeneeded = (e) => {
				const db = e.target.result;
				if (!db.objectStoreNames.contains('documents')) {
					db.createObjectStore('documents', { keyPath: 'id' });
				}
			};
			req.onsuccess = () => {
				const db = req.result;
				const tx = db.transaction('documents', 'readwrite');
				tx.objectStore('documents').put(d);
				tx.oncomplete = () => resolve();
				tx.onerror = () => reject(tx.error);
			};
			req.onerror = () => reject(req.error);
		});
	}, doc);
}

/**
 * Read a document from IndexedDB by id.
 * @param {import('@playwright/test').Page} page
 * @param {string} id
 * @returns {Promise<object|undefined>}
 */
export async function getDocumentFromDB(page, id) {
	return page.evaluate(async (docId) => {
		return new Promise((resolve, reject) => {
			const req = indexedDB.open('waffle-iron', 1);
			req.onupgradeneeded = (e) => {
				const db = e.target.result;
				if (!db.objectStoreNames.contains('documents')) {
					db.createObjectStore('documents', { keyPath: 'id' });
				}
			};
			req.onsuccess = () => {
				const db = req.result;
				const tx = db.transaction('documents', 'readonly');
				const getReq = tx.objectStore('documents').get(docId);
				getReq.onsuccess = () => resolve(getReq.result);
				getReq.onerror = () => reject(getReq.error);
			};
			req.onerror = () => reject(req.error);
		});
	}, id);
}

/**
 * Create a standard v3 test document with sensible defaults.
 * @param {object} overrides
 * @returns {object} document object suitable for seedDocument
 */
export function makeTestDocument(overrides = {}) {
	const id = overrides.id || 'testdoc1';
	const now = Date.now();
	const tabId = crypto.randomUUID();
	return {
		id,
		json: JSON.stringify({
			format: 'waffle-iron',
			version: 3,
			document: {
				name: overrides.name || 'Test Document',
				created: new Date(now).toISOString(),
				modified: new Date(now).toISOString(),
			},
			tabs: [{
				id: tabId,
				name: 'Part 1',
				kind: {
					type: 'Part',
					features: { features: [], active_index: null },
				},
			}],
			active_tab: tabId,
		}),
		created: now,
		modified: now,
		...overrides,
	};
}

export { expect };
