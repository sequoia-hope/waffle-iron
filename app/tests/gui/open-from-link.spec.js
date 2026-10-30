/**
 * v4 Phase 2 — open-from-link (specs/waffle_v4_document_model.md §7.1, §7.4):
 * a share link is a locator. Opening `/open?remote=&path=&ref=` fetches the
 * document at the RESOLVED commit from a mocked GitHub, creates a linked
 * read-only local record, and shows the read-only banner; Ctrl+S refuses;
 * "Fork to edit" makes an editable copy with a new document id whose
 * `Relative` sources are rebased to Git locators pinned at that commit; the
 * legacy `/?src=<raw url>` link redirects here; a private repo asks for a
 * per-host token and retries; the GitHub provider's share URL is this link.
 */
import fs from 'fs';
import { test as rawTest, expect } from '@playwright/test';
import { getDocumentFromDB } from './helpers/waffle-test.js';
import { clickTool } from './helpers/toolbar.js';

const CUBE_STEP = fs.readFileSync(new URL('./fixtures/cube.step', import.meta.url), 'utf8');

const SHA = '9fceb02a9fceb02a9fceb02a9fceb02a9fceb02a';
const REMOTE = 'https://github.com/acme/parts';
const PATH = 'brackets/bracket.waffle';
const DOC_ID = '6f1c2a4e-1111-4222-8333-444455556666';
const TAB_ID = '9068ef01-1111-4222-8333-444455556666';
const SRC_ID = '3b9e0000-1111-4222-8333-444455556666';

/** The shared document: one empty Part tab and one Relative source. */
function sharedDoc() {
	return {
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: { id: DOC_ID, name: 'Shared Bracket', created: '2026-09-01T00:00:00.000Z', modified: '2026-09-01T00:00:00.000Z', display_unit: 'mm' },
		sources: [
			{ id: SRC_ID, name: 'bolt.waffle', kind: { type: 'Waffle' }, locator: { type: 'Relative', path: '../fasteners/bolt.waffle' } }
		],
		tabs: [{ id: TAB_ID, name: 'Part 1', kind: { type: 'Part', features: { features: [], active_index: null } } }],
		active_tab: TAB_ID
	};
}

const b64 = (s) => Buffer.from(s, 'utf8').toString('base64');

/** Mock the GitHub API for acme/parts. `needsToken` ⇒ 401 without Authorization. */
async function mockGitHub(page, { needsToken = false } = {}) {
	const calls = [];
	await page.route('https://api.github.com/**', async (route) => {
		const req = route.request();
		const url = req.url();
		calls.push(url);
		if (needsToken && !req.headers()['authorization']) {
			return route.fulfill({ status: 401, contentType: 'application/json', body: '{"message":"Requires authentication"}' });
		}
		if (url.endsWith('/repos/acme/parts/commits/main')) {
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA }) });
		}
		if (url.includes(`/repos/acme/parts/contents/brackets/bracket.waffle?ref=${SHA}`)) {
			return route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ content: b64(JSON.stringify(sharedDoc())), sha: 'b10b', encoding: 'base64' })
			});
		}
		return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
	});
	return calls;
}

/**
 * Wait until the editor holds the LINKED document. `activeDocId` alone is not
 * enough: the editor's direct-`/` bootstrap mints one before
 * `loadPendingDocument` adopts the pending link (the legacy `/?src=` path
 * visits `/` first, so the engine is already up when `/open` hands off).
 */
async function waitForEditor(page) {
	await page.waitForURL('/', { timeout: 20000 });
	await page.waitForFunction(() => typeof window.__waffle !== 'undefined', { timeout: 30000 });
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
	await page.waitForFunction(() => window.__waffle?.getDocumentState?.()?.documentLink != null, { timeout: 15000 });
}

const OPEN_URL = `/open?remote=${encodeURIComponent(REMOTE)}&path=${encodeURIComponent(PATH)}&ref=main`;

