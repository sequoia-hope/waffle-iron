/**
 * Yang Debug Pane (PR-VIZ-3b) — RED-phase Playwright tests.
 *
 * Spec: specs/yang_pr_viz_3b_app_debug_pane.md
 * Plan: ~/.claude/plans/reactive-juggling-sloth.md (sub-phase 0b)
 *
 * These 4 tests MUST FAIL on commit 218d6e3 — the toolbar button, pane,
 * feature dropdown, stage selector and ⚠ click handler do not yet exist.
 * Implementer-p (sub-phase 0c) turns them GREEN by building the UI on top
 * of PR-VIZ-3a's __waffle.{setYangDebugCapture,getYangStages,clearYangDebugCaptures}
 * exports.
 *
 * Per CLAUDE.md GUI test rules: every test sets up collectCrashErrors at the
 * top and asserts expectNoAnyCrash at the bottom. No swallowed assertion errors.
 *
 * Test #3 is the LOAD-BEARING canary (adversary-11 §6 + spec §7). If it fails
 * because PR-VIZ-3a's FinishSketch dispatch path was not wrapped (deviation #4),
 * that failure IS the empirical signal we want — DO NOT soften the assertion.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	collectCrashErrors,
	expectNoAnyCrash,
	getFeatureTree,
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

/** Create a sketch with a rectangle and finish it (mirrors extrude-second-depth pattern). */
async function createSketchWithRect(waffle, x1 = -80, y1 = -60, x2 = 80, y2 = 60) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, x1, y1, x2, y2);
	await waitForEntityCount(waffle.page, 8, 5000);
	await clickFinishSketch(waffle.page);
}

