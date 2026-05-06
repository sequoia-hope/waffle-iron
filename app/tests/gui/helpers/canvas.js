/**
 * Canvas drawing interaction helpers — mouse interactions relative to canvas center.
 */

/**
 * Get the main 3D viewport canvas element's bounding box.
 *
 * Scoped to the Threlte viewport (`[data-testid="viewport"] canvas`) rather
 * than the bare `canvas` selector, so auxiliary mini-canvases (e.g.
 * YangDebugPane, ThumbnailViewport) do not trip Playwright's strict-mode
 * "resolved to N elements" violation.
 * @param {import('@playwright/test').Page} page
 * @returns {Promise<{x: number, y: number, width: number, height: number, centerX: number, centerY: number} | null>}
 */
export async function getCanvasBounds(page) {
	let canvas = page.locator('[data-testid="viewport"] canvas');
	if (await canvas.count() === 0) canvas = page.locator('canvas').first();
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
 * Draw a line by dragging from start to end.
 * Uses mouse.down -> mouse.move(steps) -> mouse.up
 * @param {import('@playwright/test').Page} page
 * @param {number} x1 - start x offset from center
 * @param {number} y1 - start y offset from center
 * @param {number} x2 - end x offset from center
 * @param {number} y2 - end y offset from center
 * @param {number} steps - number of intermediate move steps
 */
export async function dragLine(page, x1, y1, x2, y2, steps = 10) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	const sx = bounds.centerX + x1, sy = bounds.centerY + y1;
	const ex = bounds.centerX + x2, ey = bounds.centerY + y2;
	await page.mouse.move(sx, sy);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(sx + (ex - sx) * t, sy + (ey - sy) * t);
	}
	await page.mouse.up();
	await page.waitForTimeout(150);
}

/**
 * Draw a rectangle by dragging from one corner to the opposite corner.
 * @param {import('@playwright/test').Page} page
 * @param {number} x1 - first corner x offset from center
 * @param {number} y1 - first corner y offset from center
 * @param {number} x2 - opposite corner x offset from center
 * @param {number} y2 - opposite corner y offset from center
 * @param {number} steps - number of intermediate move steps
 */
export async function dragRectangle(page, x1, y1, x2, y2, steps = 10) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	const sx = bounds.centerX + x1, sy = bounds.centerY + y1;
	const ex = bounds.centerX + x2, ey = bounds.centerY + y2;
	await page.mouse.move(sx, sy);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(sx + (ex - sx) * t, sy + (ey - sy) * t);
	}
	await page.mouse.up();
	await page.waitForTimeout(150);
}

/**
 * Draw a circle by dragging from center to edge.
 * @param {import('@playwright/test').Page} page
 * @param {number} cx - center x offset from canvas center
 * @param {number} cy - center y offset from canvas center
 * @param {number} rx - edge x offset from canvas center
 * @param {number} ry - edge y offset from canvas center
 * @param {number} steps - number of intermediate move steps
 */
export async function dragCircle(page, cx, cy, rx, ry, steps = 10) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	const sx = bounds.centerX + cx, sy = bounds.centerY + cy;
	const ex = bounds.centerX + rx, ey = bounds.centerY + ry;
	await page.mouse.move(sx, sy);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(sx + (ex - sx) * t, sy + (ey - sy) * t);
	}
	await page.mouse.up();
	await page.waitForTimeout(150);
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

/**
 * Draw an arc with the arc tool — three clicks (center, start, end).
 * @param {import('@playwright/test').Page} page
 * @param {number} cx - center x offset from canvas center
 * @param {number} cy - center y offset from canvas center
 * @param {number} sx - arc start x offset from canvas center
 * @param {number} sy - arc start y offset from canvas center
 * @param {number} ex - arc end x offset from canvas center
 * @param {number} ey - arc end y offset from canvas center
 */
export async function drawArc(page, cx, cy, sx, sy, ex, ey) {
	await clickAt(page, cx, cy);
	await clickAt(page, sx, sy);
	await clickAt(page, ex, ey);
}

/**
 * Draw an arc by dragging — mousedown at center, move to start point, click, move to end, release.
 * Note: Arc drag drawing may vary based on implementation. This is a simplified version.
 * @param {import('@playwright/test').Page} page
 * @param {number} cx - center x offset from canvas center
 * @param {number} cy - center y offset from canvas center
 * @param {number} sx - arc start x offset from canvas center
 * @param {number} sy - arc start y offset from canvas center
 * @param {number} ex - arc end x offset from canvas center
 * @param {number} ey - arc end y offset from canvas center
 * @param {number} steps - number of intermediate move steps
 */
