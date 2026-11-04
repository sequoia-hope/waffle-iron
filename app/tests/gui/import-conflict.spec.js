/**
 * Targeted regression spec for import conflict protection and UUID lookup
 * resolution across `loadProject` (File→Open) and agent `document_import`:
 *
 *   1. Importing an older `.waffle` export into an existing stored `document.id`
 *      conflicts instead of silently overwriting the newer stored document;
 *      Reload, Save as a copy, and explicit Replace all work.
 *   2. Ordinary cross-tab edit conflicts do NOT expose the Replace button
 *      (preserving Reload / Save as a copy only).
 *   3. Agent `document_import` into an existing `document.id` refuses with
 *      `SaveFailed` without `replace: true`, refuses with `UserDeclined` when
 *      the user dismisses confirmation, and replaces only when confirmed.
 *   4. A UUID matching a `/open` linked read-only cache record (`doc.link`) or
 *      an older 8-char-keyed own record (`'legacy01'`) never triggers a
 *      spurious cross-key conflict.
 */
import { test as rawTest, expect } from '@playwright/test';
import { getDocumentFromDB, seedDocument } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const DOC_ID = '6a001122-3333-4444-8555-666677778888';
const LINKED_UUID = '6b001122-3333-4444-8555-666677778888';
const LEGACY_UUID = '6c001122-3333-4444-8555-666677778888';

function makeV4Doc({ id = DOC_ID, name = 'Stored Newer', unit = 'in' } = {}) {
	const tabId = '6d001122-3333-4444-8555-666677778888';
	return {
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name,
			created: '2026-10-01T00:00:00.000Z',
			modified: '2026-10-02T00:00:00.000Z',
			display_unit: unit
		},
		sources: [],
		tabs: [
			{
				id: tabId,
				name: 'Part 1',
				kind: { type: 'Part', features: { features: [], active_index: null } }
			}
		],
		active_tab: tabId
	};
}

async function waitForEditorReady(page) {
	await page.waitForURL('/', { timeout: 15000 });
	await page.waitForFunction(
		() =>
			window.__waffle?.getState()?.engineReady === true &&
			typeof window.__waffleAgentExecutor?.executeTool === 'function',
		null,
		{ timeout: 30000 }
	);
}

async function runAgentDocumentImport(page, args) {
	await page.evaluate((toolArgs) => {
		window.__agentImportRes = null;
		void (async () => {
			try {
				window.__agentImportRes = await window.__waffleAgentExecutor.executeTool(
					'document_import',
					toolArgs,
					{
						agentName: 'import-conflict-spec',
						isPaused: () => false,
						pause: () => {},
						isCancelled: () => false
					}
				);
			} catch (err) {
				window.__agentImportRes = { __unexpectedError: String(err?.stack || err) };
			}
		})();
	}, args);
	await page.waitForFunction(() => window.__agentImportRes !== null, null, { timeout: 15000 });
	return page.evaluate(() => window.__agentImportRes);
}

async function importViaFilePicker(page, fileName, docObj) {
	const fcPromise = page.waitForEvent('filechooser');
	await page.evaluate(() => {
		void window.__waffle.loadProject();
	});
	const fc = await fcPromise;
	await fc.setFiles({
		name: fileName,
		mimeType: 'application/json',
		buffer: Buffer.from(JSON.stringify(docObj))
	});
}