test.describe('Yang Debug Pane (PR-VIZ-3b)', () => {
	test('test_yang_debug_toolbar_button_toggles_pane', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Pane should not be visible initially.
		await expect(page.locator('[data-testid="yang-debug-pane"]')).not.toBeVisible();

		// Click toolbar button → pane visible.
		await page.locator('[data-testid="toolbar-btn-yang-debug"]').click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).toBeVisible({ timeout: 3000 });

		// Click again → pane hidden.
		await page.locator('[data-testid="toolbar-btn-yang-debug"]').click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).not.toBeVisible({ timeout: 3000 });

		expectNoAnyCrash(crashes);
	});

	test('test_yang_debug_capture_round_trip_via_ui', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Open pane (this auto-arms capture per spec §3).
		await page.locator('[data-testid="toolbar-btn-yang-debug"]').click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).toBeVisible({ timeout: 3000 });

		// Create a sketch + extrude → produces a feature whose Yang stages are captured.
		await createSketchWithRect(waffle);
		await waitForFeatureCount(page, 1, 10000);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		// Get the extrude feature id from the tree.
		const tree = await getFeatureTree(page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature, 'Extrude feature must exist').toBeTruthy();

		// Verify the pane's feature dropdown is populated and includes the extrude.
		const dropdown = page.locator('[data-testid="yang-debug-feature-select"]');
		await expect(dropdown).toBeVisible({ timeout: 3000 });
		const dropdownValues = await dropdown.locator('option').evaluateAll(opts =>
			opts.map(o => o.value)
		);
		expect(dropdownValues, 'dropdown should include the extrude feature id').toContain(extrudeFeature.id);

		// Close pane (auto-disarms + clears per spec §3).
		await page.locator('[data-testid="toolbar-btn-yang-debug"]').click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).not.toBeVisible({ timeout: 3000 });

		expectNoAnyCrash(crashes);
	});

	test('test_yang_debug_canary_sketch_extrude_boolean', async ({ waffle }) => {
		// LOAD-BEARING canary per adversary-11 §6 and spec §7.
		// Empirically validates the FinishSketch partial-coverage gap from
		// PR-VIZ-3a deviation #4. If `stages.length > 0` is FALSE, that is the
		// signal — do NOT soften this assertion. Escalate to team-lead instead.
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Open pane to arm capture BEFORE creating any features.
		await page.locator('[data-testid="toolbar-btn-yang-debug"]').click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).toBeVisible({ timeout: 3000 });

		// First sketch + extrude: a base box.
		await createSketchWithRect(waffle, -80, -60, 80, 60);
		await waitForFeatureCount(page, 1, 10000);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);

		// Second sketch + extrude that overlaps → triggers a boolean union.
		await createSketchWithRect(waffle, -40, -40, 120, 100);
		await waitForFeatureCount(page, 3, 10000);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('15');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 4, 15000);

		// Identify the second extrude (the one that triggers the boolean).
		const tree = await getFeatureTree(page);
		const extrudes = tree.features.filter(f => f.operation?.type === 'Extrude');
		expect(extrudes.length, 'expected two Extrude features').toBeGreaterThanOrEqual(2);
		const secondExtrude = extrudes[extrudes.length - 1];
		expect(secondExtrude, 'second Extrude must exist').toBeTruthy();

		// Pull the capture for the second extrude via __waffle.getYangStages.
		// Returns a JSON string ("null" string when absent) per PR-VIZ-3a spec §4.
		const captureJson = await page.evaluate(
			(id) => window.__waffle?.getYangStages?.(id) ?? null,
			secondExtrude.id
		);
		expect(captureJson, '__waffle.getYangStages must exist and return a string').toBeTruthy();
		expect(typeof captureJson).toBe('string');

		const capture = JSON.parse(captureJson);
		expect(capture, 'capture must not be null — boolean feature should have been captured').not.toBeNull();
		expect(Array.isArray(capture.stages), 'capture.stages must be an array').toBe(true);

		// LOAD-BEARING ASSERTION — DO NOT SOFTEN.
		// Failure here means the boolean/extrude dispatch path is not wrapped
		// by capture (PR-VIZ-3a deviation #4 was not benign). Escalate.
		expect(
			capture.stages.length,
			'second extrude (boolean) must have stages.length > 0 — if 0, the FinishSketch/extrude dispatch path is not wrapped (PR-VIZ-3a deviation #4); escalate to team-lead, do not soften'
		).toBeGreaterThan(0);

		expectNoAnyCrash(crashes);
	});

	test('test_yang_debug_error_icon_click_jumps_to_failed_stage', async ({ waffle }) => {
		// Test scope: the click-handler wiring on the FeatureTree ⚠ icon —
		// click → pane opens → that feature is selected. Tests #2 and #3 already
		// cover the dropdown population and stages-array pathways; this test
		// isolates the error-icon → pane lift behavior from spec §6.
		//
		// SETUP STRATEGY (synthetic injection, per team-lead resolution
		// 2026-05-04 option E): we use `__waffle.injectFeatureError(featureId,
		// message)` — a small ~5 LOC test-only helper added as part of
		// implementer-p's deliverable — to put a known feature into the
		// `featureErrors` map directly. Rationale: empirical verification on
		// `218d6e3` (test-author-d → team-lead two heartbeats) found that no
		// existing GUI flow reliably produces a `featureErrors` entry — F0031
		// fails at the assay watertight-oracle level (not engine-level), and
		// the cyl-cyl-cut "no Z overlap" bug has been FIXED on current main
		// (`cyl-cyl-cut-regression.spec.js` g3 explicitly asserts no error and
		// passes). Synthetic injection is the cleanest RED→GREEN signal for
		// the click-handler contract — and since the production failure path
		// populates the SAME map via `msg.errors` from rebuild, the click
		// handler is exercised identically.
		//
		// SCOPE NOTE: this test does NOT assert on `failed_at_stage` rendering,
		// because an injected error has no associated `yang_debug_captures`
		// entry. The full failed_at_stage flow is implicit in production
		// (real failures populate both `featureErrors` AND
		// `yang_debug_captures`); here we only verify the click handler wires
		// pane open + dropdown selection per spec §6.
		const page = waffle.page;
		const crashes = collectCrashErrors(page);

		// Setup: create a basic extrude so a feature exists in the tree.
		await createSketchWithRect(waffle);
		await waitForFeatureCount(page, 1, 10000);
		await clickExtrude(page);
		await page.locator('[data-testid="extrude-depth"]').fill('10');
		await page.locator('[data-testid="extrude-apply"]').click();
		await waitForFeatureCount(page, 2, 10000);
		await waitForMeshWithGeometry(page);

		// Inject a synthetic featureError on the extrude. Implementer-p adds
		// `__waffle.injectFeatureError` as a test-only helper to the existing
		// `__waffle` block at store.svelte.js:469-507 — minimal extension
		// (~5 LOC) that calls the same Map-reassignment pattern as the
		// real msg.errors handler at L332-340.
		const tree = await getFeatureTree(page);
		const extrudeFeature = tree.features.find(f => f.operation?.type === 'Extrude');
		expect(extrudeFeature, 'Extrude feature must exist for injection target').toBeTruthy();

		await page.evaluate(({ id, msg }) => {
			window.__waffle.injectFeatureError(id, msg);
		}, { id: extrudeFeature.id, msg: 'synthetic test error for PR-VIZ-3b ⚠ click handler' });

		// Wait briefly for Svelte reactivity (Map reassignment → derived
		// `featureErrors` in FeatureTree → ⚠ render).
		await page.waitForTimeout(300);

		// Find the rendered ⚠ icon. Spec §6 preserves
		// `data-testid="feature-error-{i}"` from the existing static span at
		// `FeatureTree.svelte:410`, so the selector survives the span→button
		// conversion.
		const errorIcons = page.locator('[data-testid^="feature-error-"]');
		await expect(errorIcons.first()).toBeVisible({ timeout: 5000 });

		// Pane should not yet be visible.
		await expect(page.locator('[data-testid="yang-debug-pane"]')).not.toBeVisible();

		// Click the ⚠ icon. RED today: it's a `<span>` with no onclick — the
		// click event fires but does nothing. GREEN post-fix: it's a `<button>`
		// whose onclick calls `selectYangDebugFeature(feature.id)` +
		// `toggleYangDebugPane()` per spec §6.
		await errorIcons.first().click();
		await expect(page.locator('[data-testid="yang-debug-pane"]')).toBeVisible({ timeout: 3000 });

		// The injected-error feature must be selected in the dropdown.
		const dropdown = page.locator('[data-testid="yang-debug-feature-select"]');
		await expect(dropdown).toBeVisible({ timeout: 3000 });
		const selectedFeatureId = await dropdown.inputValue();
		expect(
			selectedFeatureId,
			'⚠ click must select the failing feature in the dropdown'
		).toBe(extrudeFeature.id);

		expectNoAnyCrash(crashes);
	});
});
