#!/usr/bin/env node
// Gate: no GUI spec may swallow a failed expected-state wait (CLAUDE.md "GUI
// Test Rules": "NEVER swallow assertion errors. No try/catch around
// expected-state waits"; remediation plan G6).
//
// Fails on a `try { … }` whose body awaits waitForEntityCount /
// waitForFeatureCount and whose `catch` does not rethrow, and on a
// `.catch(` chained onto either wait. 205 such sites in 36 specs were removed
// on 2026-10-29; a timeout there had turned into a silent pass.
//
// Run by `scripts/test.sh gui-fast` before Playwright starts. Usage:
//   node scripts/check-gui-swallowed-waits.mjs [dir]   (default app/tests)

import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dir = path.resolve(process.argv[2] ?? path.join(root, 'app', 'tests'));
const WAIT = /\bwaitFor(?:Entity|Feature)Count\s*\(/;

/** @param {string} d @returns {string[]} */
function specFiles(d) {
	return readdirSync(d).flatMap((name) => {
		const p = path.join(d, name);
		if (statSync(p).isDirectory()) return name === 'node_modules' ? [] : specFiles(p);
		return /\.(m?js|ts)$/.test(name) ? [p] : [];
	});
}

/** Index just past the `}` matching the `{` before `i`. */
function closeBrace(s, i) {
	let depth = 1;
	while (depth > 0 && i < s.length) {
		if (s[i] === '{') depth++;
		else if (s[i] === '}') depth--;
		i++;
	}
	return i;
}

const lineOf = (s, i) => s.slice(0, i).split('\n').length;
const hits = [];
for (const file of specFiles(dir)) {
	const s = readFileSync(file, 'utf8');
	for (const m of s.matchAll(/\btry\s*\{/g)) {
		const bodyEnd = closeBrace(s, m.index + m[0].length);
		const body = s.slice(m.index + m[0].length, bodyEnd - 1);
		const c = /^\s*catch\s*(?:\([^)]*\))?\s*\{/.exec(s.slice(bodyEnd));
		if (!c) continue;
		const cStart = bodyEnd + c[0].length;
		const cBody = s.slice(cStart, closeBrace(s, cStart) - 1);
		if (WAIT.test(body) && !/\bthrow\b/.test(cBody)) hits.push(`${path.relative(root, file)}:${lineOf(s, m.index)}  try/catch swallows a wait`);
	}
	for (const m of s.matchAll(/\bwaitFor(?:Entity|Feature)Count\s*\([^;]*?\)\s*\.catch\s*\(/g)) {
		hits.push(`${path.relative(root, file)}:${lineOf(s, m.index)}  .catch() swallows a wait`);
	}
}

if (hits.length) {
	console.error('GUI specs swallow failed expected-state waits (CLAUDE.md "GUI Test Rules"):');
	for (const h of hits) console.error(`  ${h}`);
	console.error('Let the wait throw; to keep a diagnostic, dump state in the catch and rethrow.');
	process.exit(1);
}
console.log(`check-gui-swallowed-waits: ok (${specFiles(dir).length} files)`);
