import { test as rawTest, expect } from '@playwright/test';
import { createExtrudedBox } from './helpers/geometry.js';
import { seedDocument, makeTestDocument } from './helpers/waffle-test.js';

/**
 * A document saved to browser storage reopens WITH its content from the home
 * page. `home-page.spec.js` checks only that clicking a card navigates; a
 * card that opens an empty document passed it (user report 2026-11-17:
 * "every project, despite having thumbnails, opens empty").
 */

/** Every stored document record in the local IndexedDB store. */
async function storedDocuments(page) {
	return page.evaluate(
		() =>
			new Promise((resolve) => {
				const req = indexedDB.open('waffle-iron');
				req.onerror = () => resolve([]);
				req.onsuccess = () => {
					const db = req.result;
					if (!db.objectStoreNames.contains('documents')) return resolve([]);
					const all = db.transaction('documents', 'readonly').objectStore('documents').getAll();
					all.onsuccess = () => resolve(all.result || []);
					all.onerror = () => resolve([]);
				};
			})
	);
}

/** Features in a stored `.waffle` record's active Part tab. */
function storedFeatureCount(record) {
	try {
		const doc = JSON.parse(record.json);
		const tab = (doc.tabs || []).find((t) => t.id === doc.active_tab) || (doc.tabs || [])[0];
		return tab?.kind?.features?.features?.length ?? 0;
	} catch {
		return 0;
	}
}

rawTest.describe('Home page → open', () => {
	rawTest('a stored document reopens with its features', async ({ page }) => {
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
			timeout: 30000
		});
		await createExtrudedBox(page);
		const built = await page.evaluate(() => window.__waffle.getFeatureTree().features.length);
		expect(built).toBeGreaterThanOrEqual(2);

		// The autosave reaches browser storage: some record holds the features.
		let saved = null;
		await expect
			.poll(
				async () => {
					const recs = await storedDocuments(page);
					saved = recs.find((r) => storedFeatureCount(r) >= built) || null;
					return saved !== null;
				},
				{ timeout: 20000 }
			)
			.toBe(true);

		// Leave, and open the same record from its home card.
		await page.goto('/home');
		const card = page.locator('[data-testid="document-card"]', {
			hasText: JSON.parse(saved.json).document?.name || ''
		});
		await expect(card.first()).toBeVisible({ timeout: 15000 });
		await card.first().click();
		await page.waitForURL((url) => url.pathname === '/' || url.pathname.endsWith('/'), {
			timeout: 15000
		});
		await page.waitForFunction(
			(n) => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) >= n,
			built,
			{ timeout: 20000 }
		);
		const reopened = await page.evaluate(() => window.__waffle.getFeatureTree().features.length);
		expect(reopened).toBe(built);
	});

	rawTest('a stored document that cannot load says so instead of opening empty', async ({ page }) => {
		// Saved by a newer app: the engine must refuse it, and the refusal
		// has to reach the user — a silent failure left the empty startup
		// document on screen.
		const doc = makeTestDocument({ id: 'toonew01', name: 'From The Future' });
		const json = JSON.parse(doc.json);
		json.version = 9999;
		json.min_reader_version = 9999;
		doc.json = JSON.stringify(json);
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		const card = page.locator('[data-testid="document-card"]', { hasText: 'From The Future' });
		await expect(card.first()).toBeVisible({ timeout: 15000 });
		await card.first().click();
		await expect(page.locator('[data-testid="toast-container"]')).toContainText(
			"Couldn't open From The Future",
			{ timeout: 20000 }
		);
	});
});