rawTest.describe('Open from link', () => {
	rawTest('a public repo link opens as a linked, read-only document', async ({ page }) => {
		const calls = await mockGitHub(page);
		await page.goto(OPEN_URL);
		await waitForEditor(page);

		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.readOnly).toBe(true);
		expect(state.documentName).toBe('Shared Bracket');
		expect(state.documentId).toBe(DOC_ID);
		expect(state.documentLink.locator).toEqual({ type: 'Git', remote: REMOTE, path: PATH, ref: { type: 'Branch', name: 'main' } });
		expect(state.documentLink.resolved.commit).toBe(SHA);
		expect(state.documentLink.contentHash).toBe('git-blob-sha1:b10b');
		// Content was fetched at the resolved commit, not at the branch name.
		expect(calls.some((u) => u.includes(`?ref=${SHA}`))).toBe(true);
		expect(calls.some((u) => u.includes('?ref=main'))).toBe(false);

		// Banner names the source and the commit.
		const banner = page.locator('[data-testid="linked-doc-banner"]');
		await expect(banner).toBeVisible();
		await expect(page.locator('[data-testid="linked-doc-locator"]')).toContainText('github.com/acme/parts/brackets/bracket.waffle @ main');
		await expect(page.locator('[data-testid="linked-doc-commit"]')).toContainText(SHA.slice(0, 7));

		// The linked record is in local storage, carrying its provenance.
		const stored = await getDocumentFromDB(page, state.activeDocId);
		expect(stored.link.readOnly).toBe(true);
		expect(stored.link.locator.path).toBe(PATH);
		expect(JSON.parse(stored.json).document.id).toBe(DOC_ID);

		// Ctrl+S is refused with a read-only toast and writes nothing.
		await page.keyboard.press('Control+s');
		await page.waitForFunction(
			() => (window.__waffle.getToasts() || []).some((t) => /read-only/i.test(t.message)),
			{ timeout: 5000 }
		);
		const after = await getDocumentFromDB(page, state.activeDocId);
		expect(after.modified).toBe(stored.modified);
	});

	rawTest('fork makes an editable copy with a new id and commit-pinned sources', async ({ page }) => {
		await mockGitHub(page);
		await page.goto(OPEN_URL);
		await waitForEditor(page);
		const before = await page.evaluate(() => window.__waffle.getDocumentState());

		await page.locator('[data-testid="linked-doc-fork"]').click();
		await page.waitForFunction(() => window.__waffle.getDocumentState().readOnly === false, { timeout: 15000 });
		await page.waitForFunction(
			(prev) => window.__waffle.getDocumentState().activeDocId !== prev,
			before.activeDocId,
			{ timeout: 15000 }
		);
		const after = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(after.documentLink).toBeNull();
		expect(after.documentId).not.toBe(DOC_ID);
		expect(after.documentName).toBe('Shared Bracket');
		await expect(page.locator('[data-testid="linked-doc-banner"]')).toHaveCount(0);

		// The fork was saved (wait for the record) with rebased sources.
		await page.waitForFunction(async (id) => {
			return await new Promise((resolve) => {
				const req = indexedDB.open('waffle-iron', 1);
				req.onsuccess = () => {
					const get = req.result.transaction('documents', 'readonly').objectStore('documents').get(id);
					get.onsuccess = () => resolve(!!get.result);
					get.onerror = () => resolve(false);
				};
				req.onerror = () => resolve(false);
			});
		}, after.activeDocId, { timeout: 15000 });
		const fork = await getDocumentFromDB(page, after.activeDocId);
		expect(fork.link ?? null).toBeNull();
		const forkJson = JSON.parse(fork.json);
		expect(forkJson.document.id).toBe(after.documentId);
		expect(forkJson.sources).toHaveLength(1);
		expect(forkJson.sources[0].id).toBe(SRC_ID);
		// `host` stays absent (⇒ inferred): the share link carried none, and the
		// rebase copies the base locator's host verbatim.
		expect(forkJson.sources[0].locator).toEqual({
			type: 'Git',
			remote: REMOTE,
			path: 'fasteners/bolt.waffle',
			ref: { type: 'Commit', sha: SHA }
		});
		expect(forkJson.sources[0].resolved.commit).toBe(SHA);

		// The original linked record is untouched.
		const original = await getDocumentFromDB(page, before.activeDocId);
		expect(JSON.parse(original.json).sources[0].locator.type).toBe('Relative');
		expect(original.link.readOnly).toBe(true);
	});

	rawTest('the legacy /?src=<raw url> share link redirects to /open and opens', async ({ page }) => {
		await mockGitHub(page);
		const raw = `https://raw.githubusercontent.com/acme/parts/main/${PATH}`;
		await page.goto(`/?src=${encodeURIComponent(raw)}`);
		await waitForEditor(page);
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.readOnly).toBe(true);
		expect(state.documentLink.locator.remote).toBe(REMOTE);
		expect(state.documentLink.locator.host).toBe('github');
	});

	rawTest('a private repo asks for a host token, stores it per host, and retries', async ({ page }) => {
		await mockGitHub(page, { needsToken: true });
		await page.goto('/home');
		await page.evaluate(() => localStorage.clear());
		await page.goto(OPEN_URL);
		const form = page.locator('[data-testid="open-token-form"]');
		await expect(form).toBeVisible({ timeout: 15000 });
		await expect(form).toContainText('https://github.com');
		await page.locator('[data-testid="open-token-input"]').fill('ghp_secret');
		await page.locator('[data-testid="open-token-submit"]').click();
		await waitForEditor(page);
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.readOnly).toBe(true);
		const tokens = await page.evaluate(() => JSON.parse(localStorage.getItem('waffle-host-tokens') || '[]'));
		expect(tokens).toEqual([{ host_url: 'https://github.com', token: 'ghp_secret', kind: 'github' }]);
	});

	rawTest('an unreadable link reports the error instead of an empty editor', async ({ page }) => {
		await page.route('https://api.github.com/**', (route) =>
			route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' })
		);
		await page.goto(OPEN_URL);
		await expect(page.locator('[data-testid="open-error"]')).toBeVisible({ timeout: 15000 });
		await expect(page.locator('[data-testid="open-error"]')).toContainText('not found');

		await page.goto('/open?remote=https://github.com/acme/parts');
		await expect(page.locator('[data-testid="open-error"]')).toContainText('remote and path');
	});

	rawTest("the GitHub provider's share URL is an /open locator link", async ({ page }) => {
		await page.route('https://api.github.com/repos/acme/parts/contents/.waffle-index.json**', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					content: b64(JSON.stringify([{ id: 'g1', name: 'Bracket', filename: 'bracket.waffle', created: 1, modified: 1 }])),
					sha: 'idx',
					encoding: 'base64'
				})
			})
		);
		await page.goto('/home');
		const r = await page.evaluate(async () => {
			const { GitHubStore } = await import('/src/lib/storage/github.js');
			const store = new GitHubStore('tok', 'acme', 'parts');
			return { share: await store.getShareUrl('g1'), locator: await store.getLocator('g1') };
		});
		expect(r.share).toBe(`http://localhost:5173/open?remote=${encodeURIComponent(REMOTE)}&path=bracket.waffle&ref=main`);
		expect(r.locator).toEqual({ type: 'Git', remote: REMOTE, path: 'bracket.waffle', ref: { type: 'Branch', name: 'main' }, host: 'github' });
	});
});

