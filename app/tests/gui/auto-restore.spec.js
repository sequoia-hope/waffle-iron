/**
 * The reload restore offer (`AutoRestoreDialog`): Discard dismisses the offer
 * and never deletes the stored document; Restore reopens the WHOLE document
 * (every tab, its identity) so the next autosave cannot overwrite it with a
 * one-tab copy.
 */
import { test, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { createExtrudedBox } from './helpers/geometry.js';

/** Every record in the local IndexedDB document store, with its parsed tab count. */
async function storedDocs(page) {
	return page.evaluate(
		() =>
			new Promise((resolve, reject) => {
				const open = indexedDB.open('waffle-iron');
				open.onerror = () => reject(open.error);
				open.onsuccess = () => {
					const db = open.result;
					if (!db.objectStoreNames.contains('documents')) return resolve([]);
					const req = db.transaction('documents', 'readonly').objectStore('documents').getAll();
					req.onerror = () => reject(req.error);
					req.onsuccess = () =>
						resolve(
							req.result.map((d) => {
								const parsed = JSON.parse(d.json);
								return { id: d.id, documentId: parsed.document?.id, tabs: parsed.tabs?.length ?? 0 };
							})
						);
				};
			})
	);
}

async function waitEngine(page) {
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
}

/** A two-tab document with geometry, autosaved to this browser; returns its record. */
async function seedTwoTabDocument(page) {
	await page.goto('/');
	await waitEngine(page);
	await createExtrudedBox(page);
	await page.evaluate(() => window.__waffle.addTab('Part'));
	await expect.poll(async () => (await storedDocs(page)).find((d) => d.tabs === 2), { timeout: 15000 }).toBeTruthy();
	const docs = await storedDocs(page);
	expect(docs).toHaveLength(1);
	return docs[0];
}

test.describe('Reload restore offer', () => {
	test('Discard dismisses the offer and keeps the stored document', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const seeded = await seedTwoTabDocument(page);

		await page.reload();
		await waitEngine(page);
		await expect(page.getByTestId('auto-restore-dialog')).toBeVisible();
		await page.getByTestId('auto-restore-discard').click();
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);

		expect(await storedDocs(page)).toEqual([seeded]);
		expectNoAnyCrash(crashes);
	});

	test('Restore reopens every tab under the same identity', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const seeded = await seedTwoTabDocument(page);

		await page.reload();
		await waitEngine(page);
		await page.getByTestId('auto-restore-restore').click();
		await expect(page.getByTestId('auto-restore-dialog')).toHaveCount(0);

		const info = await page.evaluate(() => window.__waffle.getDocumentInfo());
		expect(info.storageId).toBe(seeded.id);
		expect(info.documentId).toBe(seeded.documentId);
		expect(info.tabs).toHaveLength(2);

		// A later edit autosaves back into the same record with both tabs intact.
		// (Wait for the reopened body first: the open cancels autosaves scheduled mid-load.)
		await page.waitForFunction(() => window.__waffle.getMeshes().length >= 1, null, { timeout: 30000 });
		await page.evaluate(() => window.__waffle.addTab('Part'));
		await expect.poll(async () => (await storedDocs(page)).map((d) => d.tabs), { timeout: 15000 }).toEqual([3]);
		expect((await storedDocs(page))[0].documentId).toBe(seeded.documentId);
		expectNoAnyCrash(crashes);
	});
});
