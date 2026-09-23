/**
 * Reload restore (`restoreOnReload` setting, `$lib/storage/drafts.js`,
 * `AutoRestoreDialog`):
 * - `auto` (default) reopens this tab's work without asking;
 * - `ask` offers it; Discard drops only this tab's draft, never the stored
 *   document, and Restore reopens the WHOLE document (every tab, its identity)
 *   so the next autosave cannot overwrite it with a one-tab copy;
 * - `never` starts empty;
 * - hiding the tab stores a pending edit at once instead of after the delay.
 */
import { test, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { createExtrudedBox } from './helpers/geometry.js';

/**
 * Every record of an IndexedDB object store, with the parsed `.waffle` identity and tab count.
 * @param {import('@playwright/test').Page} page
 * @param {'documents' | 'drafts'} which
 */
async function records(page, which) {
	const [dbName, storeName] = which === 'documents' ? ['waffle-iron', 'documents'] : ['waffle-iron-drafts', 'drafts'];
	return page.evaluate(
		([dbName, storeName]) =>
			new Promise((resolve, reject) => {
				const open = indexedDB.open(dbName);
				open.onerror = () => reject(open.error);
				open.onsuccess = () => {
					const db = open.result;
					if (!db.objectStoreNames.contains(storeName)) {
						db.close();
						return resolve([]);
					}
					const req = db.transaction(storeName, 'readonly').objectStore(storeName).getAll();
					req.onerror = () => reject(req.error);
					req.onsuccess = () => {
						db.close();
						resolve(
							req.result.map((r) => {
								const parsed = JSON.parse(r.json);
								return {
									key: r.tabKey ?? r.id,
									docId: r.docId ?? r.id,
									documentId: parsed.document?.id,
									tabs: parsed.tabs?.length ?? 0,
									sketchEntities: r.sketch?.entities?.length ?? null
								};
							})
						);
					};
				};
			}),
		[dbName, storeName]
	);
}

async function waitEngine(page) {
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
}

/** Persist the reload policy before the app first loads (kept across reloads). */
async function setPolicy(page, policy) {
	await page.addInitScript((p) => {
		try {
			if (!localStorage.getItem('waffle:settings')) {
				localStorage.setItem('waffle:settings', JSON.stringify({ restoreOnReload: p }));
			}
		} catch {}
	}, policy);
}

/** A two-tab document with geometry, autosaved to this browser; returns its record. */
async function seedTwoTabDocument(page) {
	await page.goto('/');
	await waitEngine(page);
	await createExtrudedBox(page);
	await page.evaluate(() => window.__waffle.addTab('Part'));
	await expect.poll(async () => (await records(page, 'documents')).map((d) => d.tabs), { timeout: 15000 }).toEqual([2]);
	await expect.poll(async () => (await records(page, 'drafts')).map((d) => d.tabs), { timeout: 15000 }).toEqual([2]);
	return (await records(page, 'documents'))[0];
}

test.describe('Reload restore', () => {
	test('auto (default): a reload reopens this tab\'s work without asking', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const seeded = await seedTwoTabDocument(page);

		await page.reload();
		await waitEngine(page);
		await page.waitForFunction(() => window.__waffle.getMeshes().length >= 1, null, { timeout: 30000 });
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);

		const info = await page.evaluate(() => window.__waffle.getDocumentInfo());
		expect(info.storageId).toBe(seeded.docId);
		expect(info.documentId).toBe(seeded.documentId);
		expect(info.tabs).toHaveLength(2);
		expectNoAnyCrash(crashes);
	});

	test('a reload that lost sessionStorage still reopens this tab\'s work', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const seeded = await seedTwoTabDocument(page);

		// An iOS tab discard does not reliably keep sessionStorage, and with it
		// the tab key the draft is filed under. The key of the last hidden tab is
		// mirrored to localStorage on hide/unload and adopted once by a tab that
		// has none — the reload's own pagehide is that moment.
		await page.evaluate(() => sessionStorage.clear());
		await page.reload();
		await waitEngine(page);
		await page.waitForFunction(() => window.__waffle.getMeshes().length >= 1, null, { timeout: 30000 });

		const info = await page.evaluate(() => window.__waffle.getDocumentInfo());
		expect(info.storageId).toBe(seeded.docId);
		expect(info.tabs).toHaveLength(2);
		// Adopted, not copied: the tab now files its draft under the same key.
		const drafts = await records(page, 'drafts');
		expect(drafts.map((d) => d.docId)).toEqual([seeded.docId]);
		expectNoAnyCrash(crashes);
	});

	test('hiding the tab stores a pending edit at once', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await seedTwoTabDocument(page);

		// The autosave delay is 3 s; a hidden tab must not wait for it.
		await page.evaluate(() => window.__waffle.addTab('Part'));
		await page.evaluate(() => {
			Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
			document.dispatchEvent(new Event('visibilitychange'));
		});
		await expect.poll(async () => (await records(page, 'drafts')).map((d) => d.tabs), { timeout: 1500 }).toEqual([3]);
		await expect.poll(async () => (await records(page, 'documents')).map((d) => d.tabs), { timeout: 1500 }).toEqual([3]);
		expectNoAnyCrash(crashes);
	});

	test('an open, unfinished sketch comes back after a reload', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await page.goto('/');
		await waitEngine(page);
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
		await page.waitForFunction(() => window.__waffle.getState().sketchMode.active === true, null, { timeout: 5000 });
		// Fixture setup (not a drawing test): three points and two lines, never finished.
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 1, x: -20, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 2, x: 20, y: -10, construction: false });
			w.addSketchEntity({ type: 'Point', id: 3, x: 20, y: 15, construction: false });
			w.addSketchEntity({ type: 'Line', id: 4, start_id: 1, end_id: 2, construction: false });
			w.addSketchEntity({ type: 'Line', id: 5, start_id: 2, end_id: 3, construction: false });
		});
		await page.evaluate(() => {
			Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
			document.dispatchEvent(new Event('visibilitychange'));
		});
		await expect.poll(async () => (await records(page, 'drafts')).map((d) => d.sketchEntities), { timeout: 5000 }).toEqual([5]);

		await page.reload();
		await waitEngine(page);
		await page.waitForFunction(() => window.__waffle.getState().sketchMode.active === true, null, { timeout: 30000 });
		const entities = await page.evaluate(() => window.__waffle.getEntities().map((e) => [e.id, e.type]));
		expect(entities).toEqual([[1, 'Point'], [2, 'Point'], [3, 'Point'], [4, 'Line'], [5, 'Line']]);
		const positions = await page.evaluate(() => [...window.__waffle.getPositions()].map(([id, p]) => [id, p.x, p.y]));
		expect(positions.find(([id]) => id === 3)?.slice(1)).toEqual([20, 15]);

		// Cancelling the sketch clears it from the draft: the next reload does not re-enter it.
		await page.evaluate(() => window.__waffle.exitSketch());
		await expect.poll(async () => (await records(page, 'drafts')).map((d) => d.sketchEntities), { timeout: 10000 }).toEqual([null]);
		await page.reload();
		await waitEngine(page);
		await page.waitForTimeout(1000);
		expect(await page.evaluate(() => window.__waffle.getState().sketchMode.active)).toBe(false);
		expectNoAnyCrash(crashes);
	});

	test('ask: Discard drops this tab\'s draft and keeps the stored document', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await setPolicy(page, 'ask');
		const seeded = await seedTwoTabDocument(page);

		await page.reload();
		await waitEngine(page);
		await expect(page.getByTestId('auto-restore-dialog')).toBeVisible();
		await page.getByTestId('auto-restore-discard').click();
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);

		expect(await records(page, 'documents')).toEqual([seeded]);
		expect(await records(page, 'drafts')).toEqual([]);
		expectNoAnyCrash(crashes);
	});

	test('ask: Restore reopens every tab under the same identity', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await setPolicy(page, 'ask');
		const seeded = await seedTwoTabDocument(page);

		await page.reload();
		await waitEngine(page);
		await page.getByTestId('auto-restore-restore').click();
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);

		const info = await page.evaluate(() => window.__waffle.getDocumentInfo());
		expect(info.storageId).toBe(seeded.docId);
		expect(info.documentId).toBe(seeded.documentId);
		expect(info.tabs).toHaveLength(2);

		// A later edit autosaves back into the same record with both tabs intact.
		// (Wait for the reopened body first: the open cancels autosaves scheduled mid-load.)
		await page.waitForFunction(() => window.__waffle.getMeshes().length >= 1, null, { timeout: 30000 });
		await page.evaluate(() => window.__waffle.addTab('Part'));
		await expect.poll(async () => (await records(page, 'documents')).map((d) => d.tabs), { timeout: 15000 }).toEqual([3]);
		expect((await records(page, 'documents'))[0].documentId).toBe(seeded.documentId);
		expectNoAnyCrash(crashes);
	});

	test('never, chosen in Settings: a reload starts empty and offers nothing', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const seeded = await seedTwoTabDocument(page);

		await page.getByTestId('toolbar-btn-settings').click();
		await page.getByTestId('setting-restore-on-reload').selectOption('never');
		expect((await page.evaluate(() => window.__waffle.getSettings())).restoreOnReload).toBe('never');

		await page.reload();
		await waitEngine(page);
		await page.waitForTimeout(1000);
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);
		const info = await page.evaluate(() => window.__waffle.getDocumentInfo());
		expect(info.storageId).not.toBe(seeded.docId);
		expect(await page.evaluate(() => window.__waffle.getMeshes().length)).toBe(0);
		expect((await records(page, 'documents'))[0]).toEqual(seeded);
		expectNoAnyCrash(crashes);
	});
});
