/**
 * Agent-link failure F10 (docs/notes/agent_bicycle_session_failures_2026_09_14.md):
 * after a tab reload reopened a heavy document, the agent link resumed before
 * the document had loaded and `model_summary` answered `features: []` for minutes
 * — the store still held the blank bootstrap tree. Calls must be refused with
 * `UserBusy` while a document loads, and answered once it has.
 *
 * Deterministic without a relay: `openDocumentRecord` sets the load flag before
 * its first await, so a call made in the same tick always lands during the load.
 * Uses the page's own store and executor instances (window test hooks) — a module
 * imported by URL from the test would be a second, unconnected instance.
 */
import { test, expect } from './helpers/waffle-test.js';

test.describe('Agent calls during a document load', () => {
	test('are refused with UserBusy while the document loads, then answered', async ({ waffle }) => {
		const page = waffle.page;
		await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.executeTool === 'function', {
			timeout: 15000
		});

		const result = await page.evaluate(async () => {
			const w = window.__waffle;
			const { executeTool } = window.__waffleAgentExecutor;
			const ctx = {
				agentName: 'load-gate-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false
			};
			// A document this engine composed itself, so the load cannot fail on format.
			const json = await w.buildDocumentJson();
			const docId = JSON.parse(json).document.id;
			const opening = w.openDocumentRecord(docId, json);
			const reasonDuring = w.getDocumentLoadBusyReason();
			const during = await executeTool('model_summary', {}, ctx);
			await opening;
			const after = await executeTool('model_summary', {}, ctx);
			return { reasonDuring, during, after, reasonAfter: w.getDocumentLoadBusyReason() };
		});

		expect(result.reasonDuring).toBe('loading');
		expect(result.during.isError).toBe(true);
		expect(result.during.structuredContent.error.code).toBe('UserBusy');
		expect(result.during.structuredContent.error.details.reason).toBe('loading');

		expect(result.reasonAfter).toBeNull();
		expect(result.after.isError).toBe(false);
		expect(Array.isArray(result.after.structuredContent.features)).toBe(true);
	});
});
