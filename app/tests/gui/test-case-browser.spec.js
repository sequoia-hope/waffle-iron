/**
 * Test Case Browser — Playwright tests for the test case management UI.
 *
 * Tests the full lifecycle: open/close browser panel, save test cases,
 * load them back, delete them, and filter by outcome/tag.
 */
import { test, expect } from './helpers/waffle-test.js';
import {
	clickSketch,
	clickRectangle,
	clickFinishSketch,
	clickExtrude,
	pressKey,
} from './helpers/toolbar.js';
import { drawRectangle } from './helpers/canvas.js';
import {
	waitForEntityCount,
	waitForFeatureCount,
	waitForMeshWithGeometry,
} from './helpers/state.js';

// ─── Helpers ──────────────────────────────────────────────────────────

/** Click the Tests button in the toolbar. */
async function clickTestsButton(page) {
	await page.locator('[data-testid="toolbar-btn-tests"]').click();
}

/** Wait for the test case browser panel to be visible. */
async function waitForBrowserOpen(page, timeout = 3000) {
	await page.locator('[data-testid="test-case-browser"]').waitFor({
		state: 'visible',
		timeout,
	});
}

/** Wait for the test case browser panel to be hidden. */
async function waitForBrowserClosed(page, timeout = 3000) {
	await page.locator('[data-testid="test-case-browser"]').waitFor({
		state: 'hidden',
		timeout,
	});
}

/** Clean up any test cases created during tests via the API. */
async function cleanupTestCases(page) {
	const manifest = await page.evaluate(async () => {
		const res = await fetch('/api/test-cases');
		return res.json();
	});
	for (const tc of manifest.cases) {
		await page.evaluate(async (id) => {
			await fetch(`/api/test-cases/${id}`, { method: 'DELETE' });
		}, tc.id);
	}
}

/** Create a test case via the API directly (for test setup). */
async function createTestCaseViaApi(page, { name, description, expectedOutcome, tags, waffleData }) {
	return page.evaluate(async (data) => {
		const res = await fetch('/api/test-cases', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(data),
		});
		return res.json();
	}, { name, description, expectedOutcome, tags, waffleData: waffleData || '{"features":[]}' });
}

/** Create a sketch + extrude so the model has saveable content. */
async function createSimpleModel(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try { await waitForEntityCount(waffle.page, 8, 5000); } catch { /* best effort */ }
	await clickFinishSketch(waffle.page);
	try { await waitForFeatureCount(waffle.page, 1, 10000); } catch { /* best effort */ }
	await clickExtrude(waffle.page);
	await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
	await waffle.page.locator('[data-testid="extrude-apply"]').click();
	try { await waitForFeatureCount(waffle.page, 2, 10000); } catch { /* best effort */ }
}

// ─── Tests ────────────────────────────────────────────────────────────

