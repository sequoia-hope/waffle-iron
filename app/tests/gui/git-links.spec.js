/**
 * v4 Phase 2 — git-aware links in the app (specs/waffle_v4_document_model.md
 * §2.4, §7): remote parsing, git-blob hashing (must equal `git hash-object`
 * and the Rust `git_blob_sha1`), share-link ↔ locator mapping, `Relative`
 * resolution, per-host tokens, the content cache, and the GitHub / GitLab /
 * Gitea API adapters against mocked endpoints. The modules run in the browser
 * (WebCrypto, IndexedDB), imported straight from the dev server.
 */
import { createServer } from 'node:http';
import { test, expect } from '@playwright/test';

const GIT = '/src/lib/storage/git';

/** Run `fn` in the page with the named module loaded. */
async function withModule(page, name, fn, arg) {
	return page.evaluate(
		async ({ path, src, arg }) => {
			const m = await import(path);
			// eslint-disable-next-line no-new-func
			const f = new Function('m', 'arg', `return (${src})(m, arg);`);
			return f(m, arg);
		},
		{ path: `${GIT}/${name}.js`, src: fn.toString(), arg }
	);
}

test.describe('git links: remote parsing', () => {
	test('normalizes clone URLs and infers the host adapter', async ({ page }) => {
		await page.goto('/home');
		const r = await withModule(page, 'remote', (m) => ({
			norm: m.normalizeRemote('https://GitHub.com/Acme/Parts.git/'),
			parsed: m.parseRemote('https://github.com/acme/parts.git'),
			sub: m.parseRemote('https://gitlab.com/group/sub/repo'),
			hosts: [
				'https://github.com/a/b',
				'https://gitlab.com/a/b',
				'https://gitlab.example.org/a/b',
				'https://codeberg.org/a/b',
				'https://git.example.com/a/b'
			].map(m.inferHost),
			sha: [m.isCommitSha('9fceb02a'.repeat(5)), m.isCommitSha('main')],
			refs: ['main', 'release/1.2', '-bad', 'a..b', 'a b', 'refs/heads/x'].map(m.isValidRefName),
			paths: ['parts/bracket.waffle', '/abs', 'a/../b', 'a//b', 'x'].map(m.isValidRepoPath),
			bad: [m.parseRemote('https://github.com/onlyowner'), m.parseRemote('ssh://git@github.com/a/b')]
		}));
		expect(r.norm).toBe('https://github.com/Acme/Parts');
		expect(r.parsed).toEqual({ origin: 'https://github.com', host: 'github.com', repoPath: 'acme/parts', owner: 'acme', repo: 'parts' });
		expect(r.sub.owner).toBe('group/sub');
		expect(r.sub.repo).toBe('repo');
		expect(r.hosts).toEqual(['github', 'gitlab', 'gitlab', 'gitea', 'generic']);
		expect(r.sha).toEqual([true, false]);
		expect(r.refs).toEqual([true, true, false, false, false, true]);
		expect(r.paths).toEqual([true, false, false, false, true]);
		expect(r.bad).toEqual([null, null]);
	});
});

test.describe('git links: content hash', () => {
	test('git-blob-sha1 matches git hash-object vectors', async ({ page }) => {
		await page.goto('/home');
		const r = await withModule(page, 'hash', async (m) => ({
			empty: await m.gitBlobSha1(''),
			hello: await m.gitBlobSha1('hello\n'),
			utf8: await m.gitBlobSha1('héllo\n'),
			match: await m.checkContentHash('git-blob-sha1:ce013625030ba8dba906f756967f9e9ca394464a', 'hello\n'),
			mismatch: await m.checkContentHash('git-blob-sha1:ce013625030ba8dba906f756967f9e9ca394464a', 'hello'),
			none: await m.checkContentHash('sha256:abc', 'hello\n'),
			absent: await m.checkContentHash(null, 'x'),
			sha: m.blobShaOf('git-blob-sha1:abc'),
			wrap: m.contentHashFromBlobSha('ABC')
		}));
		// Known vectors (spec §5): empty string and "hello\n".
		expect(r.empty).toBe('git-blob-sha1:e69de29bb2d1d6434b8b29ae775ad8c2e48c5391');
		expect(r.hello).toBe('git-blob-sha1:ce013625030ba8dba906f756967f9e9ca394464a');
		// Byte length, not char length: `printf 'héllo\n' | git hash-object --stdin`
		// (computed 2026-09-08).
		expect(r.utf8).toBe('git-blob-sha1:5fb50d3c93474f139362304b663fe44e9d17a26e');
		expect([r.match, r.mismatch, r.none, r.absent]).toEqual(['match', 'mismatch', 'none', 'none']);
		expect(r.sha).toBe('abc');
		expect(r.wrap).toBe('git-blob-sha1:abc');
	});
});