/**
 * Perform a single-finger touch drag via synthetic PointerEvents dispatched
 * directly on the canvas element. Uses pointerType='touch' so OrbitControls
 * processes them as touch input.
 * @param {import('@playwright/test').Page} page
 * @param {number} startX - absolute screen x
 * @param {number} startY - absolute screen y
 * @param {number} endX - absolute screen x
 * @param {number} endY - absolute screen y
 * @param {number} steps - number of intermediate move steps
 */
export async function touchDrag(page, startX, startY, endX, endY, steps = 5) {
	// Dispatch synthetic PointerEvents on the canvas with pointerType='touch'.
	// Events bubble to OrbitControls' domElement (canvas parent / Threlte wrapper).
	await page.evaluate(({ sx, sy, ex, ey, n }) => {
		const canvas = document.querySelector('[data-testid="viewport"] canvas') || document.querySelector('canvas');
		if (!canvas) throw new Error('Canvas not found');

		const fire = (type, x, y) => {
			canvas.dispatchEvent(new PointerEvent(type, {
				bubbles: true,
				cancelable: true,
				clientX: x,
				clientY: y,
				pointerId: 1,
				pointerType: 'touch',
				isPrimary: true,
				pressure: type === 'pointerup' ? 0 : 0.5,
			}));
		};

		fire('pointerdown', sx, sy);
		for (let i = 1; i <= n; i++) {
			const t = i / n;
			fire('pointermove', sx + (ex - sx) * t, sy + (ey - sy) * t);
		}
		fire('pointerup', ex, ey);
	}, { sx: startX, sy: startY, ex: endX, ey: endY, n: steps });
	// Wait for OrbitControls damping to apply across multiple animation frames
	await page.waitForTimeout(300);
}

/**
 * Simulate a long-press touch gesture (pointerdown, hold, pointerup).
 * Triggers the long-press context menu if held long enough without moving.
 * @param {import('@playwright/test').Page} page
 * @param {number} x - absolute screen x
 * @param {number} y - absolute screen y
 * @param {number} holdMs - how long to hold before releasing (default 600ms > 500ms threshold)
 */
export async function longPressTouch(page, x, y, holdMs = 600) {
	await page.evaluate(({ cx, cy }) => {
		const canvas = document.querySelector('[data-testid="viewport"] canvas') || document.querySelector('canvas');
		if (!canvas) throw new Error('Canvas not found');
		canvas.dispatchEvent(new PointerEvent('pointerdown', {
			bubbles: true, cancelable: true,
			clientX: cx, clientY: cy,
			pointerId: 1, pointerType: 'touch', isPrimary: true, pressure: 0.5,
		}));
	}, { cx: x, cy: y });

	await page.waitForTimeout(holdMs);

	await page.evaluate(({ cx, cy }) => {
		const canvas = document.querySelector('[data-testid="viewport"] canvas') || document.querySelector('canvas');
		if (!canvas) throw new Error('Canvas not found');
		canvas.dispatchEvent(new PointerEvent('pointerup', {
			bubbles: true, cancelable: true,
			clientX: cx, clientY: cy,
			pointerId: 1, pointerType: 'touch', isPrimary: true, pressure: 0,
		}));
	}, { cx: x, cy: y });

	// Allow context menu to render
	await page.waitForTimeout(100);
}

export async function dragArc(page, cx, cy, sx, sy, ex, ey, steps = 10) {
	const bounds = await getCanvasBounds(page);
	if (!bounds) throw new Error('Canvas not visible');
	// Arc is 3-click: center, start, end — drag version moves between points
	const cxAbs = bounds.centerX + cx, cyAbs = bounds.centerY + cy;
	const sxAbs = bounds.centerX + sx, syAbs = bounds.centerY + sy;
	const exAbs = bounds.centerX + ex, eyAbs = bounds.centerY + ey;
	// Click center
	await page.mouse.click(cxAbs, cyAbs);
	await page.waitForTimeout(150);
	// Drag from start to end
	await page.mouse.move(sxAbs, syAbs);
	await page.mouse.down();
	for (let i = 1; i <= steps; i++) {
		const t = i / steps;
		await page.mouse.move(sxAbs + (exAbs - sxAbs) * t, syAbs + (eyAbs - syAbs) * t);
	}
	await page.mouse.up();
	await page.waitForTimeout(150);
}
