/**
 * Trace suite — replays pre-recorded interaction traces and verifies
 * that the app remains responsive and doesn't crash.
 *
 * Each trace is a JSON file with a sequence of click, drag, wheel, key,
 * wait, and evaluate actions. After replaying, we verify the canvas is
 * still visible and the __waffle API is still responsive.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { replayTrace, loadTrace } from '../helpers/trace.js';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TRACES_DIR = path.resolve(__dirname, '../../fixtures/traces');

/**
 * Helper: replay a trace file and assert canvas is still alive after.
 */
async function replayAndVerify(waffle, traceName) {
	const tracePath = path.join(TRACES_DIR, traceName);
	const steps = await loadTrace(tracePath);
	await replayTrace(waffle.page, steps);

	// Canvas should still be visible
	const canvas = waffle.page.locator('canvas');
	await expect(canvas).toBeVisible();

	// __waffle API should still be responsive
	const apiAlive = await waffle.page.evaluate(
		() => typeof window.__waffle?.getState === 'function'
	);
	expect(apiAlive).toBe(true);
}

test.describe('trace suite — viewport', () => {
	test('orbit trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'viewport-orbit.json');
	});

	test('pan trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'viewport-pan.json');
	});

	test('zoom trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'viewport-zoom.json');
	});

	test('fit-all trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'viewport-fit-all.json');
	});

	test('orbit+zoom combo trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'viewport-orbit-zoom-combo.json');
	});
});

test.describe('trace suite — sketch', () => {
	test('draw line trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'sketch-draw-line.json');
	});

	test('draw rectangle trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'sketch-draw-rectangle.json');
	});

	test('draw circle trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'sketch-draw-circle.json');
	});

	test('snap to origin trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'sketch-snap-origin.json');
	});

	test('constrain horizontal trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'sketch-constrain-horizontal.json');
	});
});

test.describe('trace suite — selection', () => {
	test('face pick trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'selection-face-pick.json');
	});

	test('edge pick trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'selection-edge-pick.json');
	});

	test('box select window trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'selection-box-select-window.json');
	});

	test('box select crossing trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'selection-box-select-crossing.json');
	});

	test('select other trace replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'selection-select-other.json');
	});
});

test.describe('trace suite — workflow', () => {
	test('sketch-extrude workflow replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'workflow-sketch-extrude.json');
	});

	test('sketch-on-face workflow replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'workflow-sketch-on-face.json');
	});

	test('multi-feature workflow replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'workflow-multi-feature.json');
	});

	test('orbit-select-sketch workflow replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'workflow-orbit-select-sketch.json');
	});

	test('zoom-select-zoom workflow replays without crash', async ({ waffle }) => {
		await replayAndVerify(waffle, 'workflow-zoom-select-zoom.json');
	});
});