test.describe('git links: locators and share links', () => {
	test('share link round-trips a Git locator and maps legacy raw URLs', async ({ page }) => {
		await page.goto('/home');
		const r = await withModule(page, 'locator', (m) => {
			const loc = { type: 'Git', remote: 'https://github.com/acme/parts', path: 'brackets/bracket.waffle', ref: { type: 'Branch', name: 'main' } };
			const link = m.buildOpenLink(loc, 'https://app.example/base/');
			const back = m.parseOpenParams(new URL(link).searchParams);
			const tagLink = m.buildOpenLink({ ...loc, ref: { type: 'Tag', name: 'v1.0' } }, 'https://app.example');
			const tagBack = m.parseOpenParams(new URL(tagLink).searchParams);
			const sha = '9fceb02a'.repeat(5);
			return {
				link,
				back,
				tagBack,
				refs: ['main', sha, 'refs/tags/v1', 'tags/v2', 'refs/heads/dev', '-x', ''].map(m.parseRef),
				missing: m.parseOpenParams(new URLSearchParams('remote=https://github.com/a/b')),
				badRef: m.parseOpenParams(new URLSearchParams('remote=https://github.com/a/b&path=x.waffle&ref=a..b')),
				badPath: m.parseOpenParams(new URLSearchParams('remote=https://github.com/a/b&path=/x.waffle')),
				url: m.parseOpenParams(new URLSearchParams('url=https://example.com/x.waffle')),
				legacy: [
					'https://raw.githubusercontent.com/acme/parts/main/brackets/bracket.waffle',
					'https://github.com/acme/parts/blob/v1.0/a/b.waffle',
					'https://gitlab.com/group/sub/repo/-/raw/dev/dir/file.waffle',
					'https://codeberg.org/acme/parts/raw/branch/main/x.waffle',
					'https://codeberg.org/acme/parts/src/tag/v2/x.waffle',
					'https://example.com/files/x.waffle',
					'http://insecure.example/x.waffle'
				].map(m.locatorFromLegacySrc),
				viaSrc: m.parseOpenParams(new URLSearchParams(`src=${encodeURIComponent('https://raw.githubusercontent.com/acme/parts/main/x.waffle')}`)),
				rel: [
					m.resolveRelative(loc, '../fasteners/bolt.waffle'),
					m.resolveRelative(loc, 'sub/plate.waffle'),
					m.resolveRelative(loc, '../../escape.waffle'),
					m.resolveRelative(null, 'x.waffle')
				],
				join: [m.joinRepoPath('a/b/c.waffle', './d.waffle'), m.joinRepoPath('c.waffle', 'd.waffle'), m.joinRepoPath('c.waffle', '../d.waffle')],
				describe: [m.describeLocator(loc), m.describeLocator({ type: 'Git', remote: 'https://github.com/a/b', path: 'p', ref: { type: 'Commit', sha } }), m.describeLocator({ type: 'Embedded' }), m.describeLocator({ type: 'Kicad' })]
			};
		});
		expect(r.link).toBe('https://app.example/base/open?remote=https%3A%2F%2Fgithub.com%2Facme%2Fparts&path=brackets%2Fbracket.waffle&ref=main');
		expect(r.back).toEqual({ locator: { type: 'Git', remote: 'https://github.com/acme/parts', path: 'brackets/bracket.waffle', ref: { type: 'Branch', name: 'main' } } });
		expect(r.tagBack.locator.ref).toEqual({ type: 'Tag', name: 'v1.0' });
		expect(r.refs).toEqual([
			{ type: 'Branch', name: 'main' },
			{ type: 'Commit', sha: '9fceb02a'.repeat(5) },
			{ type: 'Tag', name: 'v1' },
			{ type: 'Tag', name: 'v2' },
			{ type: 'Branch', name: 'dev' },
			null,
			null
		]);
		expect(r.missing.error).toMatch(/remote and path/);
		expect(r.badRef.error).toMatch(/invalid ref/);
		expect(r.badPath.error).toMatch(/invalid path/);
		expect(r.url).toEqual({ locator: { type: 'Url', url: 'https://example.com/x.waffle' } });
		expect(r.legacy).toEqual([
			{ type: 'Git', remote: 'https://github.com/acme/parts', path: 'brackets/bracket.waffle', ref: { type: 'Branch', name: 'main' }, host: 'github' },
			{ type: 'Git', remote: 'https://github.com/acme/parts', path: 'a/b.waffle', ref: { type: 'Branch', name: 'v1.0' }, host: 'github' },
			{ type: 'Git', remote: 'https://gitlab.com/group/sub/repo', path: 'dir/file.waffle', ref: { type: 'Branch', name: 'dev' }, host: 'gitlab' },
			{ type: 'Git', remote: 'https://codeberg.org/acme/parts', path: 'x.waffle', ref: { type: 'Branch', name: 'main' }, host: 'gitea' },
			{ type: 'Git', remote: 'https://codeberg.org/acme/parts', path: 'x.waffle', ref: { type: 'Tag', name: 'v2' }, host: 'gitea' },
			{ type: 'Url', url: 'https://example.com/files/x.waffle' },
			null
		]);
		expect(r.viaSrc.locator.path).toBe('x.waffle');
		expect(r.rel).toEqual([
			{ type: 'Git', remote: 'https://github.com/acme/parts', path: 'fasteners/bolt.waffle', ref: { type: 'Branch', name: 'main' } },
			{ type: 'Git', remote: 'https://github.com/acme/parts', path: 'brackets/sub/plate.waffle', ref: { type: 'Branch', name: 'main' } },
			null,
			null
		]);
		expect(r.join).toEqual(['a/b/d.waffle', 'd.waffle', null]);
		expect(r.describe[0]).toBe('github.com/acme/parts/brackets/bracket.waffle @ main');
		expect(r.describe[1]).toBe('github.com/a/b/p @ 9fceb02');
		expect(r.describe[2]).toBe('embedded');
		expect(r.describe[3]).toBe('Kicad (unsupported)');
	});
});

