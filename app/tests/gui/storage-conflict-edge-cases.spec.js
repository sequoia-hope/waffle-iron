/**
 * Targeted regression spec for storage conflict edge cases:
 *
 *   1. Pre-branch drafts missing `baseRev` (`draft.baseRev === undefined`) adopt
 *      the stored revision at restore time when the stored JSON equals the
 *      draft's JSON (no noisy conflict), and conflict safely when they differ.
 *   2. `localFallbackState` seeds from a leftover `localFallback` record across
 *      page reloads so subsequent offline saves update it instead of failing
 *      silently, and surfaces `documentConflict` if a non-fallback local record
 *      conflicts.
 *   3. Home-page rename conflicts render a visible `[data-testid="home-conflict-notice"]`
 *      banner instead of only logging to `console.warn`.
 *   4. `openFromLocator` returns coherent `(json, rev, documentName)` matching
 *      the winning stored record when `local.put` conflicts.
 */
import { test as rawTest, expect } from '@playwright/test';
import { getDocumentFromDB, seedDocument } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const DOC_DRAFT_MATCH = '8a001122-3333-4444-8555-666677778888';
const DOC_DRAFT_DIFF = '8b001122-3333-4444-8555-666677778888';
const DOC_FALLBACK = '8c001122-3333-4444-8555-666677778888';
const DOC_FALLBACK_CONFLICT = '8d001122-3333-4444-8555-666677778888';

