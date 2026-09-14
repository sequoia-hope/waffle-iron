/**
 * Agent-link executor: maps `call` frames to tool implementations and builds
 * `result` frames (specs/waffle_mcp_server.md §2.3, §2.7, §3.2, §6.1).
 *
 * Commands pass the page state gates (G3–G7), then run holding the engine lock
 * as 'agent' for the whole call (I6): user sends wait, modeling UI is refused
 * (G8). Queries that send a bridge message take the lock too; the others read
 * store state only.
 */
import {
	AGENT_WORKING_HINT,
	EngineLockTimeout,
	getActiveTabId,
	getDocumentTabs,
	getToolHint,
	getUserBusyReason,
	isDocumentReadOnly,
	isEngineCrashed,
	isEngineReady,
	sendAgentMessage,
	setAgentActivity,
	setToolHint,
	withEngineLock
} from '$lib/engine/store.svelte.js';
import { COMMANDS, snapshotNow } from './commands.js';
import { sameModel } from './delta.js';
import { DOCUMENT_COMMANDS, DOCUMENT_QUERIES } from './documents.js';
import { EXPORT_QUERIES } from './export.js';
import { QUERIES } from './queries.js';
import { ToolFailure, toolError } from './results.js';
import { TOOL_NAMES } from './tools/index.js';
import { VIEWPORT_QUERIES } from './viewport.js';

export { toolError } from './results.js';

/** G2: how long a call waits for a user action to release the engine lock. */
export const AGENT_LOCK_WAIT_MS = 10000;

/** Commands that are not an engine undo step, so a cancelled call cannot be undone (A18). */
const NOT_UNDOABLE = new Set(['rollback_set']);

const BUSY_MESSAGES = {
	sketch_mode: 'The user is editing a sketch in Waffle Iron. Wait until they finish.',
	feature_dialog: 'The user has a feature dialog open in Waffle Iron. Wait until they close it.',
	edit_context: 'The user is editing a part in an assembly context. Wait until they leave it.'
};

/**
 * @typedef {{
 *   agentName: string,
 *   isPaused: () => boolean,
 *   pause: (reason: string) => void,
 *   isCancelled: () => boolean,
 * }} CallContext
 */

/**
 * Run `fn` holding the engine lock as 'agent'. A user action holding it longer
 * than AGENT_LOCK_WAIT_MS refuses the call (G2); another call of this agent
 * simply queues.
 * @template T
 * @param {() => Promise<T>} fn
 * @returns {Promise<T>}
 */
async function withAgentLock(fn) {
	for (;;) {
		try {
			return await withEngineLock('agent', fn, { timeoutMs: AGENT_LOCK_WAIT_MS });
		} catch (err) {
			if (!(err instanceof EngineLockTimeout)) throw err;
			if (err.holder === 'agent') continue;
			throw new ToolFailure('UserBusy', 'Waffle Iron is rebuilding a change the user made. Try again shortly.', {
				reason: 'rebuilding'
			});
		}
	}
}

/**
 * G4 and G3: the gates every mutating tool passes, or null.
 * @param {CallContext} ctx
 */
function pausedOrBusy(ctx) {
	if (ctx.isPaused()) {
		return new ToolFailure('AgentPaused', 'The user paused the agent in Waffle Iron. Wait until they resume it.', {});
	}
	const busy = getUserBusyReason();
	if (busy) return new ToolFailure('UserBusy', BUSY_MESSAGES[busy], { reason: busy });
	return null;
}

/**
 * The first page state gate that refuses an authoring command (§3.2), or null.
 * @param {CallContext} ctx
 */
function commandRefusal(ctx) {
	if (ctx.isPaused()) {
		return new ToolFailure('AgentPaused', 'The user paused the agent in Waffle Iron. Wait until they resume it.', {});
	}
	if (isDocumentReadOnly()) {
		return new ToolFailure('DocumentReadOnly', 'The open document is linked read-only; the user must fork it to allow edits.', {});
	}
	const tab = getDocumentTabs().find((t) => t.id === getActiveTabId());
	const kind = tab?.kind?.type ?? 'Part';
	if (kind !== 'Part') {
		return new ToolFailure('TabKindNotSupported', `The active tab is a ${kind} tab; agent edits work on Part tabs.`, { kind });
	}
	const busy = getUserBusyReason();
	if (busy) return new ToolFailure('UserBusy', BUSY_MESSAGES[busy], { reason: busy });
	return null;
}

