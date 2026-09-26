/**
 * Toolbar interaction helpers — click buttons by data-testid.
 */

/** Plane name → built-in plane UUID */
const PLANE_IDS = {
	front: '00000000-0000-0000-0000-000000000001',
	top: '00000000-0000-0000-0000-000000000002',
	right: '00000000-0000-0000-0000-000000000003',
};

/**
 * Click a modeling or sketch TOOL by id at any window width. The toolbar
 * collapses its tool groups into a dropdown when they do not fit (mobile, or
 * a narrow desktop window), so a tool is either an inline button or a
 * dropdown item; both carry `toolbar-btn-<id>` and never coexist.
 * @param {import('@playwright/test').Page} page
 * @param {string} id  e.g. 'sketch', 'extrude', 'line'
 */
export async function clickTool(page, id) {
	const btn = page.locator(`[data-testid="toolbar-btn-${id}"]`);
	if (await btn.isVisible()) {
		await btn.click();
		return;
	}
	// Priority+ overflow: trailing tools sit in "More ▾" while the rest stay
	// inline; once every tool is collapsed the group is one mode dropdown.
	const more = page.locator('[data-testid="toolbar-btn-more-tools"]');
	if (await more.isVisible()) {
		await more.click();
	} else {
		const inSketch = await page.evaluate(() => window.__waffle?.getState()?.sketchMode?.active === true);
		const trigger = inSketch ? 'toolbar-btn-sketch-tools-dropdown' : 'toolbar-btn-modeling-dropdown';
		await page.locator(`[data-testid="${trigger}"]`).click();
	}
	await btn.waitFor({ state: 'visible', timeout: 3000 });
	await btn.click();
}

/**
 * Whether a tool is offered at all (inline, in "More ▾", or in the mode
 * dropdown) — the width-independent form of "the button is visible".
 * Leaves any dropdown it opened closed again.
 * @param {import('@playwright/test').Page} page
 * @param {string} id
 * @returns {Promise<boolean>}
 */
export async function isToolOffered(page, id) {
	const btn = page.locator(`[data-testid="toolbar-btn-${id}"]`);
	if (await btn.isVisible()) return true;
	const more = page.locator('[data-testid="toolbar-btn-more-tools"]');
	const inSketch = await page.evaluate(() => window.__waffle?.getState()?.sketchMode?.active === true);
	const modeTrigger = page.locator(`[data-testid="${inSketch ? 'toolbar-btn-sketch-tools-dropdown' : 'toolbar-btn-modeling-dropdown'}"]`);
	const trigger = (await more.isVisible()) ? more : (await modeTrigger.isVisible()) ? modeTrigger : null;
	if (!trigger) return false;
	await trigger.click();
	const offered = await btn.isVisible();
	await page.locator('.dropdown-backdrop').first().click({ position: { x: 5, y: 5 } });
	return offered;
}

/**
 * Click a file / view ACTION by id at any window width (Save, Open, Export…,
 * Tests, Examples, Assay, Planes, Axes, Section…). When the toolbar has
 * collapsed the action group it lives in the ⋮ overflow menu.
 * @param {import('@playwright/test').Page} page
 * @param {string} id  the part after `toolbar-btn-`
 */
export async function clickToolbarAction(page, id) {
	const btn = page.locator(`[data-testid="toolbar-btn-${id}"]`);
	if (await btn.isVisible()) {
		await btn.click();
		return;
	}
	await page.locator('[data-testid="toolbar-btn-overflow"]').click();
	await btn.waitFor({ state: 'visible', timeout: 3000 });
	await btn.click();
}

/**
 * Click the Sketch toolbar button and wait for sketch mode to activate.
 * @param {import('@playwright/test').Page} page
 */