rawTest.describe('Import conflict protection and UUID lookup resolution', () => {
	rawTest.describe.configure({ timeout: 180000 });
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest('File→Open over a newer stored document conflicts until Reload, Save as copy, or Replace', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await page.goto('/home');
		await seedDocument(page, {
			id: DOC_ID,
			json: JSON.stringify(makeV4Doc({ id: DOC_ID, name: 'Newer Stored', unit: 'in' })),
			created: 1000,
			modified: 2000
		});

		await page.goto('/');
		await waitForEditorReady(page);

		// Import an older export with the SAME document.id (`unit: 'mm'`).
		await importViaFilePicker(
			page,
			'Older Export.waffle',
			makeV4Doc({ id: DOC_ID, name: 'Older Export', unit: 'mm' })
		);
		await page.waitForFunction(
			() => window.__waffle.getDocumentState().documentName === 'Older Export',
			null,
			{ timeout: 15000 }
		);

		// Saving conflicts instead of silently overwriting `Newer Stored`.
		await page.keyboard.press('Control+s');
		const dialog = page.locator('[data-testid="document-conflict-dialog"]');
		await expect(dialog).toBeVisible({ timeout: 10000 });
		await expect(page.locator('[data-testid="conflict-replace-btn"]')).toBeVisible();

		const untouched1 = await getDocumentFromDB(page, DOC_ID);
		expect(JSON.parse(untouched1.json).document.name).toBe('Newer Stored');
		expect(JSON.parse(untouched1.json).document.display_unit).toBe('in');

		// 1. Save as a copy creates a distinct UUID record and leaves DOC_ID untouched.
		await page.locator('[data-testid="conflict-save-copy-btn"]').click();
		await expect(dialog).toBeHidden({ timeout: 10000 });
		const copyState = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(copyState.activeDocId).not.toBe(DOC_ID);
		expect(copyState.documentName).toBe('Older Export (copy)');
		const untouched2 = await getDocumentFromDB(page, DOC_ID);
		expect(JSON.parse(untouched2.json).document.name).toBe('Newer Stored');

		// 2. Re-import and choose Reload -> loads `Newer Stored` (`in`), discarding the import.
		await importViaFilePicker(
			page,
			'Older Export.waffle',
			makeV4Doc({ id: DOC_ID, name: 'Older Export', unit: 'mm' })
		);
		await page.waitForFunction(
			() => window.__waffle.getDocumentState().documentName === 'Older Export',
			null,
			{ timeout: 15000 }
		);
		await page.keyboard.press('Control+s');
		await expect(dialog).toBeVisible({ timeout: 10000 });
		await page.locator('[data-testid="conflict-reload-btn"]').click();
		await expect(dialog).toBeHidden({ timeout: 10000 });
		await page.waitForFunction(
			() =>
				window.__waffle.getDocumentState().documentName === 'Newer Stored' &&
				window.__waffle.getDocumentState().documentDisplayUnit === 'in',
			null,
			{ timeout: 15000 }
		);

		// Ordinary cross-tab edit conflict on the now-open stored doc must NOT offer Replace.
		await page.evaluate(async (id) => {
			const { getStore } = await import('/src/lib/storage/index.js');
			const store = getStore();
			const cur = await store.get(id);
			const parsed = JSON.parse(cur.doc.json);
			parsed.document.name = 'Other Tab Edit';
			await store.put(
				{ ...cur.doc, json: JSON.stringify(parsed), modified: Date.now() },
				{ base: cur.rev }
			);
		}, DOC_ID);
		await page.keyboard.press('Control+s');
		await expect(dialog).toBeVisible({ timeout: 10000 });
		await expect(page.locator('[data-testid="conflict-replace-btn"]')).toHaveCount(0);
		await page.locator('[data-testid="conflict-reload-btn"]').click();
		await expect(dialog).toBeHidden({ timeout: 10000 });

		// 3. Re-import and explicitly click Replace -> updates DOC_ID in place.
		await importViaFilePicker(
			page,
			'Intentional Replace.waffle',
			makeV4Doc({ id: DOC_ID, name: 'Intentional Replace', unit: 'cm' })
		);
		await page.waitForFunction(
			() => window.__waffle.getDocumentState().documentName === 'Intentional Replace',
			null,
			{ timeout: 15000 }
		);
		await page.keyboard.press('Control+s');
		await expect(dialog).toBeVisible({ timeout: 10000 });
		await page.locator('[data-testid="conflict-replace-btn"]').click();
		await expect(dialog).toBeHidden({ timeout: 10000 });
		await page.waitForFunction(
			async (id) => {
				const { getStore } = await import('/src/lib/storage/index.js');
				const rec = await getStore().get(id);
				return rec && JSON.parse(rec.doc.json).document.display_unit === 'cm';
			},
			DOC_ID,
			{ timeout: 15000 }
		);
		expectNoAnyCrash(crashes);
	});

	rawTest('agent document_import refuses silent overwrite and requires user-confirmed replace', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await page.goto('/home');
		await seedDocument(page, {
			id: DOC_ID,
			json: JSON.stringify(makeV4Doc({ id: DOC_ID, name: 'Stored Newer', unit: 'in' })),
			created: 1000,
			modified: 2000
		});
		await page.goto('/');
		await waitForEditorReady(page);
		// Dismiss the startup restore prompt so `getDocumentLoadBusyReason()` is null
		// (leaving the stored IndexedDB document untouched).
		await page.evaluate(() => window.__waffle.discardAutoSave());
		await page.waitForFunction(
			() => window.__waffle.getDocumentLoadBusyReason() === null,
			null,
			{ timeout: 5000 }
		);

		const exportText = JSON.stringify(makeV4Doc({ id: DOC_ID, name: 'Older Export', unit: 'cm' }));

		// 1. Without `replace: true`: refused with SaveFailed; stored document untouched.
		const refusedNoFlag = await runAgentDocumentImport(page, {
			file_name: 'Older Export.waffle',
			text: exportText
		});
		expect(refusedNoFlag.isError).toBe(true);
		expect(refusedNoFlag.structuredContent.error.code).toBe('SaveFailed');
		const check1 = await getDocumentFromDB(page, DOC_ID);
		expect(JSON.parse(check1.json).document.name).toBe('Stored Newer');

		// 2. With `replace: true`, user dismisses confirm -> UserDeclined; stored document untouched.
		page.once('dialog', (d) => d.dismiss());
		const declined = await runAgentDocumentImport(page, {
			file_name: 'Older Export.waffle',
			text: exportText,
			replace: true
		});
		expect(declined.isError).toBe(true);
		expect(declined.structuredContent.error.code).toBe('UserDeclined');
		const check2 = await getDocumentFromDB(page, DOC_ID);
		expect(JSON.parse(check2.json).document.name).toBe('Stored Newer');

		// 3. With `replace: true`, user accepts confirm -> replaces stored document cleanly.
		page.once('dialog', (d) => d.accept());
		const accepted = await runAgentDocumentImport(page, {
			file_name: 'Replaced Export.waffle',
			text: exportText,
			replace: true
		});
		expect(accepted.isError).toBe(false);
		expect(accepted.structuredContent.storage_id).toBe(DOC_ID);
		expect(accepted.structuredContent.name).toBe('Replaced Export');
		const check3 = await getDocumentFromDB(page, DOC_ID);
		expect(JSON.parse(check3.json).document.name).toBe('Replaced Export');
		expect(JSON.parse(check3.json).document.display_unit).toBe('cm');
		expectNoAnyCrash(crashes);
	});

	rawTest('UUID resolving to a linked record or an 8-char legacy record never causes a spurious cross-key conflict', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await page.goto('/home');

		// 1. Seed a `/open` linked read-only record under 8-char key `lnk00001` whose JSON carries LINKED_UUID.
		await seedDocument(page, {
			id: 'lnk00001',
			json: JSON.stringify(makeV4Doc({ id: LINKED_UUID, name: 'Shared Copy', unit: 'mm' })),
			created: 1000,
			modified: 1500,
			link: {
				locator: { type: 'Url', url: 'https://example.invalid/shared.waffle' },
				resolved: null,
				contentHash: 'git-blob-sha1:0000000000000000000000000000000000000000',
				readOnly: true,
				name: 'shared.waffle'
			}
		});
		// 2. Seed an older 8-char-keyed own record `legacy01` whose JSON carries LEGACY_UUID.
		await seedDocument(page, {
			id: 'legacy01',
			json: JSON.stringify(makeV4Doc({ id: LEGACY_UUID, name: 'Legacy Own', unit: 'in' })),
			created: 1000,
			modified: 1800
		});

		await page.goto('/');
		await waitForEditorReady(page);

		// Importing a file with LINKED_UUID must create an own record at LINKED_UUID with ZERO conflict.
		await importViaFilePicker(
			page,
			'My Own Copy.waffle',
			makeV4Doc({ id: LINKED_UUID, name: 'My Own Copy', unit: 'cm' })
		);
		await page.waitForFunction(
			() => window.__waffle.getDocumentState().documentName === 'My Own Copy',
			null,
			{ timeout: 15000 }
		);
		await page.keyboard.press('Control+s');
		await page.waitForFunction(
			async (id) => {
				const { getStore } = await import('/src/lib/storage/index.js');
				const own = await getStore().get(id);
				return own && !own.doc.link && own.doc.id === id;
			},
			LINKED_UUID,
			{ timeout: 15000 }
		);
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeHidden();
		const linkedStillThere = await getDocumentFromDB(page, 'lnk00001');
		expect(linkedStillThere.link?.readOnly).toBe(true);

		// Importing a file with LEGACY_UUID targets `legacy01` (not a cross-key base mismatch),
		// conflicts cleanly against `legacy01`, and Replace updates `legacy01` in place.
		await importViaFilePicker(
			page,
			'Legacy Updated.waffle',
			makeV4Doc({ id: LEGACY_UUID, name: 'Legacy Updated', unit: 'cm' })
		);
		await page.waitForFunction(
			() => window.__waffle.getDocumentState().documentName === 'Legacy Updated',
			null,
			{ timeout: 15000 }
		);
		const preSaveState = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(preSaveState.activeDocId).toBe('legacy01');
		expect(preSaveState.documentId).toBe(LEGACY_UUID);

		await page.keyboard.press('Control+s');
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeVisible({ timeout: 10000 });
		const conflict = await page.evaluate(() => window.__waffle.getDocumentConflict());
		expect(conflict.docId).toBe('legacy01');
		expect(conflict.theirsRev).toBeTruthy();

		await page.locator('[data-testid="conflict-replace-btn"]').click();
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeHidden({ timeout: 10000 });
		const legacyAfter = await getDocumentFromDB(page, 'legacy01');
		expect(JSON.parse(legacyAfter.json).document.name).toBe('Legacy Updated');
		expect(JSON.parse(legacyAfter.json).document.display_unit).toBe('cm');
		// `getDocumentFromDB` reads raw `IDBObjectStore.get(key)`, which resolves
		// `undefined` when no duplicate record exists under `LEGACY_UUID`.
		const duplicateAtUuid = await getDocumentFromDB(page, LEGACY_UUID);
		expect(duplicateAtUuid).toBeUndefined();
		const resolvedByUuid = await page.evaluate(async (id) => {
			const { getStore } = await import('/src/lib/storage/index.js');
			return getStore().get(id);
		}, LEGACY_UUID);
		expect(resolvedByUuid?.doc?.id).toBe('legacy01');
		expect(JSON.parse(resolvedByUuid.doc.json).document.name).toBe('Legacy Updated');
		expectNoAnyCrash(crashes);
	});
});
