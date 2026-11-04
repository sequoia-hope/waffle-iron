#!/usr/bin/env node
/**
 * Build the static SvelteKit app under a non-root `BASE_PATH` (default:
 * `/apps/waffle`, the path Boardroom serves Waffle Iron at) directly into a
 * temporary directory via `WAFFLE_BUILD_OUT_DIR` (never touching `app/build`)
 * and verify that no build output file references a root-absolute app path
 * (`/_app`, `/pkg`, `/api`, `/examples`, `/assay`, `/icons`, `/manifest.json`,
 * app routes `/home`, `/doc`, `/open`, `/view`, `/agent`, HTML `href`/`src`/
 * `action`, or CSS `url(/...)` outside `BASE_PATH`).
 */

import { execFileSync } from 'node:child_process';
import {
	existsSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	statSync
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptFile = fileURLToPath(import.meta.url);
const appDir = dirname(dirname(scriptFile));

const BINARY_EXTENSIONS = new Set([
	'.wasm',
	'.gz',
	'.png',
	'.jpg',
	'.jpeg',
	'.gif',
	'.webp',
	'.ico',
	'.woff',
	'.woff2',
	'.ttf',
	'.otf',
	'.stl',
	'.step',
	'.stp'
]);

function escapeRegExp(s) {
	return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function walkFiles(dir, out = []) {
	for (const entry of readdirSync(dir)) {
		const full = join(dir, entry);
		const st = statSync(full);
		if (st.isDirectory()) {
			walkFiles(full, out);
		} else if (!BINARY_EXTENSIONS.has(extname(entry).toLowerCase())) {
			out.push(full);
		}
	}
	return out;
}

function lineNumberAt(text, index) {
	let line = 1;
	for (let i = 0; i < index; i++) {
		if (text.charCodeAt(i) === 10) line++;
	}
	return line;
}

/**
 * Normalize and validate a non-root base path (e.g. `"/apps/waffle"`).
 * @param {string} raw
 * @returns {string}
 */
export function normalizeCheckBasePath(raw) {
	const trimmed = String(raw || '').trim();
	if (!trimmed) {
		throw new Error(`BASE_PATH must be a non-root path (got ${JSON.stringify(raw)})`);
	}
	const normalized = (trimmed.startsWith('/') ? trimmed : `/${trimmed}`).replace(/\/+$/, '');
	if (!normalized || normalized === '/') {
		throw new Error(`BASE_PATH must be a non-root path (got ${JSON.stringify(raw)})`);
	}
	return normalized;
}

/**
 * Scan a static SvelteKit build directory for root-absolute URL/path leaks
 * that bypass `basePath`.
 *
 * @param {string} outDir
 * @param {string} basePath
 * @returns {{ filesChecked: number, problems: string[] }}
 */
export function scanBuildOutput(outDir, basePath) {
	const normalizedBase = normalizeCheckBasePath(basePath);
	const problems = [];

	for (const required of ['index.html', 'manifest.json']) {
		if (!existsSync(join(outDir, required))) {
			problems.push(`${required}: missing from build output`);
		}
	}

	const manifestPath = join(outDir, 'manifest.json');
	if (existsSync(manifestPath)) {
		const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
		const manifestUrl = `https://example.invalid${normalizedBase}/manifest.json`;
		for (const field of ['start_url', 'scope']) {
			if (manifest[field] == null) continue;
			const resolved = new URL(manifest[field], manifestUrl);
			if (
				resolved.pathname !== normalizedBase &&
				!resolved.pathname.startsWith(`${normalizedBase}/`)
			) {
				problems.push(
					`manifest.json: ${field}=${JSON.stringify(manifest[field])} resolves to ${resolved.pathname}, outside ${normalizedBase}`
				);
			}
		}
	}

	const escapedBase = escapeRegExp(normalizedBase);
	// Disallow:
	//   1. Root-absolute static/API asset prefixes (`/_app`, `/pkg`, `/api`,
	//      `/examples`, `/assay`, `/icons`, `/manifest.json`) unless concatenated
	//      onto a prefix (`base + "/..."`).
	//   2. Root-absolute app route strings (`/home`, `/doc`, `/open`, `/view`,
	//      `/agent`), excluding SvelteKit's route dictionary keys (`"/home":[...]`),
	//      prefix concatenations (`base + "/home"`), and `.endsWith("/open")`.
	//   3. Root-absolute HTML attributes (`href="/..."`, `src="/..."`, `action="/..."`)
	//      outside `normalizedBase`.
	//   4. Root-absolute CSS `url(/...)` references outside `normalizedBase`.
	const patterns = [
		{
			name: 'root-absolute static/API path',
			re: /(?<!\+\s*)["'`]\/(?:_app|pkg|api|examples|assay|icons|manifest\.json)(?:[/"'`?#]|$)/g
		},
		{
			name: 'root-absolute app route path',
			re: /(?<!\+\s*)(?<!\.endsWith\(\s*)["'`]\/(?:home|doc|open|view|agent)(?:[/"'`?#]|$)(?!(?:[^"'`\r\n]*["'`])?\s*:\s*\[)/g
		},
		{
			name: 'root-absolute href/src/action',
			re: new RegExp(
				`\\b(?:href|src|action)=["']\\/(?!\\/|${escapedBase.slice(1)}(?:[/"'?#]|$))[^"']*["']`,
				'g'
			)
		},
		{
			name: 'root-absolute CSS url()',
			re: new RegExp(
				`\\burl\\(\\s*["']?\\/(?!\\/|${escapedBase.slice(1)}(?:[/"')?#\\s]|$))[^"')\\s]+["']?\\s*\\)`,
				'gi'
			)
		}
	];

	const files = existsSync(outDir) ? walkFiles(outDir) : [];
	let filesChecked = 0;
	for (const file of files) {
		// Skip shipped assay fixture metadata and `.waffle` JSON models;
		// inspect all HTML, JS, CSS, manifest, SVG, and app assets.
		const rel = relative(outDir, file);
		if (rel.startsWith('assay/') || rel.endsWith('.waffle')) continue;
		filesChecked++;
		const text = readFileSync(file, 'utf8');
		for (const { name, re } of patterns) {
			re.lastIndex = 0;
			let match;
			while ((match = re.exec(text)) !== null) {
				const line = lineNumberAt(text, match.index);
				const snippet = text
					.slice(Math.max(0, match.index - 24), Math.min(text.length, match.index + match[0].length + 24))
					.replace(/\s+/g, ' ');
				problems.push(`${rel}:${line}: ${name}: ${match[0]} (context: ${snippet})`);
			}
		}
	}

	return { filesChecked, problems };
}

/**
 * Run a full static SvelteKit build into a temporary `WAFFLE_BUILD_OUT_DIR`
 * (leaving `app/build` untouched) and scan the resulting output.
 *
 * @param {string} rawBasePath
 * @param {{ env?: NodeJS.ProcessEnv, tmpBaseDir?: string }} [options]
 * @returns {{ exitCode: number, filesChecked: number, problems: string[] }}
 */
export function runBasePathBuildCheck(rawBasePath, options = {}) {
	const basePath = normalizeCheckBasePath(rawBasePath);
	const tmpParent = options.tmpBaseDir || tmpdir();
	const tmpRoot = mkdtempSync(join(tmpParent, 'waffle-base-path-build-'));
	const outDir = join(tmpRoot, 'out');
	let exitCode = 0;
	let filesChecked = 0;
	/** @type {string[]} */
	let problems = [];

	try {
		execFileSync('npm', ['run', 'build'], {
			cwd: appDir,
			env: {
				...(options.env ?? process.env),
				BASE_PATH: basePath,
				WAFFLE_BUILD_OUT_DIR: outDir
			},
			stdio: 'inherit'
		});

		if (!existsSync(outDir)) {
			problems = [`expected build output at ${outDir}`];
			exitCode = 1;
		} else {
			const scan = scanBuildOutput(outDir, basePath);
			filesChecked = scan.filesChecked;
			problems = scan.problems;
			if (problems.length > 0) {
				exitCode = 1;
			}
		}
	} catch (err) {
		problems = [String(err?.message || err)];
		exitCode = 1;
	} finally {
		rmSync(tmpRoot, { recursive: true, force: true });
	}

	return { exitCode, filesChecked, problems };
}

function main() {
	const rawArg = process.argv[2] || process.env.BASE_PATH || '/apps/waffle';
	let basePath;
	try {
		basePath = normalizeCheckBasePath(rawArg);
	} catch (err) {
		console.error(`check-base-path-build: ${err.message || err}`);
		process.exitCode = 1;
		return;
	}

	const { exitCode, filesChecked, problems } = runBasePathBuildCheck(basePath);
	if (exitCode !== 0) {
		console.error(
			`check-base-path-build: FAILED (${problems.length} issue(s) under BASE_PATH=${basePath})`
		);
		for (const p of problems) {
			console.error(`  ${p}`);
		}
		process.exitCode = exitCode;
		return;
	}

	console.log(`check-base-path-build: ok (BASE_PATH=${basePath}, ${filesChecked} files checked)`);
}

if (process.argv[1] && resolve(process.argv[1]) === scriptFile) {
	main();
}
