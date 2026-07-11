/**
 * Toast notification tests — verifies the ToastContainer renders correctly,
 * toasts appear/dismiss as expected, and engine errors surface as toasts.
 */
import { test, expect } from './helpers/waffle-test.js';

/**
 * Sketch a closed square on the XY plane and finish the sketch.
 * API entity creation is test SETUP (per GUI test rules), not drawing-mode
 * coverage — these tests exercise toast behavior, not pointer drawing.
 */
async function drawSquare(page, x0, y0, x1, y1) {
	const before = await page.evaluate(() => window.__waffle?.getFeatureTree()?.features?.length ?? 0);
	await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	await page.evaluate(([ax0, ay0, ax1, ay1]) => {
		const w = window.__waffle;
		w.addSketchEntity({ type: 'Point', id: 1, x: ax0, y: ay0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 2, x: ax1, y: ay0, construction: false });
		w.addSketchEntity({ type: 'Point', id: 3, x: ax1, y: ay1, construction: false });
		w.addSketchEntity({ type: 'Point', id: 4, x: ax0, y: ay1, construction: false });
		w.addSketchEntity({ type: 'Line', id: 5, start_id: 1, end_id: 2, construction: false });
		w.addSketchEntity({ type: 'Line', id: 6, start_id: 2, end_id: 3, construction: false });
		w.addSketchEntity({ type: 'Line', id: 7, start_id: 3, end_id: 4, construction: false });
		w.addSketchEntity({ type: 'Line', id: 8, start_id: 4, end_id: 1, construction: false });
	}, [x0, y0, x1, y1]);
	await page.waitForTimeout(200);
	await page.evaluate(() => window.__waffle.finishSketch());
	await page.waitForFunction(
		(n) => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) > n,
		before,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);
}

/** Extrude the last sketch with combine=Add (plus optional extras, e.g. targets). */
async function applyAddExtrude(page, depth, extraOpts = {}) {
	const before = await page.evaluate(() => window.__waffle.getFeatureTree().features.length);
	await page.evaluate(() => window.__waffle.showExtrudeDialog());
	await page.waitForTimeout(100);
	await page.evaluate(([d, opts]) => window.__waffle.applyExtrude(d, 0, false, { combine: 'Add', ...opts }), [depth, extraOpts]);
	await page.waitForFunction(
		(n) => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) > n,
		before,
		{ timeout: 10000 }
	);
	await page.waitForTimeout(200);
}