// ---------------------------------------------------------- P2-3: sources

const STEP_SRC_ID = '5e7a0000-1111-4222-8333-444455556666';
const IMPORT_FEATURE_ID = '7b1c0000-1111-4222-8333-444455556666';

/** The shared doc plus a linked STEP source (Relative) and the feature using it. */
function sharedDocWithStep() {
	const d = sharedDoc();
	d.sources.push({
		id: STEP_SRC_ID,
		name: 'cube.step',
		kind: { type: 'Step' },
		locator: { type: 'Relative', path: '../parts/cube.step' },
		pack: false
	});
	d.tabs[0].kind.features.features.push({
		id: IMPORT_FEATURE_ID,
		name: 'Import cube.step',
		suppressed: false,
		references: [],
		operation: {
			type: 'ImportedBody',
			params: { file_name: 'cube.step', source_id: STEP_SRC_ID, translation_m: [0, 0, 0], rotation_deg: [0, 0, 0], scale: 1 }
		}
	});
	return d;
}

/** GitHub mock serving the linked document AND the STEP file at the commit. */
async function mockGitHubWithStep(page) {
	const calls = [];
	await page.route('https://api.github.com/**', async (route) => {
		const url = route.request().url();
		calls.push(url);
		if (url.endsWith('/repos/acme/parts/commits/main')) {
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA }) });
		}
		if (url.includes(`/repos/acme/parts/contents/brackets/bracket.waffle?ref=${SHA}`)) {
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64(JSON.stringify(sharedDocWithStep())), sha: 'b10b', encoding: 'base64' }) });
		}
		if (url.includes(`/repos/acme/parts/contents/parts/cube.step?ref=${SHA}`)) {
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64(CUBE_STEP), sha: 'c0be', encoding: 'base64' }) });
		}
		return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
	});
	return calls;
}

