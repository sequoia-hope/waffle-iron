/**
 * Trace replay system for deterministic GUI test reproduction.
 *
 * A trace is a JSON array of steps. Each step has an `action` and `params`,
 * with an optional `assert` that checks `window.__waffle` state.
 *
 * Supported actions:
 *   click(x, y)             — click at absolute page coordinates
 *   drag(x1, y1, x2, y2)   — mouse drag from (x1,y1) to (x2,y2) with 5 steps
 *   wheel(x, y, delta)      — mouse wheel at position with deltaY
 *   key(k)                  — keyboard press
 *   wait(ms)                — wait for ms milliseconds
 *   evaluate(fnString)      — page.evaluate the given function string
 */
import { readFile } from 'fs/promises';
import { expect } from '@playwright/test';

/**
 * Replay a single trace step on the page.
 * @param {import('@playwright/test').Page} page
 * @param {object} step - { action, params, assert? }
 */
async function replayStep(page, step) {
	const { action, params } = step;

	switch (action) {
		case 'click': {
			const [x, y] = params;
			await page.mouse.click(x, y);
			await page.waitForTimeout(150);
			break;
		}
		case 'drag': {
			const [x1, y1, x2, y2] = params;
			await page.mouse.move(x1, y1);
			await page.mouse.down();
			const steps = 5;
			for (let i = 1; i <= steps; i++) {
				const t = i / steps;
				await page.mouse.move(
					x1 + (x2 - x1) * t,
					y1 + (y2 - y1) * t
				);
			}
			await page.mouse.up();
			await page.waitForTimeout(200);
			break;
		}
		case 'wheel': {
			const [x, y, delta] = params;
			await page.mouse.move(x, y);
			await page.mouse.wheel(0, delta);
			await page.waitForTimeout(200);
			break;
		}
		case 'key': {
			const [k] = params;
			await page.keyboard.press(k);
			await page.waitForTimeout(100);
			break;
		}
		case 'wait': {
			const [ms] = params;
			await page.waitForTimeout(ms);
			break;
		}
		case 'evaluate': {
			const [fnString] = params;
			await page.evaluate(fnString);
			break;
		}
		default:
			throw new Error(`Unknown trace action: ${action}`);
	}
}

/**
 * Run an assertion against `window.__waffle` state.
 * @param {import('@playwright/test').Page} page
 * @param {object} assertion - { fn, path?, check?, expected }
 */
async function runAssertion(page, assertion) {
	const { fn, path, check, expected } = assertion;

	const actual = await page.evaluate(({ fn, path, check }) => {
		const waffle = window.__waffle;
		if (!waffle) return undefined;

		let result = waffle[fn]();

		if (path) {
			const parts = path.split('.');
			for (const part of parts) {
				if (result == null) return undefined;
				result = result[part];
			}
		}

		if (check) {
			if (result == null) return undefined;
			return result[check];
		}

		return result;
	}, { fn, path, check });

	expect(actual).toEqual(expected);
}

/**
 * Replay a full trace (array of steps) on the page.
 * Each step is executed in order. If a step has an `assert` property,
 * the assertion is checked after that step completes.
 * @param {import('@playwright/test').Page} page
 * @param {Array<object>} steps - array of { action, params, assert? }
 */
export async function replayTrace(page, steps) {
	for (const step of steps) {
		await replayStep(page, step);

		if (step.assert) {
			await runAssertion(page, step.assert);
		}
	}
}

/**
 * Load a trace from a JSON file on disk.
 * @param {string} path - absolute or relative path to the JSON trace file
 * @returns {Promise<Array<object>>}
 */
export async function loadTrace(path) {
	const content = await readFile(path, 'utf-8');
	return JSON.parse(content);
}