test.describe('toast notifications', () => {
	test('toast container present', async ({ waffle }) => {
		const container = waffle.page.locator('[data-testid="toast-container"]');
		await expect(container).toBeAttached();
	});

	test('showToast error renders', async ({ waffle }) => {
		const page = waffle.page;

		// Clear any existing toasts
		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('error', 'Test error message'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-error/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Test error message');
	});

	test('showToast success renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('success', 'Operation succeeded'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-success/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Operation succeeded');
	});

	test('showToast info renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Informational notice'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-info/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Informational notice');
	});

	test('showToast warning renders', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const id = await page.evaluate(() => window.__waffle.showToast('warning', 'Warning notice'));
		expect(id).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).toHaveClass(/toast-warning/);

		const message = await toast.locator('.toast-message').textContent();
		expect(message).toBe('Warning notice');
	});

	test('toast close button dismisses', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show a toast with a long duration so it does not auto-dismiss
		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Dismiss me', 30000));

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });

		// Click the close button
		const closeBtn = page.locator(`[data-testid="toast-close-${id}"]`);
		await expect(closeBtn).toBeVisible();
		await closeBtn.click();

		// Toast should be removed from the DOM
		await expect(toast).not.toBeAttached({ timeout: 3000 });
	});

	test('multiple toasts stack', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show 3 toasts with long duration
		await page.evaluate(() => {
			window.__waffle.showToast('error', 'First toast', 30000);
			window.__waffle.showToast('warning', 'Second toast', 30000);
			window.__waffle.showToast('info', 'Third toast', 30000);
		});

		// Wait for the toasts to render
		await page.waitForTimeout(300);

		const container = page.locator('[data-testid="toast-container"]');
		const toastItems = container.locator('.toast');
		await expect(toastItems).toHaveCount(3);
	});

	test('toast auto-dismisses', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show a toast with a short 500ms duration
		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Auto-dismiss test', 500));

		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });

		// Wait for auto-dismiss — should disappear within 1500ms
		await expect(toast).not.toBeAttached({ timeout: 1500 });
	});

	test('first extrude with default Add is silent — no standalone toast, ever', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// First extrude with combine=Add on an empty document: ShareAFace finds
		// no target body → benign standalone. Spec §4.1 (amended 2026-07-11):
		// this must be SILENT — Add is the dialog default, so this is the most
		// common flow in the app.
		await drawSquare(page, -30, -30, 30, 30);
		await applyAddExtrude(page, 60);

		// Watch for any standalone toast for a beat, then sketch on the same
		// document (rebuild replays the extrude) and watch again.
		await page.waitForTimeout(1000);
		let toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.filter(t => t.message.includes('standalone')).length).toBe(0);

		const before = await page.evaluate(() => window.__waffle.getFeatureTree().features.length);
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({ type: 'Point', id: 1, x: 5, y: 5, construction: false });
		});
		await page.waitForTimeout(200);
		await page.evaluate(() => window.__waffle.finishSketch());
		await page.waitForFunction(
			(n) => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) > n,
			before,
			{ timeout: 10000 }
		);
		await page.waitForTimeout(1000);

		toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.filter(t => t.message.includes('standalone')).length).toBe(0);
	});

	test('persisted rebuild warning toasts once, not on every rebuild', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Box A at the origin.
		await drawSquare(page, -30, -30, 30, 30);
		await applyAddExtrude(page, 60);
		const extrudeId = await page.evaluate(() => {
			const feats = window.__waffle.getFeatureTree().features;
			return feats.find(f => f.operation?.type === 'Extrude').id;
		});

		// Box B far away, explicitly targeting body A with Add. The disjoint
		// union keeps both bodies and bakes a "remain disjoint" warning into
		// the feature's diagnostics. It must toast ONCE here.
		await drawSquare(page, 200, 200, 260, 260);
		await applyAddExtrude(page, 60, {
			targets: [{
				kind: { type: 'Solid' },
				anchor: { type: 'FeatureOutput', feature_id: extrudeId, output_key: { type: 'Main' } },
				selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
				policy: { type: 'BestEffort' }
			}]
		});
		await page.waitForFunction(
			() => window.__waffle.getToasts().some(t => t.message.includes('disjoint')),
			{ timeout: 5000 }
		);

		// Acknowledge everything (also resets the toast repeat-suppression
		// window, so a re-fire WOULD show — this isolates the rebuild-diff
		// logic from the toast rate limiter).
		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Sketch again on the same document — the rebuild replays the extrude
		// and its persisted warning. No new toast may appear.
		const before = await page.evaluate(() => window.__waffle.getFeatureTree().features.length);
		await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true,
			{ timeout: 5000 }
		);
		await page.evaluate(() => {
			window.__waffle.addSketchEntity({ type: 'Point', id: 1, x: 5, y: 5, construction: false });
		});
		await page.waitForTimeout(200);
		await page.evaluate(() => window.__waffle.finishSketch());
		await page.waitForFunction(
			(n) => (window.__waffle?.getFeatureTree()?.features?.length ?? 0) > n,
			before,
			{ timeout: 10000 }
		);
		await page.waitForTimeout(1000);

		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		const disjointToasts = toasts.filter(t => t.message.includes('disjoint'));
		expect(disjointToasts.length).toBe(0);
	});

	test('engine error triggers toast', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Trigger an error via showToast (simulating engine error path)
		await page.evaluate(() => window.__waffle.showToast('error', 'Simulated engine error'));

		// Verify the error toast appeared in DOM
		const errorToast = page.locator('.toast-error');
		await expect(errorToast).toBeVisible({ timeout: 2000 });
		await expect(errorToast).toContainText('Simulated engine error');

		// Verify via API
		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		const hasError = toasts.some(t => t.level === 'error');
		expect(hasError).toBe(true);
	});

	test('duplicate toast while visible is suppressed', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		const [first, second] = await page.evaluate(() => [
			window.__waffle.showToast('warning', 'Duplicate message', 30000),
			window.__waffle.showToast('warning', 'Duplicate message', 30000),
		]);

		// The second call must not stack a duplicate: it returns the visible
		// toast's id and the container holds exactly one toast.
		expect(second).toBe(first);
		await page.waitForTimeout(300);
		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(1);
	});

	test('re-show within suppress window after dismissal is suppressed', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show with a short duration and let it auto-dismiss.
		const id = await page.evaluate(() => window.__waffle.showToast('info', 'Windowed message', 300));
		const toast = page.locator(`[data-testid="toast-item-${id}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
		await expect(toast).not.toBeAttached({ timeout: 2000 });

		// Re-showing the same (level, message) inside the suppress window is a
		// no-op signalled by a 0 return.
		const again = await page.evaluate(() => window.__waffle.showToast('info', 'Windowed message', 300));
		expect(again).toBe(0);
		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(0);
	});

	test('stack past the cap auto-clears older toasts', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// TOAST_STACK_MAX is 4 — a 5th distinct toast clears the pile and
		// survives alone.
		await page.evaluate(() => {
			for (let i = 1; i <= 5; i++) {
				window.__waffle.showToast('info', `Burst toast ${i}`, 30000);
			}
		});
		await page.waitForTimeout(300);

		const toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(1);
		expect(toasts[0].message).toBe('Burst toast 5');
	});

	test('dismissAllToasts resets repeat suppression', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		await page.evaluate(() => window.__waffle.showToast('info', 'Reset message', 30000));
		// Explicit clear acknowledges the toast — the same message may show
		// again immediately.
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		const again = await page.evaluate(() => window.__waffle.showToast('info', 'Reset message', 30000));
		expect(again).toBeGreaterThan(0);

		const toast = page.locator(`[data-testid="toast-item-${again}"]`);
		await expect(toast).toBeVisible({ timeout: 3000 });
	});

	test('programmatic dismiss works', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate(() => window.__waffle.dismissAllToasts());

		// Show several toasts with long duration
		await page.evaluate(() => {
			window.__waffle.showToast('error', 'Toast A', 30000);
			window.__waffle.showToast('info', 'Toast B', 30000);
			window.__waffle.showToast('success', 'Toast C', 30000);
		});

		await page.waitForTimeout(300);

		// Verify they are present
		let toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(3);

		// Dismiss all
		await page.evaluate(() => window.__waffle.dismissAllToasts());
		await page.waitForTimeout(300);

		// Verify all are gone
		toasts = await page.evaluate(() => window.__waffle.getToasts());
		expect(toasts.length).toBe(0);

		// Verify no toast elements remain in the DOM
		const container = waffle.page.locator('[data-testid="toast-container"]');
		const toastItems = container.locator('.toast');
		await expect(toastItems).toHaveCount(0);
	});
});
