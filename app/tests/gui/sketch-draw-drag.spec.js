/**
 * Click-drag drawing mode tests.
 *
 * These tests verify that click-drag (as opposed to click-click) drawing
 * modes work correctly for all sketch tools, with focus on tool state
 * transitions, sequential operations, and mixed interaction modes.
 *
 * Per CLAUDE.md: "Every drawing mode needs BOTH click-click AND click-drag
 * tests."
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickLine, clickRectangle, clickCircle } from './helpers/toolbar.js';
import { dragLine, dragRectangle, dragCircle, drawLine } from './helpers/canvas.js';
import { getEntityCountByType, waitForEntityCount, getToolState, getDrawingState, waitForToolState } from './helpers/state.js';

test.describe('click-drag drawing modes', () => {
    test.beforeEach(async ({ waffle }) => {
        await clickSketch(waffle.page);
    });

    test('drag line creates 2 points and 1 line', async ({ waffle }) => {
        const page = waffle.page;
        await dragLine(page, -100, 0, 100, 0);
        await waitForEntityCount(page, 3, 5000);
        expect(await getEntityCountByType(page, 'Point')).toBe(2);
        expect(await getEntityCountByType(page, 'Line')).toBe(1);
    });

    test('drag rectangle creates 4 points and 4 lines', async ({ waffle }) => {
        const page = waffle.page;
        await clickRectangle(page);
        await dragRectangle(page, -80, -60, 80, 60);
        await waitForEntityCount(page, 8, 5000);
        expect(await getEntityCountByType(page, 'Point')).toBe(4);
        expect(await getEntityCountByType(page, 'Line')).toBe(4);
    });

    test('drag circle creates 1 center point and 1 circle', async ({ waffle }) => {
        const page = waffle.page;
        await clickCircle(page);
        await dragCircle(page, 0, 0, 60, 0);
        await waitForEntityCount(page, 2, 5000);
        expect(await getEntityCountByType(page, 'Point')).toBe(1);
        expect(await getEntityCountByType(page, 'Circle')).toBe(1);
    });

    test('drag line tool state transitions: idle -> idle after completion', async ({ waffle }) => {
        const page = waffle.page;
        // Before drawing, tool should be idle
        const stateBefore = await getToolState(page);
        expect(stateBefore).toBe('idle');

        // Draw a line by drag
        await dragLine(page, -80, 0, 80, 0);
        await waitForEntityCount(page, 3, 5000);

        // After completing the drag, tool returns to idle
        const stateAfter = await getToolState(page);
        expect(stateAfter).toBe('idle');
    });

    test('drag rectangle tool state returns to idle after completion', async ({ waffle }) => {
        const page = waffle.page;
        await clickRectangle(page);
        const stateBefore = await getToolState(page);
        expect(stateBefore).toBe('idle');

        await dragRectangle(page, -70, -50, 70, 50);
        await waitForEntityCount(page, 8, 5000);

        const stateAfter = await getToolState(page);
        expect(stateAfter).toBe('idle');
    });

    test('multiple drag lines in sequence create correct entity counts', async ({ waffle }) => {
        const page = waffle.page;

        // First drag line
        await dragLine(page, -100, -50, 100, -50);
        await waitForEntityCount(page, 3, 5000);

        // Second drag line
        await dragLine(page, -100, 0, 100, 0);
        await waitForEntityCount(page, 6, 5000);

        // Third drag line
        await dragLine(page, -100, 50, 100, 50);
        await waitForEntityCount(page, 9, 5000);

        expect(await getEntityCountByType(page, 'Point')).toBe(6);
        expect(await getEntityCountByType(page, 'Line')).toBe(3);
    });

    test('drag then click-click in sequence works correctly', async ({ waffle }) => {
        const page = waffle.page;

        // First: drag a line
        await dragLine(page, -100, -30, 100, -30);
        await waitForEntityCount(page, 3, 5000);

        // Second: click-click a line
        await drawLine(page, -100, 30, 100, 30);
        await waitForEntityCount(page, 6, 5000);

        expect(await getEntityCountByType(page, 'Point')).toBe(4);
        expect(await getEntityCountByType(page, 'Line')).toBe(2);
    });

    test('very short drag below threshold produces consistent state', async ({ waffle }) => {
        const page = waffle.page;
        // Drag only ~1 pixel — below DRAG_THRESHOLD_PX=5
        // This should act as a click (firstPointPlaced) or be ignored (idle)
        await dragLine(page, 0, 0, 1, 1);
        // Short drag might be treated as a click and wait for second point
        // or might be ignored. Either way, verify state is consistent.
        const state = await getToolState(page);
        // Tool should either be idle (drag ignored) or firstPointPlaced (treated as click)
        expect(['idle', 'firstPointPlaced']).toContain(state);
    });
});
