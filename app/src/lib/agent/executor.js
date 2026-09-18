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
import { ASSEMBLY_COMMANDS, ASSEMBLY_QUERIES } from './assembly.js';
import { snapshotNow } from './commands.js';
import { newlyErroring, sameModel } from './delta.js';
import { DOCUMENT_COMMANDS, DOCUMENT_QUERIES } from './documents.js';
import { deliverDownload } from './export.js';
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
			const result = toolAnswer(answer);
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
 * Document-level tools (open, new, save, tab switch) and the assembly tools
 * run the store's own multi-message flows, which send through the gated user
 * path; they cannot hold the agent lock for the whole call, because each of
 * their sends waits for it. The agent activity still refuses the modeling UI
 * meanwhile (G8), and G3 and G4 apply. (An assembly edit is one
 * `EditAssembly` send, which the store's `editAssembly` makes through the
 * gated path too; its own gate — an Assembly tab must be active — is the
 * inverse of G7 and lives in `assembly.js`.)
 * @param {string} tool
 * @param {(args: any, ctx: CallContext) => Promise<object>} run
 * @param {Record<string, unknown>} args
 * @param {CallContext} ctx
 */
/** The tail of the document-command queue: the previous call's completion. */
let documentCommandTail = Promise.resolve();

async function runDocumentCommand(tool, run, args, ctx) {
	// One at a time (I6 for this path). The relay forwards calls as they
	// arrive; two assembly edits in flight at once each mutate the store's tab
	// copy and each send it — and the slower answer overwrites the faster
	// one's edit (measured: connectors added concurrently vanished). Queue
	// here, since these flows cannot hold the engine lock for the whole call.
	const turn = documentCommandTail.then(async () => {
		const refusal = pausedOrBusy(ctx);
		if (refusal) throw refusal;
		setAgentActivity({ tool, agentName: ctx.agentName, quietErrors: false });
		try {
			return await run(args, ctx);
		} finally {
			setAgentActivity(null);
			if (getToolHint() === AGENT_WORKING_HINT) setToolHint(null);
		}
	});
	// The chain never rejects: the next call must run whatever this one did.
	documentCommandTail = turn.then(
		() => undefined,
		() => undefined
	);
	return turn;
}


/**
 * The engine's answer to a `Tool` send, in the `result` frame's shape.
 * @param {any} answer
 * @returns {{content: object[], structuredContent: object, isError: boolean}}
 */
function toolAnswer(answer) {
	return {
		content: answer?.content ?? [],
		structuredContent: answer?.structuredContent ?? {},
		isError: !!answer?.isError
	};
}

/**
 * Tools whose semantics live in the engine (`crates/wasm-bridge/src/tools`,
 * `specs/waffle_server_mode.md` §2.3 S3): the page sends `Tool` and renders
 * the answer. There is no JS implementation to fall back to — C4b deleted the
 * authoring bodies, C5b the read-only ones, C6 the export pair's — so these
 * two sets are also the routing table: a name here reaches the engine and
 * nothing else.
 *
 * `ENGINE_QUERIES` change nothing and pass no authoring gate. They were
 * migrated shadowed (both implementations ran, `structuredContent` compared)
 * until the differential was green on a real model; `ENGINE_COMMANDS` could
 * not be (a step that changes the document cannot run twice on it), so what
 * they produced before the deletion is recorded in
 * `app/tests/gui/fixtures/agent-authoring-goldens.json` and
 * `agent-rust-authoring.spec.js` holds the engine to it. The export pair was
 * cut over against `agent-export-import.spec.js`, which drives the real relay
 * and predates the port; its `deliver:"download"` half stays in this page
 * (§3.3) — see `runEngineQuery`.
 *
 * Keep in sync with `tools::MIGRATED` (the union of both) and `tools::mutates`
 * (exactly `ENGINE_COMMANDS`); `agent-rust-tools.spec.js` and
 * `agent-rust-authoring.spec.js` pin them.
 */
const ENGINE_QUERIES = new Set([
	'model_summary',
	'feature_get',
	'body_measure',
	'face_list',
	'sketch_regions',
	'expression_evaluate',
	'export_step',
	'export_stl'
]);

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
	'redo',
	'sketch_create'
]);

/**
 * Run one read-only tool in the ENGINE (S3 C5b). It changes nothing, so no
 * authoring gate applies; it still holds the agent lock (I6), as every JS
 * query that sent a bridge message did — a user send in the middle of an
 * agent's read would answer about a different model.
 *
 * An export the agent asked to `deliver:"download"` comes back with the file
 * OUT OF BAND (`download`, S3 C6): the answer only describes it, and this
 * page hands it to the browser. `toolAnswer` drops the field, so the relay
 * never sees the file — but had this page not delivered it, nothing
 * downstream would notice either, which is why it is done here and not left
 * to a caller.
 *
 * @param {string} tool
 * @param {Record<string, unknown>} args
 */
async function runEngineQuery(tool, args) {
	return withAgentLock(async () => {
		const answer = await sendAgentMessage({ type: 'Tool', name: tool, arguments: args });
		if (answer?.download) deliverDownload(answer.download);
		return toolAnswer(answer);
	});
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
		? (QUERIES[tool] ?? DOCUMENT_QUERIES[tool] ?? VIEWPORT_QUERIES[tool] ?? ASSEMBLY_QUERIES[tool])
		: undefined;
	const documentCommand = known ? (DOCUMENT_COMMANDS[tool] ?? ASSEMBLY_COMMANDS[tool]) : undefined;
	// The engine sets are where those tools' implementations live now: none
	// of them has a JS body, so without them every one would be reported as a
	// tool this page lacks.
	if (!query && !documentCommand && !ENGINE_QUERIES.has(tool) && !ENGINE_COMMANDS.has(tool)) {
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
		if (ENGINE_QUERIES.has(tool)) return await runEngineQuery(tool, args);
		// Whatever reaches here is an engine command: the guard above refused
		// anything that is not a query, a document command, or one of these.
		return await runEngineCommand(tool, args, ctx);
	} catch (err) {
		if (err instanceof ToolFailure) return toolError(err.code, err.detail, err.details);
		return toolError('Internal', `${tool} failed in the page: ${err?.message ?? String(err)}`, {});
	}
}

// Test hook (agent-document-load-gate.spec.js, agent-rust-*.spec.js): run a
// tool in this page's own executor, without a relay, and read the routing
// table the specs hold to the engine's `MIGRATED` list.
if (typeof window !== 'undefined') {
	window.__waffleAgentExecutor = {
		executeTool,
		engineQueries: () => [...ENGINE_QUERIES],
		engineTools: () => [...ENGINE_COMMANDS]
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
