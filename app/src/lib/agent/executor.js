/**
 * Agent-link executor: maps `call` frames to tool implementations and builds
 * `result` frames (specs/waffle_mcp_server.md §2.3, §2.7, §6.1).
 *
 * Phase 0 has one read-only page tool and no engine lock. Queries read store
 * state directly; they never send bridge messages.
 */
import {
	getBodies,
	getDocumentName,
	getFeatureErrors,
	getFeatureTree,
	getRebuildWarnings,
	isEngineReady
} from '$lib/engine/store.svelte.js';
import { summarizeModel } from './summary.js';
import { TOOL_NAMES } from './tools/index.js';

/**
 * An `isError` tool result carrying `structuredContent.error = {code, message, details}` (I10).
 * @param {string} code
 * @param {string} message
 * @param {Record<string, unknown>} [details]
 */
export function toolError(code, message, details = {}) {
	return {
		content: [{ type: 'text', text: `${code}: ${message}` }],
		structuredContent: { error: { code, message, details } },
		isError: true
	};
}

/** @param {Record<string, unknown>} structured */
function toolOk(structured) {
	return {
		content: [{ type: 'text', text: JSON.stringify(structured) }],
		structuredContent: structured,
		isError: false
	};
}

/**
 * Plain JSON snapshot of reactive store state ($state proxies do not survive postMessage/JSON reliably).
 * @template T
 * @param {T} value
 * @returns {T}
 */
function snapshot(value) {
	return JSON.parse(JSON.stringify(value));
}

const IMPLEMENTATIONS = {
	model_summary: () =>
		toolOk(
			summarizeModel({
				documentName: getDocumentName(),
				featureTree: snapshot(getFeatureTree()),
				featureErrors: new Map(getFeatureErrors()),
				bodies: getBodies(),
				warnings: getRebuildWarnings()
			})
		)
};

/**
 * Run one tool call.
 * @param {string} tool
 * @param {Record<string, unknown>} _args
 * @returns {Promise<{content: object[], structuredContent: object, isError: boolean}>}
 */
export async function executeTool(tool, _args) {
	const impl = TOOL_NAMES.has(tool) ? IMPLEMENTATIONS[tool] : undefined;
	if (!impl) {
		return toolError('ToolUnavailable', `This page has no tool named "${tool}".`, { tool });
	}
	// G6: queries are refused only while the engine is not ready. The store does
	// not yet distinguish a crashed worker from one still loading (§3.2 G6
	// EngineCrashed needs a store getter; Phase 1).
	if (!isEngineReady()) {
		return toolError('EngineNotReady', 'The Waffle Iron engine is not ready in this tab.', {});
	}
	try {
		return impl(_args);
	} catch (err) {
		return toolError('Internal', `${tool} failed in the page: ${err?.message ?? String(err)}`, {});
	}
}

/**
 * Turn a `call` frame into its `result` frame.
 * @param {{ id: string, tool: string, arguments?: Record<string, unknown> }} frame
 */
export async function executeCall(frame) {
	const result = await executeTool(frame.tool, frame.arguments ?? {});
	return { type: 'result', id: frame.id, ...result };
}