test.describe('test case browser', () => {
	test.beforeEach(async ({ waffle }) => {
		await cleanupTestCases(waffle.page);
	});

	test.afterEach(async ({ waffle }) => {
		await cleanupTestCases(waffle.page);
	});

	test('Tests button opens and closes the browser panel', async ({ waffle }) => {
		const page = waffle.page;

		// Panel should not be visible initially
		await expect(page.locator('[data-testid="test-case-browser"]')).not.toBeVisible();

		// Click Tests button to open
		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		// Panel is visible with expected elements
		await expect(page.locator('[data-testid="tcb-save-current"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-filter-outcome"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-filter-tag"]')).toBeVisible();

		// Click close button
		await page.locator('[data-testid="tcb-close"]').click();
		await waitForBrowserClosed(page);
	});

	test('Ctrl+Shift+T toggles the browser panel', async ({ waffle }) => {
		const page = waffle.page;

		// Open with keyboard shortcut
		await page.keyboard.press('Control+Shift+T');
		await waitForBrowserOpen(page);

		// Close with same shortcut
		await page.keyboard.press('Control+Shift+T');
		await waitForBrowserClosed(page);
	});

	test('empty state shows message', async ({ waffle }) => {
		const page = waffle.page;

		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		// Should show empty message
		const emptyMsg = page.locator('[data-testid="tcb-list"] .tcb-empty');
		await expect(emptyMsg).toBeVisible();
		await expect(emptyMsg).toContainText('No test cases');
	});

	test('save current model as test case', async ({ waffle }) => {
		const page = waffle.page;

		// Create a model first
		await createSimpleModel(waffle);

		// Open browser and click Save Current
		await clickTestsButton(page);
		await waitForBrowserOpen(page);
		await page.locator('[data-testid="tcb-save-current"]').click();

		// Save dialog should appear
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).toBeVisible();

		// Fill in the form
		await page.locator('[data-testid="stcd-name"]').fill('My Test Case');
		await page.locator('[data-testid="stcd-description"]').fill('A simple box extrusion');
		await page.locator('[data-testid="stcd-tags"]').fill('boolean, extrude');

		// Click Save
		await page.locator('[data-testid="stcd-save"]').click();

		// Dialog should close
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).not.toBeVisible({ timeout: 5000 });

		// Test case should appear in the list
		await expect(page.locator('[data-testid="tcb-case-my-test-case"]')).toBeVisible({ timeout: 5000 });
		await expect(page.locator('[data-testid="tcb-case-my-test-case"] .tcb-case-name')).toContainText('My Test Case');
	});

	test('cancel save dialog without saving', async ({ waffle }) => {
		const page = waffle.page;

		await clickTestsButton(page);
		await waitForBrowserOpen(page);
		await page.locator('[data-testid="tcb-save-current"]').click();

		// Dialog appears
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).toBeVisible();

		// Click Cancel
		await page.locator('[data-testid="stcd-cancel"]').click();
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).not.toBeVisible();

		// No test case should be created
		const emptyMsg = page.locator('[data-testid="tcb-list"] .tcb-empty');
		await expect(emptyMsg).toBeVisible();
	});

	test('load a test case', async ({ waffle }) => {
		const page = waffle.page;

		// Create a test case via API
		await createTestCaseViaApi(page, {
			name: 'Load Me',
			description: 'Test case to load',
			expectedOutcome: 'should_pass',
			tags: ['test'],
			waffleData: '{"features":[]}',
		});

		// Open browser
		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		// Wait for list to populate
		await expect(page.locator('[data-testid="tcb-case-load-me"]')).toBeVisible({ timeout: 5000 });

		// Click Load button
		await page.locator('[data-testid="tcb-load-load-me"]').click();

		// Should see a toast confirming load
		await page.waitForTimeout(500);
	});

	test('delete a test case', async ({ waffle }) => {
		const page = waffle.page;

		// Create a test case via API
		await createTestCaseViaApi(page, {
			name: 'Delete Me',
			description: 'Will be deleted',
			expectedOutcome: 'should_pass',
			tags: [],
		});

		// Open browser
		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		// Wait for case to appear
		await expect(page.locator('[data-testid="tcb-case-delete-me"]')).toBeVisible({ timeout: 5000 });

		// Click Delete
		await page.locator('[data-testid="tcb-delete-delete-me"]').click();

		// Case should disappear
		await expect(page.locator('[data-testid="tcb-case-delete-me"]')).not.toBeVisible({ timeout: 5000 });

		// Verify via API it's gone
		const manifest = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases');
			return res.json();
		});
		expect(manifest.cases.find(c => c.id === 'delete-me')).toBeUndefined();
	});

	test('filter by expected outcome', async ({ waffle }) => {
		const page = waffle.page;

		// Create test cases with different outcomes
		await createTestCaseViaApi(page, {
			name: 'Passing Case',
			expectedOutcome: 'should_pass',
			tags: [],
		});
		await createTestCaseViaApi(page, {
			name: 'Failing Case',
			expectedOutcome: 'known_failure',
			tags: [],
		});
		await createTestCaseViaApi(page, {
			name: 'Regression Case',
			expectedOutcome: 'regression',
			tags: [],
		});

		// Open browser
		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		// All three visible initially
		await expect(page.locator('[data-testid="tcb-case-passing-case"]')).toBeVisible({ timeout: 5000 });
		await expect(page.locator('[data-testid="tcb-case-failing-case"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-regression-case"]')).toBeVisible();

		// Filter to should_pass only
		await page.locator('[data-testid="tcb-filter-outcome"]').selectOption('should_pass');
		await expect(page.locator('[data-testid="tcb-case-passing-case"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-failing-case"]')).not.toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-regression-case"]')).not.toBeVisible();

		// Filter to known_failure
		await page.locator('[data-testid="tcb-filter-outcome"]').selectOption('known_failure');
		await expect(page.locator('[data-testid="tcb-case-passing-case"]')).not.toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-failing-case"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-regression-case"]')).not.toBeVisible();

		// Back to all
		await page.locator('[data-testid="tcb-filter-outcome"]').selectOption('all');
		await expect(page.locator('[data-testid="tcb-case-passing-case"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-failing-case"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-regression-case"]')).toBeVisible();
	});

	test('filter by tag', async ({ waffle }) => {
		const page = waffle.page;

		await createTestCaseViaApi(page, {
			name: 'Boolean Test',
			expectedOutcome: 'should_pass',
			tags: ['boolean', 'union'],
		});
		await createTestCaseViaApi(page, {
			name: 'Extrude Test',
			expectedOutcome: 'should_pass',
			tags: ['extrude'],
		});

		// Open browser
		await clickTestsButton(page);
		await waitForBrowserOpen(page);
		await expect(page.locator('[data-testid="tcb-case-boolean-test"]')).toBeVisible({ timeout: 5000 });
		await expect(page.locator('[data-testid="tcb-case-extrude-test"]')).toBeVisible();

		// Type 'boolean' in tag filter
		await page.locator('[data-testid="tcb-filter-tag"]').fill('boolean');
		await expect(page.locator('[data-testid="tcb-case-boolean-test"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-extrude-test"]')).not.toBeVisible();

		// Clear filter
		await page.locator('[data-testid="tcb-filter-tag"]').fill('');
		await expect(page.locator('[data-testid="tcb-case-boolean-test"]')).toBeVisible();
		await expect(page.locator('[data-testid="tcb-case-extrude-test"]')).toBeVisible();
	});

	test('refresh button reloads test cases', async ({ waffle }) => {
		const page = waffle.page;

		// Open browser (empty)
		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		const emptyMsg = page.locator('[data-testid="tcb-list"] .tcb-empty');
		await expect(emptyMsg).toBeVisible();

		// Create a test case via API (bypassing the UI)
		await createTestCaseViaApi(page, {
			name: 'Refresh Test',
			expectedOutcome: 'should_pass',
			tags: [],
		});

		// Click refresh
		await page.locator('[data-testid="tcb-refresh"]').click();

		// New case should appear
		await expect(page.locator('[data-testid="tcb-case-refresh-test"]')).toBeVisible({ timeout: 5000 });
	});

	test('test case shows tags and description', async ({ waffle }) => {
		const page = waffle.page;

		await createTestCaseViaApi(page, {
			name: 'Detailed Case',
			description: 'A detailed description',
			expectedOutcome: 'known_failure',
			tags: ['tag-a', 'tag-b'],
		});

		await clickTestsButton(page);
		await waitForBrowserOpen(page);

		const caseEl = page.locator('[data-testid="tcb-case-detailed-case"]');
		await expect(caseEl).toBeVisible({ timeout: 5000 });

		// Description visible
		await expect(caseEl.locator('.tcb-case-desc')).toContainText('A detailed description');

		// Tags visible
		await expect(caseEl.locator('.tcb-tag').first()).toContainText('tag-a');
		await expect(caseEl.locator('.tcb-tag').last()).toContainText('tag-b');

		// Outcome dot has warning class (known_failure)
		await expect(caseEl.locator('.tcb-dot')).toHaveClass(/dot-warn/);
	});

	test('save button disabled when name is empty', async ({ waffle }) => {
		const page = waffle.page;

		await clickTestsButton(page);
		await waitForBrowserOpen(page);
		await page.locator('[data-testid="tcb-save-current"]').click();

		await expect(page.locator('[data-testid="save-test-case-dialog"]')).toBeVisible();

		// Clear the name field
		await page.locator('[data-testid="stcd-name"]').fill('');

		// Save button should be disabled
		await expect(page.locator('[data-testid="stcd-save"]')).toBeDisabled();

		// Type a name — should enable
		await page.locator('[data-testid="stcd-name"]').fill('Valid Name');
		await expect(page.locator('[data-testid="stcd-save"]')).toBeEnabled();
	});

	test('escape closes save dialog', async ({ waffle }) => {
		const page = waffle.page;

		await clickTestsButton(page);
		await waitForBrowserOpen(page);
		await page.locator('[data-testid="tcb-save-current"]').click();
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).toBeVisible();

		// Focus inside the dialog first, then press Escape
		await page.locator('[data-testid="stcd-name"]').focus();
		await page.keyboard.press('Escape');
		await expect(page.locator('[data-testid="save-test-case-dialog"]')).not.toBeVisible();
	});

	test('API CRUD roundtrip', async ({ waffle }) => {
		const page = waffle.page;

		// Create
		const entry = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					name: 'API Test',
					description: 'Created via API',
					expectedOutcome: 'should_pass',
					tags: ['api'],
					waffleData: '{"test":true}',
				}),
			});
			return res.json();
		});
		expect(entry.id).toBe('api-test');
		expect(entry.name).toBe('API Test');

		// Read manifest
		const manifest = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases');
			return res.json();
		});
		expect(manifest.cases).toHaveLength(1);
		expect(manifest.cases[0].id).toBe('api-test');

		// Read .waffle file
		const waffleData = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases/api-test');
			return res.text();
		});
		expect(waffleData).toBe('{"test":true}');

		// Update metadata
		const updated = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases/api-test', {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ description: 'Updated description' }),
			});
			return res.json();
		});
		expect(updated.description).toBe('Updated description');

		// Delete
		await page.evaluate(async () => {
			await fetch('/api/test-cases/api-test', { method: 'DELETE' });
		});
		const afterDelete = await page.evaluate(async () => {
			const res = await fetch('/api/test-cases');
			return res.json();
		});
		expect(afterDelete.cases).toHaveLength(0);
	});
});