function makeV4Doc({ id, name = 'Edge Case Doc', unit = 'mm' }) {
	const tabId = '8e001122-3333-4444-8555-666677778888';
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

async function seedRawLegacyDraft(page, { tabKey, docId, name, json, modified }) {
	await page.evaluate(
		async (record) => {
			sessionStorage.setItem('waffle-tab-key', record.tabKey);
			await new Promise((resolve, reject) => {
				const openReq = indexedDB.open('waffle-iron-drafts', 1);
				openReq.onupgradeneeded = () => {
					const db = openReq.result;
					if (!db.objectStoreNames.contains('drafts')) {
						db.createObjectStore('drafts', { keyPath: 'tabKey' }).createIndex('modified', 'modified');
					}
				};
				openReq.onsuccess = () => {
					const db = openReq.result;
					const tx = db.transaction('drafts', 'readwrite');
					// Intentionally omit `baseRev` and `baseRevProvider` to match pre-branch drafts.
					tx.objectStore('drafts').put({
						tabKey: record.tabKey,
						docId: record.docId,
						name: record.name,
						json: record.json,
						sketch: null,
						modified: record.modified
					});
					tx.oncomplete = () => {
						db.close();
						resolve();
					};
					tx.onerror = () => reject(tx.error);
				};
				openReq.onerror = () => reject(openReq.error);
			});
		},
		{ tabKey, docId, name, json, modified }
	);
}

rawTest.describe('Storage conflict edge cases', () => {
	// Cover `context` fixture setup (`browser.newContext`) when sharing a 6-worker
	// run with `examples.spec.js` (Gravel bike / Eiffel Tower), matching
	// `examples.spec.js:39-47`, and drop the WASM/WebGL renderer on teardown
	// (`examples.spec.js:68-70`) so the worker's next test starts clean.
	rawTest.describe.configure({ timeout: 180000 });
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest('pre-branch drafts without baseRev restore cleanly when matching stored JSON and conflict when diverged', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		// Seed on a plain static asset of the same origin so SvelteKit's layout
		// does not start `initEngine()` / `tabKey()` / `pagehide` while seeding.
		await page.goto('/manifest.json');

		const now = Date.now();
		const sameJson = JSON.stringify(makeV4Doc({ id: DOC_DRAFT_MATCH, name: 'Matching Draft', unit: 'mm' }));
		await seedDocument(page, {
			id: DOC_DRAFT_MATCH,
			json: sameJson,
			created: now - 10_000,
			modified: now - 9_000
		});
		await seedRawLegacyDraft(page, {
			tabKey: 'legacy-tab-match',
			docId: DOC_DRAFT_MATCH,
			name: 'Matching Draft',
			json: sameJson,
			modified: now - 8_000
		});

		// Navigate to editor; startup restore prompt offers the tab's pre-branch draft.
		await page.goto('/');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.engineReady === true && window.__waffle?.getAutoRestoreState()?.available === true,
			null,
			{ timeout: 30000 }
		);
		const offerMatch = await page.evaluate(() => window.__waffle.getAutoRestoreState());
		expect(offerMatch?.source).toBe('draft');
		expect(offerMatch?.docId).toBe(DOC_DRAFT_MATCH);
		const restoredMatch = await page.evaluate(() => window.__waffle.restoreAutoSave());
		expect(restoredMatch).toBe(true);
		await page.waitForFunction(
			(id) => window.__waffle.getDocumentState().activeDocId === id,
			DOC_DRAFT_MATCH,
			{ timeout: 15000 }
		);

		// Saving after restoring an identical pre-branch draft must NOT conflict.
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		await page.keyboard.press('Control+s');
		await expect(page.locator('[data-testid="toast-container"]')).toContainText('Saved', {
			timeout: 10000
		});
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeHidden();

		// Leave `/` BEFORE seeding the second tabKey/draft so the first editor's
		// `pagehide` (`rememberTabKey()` / `flushAutoSave()`) cannot race with seeding.
		await page.goto('/manifest.json');
		const now2 = Date.now();
		const storedJson = JSON.stringify(makeV4Doc({ id: DOC_DRAFT_DIFF, name: 'Newer Stored', unit: 'in' }));
		const divergedDraftJson = JSON.stringify(makeV4Doc({ id: DOC_DRAFT_DIFF, name: 'Older Draft', unit: 'cm' }));
		await seedDocument(page, {
			id: DOC_DRAFT_DIFF,
			json: storedJson,
			created: now2 - 6_000,
			modified: now2 - 5_000
		});
		await seedRawLegacyDraft(page, {
			tabKey: 'legacy-tab-diff',
			docId: DOC_DRAFT_DIFF,
			name: 'Older Draft',
			json: divergedDraftJson,
			modified: now2 - 4_000
		});

		await page.goto('/');
		await page.waitForFunction(
			() => window.__waffle?.getState()?.engineReady === true && window.__waffle?.getAutoRestoreState()?.available === true,
			null,
			{ timeout: 30000 }
		);
		const offerDiff = await page.evaluate(() => window.__waffle.getAutoRestoreState());
		expect(offerDiff?.source).toBe('draft');
		expect(offerDiff?.docId).toBe(DOC_DRAFT_DIFF);
		const restoredDiff = await page.evaluate(() => window.__waffle.restoreAutoSave());
		expect(restoredDiff).toBe(true);
		await page.waitForFunction(
			(id) => window.__waffle.getDocumentState().activeDocId === id,
			DOC_DRAFT_DIFF,
			{ timeout: 15000 }
		);

		await page.keyboard.press('Control+s');
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeVisible({ timeout: 10000 });
		const preserved = await getDocumentFromDB(page, DOC_DRAFT_DIFF);
		expect(JSON.parse(preserved.json).document.name).toBe('Newer Stored');
		expectNoAnyCrash(crashes);
	});

	rawTest('leftover localFallback record updates across sessions and unrelated local collision surfaces conflict', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
			timeout: 30000
		});

		async function renameAndFlushOffline({
			docId,
			json,
			remoteRev,
			newName,
			registerOfflineProvider = false
		}) {
			await page.evaluate(
				async ({ id, docJson, rev, name, register }) => {
					if (register) {
						const { registerProvider, setActiveProvider } = await import(
							'/src/lib/storage/index.js'
						);
						registerProvider({
							id: 'git:offline-test',
							label: 'Offline Git',
							canShare: false,
							async list() {
								return [];
							},
							async get() {
								return null;
							},
							async put() {
								throw new Error('offline network error');
							},
							async delete() {}
						});
						setActiveProvider('git:offline-test');
					}
					await window.__waffle.openDocumentRecord(id, docJson, null, rev, 'git:offline-test');
					window.__waffle.setProjectName(name);
				},
				{
					id: docId,
					docJson: json,
					rev: remoteRev,
					name: newName,
					register: registerOfflineProvider
				}
			);
			await page.waitForFunction(
				() => window.__waffle.hasPendingAutoSave() === true,
				null,
				{ timeout: 10000 }
			);
			const flushed = await page.evaluate(() => window.__waffle.commitPendingAutoSave());
			expect(flushed).toBe(true);
		}

		const initialJson = JSON.stringify(makeV4Doc({ id: DOC_FALLBACK, name: 'Remote Doc', unit: 'mm' }));

		// Session 1: register an offline remote provider whose `put` rejects with a network error.
		await renameAndFlushOffline({
			docId: DOC_FALLBACK,
			json: initialJson,
			remoteRev: 'remote-sha-1',
			newName: 'Offline Session 1',
			registerOfflineProvider: true
		});

		const afterSession1 = await getDocumentFromDB(page, DOC_FALLBACK);
		expect(afterSession1.localFallback).toBe(true);
		expect(JSON.parse(afterSession1.json).document.name).toBe('Offline Session 1');

		// Session 2: reload page so `localFallbackState` resets to `{ docId: null, rev: null }`.
		await page.reload();
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
			timeout: 30000
		});

		await renameAndFlushOffline({
			docId: DOC_FALLBACK,
			json: afterSession1.json,
			remoteRev: 'remote-sha-1',
			newName: 'Offline Session 2',
			registerOfflineProvider: true
		});

		const afterSession2 = await getDocumentFromDB(page, DOC_FALLBACK);
		expect(JSON.parse(afterSession2.json).document.name).toBe('Offline Session 2');
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeHidden();

		// Unrelated non-fallback local record at `DOC_FALLBACK_CONFLICT` must NOT fail silently.
		const now = Date.now();
		await seedDocument(page, {
			id: DOC_FALLBACK_CONFLICT,
			json: JSON.stringify(makeV4Doc({ id: DOC_FALLBACK_CONFLICT, name: 'Unrelated Local Doc', unit: 'in' })),
			created: now - 2_000,
			modified: now - 1_000
		});
		await renameAndFlushOffline({
			docId: DOC_FALLBACK_CONFLICT,
			json: JSON.stringify(makeV4Doc({ id: DOC_FALLBACK_CONFLICT, name: 'Remote Copy', unit: 'mm' })),
			remoteRev: 'remote-sha-9',
			newName: 'Colliding Offline Edit'
		});
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeVisible({ timeout: 10000 });
		const preservedLocal = await getDocumentFromDB(page, DOC_FALLBACK_CONFLICT);
		expect(JSON.parse(preservedLocal.json).document.name).toBe('Unrelated Local Doc');
		expectNoAnyCrash(crashes);
	});

	rawTest('home-page rename conflict displays a visible notice banner and refreshes the grid', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const docId = '8f001122-3333-4444-8555-666677778888';
		await page.goto('/home');
		await seedDocument(page, {
			id: docId,
			json: JSON.stringify(makeV4Doc({ id: docId, name: 'Before Rename', unit: 'mm' })),
			created: 1000,
			modified: 2000
		});
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toContainText('Before Rename', {
			timeout: 10000
		});

		// Inject a concurrent update right after `handleRename` reads `provider.get(docId)`.
		await page.evaluate(async (id) => {
			const { getActiveProvider } = await import('/src/lib/storage/index.js');
			const provider = getActiveProvider();
			const origGet = provider.get.bind(provider);
			let intercepted = false;
			provider.get = async (targetId) => {
				const found = await origGet(targetId);
				if (!intercepted && targetId === id && found) {
					intercepted = true;
					const parsed = JSON.parse(found.doc.json);
					parsed.document.name = 'Concurrent Winner';
					await provider.put(
						{ ...found.doc, json: JSON.stringify(parsed), modified: Date.now() },
						{ base: found.rev }
					);
				}
				return found;
			};
		}, docId);

		await page.locator('[data-testid="doc-menu-btn"]').first().click();
		await page.locator('[data-testid="doc-ctx-rename"]').click();
		const input = page.locator('[data-testid="doc-rename-input"]');
		await expect(input).toBeVisible();
		await input.fill('Stale Rename');
		await input.press('Enter');

		const notice = page.locator('[data-testid="home-conflict-notice"]');
		await expect(notice).toBeVisible({ timeout: 10000 });
		await expect(notice).toContainText('Before Rename');
		await expect(page.locator('[data-testid="document-card"]')).toContainText('Concurrent Winner');

		await page.locator('[data-testid="home-conflict-notice-dismiss"]').click();
		await expect(notice).toBeHidden();
		expectNoAnyCrash(crashes);
	});

	rawTest('openFromLocator returns coherent (json, rev, documentName) when local.put conflicts', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const sharedUrl = 'https://example.invalid/coherent-link.waffle';
		const staleFetchedJson = JSON.stringify(
			makeV4Doc({
				id: '89001122-3333-4444-8555-666677778888',
				name: 'Stale Fetched Copy',
				unit: 'mm'
			})
		);
		const winnerStoredJson = JSON.stringify(
			makeV4Doc({
				id: '89001122-3333-4444-8555-666677778888',
				name: 'Winner Stored Copy',
				unit: 'in'
			})
		);

		await page.route(sharedUrl, async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: staleFetchedJson
			});
		});

		await page.goto('/home');
		await expect(page.locator('[data-testid="home-page"]')).toBeVisible({ timeout: 15000 });

		const result = await page.evaluate(
			async ({ url, winnerJson }) => {
				const { getStore } = await import('/src/lib/storage/index.js');
				const { openFromLocator } = await import('/src/lib/storage/open-link.js');
				const local = getStore();
				const locator = { type: 'Url', url };
				const link = {
					locator,
					resolved: null,
					contentHash: 'git-blob-sha1:1111111111111111111111111111111111111111',
					readOnly: true,
					name: 'coherent-link.waffle'
				};
				const initial = await local.put(
					{
						id: 'lnkcoh01',
						json: winnerJson.replace('Winner Stored Copy', 'Initial Seed Copy'),
						created: 1000,
						modified: 1500,
						link
					},
					{ base: null }
				);

				// Advance `lnkcoh01` right after `openFromLocator` reads its old revision.
				const origGet = local.get.bind(local);
				let raced = false;
				let winnerRev = null;
				local.get = async (id) => {
					const found = await origGet(id);
					if (!raced && id === 'lnkcoh01' && found && found.rev === initial.rev) {
						raced = true;
						const putWinner = await local.put(
							{
								id: 'lnkcoh01',
								json: winnerJson,
								created: 1000,
								modified: 2500,
								link
							},
							{ base: initial.rev }
						);
						winnerRev = putWinner.rev;
					}
					return found;
				};

				const opened = await openFromLocator(locator);
				local.get = origGet;
				const storedAfter = await local.get('lnkcoh01');
				return {
					opened,
					winnerRev,
					storedAfterRev: storedAfter.rev,
					storedAfterName: JSON.parse(storedAfter.doc.json).document.name
				};
			},
			{ url: sharedUrl, winnerJson: winnerStoredJson }
		);

		expect(result.opened.rev).toBe(result.winnerRev);
		expect(result.opened.rev).toBe(result.storedAfterRev);
		expect(JSON.parse(result.opened.json).document.name).toBe('Winner Stored Copy');
		expect(result.opened.documentName).toBe('Winner Stored Copy');
		expect(result.storedAfterName).toBe('Winner Stored Copy');
		expectNoAnyCrash(crashes);
	});
});
