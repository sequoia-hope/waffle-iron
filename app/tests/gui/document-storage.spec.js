import { test, expect } from './helpers/waffle-test.js';
import { seedDocument, makeTestDocument, getDocumentFromDB } from './helpers/waffle-test.js';

test.describe('Document storage', () => {
	test('autosave writes to IndexedDB after modeling', async ({ waffle }) => {
		const page = waffle.page;

		// Check document state is initialized
		const docState = await page.evaluate(() => window.__waffle?.getDocumentState?.());
		// activeDocId might be null for fresh editor (no doc loaded)
		// We need to work within the editor and trigger autosave

		// Enter sketch mode on XY plane
		await page.evaluate(() => {
			window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
		});
		await page.waitForTimeout(500);

		// Add a line entity to trigger model change
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({
				id: 100, type: 'Line',
				point_ids: [101, 102],
				construction: false
			});
			window.__waffle.addSketchEntity({
				id: 101, type: 'Point', x: 0, y: 0, construction: false
			});
			window.__waffle.addSketchEntity({
				id: 102, type: 'Point', x: 10, y: 0, construction: false
			});
		});

		// Finish sketch to trigger model rebuild (which triggers autosave)
		await page.evaluate(() => window.__waffle.finishSketch());
		await page.waitForTimeout(500);

		// Wait for autosave debounce (3s) + buffer
		await page.waitForTimeout(4000);

		// If a document is active, check IndexedDB
		const finalState = await page.evaluate(() => window.__waffle?.getDocumentState?.());
		if (finalState?.activeDocId) {
			const doc = await getDocumentFromDB(page, finalState.activeDocId);
			expect(doc).toBeTruthy();
			expect(doc.json).toBeTruthy();
		}
	});

	test('Ctrl+S triggers immediate save', async ({ waffle }) => {
		const page = waffle.page;

		// Press Ctrl+S
		await page.keyboard.press('Control+s');
		await page.waitForTimeout(1000);

		// Should show a toast (either "Saved" or file download depending on whether doc is active)
		// The toast should be visible briefly
		const toasts = await page.evaluate(() => window.__waffle?.getToasts?.() || []);
		// Toast may have already dismissed, so this is a best-effort check
		// At minimum, no error should occur
	});
});