export async function clickSketch(page, plane = 'front') {
	await clickTool(page, 'sketch');
	// If plane selection mode is active, select the requested plane
	const inPlaneSelectionMode = await page.evaluate(
		() => window.__waffle?.getState()?.sketchMode?.active === true
	);
	if (!inPlaneSelectionMode) {
		// Wait for plane selection prompt to appear
		const prompt = page.locator('[data-testid="sketch-plane-prompt"]');
		try {
			await prompt.waitFor({ state: 'visible', timeout: 2000 });
		} catch {
			// Prompt may not appear (e.g., sketch-on-face bypasses it)
		}
		// Select the plane via the test API
		const planeId = PLANE_IDS[plane] || PLANE_IDS.front;
		await page.evaluate((id) => {
			window.__waffle?.selectRef({ kind: { type: 'Face' }, anchor: { type: 'DatumPlane', id } });
		}, planeId);
	}
	// Wait for sketch mode to be active (toolbar switches to sketch tools)
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === true,
		{ timeout: 5000 }
	);
	// Allow Svelte reactivity to settle
	await page.waitForTimeout(200);
}

/**
 * Click the Line sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickLine(page) {
	await clickTool(page, 'line');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'line',
		{ timeout: 3000 }
	);
}

/**
 * Click the Rectangle sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickRectangle(page) {
	await clickTool(page, 'rectangle');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'rectangle',
		{ timeout: 3000 }
	);
}

/**
 * Select the Center Rectangle variant via the Rect split-button dropdown.
 * @param {import('@playwright/test').Page} page
 */
export async function clickCenterRectangle(page) {
	await page.locator('[data-testid="toolbar-btn-rectangle-menu"]').click();
	await page.locator('[data-testid="rect-variant-rectangle-center"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'rectangle-center',
		{ timeout: 3000 }
	);
}

/**
 * Click the Circle sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickCircle(page) {
	await clickTool(page, 'circle');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'circle',
		{ timeout: 3000 }
	);
}

/**
 * Click the Arc sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickArc(page) {
	await clickTool(page, 'arc');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'arc',
		{ timeout: 3000 }
	);
}

/**
 * Click the Select sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickSelect(page) {
	await clickTool(page, 'select');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'select',
		{ timeout: 3000 }
	);
}

/**
 * Click the Dimension sketch tool button.
 * @param {import('@playwright/test').Page} page
 */
export async function clickDimension(page) {
	await clickTool(page, 'dimension');
	await page.waitForFunction(
		() => window.__waffle?.getState()?.activeTool === 'dimension',
		{ timeout: 3000 }
	);
}

/**
 * Click the Finish Sketch button and wait for sketch mode to deactivate.
 * @param {import('@playwright/test').Page} page
 */
export async function clickFinishSketch(page) {
	await page.locator('[data-testid="toolbar-btn-finish-sketch"]').click();
	await page.waitForFunction(
		() => window.__waffle?.getState()?.sketchMode?.active === false,
		{ timeout: 10000 }
	);
	// Allow Svelte reactivity and engine processing to settle
	await page.waitForTimeout(300);
}

/**
 * Click the Extrude toolbar button and wait for the dialog.
 * @param {import('@playwright/test').Page} page
 */
export async function clickExtrude(page) {
	await clickTool(page, 'extrude');
	await page.locator('[data-testid="extrude-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/**
 * Click the Revolve toolbar button and wait for the dialog.
 * @param {import('@playwright/test').Page} page
 */
export async function clickRevolve(page) {
	await clickTool(page, 'revolve');
	await page.locator('[data-testid="revolve-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/**
 * Click the Pipe tool and wait for its dialog.
 * @param {import('@playwright/test').Page} page
 */
export async function clickPipe(page) {
	await clickTool(page, 'pipe');
	await page.locator('[data-testid="pipe-dialog"]').waitFor({ state: 'visible', timeout: 5000 });
}

/**
 * Press a keyboard shortcut key.
 * @param {import('@playwright/test').Page} page
 * @param {string} key
 */
export async function pressKey(page, key) {
	await page.keyboard.press(key);
	await page.waitForTimeout(100);
}

/**
 * Check if a specific toolbar button is visible.
 * @param {import('@playwright/test').Page} page
 * @param {string} buttonId
 * @returns {Promise<boolean>}
 */
export async function isToolbarButtonVisible(page, buttonId) {
	return page.locator(`[data-testid="toolbar-btn-${buttonId}"]`).isVisible();
}
