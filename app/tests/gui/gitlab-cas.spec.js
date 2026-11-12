/**
 * Regression test for Review 1 Item 5:
 * GitLab's adapter (`$lib/storage/git/hosts.js`) must perform real compare-and-swap:
 * 1. `getFile` uses `last_commit_id` (falling back to `blob_id`), and throws
 *    `GitHostError(..., 'api_error')` when neither is present — never inventing `'exists'`.
 * 2. `putFile` and `deleteFile` send `last_commit_id` when updating/deleting an
 *    existing file, and `GitProvider.put` maps GitLab's HTTP 400 stale-commit
 *    rejection to `{ conflict: { theirs_rev } }` instead of overwriting or failing raw.
 */
import { test as rawTest, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash, gotoHome } from './helpers/state.js';

const PROJECT = '/api/v4/projects/acme%2Fcas-repo';

function v4Doc(id, name, unit = 'mm') {
	const tabId = '55555555-6666-4777-8888-999999999999';
	return JSON.stringify({
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name,
			created: '2026-10-01T00:00:00.000Z',
			modified: '2026-10-01T00:00:00.000Z',
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
	});
}

rawTest.describe('GitLab compare-and-swap via last_commit_id', () => {
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'sends last_commit_id on PUT/DELETE, maps concurrent HTTP 400 stale-commit to conflict, and never invents exists',
		async ({ page }) => {
			const crashes = collectCrashErrors(page);
			const files = new Map();
			const putBodies = [];
			const deleteBodies = [];
			let commitSeq = 0;
			let omitIdsOnPath = null;
			let bumpBeforePutOnPath = null;

			const nextCommit = () =>
				`c${String(++commitSeq).padStart(39, '0')}`;

			await page.route('https://gitlab.com/api/v4/**', async (route) => {
				const req = route.request();
				const url = new URL(req.url());
				const method = req.method();
				const m = new RegExp(`^${PROJECT}/repository/files/([^/]+)$`).exec(url.pathname);
				if (!m) {
					return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
				}
				const relPath = decodeURIComponent(m[1]);

				if (method === 'GET') {
					if (!files.has(relPath)) {
						return route.fulfill({
							status: 404,
							contentType: 'application/json',
							body: '{"message":"404 File Not Found"}'
						});
					}
					const entry = files.get(relPath);
					const payload = {
						content: Buffer.from(entry.text, 'utf8').toString('base64'),
						encoding: 'base64'
					};
					if (omitIdsOnPath !== relPath) {
						payload.last_commit_id = entry.lastCommitId;
						payload.blob_id = entry.blobId;
					}
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify(payload)
					});
				}

				const body = req.postDataJSON?.() ?? {};
				if (method === 'POST') {
					if (files.has(relPath)) {
						return route.fulfill({
							status: 400,
							contentType: 'application/json',
							body: '{"message":"A file with this name already exists"}'
						});
					}
					const lastCommitId = nextCommit();
					files.set(relPath, {
						text: body.content,
						lastCommitId,
						blobId: `blob-${body.content.length}`
					});
					return route.fulfill({
						status: 201,
						contentType: 'application/json',
						body: JSON.stringify({ file_path: relPath, branch: body.branch })
					});
				}

				if (method === 'PUT') {
					putBodies.push({ path: relPath, last_commit_id: body.last_commit_id ?? null });
					if (!files.has(relPath)) {
						return route.fulfill({
							status: 400,
							contentType: 'application/json',
							body: '{"message":"A file with this name doesn\'t exist"}'
						});
					}
					if (bumpBeforePutOnPath === relPath) {
						bumpBeforePutOnPath = null;
						const cur = files.get(relPath);
						files.set(relPath, {
							...cur,
							text: cur.text,
							lastCommitId: 'c999999999999999999999999999999999999999'
						});
					}
					const cur = files.get(relPath);
					// Strict GitLab server-side CAS: require matching `last_commit_id` on PUT.
					if (!body.last_commit_id || body.last_commit_id !== cur.lastCommitId) {
						return route.fulfill({
							status: 400,
							contentType: 'application/json',
							body: '{"message":"You are attempting to update a file that has changed since you last viewed it."}'
						});
					}
					const lastCommitId = nextCommit();
					files.set(relPath, {
						text: body.content,
						lastCommitId,
						blobId: `blob-${body.content.length}`
					});
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({ file_path: relPath, branch: body.branch })
					});
				}

				if (method === 'DELETE') {
					deleteBodies.push({ path: relPath, last_commit_id: body.last_commit_id ?? null });
					const cur = files.get(relPath);
					if (cur && (!body.last_commit_id || body.last_commit_id !== cur.lastCommitId)) {
						return route.fulfill({
							status: 400,
							contentType: 'application/json',
							body: '{"message":"You are attempting to update a file that has changed since you last viewed it."}'
						});
					}
					files.delete(relPath);
					return route.fulfill({ status: 204, body: '' });
				}

				return route.fulfill({ status: 405, body: '' });
			});

			await gotoHome(page);
			await page.evaluate(() => {
				localStorage.setItem(
					'waffle-host-tokens',
					JSON.stringify([
						{ host_url: 'https://gitlab.com', token: 'glpat-cas-test', kind: 'gitlab' }
					])
				);
			});

			const cfg = {
				id: 'git:gitlab.com/acme/cas-repo',
				kind: 'gitlab',
				remote: 'https://gitlab.com/acme/cas-repo',
				branch: 'main',
				folder: '',
				label: 'GitLab · acme/cas-repo'
			};
			const docId = '50000000-1111-4222-8333-000000000005';

			// 1. Create + update + pre-check conflict + concurrent PUT 400 conflict.
			const casChecks = await page.evaluate(
				async ({ cfg, id, json0, json1, json2 }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);

					const created = await p.put(
						{ id, json: json0, created: 1000, modified: 1000 },
						{ base: null }
					);
					const updated = await p.put(
						{ id, json: json1, created: 1000, modified: 2000 },
						{ base: created.rev }
					);
					const stalePrecheck = await p.put(
						{ id, json: json2, created: 1000, modified: 3000 },
						{ base: created.rev }
					);
					return { created, updated, stalePrecheck };
				},
				{
					cfg,
					id: docId,
					json0: v4Doc(docId, 'Gear', 'mm'),
					json1: v4Doc(docId, 'Gear', 'cm'),
					json2: v4Doc(docId, 'Gear', 'in')
				}
			);

			expect(casChecks.created.rev).toBe('c000000000000000000000000000000000000001');
			expect(casChecks.updated.rev).toBe('c000000000000000000000000000000000000003');
			expect(putBodies.find((b) => b.path === 'gear.waffle')).toEqual({
				path: 'gear.waffle',
				last_commit_id: casChecks.created.rev
			});
			expect(casChecks.stalePrecheck).toEqual({
				conflict: { theirs_rev: casChecks.updated.rev }
			});

			// 2. Race between `getFile` pre-check and `PUT`: GitLab rejects stale `last_commit_id`
			//    with HTTP 400, and `GitProvider.put` maps it to `{ conflict: { theirs_rev } }`.
			bumpBeforePutOnPath = 'gear.waffle';
			const raceConflict = await page.evaluate(
				async ({ cfg, id, json2, base }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);
					return p.put({ id, json: json2, created: 1000, modified: 4000 }, { base });
				},
				{
					cfg,
					id: docId,
					json2: v4Doc(docId, 'Gear', 'in'),
					base: casChecks.updated.rev
				}
			);
			expect(raceConflict).toEqual({
				conflict: { theirs_rev: 'c999999999999999999999999999999999999999' }
			});
			expect(JSON.parse(files.get('gear.waffle').text).document.display_unit).toBe('cm');

			// 3. `delete` also sends `last_commit_id` on GitLab DELETE.
			await page.evaluate(
				async ({ cfg, id }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);
					await p.delete(id);
				},
				{ cfg, id: docId }
			);
			expect(deleteBodies).toEqual([
				{
					path: 'gear.waffle',
					last_commit_id: 'c999999999999999999999999999999999999999'
				}
			]);

			// 4. Missing both `last_commit_id` and `blob_id` must throw `api_error`, never `'exists'`.
			files.set('broken.waffle', {
				text: v4Doc(docId, 'Broken'),
				lastCommitId: 'c1',
				blobId: 'b1'
			});
			omitIdsOnPath = 'broken.waffle';
			const missingRevCheck = await page.evaluate(async () => {
				const { adapterFor } = await import('/src/lib/storage/git/hosts.js');
				const gl = adapterFor({ remote: 'https://gitlab.com/acme/cas-repo', host: 'gitlab' });
				try {
					const got = await gl.getFile(
						'https://gitlab.com/acme/cas-repo',
						'main',
						'broken.waffle',
						'glpat-cas-test'
					);
					return { threw: false, sha: got?.sha ?? null };
				} catch (err) {
					return { threw: true, code: err?.code ?? null, message: err?.message ?? '' };
				}
			});
			expect(missingRevCheck.threw).toBe(true);
			expect(missingRevCheck.code).toBe('api_error');

			expectNoAnyCrash(crashes);
		}
	);
});