/**
 * @param {string} tool
 * @param {(args: any, env: any) => Promise<object>} command
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 */
async function runCommand(tool, command, args, ctx) {
	const refusal = commandRefusal(ctx);
	if (refusal) throw refusal;
	return withAgentLock(async () => {
		// The page may have changed while the call waited for the lock (I12).
		const late = commandRefusal(ctx);
		if (late) throw late;
		const before = snapshotNow();
		setAgentActivity({ tool, agentName: ctx.agentName, quietErrors: true });
		try {
			const result = await command(args, { agentName: ctx.agentName, pause: ctx.pause });
			if (ctx.isCancelled() && !NOT_UNDOABLE.has(tool) && !sameModel(before, snapshotNow())) {
				// A18: a kernel op cannot be interrupted; undo the finished step. The
				// relay already answered the cancelled request, so this result is discarded.
				await sendAgentMessage({ type: 'Undo' }, { rebuild: true });
				return toolError('Cancelled', 'The call was cancelled; its step was undone.', { tool });
			}
			return result;
		} finally {
			setAgentActivity(null);
			if (getToolHint() === AGENT_WORKING_HINT) setToolHint(null);
		}
	});
}

/**
 * Document-level tools (open, new, save, tab switch) run the store's own
 * multi-message flows, which send through the gated user path; they cannot
 * hold the agent lock for the whole call, because each of their sends waits for
 * it. The agent activity still refuses the modeling UI meanwhile (G8), and G3
 * and G4 apply.
 * @param {string} tool
 * @param {(args: any, ctx: CallContext) => Promise<object>} run
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 */
async function runDocumentCommand(tool, run, args, ctx) {
	const refusal = pausedOrBusy(ctx);
	if (refusal) throw refusal;
	setAgentActivity({ tool, agentName: ctx.agentName, quietErrors: false });
	try {
		return await run(args, ctx);
	} finally {
		setAgentActivity(null);
		if (getToolHint() === AGENT_WORKING_HINT) setToolHint(null);
	}
}

/**
 * Run one tool call.
 * @param {string} tool
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 * @returns {Promise<{content: object[], structuredContent: object, isError: boolean}>}
 */
export async function executeTool(tool, args, ctx) {
	const known = TOOL_NAMES.has(tool);
	const query = known
		? (QUERIES[tool] ?? DOCUMENT_QUERIES[tool] ?? VIEWPORT_QUERIES[tool] ?? EXPORT_QUERIES[tool])
		: undefined;
	const command = known ? COMMANDS[tool] : undefined;
	const documentCommand = known ? DOCUMENT_COMMANDS[tool] : undefined;
	if (!query && !command && !documentCommand) {
		return toolError('ToolUnavailable', `This page has no tool named "${tool}".`, { tool });
	}
	// G6: nothing runs while the engine is not ready or has crashed.
	if (isEngineCrashed()) {
		return toolError(
			'EngineCrashed',
			'The Waffle Iron engine crashed in this tab and could not restart. Ask the user to reload the page.',
			{}
		);
	}
	if (!isEngineReady()) {
		return toolError('EngineNotReady', 'The Waffle Iron engine is not ready in this tab.', {});
	}
	try {
		if (query) {
			const env = { send: (message) => sendAgentMessage(message) };
			return query.engine ? await withAgentLock(() => query.run(args, env)) : await query.run(args, env);
		}
		if (documentCommand) return await runDocumentCommand(tool, documentCommand, args, ctx);
		return await runCommand(tool, /** @type {any} */ (command), args, ctx);
	} catch (err) {
		if (err instanceof ToolFailure) return toolError(err.code, err.detail, err.details);
		return toolError('Internal', `${tool} failed in the page: ${err?.message ?? String(err)}`, {});
	}
}

/**
 * Turn a `call` frame into its `result` frame.
 * @param {{ id: string, tool: string, arguments?: Record<string, unknown> }} frame
 * @param {CallContext} ctx
 */
export async function executeCall(frame, ctx) {
	const result = await executeTool(frame.tool, frame.arguments ?? {}, ctx);
	return { type: 'result', id: frame.id, ...result };
}