rawTest.describe('Linked sources', () => {
	rawTest('a linked document resolves its Relative STEP source against its own location and builds the body', async ({ page }) => {
		const calls = await mockGitHubWithStep(page);
		await page.goto(OPEN_URL);
		await waitForEditor(page);
		// The import feature must build — no SourceUnavailable error — once
		// the host fetched the STEP at the same commit the document came from.
		await page.waitForFunction(
			() => window.__waffle.getMeshes().length === 1 && window.__waffle.getFeatureErrors().size === 0,
			{ timeout: 30000 }
		);
		expect(calls.some((u) => u.includes(`/contents/parts/cube.step?ref=${SHA}`))).toBe(true);
		const sources = await page.evaluate(() => window.__waffle.listSources());
		const step = sources.find((s) => s.id === STEP_SRC_ID);
		expect(step.available).toBe(true);
		expect(step.resolved.commit).toBe(SHA);
		expect(step.content_hash).toMatch(/^git-blob-sha1:[0-9a-f]{40}$/);
		expect(step.locator.type).toBe('Relative');
	});

	rawTest('an unresolvable source is loud, and the document still opens', async ({ page }) => {
		await page.route('https://api.github.com/**', async (route) => {
			const url = route.request().url();
			if (url.endsWith('/repos/acme/parts/commits/main')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA }) });
			}
			if (url.includes('/contents/brackets/bracket.waffle?ref=')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64(JSON.stringify(sharedDocWithStep())), sha: 'b10b', encoding: 'base64' }) });
			}
			return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
		});
		await page.goto(OPEN_URL);
		await waitForEditor(page);
		await page.waitForFunction(
			() => (window.__waffle.getToasts() || []).some((t) => /Source unavailable/.test(t.message) && /cube\.step/.test(t.message)),
			{ timeout: 20000 }
		);
		const errors = await page.evaluate(() => [...window.__waffle.getFeatureErrors().values()].map(String));
		expect(errors.some((m) => /unavailable/i.test(m))).toBe(true);
		expect(await page.evaluate(() => window.__waffle.getDocumentState().documentName)).toBe('Shared Bracket');
		expect(await page.evaluate(() => window.__waffle.getMeshes().length)).toBe(0);
	});

	rawTest('Link STEP imports a file by URL as a linked, unpacked source pinned to the fetched commit', async ({ page }) => {
		await page.route('https://api.github.com/**', async (route) => {
			const url = route.request().url();
			if (url.endsWith('/repos/acme/parts/commits/main')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA }) });
			}
			if (url.includes(`/repos/acme/parts/contents/parts/cube.step?ref=${SHA}`)) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64(CUBE_STEP), sha: 'c0be', encoding: 'base64' }) });
			}
			return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
		});
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });

		await clickTool(page, 'import-link');
		const dialog = page.locator('[data-testid="import-link-dialog"]');
		await expect(dialog).toBeVisible();
		await page.locator('[data-testid="import-link-url"]').fill('https://github.com/acme/parts/blob/main/parts/cube.step');
		await page.locator('[data-testid="import-link-submit"]').click();
		await expect(dialog).toBeHidden({ timeout: 20000 });
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
		// Placement modal opens like a file import; apply it.
		await page.locator('[data-testid="import-apply"]').click();

		const json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources).toHaveLength(1);
		const src = json.sources[0];
		expect(src.kind).toEqual({ type: 'Step' });
		expect(src.locator).toEqual({ type: 'Git', remote: REMOTE, path: 'parts/cube.step', ref: { type: 'Branch', name: 'main' }, host: 'github' });
		expect(src.embed ?? null).toBeNull();
		expect(src.pack ?? false).toBe(false);
		expect(src.resolved.commit).toBe(SHA);
		expect(src.content_hash).toMatch(/^git-blob-sha1:[0-9a-f]{40}$/);
		const feature = json.tabs[0].kind.features.features.find((f) => f.operation.type === 'ImportedBody');
		expect(feature.operation.params.source_id).toBe(src.id);
		expect(feature.operation.params.blob ?? null).toBeNull();
		expect(json.tabs[0].kind.features.provenance[feature.id].origin).toEqual({ type: 'Import', source_id: src.id });
	});
});

