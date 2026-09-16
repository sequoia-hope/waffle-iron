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
	getDocumentLoadBusyReason,
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
import { showToast } from '$lib/ui/toast.svelte.js';
import { COMMANDS, snapshotNow } from './commands.js';
import { newlyErroring, sameModel } from './delta.js';
import { DOCUMENT_COMMANDS, DOCUMENT_QUERIES } from './documents.js';
import { EXPORT_QUERIES } from './export.js';
import { QUERIES } from './queries.js';
import { ToolFailure, toolError } from './results.js';
import { TOOL_NAMES } from './tools/index.js';
import { canonicalJson } from './tools/manifest.js';
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

const LOAD_MESSAGES = {
	restoring: 'Waffle Iron is reopening the last work in this tab. Try again shortly.',
	loading: 'Waffle Iron is still loading the document (a heavy model can take minutes to rebuild). Try again shortly.'
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
 * Run one authoring tool in the ENGINE (S3 C4), under the same page gates a
 * JS command passes.
 *
 * The engine decides what the step does, what it refuses and how the answer is
 * shaped; the page keeps what §3.3 leaves it — the lock, the busy/paused
 * gates, cancellation, and the rendering. The model update rides back with the
 * answer and reaches the store through the bridge, so the tree, meshes and
 * autosave refresh exactly as for a user action (I1).
 *
 * @param {string} tool
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 */
async function runEngineCommand(tool, args, ctx) {
	const refusal = commandRefusal(ctx);
	if (refusal) throw refusal;
	return withAgentLock(async () => {
		// The page may have changed while the call waited for the lock (I12).
		const late = commandRefusal(ctx);
		if (late) throw late;
		const before = snapshotNow();
		// `quietErrors`: the store must not toast the rebuild failures of this
		// step — they are reported once, below, as the agent's own (A2/A3).
		setAgentActivity({ tool, agentName: ctx.agentName, quietErrors: true });
		try {
			const answer = await sendAgentMessage({
				type: 'Tool',
				name: tool,
				arguments: args,
				context: { agent_name: ctx.agentName }
			});
			const result = {
				content: answer?.content ?? [],
				structuredContent: answer?.structuredContent ?? {},
				isError: !!answer?.isError
			};
			renderStepOutcome(tool, result, before, ctx);

			if (ctx.isCancelled() && !NOT_UNDOABLE.has(tool) && !sameModel(before, snapshotNow())) {
				// A18: a kernel op cannot be interrupted; undo the finished step.
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
 * Show what a step did, the way the JS commands did (§3.3: the engine decides,
 * the host renders).
 *
 * Which failures are NEW is a page question — it is a diff against what this
 * page already showed — so it is computed here rather than carried in the
 * delta, which the differential compares against the JS answer byte for byte.
 *
 * @param {string} tool
 * @param {{structuredContent: any, isError: boolean}} result
 * @param {ReturnType<typeof snapshotNow>} before
 * @param {CallContext} ctx
 */
function renderStepOutcome(tool, result, before, ctx) {
	const error = result.structuredContent?.error;
	if (result.isError) {
		// A rollback that could not restore the document exactly stops the
		// session: the engine cannot pause an agent, only say that it must be.
		if (error?.details?.pause_agent) ctx.pause('An agent step could not be rolled back exactly.');
		if (error?.details?.rolled_back) showToast('error', `Agent step rolled back: ${error.message}`);
		return;
	}
	for (const e of newlyErroring(before, snapshotNow())) {
		showToast('error', `${ctx.agentName}: Feature failed: ${e.message}`);
	}
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
 * Tools whose semantics have moved into the engine (`crates/wasm-bridge/src/tools`,
 * `specs/waffle_server_mode.md` §2.3 S3). Keep in sync with `tools::MIGRATED`.
 *
 * While a name is in this set BOTH implementations run and their
 * `structuredContent` must match: the JS answer is still the one returned, so
 * a divergence is visible without being served to an agent. The JS body is
 * deleted — and the name leaves this set — once the differential is green.
 */
const SHADOWED = new Set([
	'model_summary',
	'feature_get',
	'body_measure',
	'face_list',
	'sketch_regions',
	'expression_evaluate'
]);

/**
 * Tools whose semantics RUN in the engine (S3 C4): the page sends `Tool` and
 * renders the answer. There is no JS implementation left to fall back to —
 * C4b deleted those bodies from `commands.js` — so this set is also the
 * routing table: a name here reaches `runEngineCommand` and nothing else.
 *
 * They were never shadowed, because a step that changes the document cannot be
 * run twice on it to compare. What they produced before the deletion is
 * recorded in `app/tests/gui/fixtures/agent-authoring-goldens.json`, and
 * `agent-rust-authoring.spec.js` holds the engine to it.
 *
 * Keep in sync with `tools::mutates` and `tools::MIGRATED`.
 */
const ENGINE_COMMANDS = new Set([
	'feature_add',
	'feature_edit',
	'feature_delete',
	'feature_suppress',
	'feature_reorder',
	'feature_rename',
	'body_rename',
	'rollback_set',
	'parameters_set',
	'import_step',
	'undo',
	'redo'
]);

/** Off by default: shadowing takes the engine lock and costs a round trip. */
let shadowing = false;

/** @type {Array<{tool: string, page: string, engine: string}>} */
const shadowMismatches = [];

/** How many calls actually reached the engine: an empty mismatch list means
 * nothing unless the comparison ran. */
let shadowRuns = 0;

/**
 * Run the engine's implementation of `tool` and compare it with the page's.
 *
 * Only non-error results are compared: the refusals above (`EngineNotReady`,
 * `UserBusy`, …) are page state by §3.3, which the engine deliberately does
 * not model, so comparing them would report a difference that is by design.
 *
 * @param {string} tool
 * @param {Record<string, unknown>} args
 * @param {{content: object[], structuredContent: object, isError: boolean}} pageResult
 */
async function shadowAgainstEngine(tool, args, pageResult) {
	if (pageResult.isError) return pageResult;
	try {
		const answer = await withAgentLock(() =>
			sendAgentMessage({ type: 'Tool', name: tool, arguments: args })
		);
		shadowRuns += 1;
		const page = canonicalJson(pageResult.structuredContent);
		const engine = canonicalJson(answer?.structuredContent);
		if (page !== engine) {
			shadowMismatches.push({ tool, page, engine });
			console.error(`[agent] ${tool}: the engine and the page disagree\npage:   ${page}\nengine: ${engine}`);
		}
	} catch (err) {
		shadowMismatches.push({ tool, page: '', engine: `threw: ${err?.message ?? String(err)}` });
		console.error(`[agent] ${tool}: the engine's implementation threw`, err);
	}
	return pageResult;
}

/**
 * Run one tool call, shadowing the engine's implementation where there is one.
 * @param {string} tool
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 * @returns {Promise<{content: object[], structuredContent: object, isError: boolean}>}
 */
export async function executeTool(tool, args, ctx) {
	const result = await runTool(tool, args, ctx);
	if (!shadowing || !SHADOWED.has(tool)) return result;
	return shadowAgainstEngine(tool, args, result);
}

/**
 * @param {string} tool
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 * @returns {Promise<{content: object[], structuredContent: object, isError: boolean}>}
 */
async function runTool(tool, args, ctx) {
	const known = TOOL_NAMES.has(tool);
	const query = known
		? (QUERIES[tool] ?? DOCUMENT_QUERIES[tool] ?? VIEWPORT_QUERIES[tool] ?? EXPORT_QUERIES[tool])
		: undefined;
	const command = known ? COMMANDS[tool] : undefined;
	const documentCommand = known ? DOCUMENT_COMMANDS[tool] : undefined;
	// `ENGINE_COMMANDS` counts as an implementation: those twelve have no JS
	// body any more (C4b), so without it every one of them would be reported
	// as a tool this page does not have.
	if (!query && !command && !documentCommand && !ENGINE_COMMANDS.has(tool)) {
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
	// Until the open document is fully loaded the store describes the wrong
	// model (an empty tree, for minutes on a heavy document): refuse, never
	// answer from it (failure log F10).
	const loading = getDocumentLoadBusyReason();
	if (loading) {
		return toolError('UserBusy', LOAD_MESSAGES[loading], { reason: loading });
	}
	try {
		if (query) {
			const env = { send: (message) => sendAgentMessage(message) };
			return query.engine ? await withAgentLock(() => query.run(args, env)) : await query.run(args, env);
		}
		if (documentCommand) return await runDocumentCommand(tool, documentCommand, args, ctx);
		if (ENGINE_COMMANDS.has(tool)) return await runEngineCommand(tool, args, ctx);
		return await runCommand(tool, /** @type {any} */ (command), args, ctx);
	} catch (err) {
		if (err instanceof ToolFailure) return toolError(err.code, err.detail, err.details);
		return toolError('Internal', `${tool} failed in the page: ${err?.message ?? String(err)}`, {});
	}
}

// Test hook (agent-document-load-gate.spec.js): run a tool in this page's own
// executor, without a relay. `setShadow` drives the S3 differential
// (agent-rust-tools.spec.js): with it on, every migrated tool also runs in the
// engine and any disagreement lands in `getShadowMismatches()`.
if (typeof window !== 'undefined') {
	window.__waffleAgentExecutor = {
		executeTool,
		shadowedTools: () => [...SHADOWED],
		engineTools: () => [...ENGINE_COMMANDS],
		setShadow: (on) => {
			shadowing = !!on;
			shadowMismatches.length = 0;
			shadowRuns = 0;
		},
		getShadowMismatches: () => shadowMismatches.map((m) => ({ ...m })),
		getShadowRuns: () => shadowRuns
	};
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
