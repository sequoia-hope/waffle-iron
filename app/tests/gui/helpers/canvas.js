/**
 * Canvas drawing interaction helpers — mouse interactions relative to canvas center.
 */

/**
 * Get the canvas element's bounding box.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<{x: number, y: number, width: number, height: number, centerX: number, centerY: number} | null>}
 */
export async function getCanvasBounds(page) {
	const canvas = page.locator('canvas');
	const box = await canvas.boundingBox();
	if (!box) return null;
	return {
		x: box.x,
		y: box.y,
		width: box.width,
		height: box.height,
		centerX: box.x + box.width / 2,
		centerY: box.y + box.height / 2,
	};
}

/**
 * Click at a pixel offset from the canvas center.
 * @param {import('@playwright/test').Page} page
 * @param {number} xOffset - pixels from center (positive = right)
 * @param {number} yOffset - pixels from center (positive = down)
 */
export async function clickAt(page, xOffset, yOffset) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');

	const x = bounds.centerX + xOffset;
	const y = bounds.centerY + yOffset;
	await page.mouse.click(x, y);
	await page.waitForTimeout(150);
}

/**
 * Draw a line with the line tool — two clicks.
 * @param {import('@playwright/test').Page} page
 * @param {number} x1 - start x offset from center
 * @param {number} y1 - start y offset from center
 * @param {number} x2 - end x offset from center
 * @param {number} y2 - end y offset from center
 */
export async function drawLine(page, x1, y1, x2, y2) {
	await clickAt(page, x1, y1);
	await clickAt(page, x2, y2);
}

/**
 * Draw a rectangle with the rectangle tool — two corner clicks.
 * @param {import('@playwright/test').Page} page
 * @param {number} x1 - first corner x offset from center
 * @param {number} y1 - first corner y offset from center
 * @param {number} x2 - opposite corner x offset from center
 * @param {number} y2 - opposite corner y offset from center
 */
export async function drawRectangle(page, x1, y1, x2, y2) {
	await clickAt(page, x1, y1);
	await clickAt(page, x2, y2);
}

/**
 * Draw a circle with the circle tool — center + edge click.
 * @param {import('@playwright/test').Page} page
 * @param {number} cx - center x offset from center
 * @param {number} cy - center y offset from center
 * @param {number} rx - edge x offset from center
 * @param {number} ry - edge y offset from center
 */
export async function drawCircle(page, cx, cy, rx, ry) {
	await clickAt(page, cx, cy);
	await clickAt(page, rx, ry);
}

/**
 * Perform an orbit drag (left-button drag) on the canvas.
 * @param {import('@playwright/test').Page} page
 * @param {number} startX - start x offset from center
 * @param {number} startY - start y offset from center
 * @param {number} endX - end x offset from center
 * @param {number} endY - end y offset from center
 */
export async function orbitDrag(page, startX, startY, endX, endY) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');

	const sx = bounds.centerX + startX;
	const sy = bounds.centerY + startY;
	const ex = bounds.centerX + endX;
	const ey = bounds.centerY + endY;

	await page.mouse.move(sx, sy);
	await page.mouse.down();
	// Move in small steps for smoother drag
	const steps = 5;
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(
			sx + (ex - sx) * t,
			sy + (ey - sy) * t
		);
	}
	await page.mouse.up();
	await page.waitForTimeout(200);
}

/**
 * Move the mouse to a position on the canvas without clicking.
 * Useful for triggering hover/snap events during drawing.
 * @param {import('@playwright/test').Page} page
 * @param {number} xOffset - pixels from center
 * @param {number} yOffset - pixels from center
 */
export async function moveTo(page, xOffset, yOffset) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');

	const x = bounds.centerX + xOffset;
	const y = bounds.centerY + yOffset;
	await page.mouse.move(x, y);
	await page.waitForTimeout(100);
}

/**
 * Zoom via mouse wheel at canvas center.
 * @param {import('@playwright/test').Page} page
 * @param {number} deltaY - positive = zoom out, negative = zoom in
 */
export async function zoom(page, deltaY) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');

	await page.mouse.move(bounds.centerX, bounds.centerY);
	await page.mouse.wheel(0, deltaY);
	await page.waitForTimeout(200);
}
