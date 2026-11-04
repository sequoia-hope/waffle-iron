/**
 * Regression test for Review 1 Item 2:
 * `GitProvider.put` on rename must write the new path FIRST with a null-base
 * check and delete the old path ONLY after the new file and index succeed:
 * 1. If `putFile` at the new path fails, the old file remains intact on HEAD
 *    and `.waffle-index.json` does not dangle.
 * 2. If an unindexed file already occupies the target filename, `put` refuses
 *    before any write or delete with `{ conflict: { theirs_rev } }` instead of
 *    overwriting the target file or deleting the old file.
 * 3. On a clean rename, `PUT <new>` (with no `sha` in body) precedes `DELETE <old>`.
 */
import { test as rawTest, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

function v4Doc(id, name) {
	const tabId = '22222222-3333-4444-8555-666666666666';
	return JSON.stringify({
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name,
			created: '2026-10-01T00:00:00.000Z',
			modified: '2026-10-01T00:00:00.000Z',
			display_unit: 'mm'
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
	});
}

rawTest.describe('GitProvider rename write-before-delete and target null-base check', () => {
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'preserves old file on failed rename write, refuses unindexed target collision, and deletes old file last',
		async ({ page }) => {
			const crashes = collectCrashErrors(page);
			const files = new Map();
			const ops = [];
			let failPutOnPath = null;
			let seq = 0;

			await page.route('https://api.github.com/repos/acme/rename-safe/contents/**', async (route) => {
				const req = route.request();
				const url = new URL(req.url());
				const relPath = decodeURIComponent(
					url.pathname.replace('/repos/acme/rename-safe/contents/', '')
				);
				const method = req.method();

				if (method === 'GET') {
					if (!files.has(relPath)) {
						return route.fulfill({
							status: 404,
							contentType: 'application/json',
							body: '{"message":"Not Found"}'
						});
					}
					const entry = files.get(relPath);
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({
							content: Buffer.from(entry.text, 'utf8').toString('base64'),
							sha: entry.sha,
							encoding: 'base64'
						})
					});
				}

				if (method === 'PUT') {
					const body = req.postDataJSON?.() ?? {};
					ops.push({ method: 'PUT', path: relPath, sha: body.sha ?? null });
					if (failPutOnPath === relPath) {
						return route.fulfill({
							status: 500,
							contentType: 'application/json',
							body: '{"message":"Internal Server Error"}'
						});
					}
					if (files.has(relPath) && !body.sha) {
						return route.fulfill({
							status: 422,
							contentType: 'application/json',
							body: '{"message":"Invalid request. \\"sha\\" wasn\'t supplied."}'
						});
					}
					const text = Buffer.from(body.content, 'base64').toString('utf8');
					const nextSha = `sha-${++seq}`;
					files.set(relPath, { text, sha: nextSha });
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({ content: { sha: nextSha } })
					});
				}

				if (method === 'DELETE') {
					const body = req.postDataJSON?.() ?? {};
					ops.push({ method: 'DELETE', path: relPath, sha: body.sha ?? null });
					files.delete(relPath);
					return route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
				}

				return route.fulfill({ status: 405, body: '' });
			});

			await page.goto('/home');
			await page.evaluate(() => {
				localStorage.setItem(
					'waffle-host-tokens',
					JSON.stringify([
						{ host_url: 'https://github.com', token: 'ghp_test', kind: 'github' }
					])
				);
			});

			const docId = '2e4a3e00-1111-4222-8333-000000000002';

			// Seed initial document "Alpha" -> alpha.waffle.
			const created = await page.evaluate(
				async ({ id, json }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider({
						id: 'git:github.com/acme/rename-safe',
						kind: 'github',
						remote: 'https://github.com/acme/rename-safe',
						branch: 'main',
						folder: '',
						label: 'GitHub · acme/rename-safe'
					});
					return p.put({ id, json, created: 1000, modified: 1000 }, { base: null });
				},
				{ id: docId, json: v4Doc(docId, 'Alpha') }
			);
			expect(created.rev).toBe('sha-1');
			expect(files.has('alpha.waffle')).toBe(true);

			// 1. Failing `putFile` on `beta.waffle` during rename must NOT delete `alpha.waffle`.
			ops.length = 0;
			failPutOnPath = 'beta.waffle';
			const failedRename = await page.evaluate(
				async ({ id, json, base }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider({
						id: 'git:github.com/acme/rename-safe',
						kind: 'github',
						remote: 'https://github.com/acme/rename-safe',
						branch: 'main',
						folder: '',
						label: 'GitHub · acme/rename-safe'
					});
					let errorMessage = null;
					try {
						await p.put({ id, json, created: 1000, modified: 2000 }, { base });
					} catch (err) {
						errorMessage = err?.message ?? String(err);
					}
					const stillReadable = await p.get(id);
					return { errorMessage, stillReadable };
				},
				{ id: docId, json: v4Doc(docId, 'Beta'), base: created.rev }
			);
			failPutOnPath = null;

			expect(failedRename.errorMessage).toContain('HTTP 500');
			expect(ops.filter((o) => o.method === 'DELETE')).toEqual([]);
			expect(files.has('alpha.waffle')).toBe(true);
			expect(files.has('beta.waffle')).toBe(false);
			expect(JSON.parse(files.get('.waffle-index.json').text)[0].filename).toBe('alpha.waffle');
			expect(failedRename.stillReadable?.rev).toBe(created.rev);
			expect(JSON.parse(failedRename.stillReadable.doc.json).document.name).toBe('Alpha');

			// 2. An unindexed file already at `occupied.waffle` must refuse with `{ conflict }`
			//    before any write or delete (never overwrite it, never delete `alpha.waffle`).
			files.set('occupied.waffle', {
				text: '{"external":"unindexed"}',
				sha: 'unindexed-sha-99'
			});
			ops.length = 0;
			const occupiedRename = await page.evaluate(
				async ({ id, json, base }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider({
						id: 'git:github.com/acme/rename-safe',
						kind: 'github',
						remote: 'https://github.com/acme/rename-safe',
						branch: 'main',
						folder: '',
						label: 'GitHub · acme/rename-safe'
					});
					return p.put({ id, json, created: 1000, modified: 3000 }, { base });
				},
				{ id: docId, json: v4Doc(docId, 'Occupied'), base: created.rev }
			);

			expect(occupiedRename).toEqual({ conflict: { theirs_rev: 'unindexed-sha-99' } });
			expect(ops).toEqual([]);
			expect(files.get('occupied.waffle')).toEqual({
				text: '{"external":"unindexed"}',
				sha: 'unindexed-sha-99'
			});
			expect(files.has('alpha.waffle')).toBe(true);

			// 3. Clean rename to `gamma.waffle`: writes `gamma.waffle` with null base first,
			//    updates `.waffle-index.json`, and deletes `alpha.waffle` afterward.
			ops.length = 0;
			const cleanRename = await page.evaluate(
				async ({ id, json, base }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider({
						id: 'git:github.com/acme/rename-safe',
						kind: 'github',
						remote: 'https://github.com/acme/rename-safe',
						branch: 'main',
						folder: '',
						label: 'GitHub · acme/rename-safe'
					});
					const putRes = await p.put({ id, json, created: 1000, modified: 4000 }, { base });
					const readBack = await p.get(id);
					return { putRes, readBack };
				},
				{ id: docId, json: v4Doc(docId, 'Gamma'), base: created.rev }
			);

			expect('rev' in cleanRename.putRes).toBe(true);
			expect(ops.map((o) => `${o.method} ${o.path}`)).toEqual([
				'PUT gamma.waffle',
				'PUT .waffle-index.json',
				'DELETE alpha.waffle'
			]);
			expect(ops[0].sha).toBeNull();
			expect(files.has('alpha.waffle')).toBe(false);
			expect(files.has('gamma.waffle')).toBe(true);
			expect(JSON.parse(cleanRename.readBack.doc.json).document.name).toBe('Gamma');

			expectNoAnyCrash(crashes);
		}
	);
});