test.describe('git links: per-host tokens and content cache', () => {
	test('tokens are per host and honor the legacy GitHub token', async ({ page }) => {
		await page.goto('/home');
		const r = await withModule(page, 'tokens', (m) => {
			localStorage.clear();
			localStorage.setItem('waffle-github-token', 'ghu_legacy');
			localStorage.setItem('waffle-github-user', 'octo');
			const legacy = m.getHostToken('https://GitHub.com/');
			const legacyLogin = m.listHostTokens().find((e) => e.host_url === 'https://github.com')?.login;
			m.setHostToken('https://gitlab.example.org', 'glpat-1', { kind: 'gitlab', login: 'me' });
			const gl = m.getHostToken('https://gitlab.example.org/group/repo'.replace(/\/group.*/, ''));
			m.setHostToken('https://github.com', 'ghu_new', { kind: 'github' });
			const overridden = m.getHostToken('https://github.com');
			m.removeHostToken('https://github.com');
			const removed = m.getHostToken('https://github.com');
			const hosts = m.listHostTokens().map((e) => e.host_url);
			return { legacy, legacyLogin, gl, overridden, removed, hosts, legacyKeyGone: localStorage.getItem('waffle-github-token') };
		});
		expect(r.legacy).toBe('ghu_legacy');
		expect(r.legacyLogin).toBe('octo');
		expect(r.gl).toBe('glpat-1');
		expect(r.overridden).toBe('ghu_new');
		expect(r.removed).toBeNull();
		expect(r.hosts).toEqual(['https://gitlab.example.org']);
		expect(r.legacyKeyGone).toBeNull();
	});

	test('content cache stores by hash and survives a reload', async ({ page }) => {
		await page.goto('/home');
		await withModule(page, 'cache', async (m) => {
			await m.cacheClear();
			await m.cachePut('git-blob-sha1:aaaa', 'ISO-10303-21;');
		});
		await page.reload();
		const r = await withModule(page, 'cache', async (m) => {
			const hit = await m.cacheGet('git-blob-sha1:aaaa');
			const miss = await m.cacheGet('git-blob-sha1:bbbb');
			await m.cacheClear();
			const cleared = await m.cacheGet('git-blob-sha1:aaaa');
			return { hit, miss, cleared, none: await m.cacheGet(null) };
		});
		expect(r).toEqual({ hit: 'ISO-10303-21;', miss: null, cleared: null, none: null });
	});
});

