/**
 * The R/G/B = X/Y/Z axis triad lives in ONE place: AXIS_COLORS in
 * src/lib/config.js.
 *
 * It had grown four copies — DatumVis (lines and arrowheads), FeatureTree's
 * Origin rows, SketchRenderer's X/Y reference lines, and ConnectorFrames,
 * which drew its triad in a palette (#f38ba8 / #a6e3a1 / #89b4fa) used nowhere
 * else in the app. An axis was a different red depending on which component
 * happened to draw it. Nothing failed when they drifted, which is exactly why
 * they drifted, so this scans the source instead of the screen.
 *
 * Axis colors are deliberately NOT theme tokens — red-is-X is a convention the
 * user carries between applications. What varies per site is opacity.
 */
import { test, expect } from '@playwright/test';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';

const SRC = new URL('../src', import.meta.url).pathname;
const CONFIG = join(SRC, 'lib/config.js');

/** Every .js/.svelte file under src/, config.js excluded. */
function sourceFiles(dir, out = []) {
	for (const e of readdirSync(dir, { withFileTypes: true })) {
		const p = join(dir, e.name);
		if (e.isDirectory()) sourceFiles(p, out);
		else if (/\.(js|svelte)$/.test(e.name) && p !== CONFIG) out.push(p);
	}
	return out;
}

test.describe('axis color consolidation', () => {
	test('the axis triad is declared once, in config.js', () => {
		const config = readFileSync(CONFIG, 'utf8');
		// The canonical values, and the fact that they are frozen.
		expect(config).toMatch(/export const AXIS_COLORS = Object\.freeze\(/);
		for (const hex of ['#ff4444', '#44cc44', '#4488ff']) expect(config).toContain(hex);
		// config.js is the one file exempt from the scan below, so assert the
		// triad is really there before trusting an empty result.
		expect(config.match(/#[0-9a-f]{6}/gi).length).toBeGreaterThanOrEqual(3);

		// A fifth copy is a color LITERAL on a line that is talking about an
		// axis. Scanning for the hexes themselves is the wrong rule and was
		// tried first: #4488ff is also the sketch's default-entity blue and
		// #44cc44 its snap green, so a literal scan flags unrelated code that
		// merely shares a value. What makes a line an axis color is the word.
		const LITERAL = /#[0-9a-fA-F]{6}\b|0x[0-9a-fA-F]{6}\b/;
		const offenders = {};
		for (const file of sourceFiles(SRC)) {
			const hits = readFileSync(file, 'utf8')
				.split('\n')
				.map((line, i) => ({ line, n: i + 1 }))
				.filter(({ line }) => /axis/i.test(line) && LITERAL.test(line))
				.map(({ line, n }) => `${n}: ${line.trim()}`);
			if (hits.length) offenders[relative(SRC, file)] = hits;
		}

		// And the connector frames' old triad specifically: those three came
		// from the Catppuccin palette the UI uses for --accent/--error CSS
		// fallbacks, so only the 0x form (never a CSS fallback) is evidence.
		for (const file of sourceFiles(SRC)) {
			const hits = readFileSync(file, 'utf8')
				.split('\n')
				.map((line, i) => ({ line, n: i + 1 }))
				.filter(({ line }) => /0x(f38ba8|a6e3a1|89b4fa)\b/i.test(line))
				.map(({ line, n }) => `${n}: ${line.trim()}`);
			if (hits.length) (offenders[relative(SRC, file)] ??= []).push(...hits);
		}

		// Printing the map names the file, the line and the literal at once.
		expect(JSON.stringify(offenders, null, 1)).toBe('{}');
	});

	test('every site that draws an axis imports the shared triad', () => {
		for (const f of [
			'lib/viewport/DatumVis.svelte',
			'lib/viewport/ConnectorFrames.svelte',
			'lib/sketch/SketchRenderer.svelte',
			'lib/ui/FeatureTree.svelte'
		]) {
			const text = readFileSync(join(SRC, f), 'utf8');
			expect(`${f}: ${/AXIS_COLORS.*from '\$lib\/config\.js'/.test(text)}`).toBe(`${f}: true`);
		}
	});
});
