/**
 * Targeted regression spec for `app/scripts/check-base-path-build.mjs` and
 * `app/svelte.config.js`:
 *
 *   1. `scanBuildOutput` rejects root-absolute `'/examples'`, `'/assay'`,
 *      `'/home'`, `'/doc/'`, `'/open'`, `'/view'`, `'/agent'`, HTML `href="/..."`,
 *      and CSS `url(/...)` leaks while permitting base-prefixed paths,
 *      SvelteKit route dictionary keys (`"/home":[6]`), and `.endsWith("/open")`.
 *   2. `svelte.config.js` honors `WAFFLE_BUILD_OUT_DIR`, and
 *      `runBasePathBuildCheck` builds into a temporary directory without
 *      touching `app/build` and cleans up its temp directory in `finally` even
 *      when the scan fails (no `process.exit(1)` inside `try`).
 */
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test as rawTest, expect } from '@playwright/test';
import {
	runBasePathBuildCheck,
	scanBuildOutput
} from '../../scripts/check-base-path-build.mjs';

const specDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(specDir, '../..');

rawTest.describe('Base-path static build scanner and temp-dir isolation', () => {
	rawTest('widened scanner catches /examples, /assay, /home, routes, and CSS url(/...) leaks', async () => {
		const root = mkdtempSync(join(tmpdir(), 'waffle-scanner-spec-'));
		try {
			const cleanDir = join(root, 'clean');
			mkdirSync(cleanDir, { recursive: true });
			writeFileSync(
				join(cleanDir, 'index.html'),
				'<link rel="manifest" href="/apps/waffle/manifest.json"><script src="/apps/waffle/_app/entry.js"></script>'
			);
			writeFileSync(
				join(cleanDir, 'manifest.json'),
				JSON.stringify({ start_url: '.', scope: '.' })
			);
			writeFileSync(
				join(cleanDir, 'entry.js'),
				[
					'export const dictionary = { "/": [ 2 ], "/agent": [ 3 ], "/doc/[id]": [ 4 ], "/doc/[id]/[slug]": [ 5 ], "/home": [ 6 ], "/open": [ 7 ], "/view": [ 8 ] };',
					'if (u.pathname.endsWith("/open")) {}',
					'fetch("/apps/waffle/examples/manifest.json");',
					'fetch("/apps/waffle/assay/manifest.json");',
					'goto("/apps/waffle/home");'
				].join('\n')
			);
			writeFileSync(
				join(cleanDir, 'entry.css'),
				'.ok1{background:url(/apps/waffle/icons/logo.svg)}.ok2{src:url("./inter.woff2")}.ok3{background:url(//cdn.example.invalid/bg.png)}'
			);

			const okScan = scanBuildOutput(cleanDir, '/apps/waffle');
			expect(okScan.problems).toEqual([]);

			const checkSingleLeak = (filename, content) => {
				const dir = mkdtempSync(join(root, 'case-'));
				cpSync(cleanDir, dir, { recursive: true });
				writeFileSync(join(dir, filename), content);
				return scanBuildOutput(dir, '/apps/waffle').problems;
			};

			expect(checkSingleLeak('leak.js', `fetch('/examples/manifest.json');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', `fetch('/assay/P0001.waffle');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', `goto('/home');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', 'goto(`/doc/${id}`);').length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', `goto('/open?url=x');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', `goto('/view?code=1');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.js', `goto('/agent?code=1');`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.css', `.bad{background:url(/icons/logo.svg)}`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.css', `.bad{src:url('/fonts/inter.woff2')}`).length).toBeGreaterThan(0);
			expect(checkSingleLeak('leak.html', `<a href="/home">Home</a>`).length).toBeGreaterThan(0);
		} finally {
			rmSync(root, { recursive: true, force: true });
		}
	});

	rawTest('runBasePathBuildCheck uses WAFFLE_BUILD_OUT_DIR, never touches app/build, and cleans tmpRoot on failure', async () => {
		const isolatedTmp = mkdtempSync(join(tmpdir(), 'waffle-buildcheck-tmp-'));
		const fakeBin = join(isolatedTmp, 'bin');
		const capturedEnvFile = join(isolatedTmp, 'captured-out-dir.txt');
		mkdirSync(fakeBin, { recursive: true });

		// Fake `npm` stub that writes a planted `/examples` leak into `WAFFLE_BUILD_OUT_DIR`
		// without touching `app/build`.
		const fakeNpm = join(fakeBin, 'npm');
		writeFileSync(
			fakeNpm,
			[
				'#!/usr/bin/env node',
				'const fs = require("node:fs");',
				'const path = require("node:path");',
				'const outDir = process.env.WAFFLE_BUILD_OUT_DIR;',
				`fs.writeFileSync(${JSON.stringify(capturedEnvFile)}, String(outDir || ""));`,
				'if (!outDir) process.exit(2);',
				'fs.mkdirSync(outDir, { recursive: true });',
				'fs.writeFileSync(path.join(outDir, "index.html"), "<!doctype html>");',
				'fs.writeFileSync(path.join(outDir, "manifest.json"), JSON.stringify({ start_url: ".", scope: "." }));',
				'fs.writeFileSync(path.join(outDir, "planted.js"), "fetch(\'/examples/manifest.json\');");'
			].join('\n')
		);
		chmodSync(fakeNpm, 0o755);

		const appBuildDir = join(appRoot, 'build');
		const hadAppBuildBefore = existsSync(appBuildDir);

		try {
			const res = runBasePathBuildCheck('/apps/waffle', {
				tmpBaseDir: isolatedTmp,
				env: {
					...process.env,
					PATH: `${fakeBin}:${process.env.PATH || ''}`,
					TMPDIR: isolatedTmp
				}
			});
			expect(res.exitCode).toBe(1);
			expect(res.problems.some((p) => p.includes('/examples'))).toBe(true);

			// Verify `WAFFLE_BUILD_OUT_DIR` pointed into `isolatedTmp`, not `app/build`.
			const usedOutDir = existsSync(capturedEnvFile)
				? (await import('node:fs')).readFileSync(capturedEnvFile, 'utf8')
				: '';
			expect(usedOutDir.startsWith(join(isolatedTmp, 'waffle-base-path-build-'))).toBe(true);

			// Verify `finally` cleaned up every `waffle-base-path-build-*` temp dir despite failure.
			const leftovers = readdirSync(isolatedTmp).filter((name) =>
				name.startsWith('waffle-base-path-build-')
			);
			expect(leftovers).toEqual([]);
			expect(existsSync(appBuildDir)).toBe(hadAppBuildBefore);
		} finally {
			rmSync(isolatedTmp, { recursive: true, force: true });
		}
	});
});