test.describe('git links: host adapters', () => {
	const SHA = '9fceb02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
	const b64 = (s) => Buffer.from(s, 'utf8').toString('base64');

	test('GitHub: resolves a branch to a commit and fetches the blob at that commit', async ({ page }) => {
		const seen = [];
		await page.route('https://api.github.com/**', async (route) => {
			const url = route.request().url();
			seen.push(url);
			if (url.endsWith('/repos/acme/parts/commits/main')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA.toUpperCase() }) });
			}
			if (url.includes('/repos/acme/parts/contents/brackets/bracket.waffle?ref=' + SHA)) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64('{"name":"héllo"}\n').replace(/(.{20})/g, '$1\n'), sha: 'CE01', encoding: 'base64' }) });
			}
			if (url.endsWith('/repos/acme/parts/commits/gone')) {
				return route.fulfill({ status: 404, contentType: 'application/json', body: '{"message":"Not Found"}' });
			}
			if (url.endsWith('/repos/acme/secret/commits/main')) {
				return route.fulfill({ status: 401, contentType: 'application/json', body: '{}' });
			}
			return route.fulfill({ status: 500, body: 'unexpected' });
		});
		await page.goto('/home');
		const r = await withModule(page, 'hosts', async (m) => {
			const ok = await m.fetchGitLocator({ type: 'Git', remote: 'https://github.com/acme/parts', path: 'brackets/bracket.waffle', ref: { type: 'Branch', name: 'main' } }, null);
			const pinned = await m.adapterFor({ remote: 'https://github.com/acme/parts' }).resolveRef('https://github.com/acme/parts', { type: 'Commit', sha: 'ABC' });
			let gone, auth;
			try { await m.fetchGitLocator({ type: 'Git', remote: 'https://github.com/acme/parts', path: 'x', ref: { type: 'Branch', name: 'gone' } }); } catch (e) { gone = e.code; }
			try { await m.fetchGitLocator({ type: 'Git', remote: 'https://github.com/acme/secret', path: 'x', ref: { type: 'Branch', name: 'main' } }); } catch (e) { auth = e.code; }
			return { ok, pinned, gone, auth, kind: m.adapterFor({ remote: 'https://github.com/acme/parts' }).kind };
		});
		expect(r.kind).toBe('github');
		expect(r.ok).toEqual({ commit: SHA, text: '{"name":"héllo"}\n', blobSha: 'ce01' });
		expect(r.pinned).toEqual({ commit: 'abc' });
		expect(r.gone).toBe('not_found');
		expect(r.auth).toBe('auth_required');
		// Content was fetched at the RESOLVED commit, never at the branch name.
		expect(seen.some((u) => u.includes('?ref=' + SHA))).toBe(true);
		expect(seen.some((u) => u.includes('?ref=main'))).toBe(false);
	});

	test('GitLab: project path is URL-encoded; files API blob_id becomes the hash', async ({ page }) => {
		await page.route('https://gitlab.com/api/v4/**', async (route) => {
			const url = route.request().url();
			if (url.endsWith('/projects/group%2Fsub%2Frepo/repository/commits/dev')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ id: SHA }) });
			}
			if (url.includes('/projects/group%2Fsub%2Frepo/repository/files/dir%2Ffile.waffle?ref=' + SHA)) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64('{}'), blob_id: 'B10B' }) });
			}
			return route.fulfill({ status: 500, body: 'unexpected ' + url });
		});
		await page.goto('/home');
		const r = await withModule(page, 'hosts', async (m) =>
			m.fetchGitLocator({ type: 'Git', remote: 'https://gitlab.com/group/sub/repo', path: 'dir/file.waffle', ref: { type: 'Branch', name: 'dev' } }, 'glpat')
		);
		expect(r).toEqual({ commit: SHA, text: '{}', blobSha: 'b10b' });
	});

	test('Gitea: branches first, then tags; explicit host overrides inference', async ({ page }) => {
		await page.route('https://git.example.com/api/v1/**', async (route) => {
			const url = route.request().url();
			if (url.endsWith('/repos/acme/parts/branches/v2')) {
				return route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
			}
			if (url.endsWith('/repos/acme/parts/tags/v2')) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ commit: { sha: SHA } }) });
			}
			if (url.includes('/repos/acme/parts/contents/x.waffle?ref=' + SHA)) {
				return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ content: b64('gitea'), sha: 'aa' }) });
			}
			return route.fulfill({ status: 500, body: 'unexpected ' + url });
		});
		await page.goto('/home');
		const r = await withModule(page, 'hosts', async (m) => {
			const loc = { type: 'Git', remote: 'https://git.example.com/acme/parts', path: 'x.waffle', ref: { type: 'Branch', name: 'v2' }, host: 'gitea' };
			const ok = await m.fetchGitLocator(loc);
			let generic;
			try { await m.fetchGitLocator({ ...loc, host: undefined }); } catch (e) { generic = e.code; }
			return { ok, generic };
		});
		expect(r.ok).toEqual({ commit: SHA, text: 'gitea', blobSha: 'aa' });
		expect(r.generic).toBe('unresolvable');
	});

	test('Url locator: plain https fetch; http refused', async ({ page }) => {
		await page.route('https://files.example.com/**', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: '{"format":"waffle-iron"}' }));
		await page.goto('/home');
		const r = await withModule(page, 'hosts', async (m) => {
			const ok = await m.fetchUrlLocator('https://files.example.com/a.waffle');
			let bad;
			try { await m.fetchUrlLocator('http://files.example.com/a.waffle'); } catch (e) { bad = e.code; }
			return { ok, bad };
		});
		expect(r.ok).toEqual({ text: '{"format":"waffle-iron"}' });
		expect(r.bad).toBe('invalid');
	});
});

