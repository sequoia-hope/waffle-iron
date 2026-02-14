/**
 * State verification helpers — read-only queries via __waffle API.
 * Used to verify internal state matches expectations after GUI interactions.
 */

/**
 * Check if sketch mode is currently active.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<boolean>}
 */
export async function isSketchActive(page) {
	return page.evaluate(() => window.__waffle?.getState()?.sketchMode?.active ?? false);
}

/**
 * Get the currently active tool name.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<string>}
 */
export async function getActiveTool(page) {
	return page.evaluate(() => window.__waffle?.getState()?.activeTool ?? 'select');
}

/**
 * Get the number of sketch entities.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<number>}
 */
export async function getEntityCount(page) {
	return page.evaluate(() => window.__waffle?.getState()?.entityCount ?? 0);
}

/**
 * Get all sketch entities.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<Array<any>>}
 */
export async function getEntities(page) {
	return page.evaluate(() => window.__waffle?.getEntities() ?? []);
}

/**
 * Count entities of a specific type.
 * @param {import('@playwright/test').Page} page
 * @param {string} type - e.g. 'Point', 'Line', 'Circle', 'Arc'
 * @returns {Promise<number>}
 */
export async function getEntityCountByType(page, type) {
	return page.evaluate((t) => {
		const entities = window.__waffle?.getEntities() ?? [];
		return entities.filter(e => e.type === t).length;
	}, type);
}

/**
 * Get the feature tree.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<{features: Array<any>, active_index: number|null}>}
 */
export async function getFeatureTree(page) {
	return page.evaluate(() => window.__waffle?.getFeatureTree() ?? { features: [], active_index: null });
}

/**
 * Get the number of features in the tree.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<number>}
 */
export async function getFeatureCount(page) {
	return page.evaluate(() => {
		const tree = window.__waffle?.getFeatureTree();
		return tree?.features?.length ?? 0;
	});
}

/**
 * Check if a feature of a given type exists.
 * @param {import('@playwright/test').Page} page
 * @param {string} type - e.g. 'Sketch', 'Extrude'
 * @returns {Promise<boolean>}
 */
export async function hasFeatureOfType(page, type) {
	return page.evaluate((t) => {
		const tree = window.__waffle?.getFeatureTree();
		return (tree?.features ?? []).some(f => f.operation?.type === t);
	}, type);
}

/**
 * Get mesh data summary.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<Array<any>>}
 */
export async function getMeshes(page) {
	return page.evaluate(() => window.__waffle?.getMeshes() ?? []);
}

/**
 * Check if any mesh has actual geometry (triangles > 0).
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<boolean>}
 */
export async function hasMeshWithGeometry(page) {
	return page.evaluate(() => {
		const meshes = window.__waffle?.getMeshes() ?? [];
		return meshes.some(m => m.triangleCount > 0);
	});
}

/**
 * Wait until entity count reaches at least n.
 * @param {import('@playwright/test').Page} page
 * @param {number} n
 * @param {number} timeout
 */
export async function waitForEntityCount(page, n, timeout = 5000) {
	await page.waitForFunction(
		(expected) => (window.__waffle?.getState()?.entityCount ?? 0) >= expected,
		n,
		{ timeout }
	);
}

/**
 * Wait until feature count reaches at least n.
 * @param {import('@playwright/test').Page} page
 * @param {number} n
 * @param {number} timeout
 */
export async function waitForFeatureCount(page, n, timeout = 10000) {
	await page.waitForFunction(
		(expected) => {
			const tree = window.__waffle?.getFeatureTree();
			return (tree?.features?.length ?? 0) >= expected;
		},
		n,
		{ timeout }
	);
}

/**
 * Check if the engine is ready.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<boolean>}
 */
export async function isEngineReady(page) {
	return page.evaluate(() => window.__waffle?.getState()?.engineReady ?? false);
}

/**
 * Get the internal tool state machine state (e.g., 'idle', 'firstPointPlaced').
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<string>}
 */
export async function getToolState(page) {
	return page.evaluate(() => window.__waffle?.getToolState?.() ?? 'unknown');
}

/**
 * Get the full drawing state (tool state, isDragging, positions).
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<{toolState: string, isDragging: boolean, pointerDownPos: any, startPos: any, startPointId: any}>}
 */
export async function getDrawingState(page) {
	return page.evaluate(() => window.__waffle?.getDrawingState?.() ?? {
		toolState: 'unknown', isDragging: false,
		pointerDownPos: null, startPos: null, startPointId: null
	});
}

/**
 * Get the tool event log (last N events).
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<Array<{tool: string, event: string, x: number, y: number, toolState: string, isDragging: boolean, timestamp: number}>>}
 */
export async function getToolEventLog(page) {
	return page.evaluate(() => window.__waffle?.getToolEventLog?.() ?? []);
}

/**
 * Clear the tool event log.
 * @param {import('@playwright/test').Page} page
 */
export async function clearToolEventLog(page) {
	await page.evaluate(() => window.__waffle?.clearToolEventLog?.());
}

/**
 * Wait until the tool state matches the expected value.
 * @param {import('@playwright/test').Page} page
 * @param {string} expected
 * @param {number} timeout
 */
export async function waitForToolState(page, expected, timeout = 3000) {
	await page.waitForFunction(
		(exp) => window.__waffle?.getToolState?.() === exp,
		expected,
		{ timeout }
	);
}

/**
 * Wait until a tool event of the given type appears in the log.
 * @param {import('@playwright/test').Page} page
 * @param {string} eventType - e.g. 'pointerdown', 'pointerup'
 * @param {number} timeout
 */
export async function waitForToolEvent(page, eventType, timeout = 3000) {
	await page.waitForFunction(
		(evt) => {
			const log = window.__waffle?.getToolEventLog?.() ?? [];
			return log.some(e => e.event === evt);
		},
		eventType,
		{ timeout }
	);
}

/**
 * Get the extrude dialog state (null if not visible).
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<any>}
 */
export async function getExtrudeDialogState(page) {
	return page.evaluate(() => window.__waffle?.getExtrudeDialogState?.() ?? null);
}