// ------------------------------------------------ P2-4: the Sources panel

const SHA2 = '0a1b2c3d0a1b2c3d0a1b2c3d0a1b2c3d0a1b2c3d';

/** GitHub mock whose `main` tip moves from SHA to SHA2 after `tipMovesAfter` resolves. */
async function mockMovingTip(page, { tipMovesAfter = 1 } = {}) {
	let resolves = 0;
	await page.route('https://api.github.com/**', async (route) => {
		const url = route.request().url();
		if (url.endsWith('/repos/acme/parts/commits/main')) {
			resolves++;
			const sha = resolves > tipMovesAfter ? SHA2 : SHA;
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha }) });
		}
		const m = /\/contents\/parts\/cube\.step\?ref=([0-9a-f]{40})$/.exec(url);
		if (m && (m[1] === SHA || m[1] === SHA2)) {
			// The same geometry at both commits; a different blob id tells them apart.
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64(CUBE_STEP), sha: m[1] === SHA ? 'c0be' : 'c0bf', encoding: 'base64' }) });
		}
		return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
	});
}

async function linkCube(page) {
	await page.goto('/');
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
	const ok = await page.evaluate(() => window.__waffle.importStepFromLink('https://github.com/acme/parts/blob/main/parts/cube.step'));
	expect(ok).toBe(true);
	await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
	await page.locator('[data-testid="import-apply"]').click();
	await expect(page.locator('[data-testid="source-item-0"]')).toBeVisible();
}