test.describe('git links: files over 1 MiB on GitHub', () => {
	// GitHub answers a contents request for a large file with JSON that has no
	// content, so fetchBlob asks again with the raw media type. GitHub gives both
	// replies `Vary: Accept` and the SAME ETag. The browser revalidates its cached
	// JSON with that ETag, gets a 304, and hands the JSON back as if it were the
	// file: every linked .kicad_pcb over 1 MiB failed to parse.
	//
	// A real server, not page.route: Playwright turns the HTTP cache off while a
	// route is installed, which hides this bug.
	test('fetchBlob gets the raw file, not the cached JSON', async ({ page }) => {
		const file = '(kicad_pcb (version 20241229))\n';
		const etag = '"same-for-both-media-types"';
		const accepts = [];
		const server = createServer((req, res) => {
			accepts.push(req.headers.accept);
			const headers = {
				'Access-Control-Allow-Origin': '*',
				'Cache-Control': 'public, max-age=60, s-maxage=60',
				Vary: 'Accept, Accept-Encoding, Accept, X-Requested-With',
				ETag: etag
			};
			if (req.headers['if-none-match'] === etag) {
				res.writeHead(304, headers);
				return res.end();
			}
			if (req.headers.accept === 'application/vnd.github.raw+json') {
				res.writeHead(200, { ...headers, 'Content-Type': 'application/vnd.github.raw+json' });
				return res.end(file);
			}
			res.writeHead(200, { ...headers, 'Content-Type': 'application/json' });
			res.end(JSON.stringify({ name: 'big.kicad_pcb', sha: 'abc123', size: 1294055, encoding: 'none', content: '' }));
		});
		await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
		try {
			await page.goto('/home');
			const got = await withModule(
				page,
				'hosts',
				async (m, api) => {
					const remote = 'https://github.com/acme/board';
					const adapter = m.adapterFor({ remote });
					const apiBase = adapter.apiBase;
					adapter.apiBase = () => ({ api, repo: 'acme/board' });
					try {
						return await adapter.fetchBlob(remote, '0'.repeat(40), 'hw/big.kicad_pcb', null);
					} finally {
						adapter.apiBase = apiBase;
					}
				},
				`http://127.0.0.1:${server.address().port}`
			);
			expect(got.text).toBe(file);
			expect(got.blobSha).toBe('abc123');
			expect(accepts).toContain('application/vnd.github.raw+json');
		} finally {
			server.closeAllConnections();
			await new Promise((resolve) => server.close(resolve));
		}
	});
});