rawTest.describe('Sources panel', () => {
	rawTest('lists the linked source, pins it to the resolved commit, and pack embeds it', async ({ page }) => {
		await mockMovingTip(page, { tipMovesAfter: 99 });
		await linkCube(page);
		await expect(page.locator('[data-testid="source-status-0"]')).toContainText(`main @ ${SHA.slice(0, 7)}`);

		// Pin → ref becomes Commit{resolved}; content kept (body still there).
		await page.locator('[data-testid="source-pin-0"]').click();
		await expect(page.locator('[data-testid="source-status-0"]')).toContainText(`pinned ${SHA.slice(0, 7)}`);
		await expect(page.locator('[data-testid="source-pin-0"]')).toHaveCount(0);
		let json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources[0].locator.ref).toEqual({ type: 'Commit', sha: SHA });
		expect(json.sources[0].resolved.commit).toBe(SHA);
		expect(json.sources[0].embed ?? null).toBeNull();
		expect(await page.evaluate(() => window.__waffle.getMeshes().length)).toBe(1);

		// Pack → the file carries the content; unpack → linked again.
		await page.locator('[data-testid="source-pack-0"]').check();
		await page.waitForFunction(() => window.__waffle.getSources()[0].pack === true, { timeout: 10000 });
		json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources[0].pack).toBe(true);
		expect(json.sources[0].embed.encoding).toBe('deflate-base64');
		expect(json.sources[0].locator.type).toBe('Git');
		await page.locator('[data-testid="source-pack-0"]').uncheck();
		await page.waitForFunction(() => window.__waffle.getSources()[0].pack === false, { timeout: 10000 });
		json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources[0].embed ?? null).toBeNull();
	});

	rawTest('update-to-tip re-resolves the ref and fetches at the new commit; a pinned source never moves', async ({ page }) => {
		await mockMovingTip(page, { tipMovesAfter: 1 });
		await linkCube(page);
		await expect(page.locator('[data-testid="source-status-0"]')).toContainText(`main @ ${SHA.slice(0, 7)}`);

		await page.locator('[data-testid="source-update-0"]').click();
		await expect(page.locator('[data-testid="source-status-0"]')).toContainText(`main @ ${SHA2.slice(0, 7)}`, { timeout: 15000 });
		const json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources[0].locator.ref).toEqual({ type: 'Branch', name: 'main' });
		expect(json.sources[0].resolved.commit).toBe(SHA2);
		expect(json.sources[0].content_hash).toMatch(/^git-blob-sha1:/);
		await page.waitForFunction(
			() => (window.__waffle.getToasts() || []).some((t) => /updated .* → 0a1b2c3/.test(t.message)),
			{ timeout: 5000 }
		);

		// Pinned ⇒ "update" is not offered and the API refuses to move it.
		await page.locator('[data-testid="source-pin-0"]').click();
		await expect(page.locator('[data-testid="source-update-0"]')).toHaveCount(0);
		const r = await page.evaluate(() => window.__waffle.updateSourceToTip(window.__waffle.getSources()[0].id));
		expect(r).toBe('unchanged');
		expect((await page.evaluate(() => window.__waffle.getSources()))[0].resolved.commit).toBe(SHA2);
	});

	rawTest('an embedded source cannot be unpacked; pack-all embeds every linked one', async ({ page }) => {
		await mockMovingTip(page, { tipMovesAfter: 99 });
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
		// One embedded (file picker) source …
		await page.evaluate((text) => window.__waffle.importStepFromText('cube.step', text), CUBE_STEP);
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 1, { timeout: 30000 });
		await page.locator('[data-testid="import-apply"]').click();
		// … and one linked.
		await page.evaluate(() => window.__waffle.importStepFromLink('https://github.com/acme/parts/blob/main/parts/cube.step'));
		await page.waitForFunction(() => window.__waffle.getMeshes().length === 2, { timeout: 30000 });
		await page.locator('[data-testid="import-apply"]').click();

		await expect(page.locator('[data-testid="source-item-1"]')).toBeVisible();
		await expect(page.locator('[data-testid="source-pack-0"]')).toBeDisabled();
		await expect(page.locator('[data-testid="source-pack-0"]')).toBeChecked();
		await expect(page.locator('[data-testid="source-status-0"]')).toContainText('embedded');

		await page.locator('[data-testid="sources-pack-all"]').click();
		await page.waitForFunction(() => window.__waffle.getSources().every((s) => s.pack), { timeout: 10000 });
		await expect(page.locator('[data-testid="sources-pack-all"]')).toHaveCount(0);
		const json = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(json.sources).toHaveLength(2);
		expect(json.sources.every((s) => s.embed?.encoding === 'deflate-base64')).toBe(true);
		expect(json.sources.map((s) => s.locator.type).sort()).toEqual(['Embedded', 'Git']);
	});
});
