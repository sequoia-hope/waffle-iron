/**
 * Agent command implementations (specs/waffle_mcp_server.md §2.5 Authoring, §3.3).
 *
 * The executor calls these holding the engine lock as 'agent'. Every message
 * goes through `sendAgentMessage`, so the page's own handlers update the tree,
 * meshes and autosave exactly as for a user action (I1). A step that makes a
 * feature newly fail is undone and verified byte-exact unless the caller asked
 * to keep it (A2, A3, I3).
 */
import {
	beginSketchPlaneRef,
	computeFacePlane,
	getBodies,
	getFeatureErrors,
	getFeatureTree,
	sendAgentMessage,
	sketchRegionsRequest
} from '$lib/engine/store.svelte.js';
import { buildFinishProfiles } from '$lib/sketch/finishProfiles.js';
import { extractProfiles } from '$lib/sketch/profiles.js';
import { showToast } from '$lib/ui/toast.svelte.js';
import { modelDelta, newlyErroring, sameModel, takeSnapshot } from './delta.js';
import { requireBody, requireFeature } from './queries.js';
import { fail, plain, toolOk } from './results.js';
import { sketchInputProblem } from './sketchInput.js';

/**
 * @typedef {{ agentName: string, pause: (reason: string) => void }} CommandEnv
 */

/** Fillet, chamfer and shell are deferred project-wide (A5, I11). */
const DEFERRED = new Set(['Fillet', 'Chamfer', 'Shell']);
/** Operation kinds an agent may author through feature_add / feature_edit. */
const AUTHORABLE = new Set(['Sketch', 'Extrude', 'Revolve', 'BooleanCombine', 'DatumPlane', 'MateConnector']);

export function snapshotNow() {
	return takeSnapshot({ featureTree: getFeatureTree(), featureErrors: getFeatureErrors(), bodies: getBodies() });
}

/** @param {CommandEnv} env */
function agentProvenance(env) {
	return { origin: { type: 'Agent', name: env.agentName } };
}

/**
 * An engine rejection as a typed tool failure. Never parses message text:
 * the class comes from the bridge error's `kind` (ICR-2).
 * @param {CommandEnv} env
 * @param {any} err
 * @param {string} fallback - code when the rejection carries no engine kind (a bridge-level failure)
 */
function engineFailure(env, err, fallback) {
	const kind = err?.kind ?? null;
	const message = String(err?.message ?? err);
	const engine_error = { kind, message };
	if (err?.needsRestart) {
		env.pause('The engine crashed during an agent call. Reload the page.');
		return fail('EngineCrashed', message, { engine_error });
	}
	switch (kind?.type) {
		case 'FeatureNotFound':
			return fail('FeatureNotFound', message, { engine_error });
		case 'NothingToUndo':
			return fail('NothingToUndo', 'There is nothing to undo.', { engine_error });
		case 'NothingToRedo':
			return fail('NothingToRedo', 'There is nothing to redo.', { engine_error });
		case 'NotSupported':
			return fail('NotSupported', message, { engine_error });
	}
	if (kind) return fail('FeatureRebuildFailed', message, { engine_error });
	if (fallback === 'InvalidOperation') return fail('InvalidOperation', message, { schema_path: '/operation', reason: message });
	if (fallback === 'InvalidSketch') return fail('InvalidSketch', message, { reason: message });
	return fail(fallback, message, { engine_error });
}

/**
 * @param {CommandEnv} env
 * @param {object} message
 * @param {{ rebuild?: boolean, fallback?: string }} [opts]
 */
async function send(env, message, { rebuild = true, fallback = 'Internal' } = {}) {
	try {
		return await sendAgentMessage(plain(message), { rebuild });
	} catch (err) {
		throw engineFailure(env, err, fallback);
	}
}

/**
 * Send one model-changing message and account for it.
 *
 * `onError`: `rollback` undoes a step that makes any feature newly fail (A2,
 * A4) and throws; `keep` leaves it and marks `kept_with_error` (A3); `report`
 * leaves it without the flag (delete, suppress, … where dependents failing is
 * the expected outcome, A15).
 *
 * @param {CommandEnv} env
 * @param {object} message
 * @param {{ onError?: 'rollback' | 'keep' | 'report', before?: ReturnType<typeof snapshotNow>, fallback?: string }} [opts]
 */
async function applyStep(env, message, { onError = 'rollback', before = snapshotNow(), fallback } = {}) {
	const response = await send(env, message, { fallback });
	const after = snapshotNow();
	const typedErrors = response?.feature_errors ?? [];
	const kindOf = (id) => typedErrors.find((t) => t.feature_id === id)?.kind ?? null;
	const fresh = newlyErroring(before, after);
	const featureId = response?.feature_id ?? message.feature_id ?? null;

	if (fresh.length > 0 && onError === 'rollback') {
		const target = fresh.find((e) => e.feature_id === featureId) ?? fresh[0];
		const kind = kindOf(target.feature_id);
		await send(env, { type: 'Undo' });
		if (!sameModel(before, snapshotNow())) {
			env.pause('An agent step could not be rolled back exactly.');
			throw fail('Internal', 'The rollback did not restore the document exactly; the agent session is paused.', {
				feature_id: target.feature_id
			});
		}
		showToast('error', `Agent step rolled back: ${target.message}`);
		throw fail(kind?.type === 'NotSupported' ? 'NotSupported' : 'FeatureRebuildFailed', target.message, {
			feature_id: target.feature_id,
			engine_error: { kind, message: target.message },
			errors: fresh.map((e) => ({ ...e, kind: kindOf(e.feature_id) })),
			rolled_back: true
		});
	}

	const delta = modelDelta(before, after, { typedErrors, warnings: response?.warnings ?? [] });
	for (const e of fresh) showToast('error', `${env.agentName}: Feature failed: ${e.message}`);
	if (fresh.length > 0 && onError === 'keep') delta.kept_with_error = true;
	return { response, delta, featureId };
}

/** @param {any} operation */
function checkOperation(operation) {
	const type = operation?.type;
	if (DEFERRED.has(type)) {
		throw fail('Deferred', `${type} is deferred in Waffle Iron and cannot be authored.`, { operation: type });
	}
	if (type === 'ImportedBody') {
		throw fail('UseImportTool', 'Imported bodies come from a STEP import, not from feature_add or feature_edit.', {
			operation: type
		});
	}
	if (!AUTHORABLE.has(type)) {
		throw fail('InvalidOperation', `Operation type ${JSON.stringify(type)} cannot be authored.`, {
			schema_path: '/operation/type',
			reason: `expected one of ${[...AUTHORABLE].join(', ')}`
		});
	}
}

/** @param {string} id */
function provenanceOrigin(id) {
	return getFeatureTree()?.provenance?.[id]?.origin?.type ?? 'User';
}

/**
 * @type {Record<string, (args: any, env: CommandEnv) => Promise<object>>}
 */
export const COMMANDS = {
	async sketch_create(args, env) {
		const entities = plain(args.entities);
		const constraints = plain(args.constraints ?? []);
		const onError = args.on_error ?? 'rollback';
		const problem = sketchInputProblem(entities, constraints);
		if (problem) throw fail('InvalidSketch', problem, { reason: problem });

		/** @type {number[]} */ let origin;
		/** @type {number[]} */ let normal;
		/** @type {any} */ let faceRef = null;
		if (args.plane && 'origin' in args.plane) {
			origin = [...args.plane.origin];
			normal = [...args.plane.normal];
		} else {
			faceRef = plain(args.plane);
			const resolved = computeFacePlane(faceRef);
			if (!resolved) {
				throw fail('InvalidSketch', 'plane does not resolve to a face or datum plane of the open Part.', {
					reason: 'unresolved plane'
				});
			}
			origin = [...resolved.origin];
			normal = [...resolved.normal];
		}

		const before = snapshotNow();
		await send(env, { type: 'BeginSketch', plane: beginSketchPlaneRef(faceRef) }, { rebuild: false });
		const driving = constraints.filter((c) => !c.reference);
		const solvedMsg = await send(env, { type: 'SolveSketch', entities, constraints: driving }, { rebuild: false, fallback: 'InvalidSketch' });
		const solved = solvedMsg?.solved ?? {};
		const status = solved.status ?? { type: 'SolveFailed', reason: 'the solver returned no status' };
		const failedSolve = status.type === 'OverConstrained' || status.type === 'SolveFailed';
		if (failedSolve && onError === 'rollback') {
			throw fail('SketchSolveFailed', `The sketch did not solve (${status.type}); nothing was committed.`, {
				status: status.type,
				conflicts: status.conflicts ?? [],
				reason: status.reason ?? null
			});
		}

		/** @type {Map<number, {x: number, y: number}>} */
		const positions = new Map(
			Object.entries(solved.positions ?? {}).map(([id, p]) => [Number(id), { x: p[0] ?? p.x, y: p[1] ?? p.y }])
		);
		const radii = solved.radii ?? {};
		const solvedEntities = entities.map((e) => (e.type === 'Circle' && radii[e.id] != null ? { ...e, radius: radii[e.id] } : e));
		const { profiles, solvedPositions } = buildFinishProfiles(extractProfiles(solvedEntities, positions), solvedEntities, positions);

		const { delta, featureId } = await applyStep(
			env,
			{
				type: 'FinishSketch',
				solved_positions: solvedPositions,
				solved_profiles: profiles,
				plane_origin: origin,
				plane_normal: normal,
				entities: solvedEntities,
				constraints,
				projected: [],
				provenance: agentProvenance(env)
			},
			{ onError: failedSolve ? 'keep' : onError, before, fallback: 'InvalidSketch' }
		);

		/** @type {Record<string, unknown>} */
		const out = {
			feature_id: featureId,
			solve_status: status.type,
			dof: status.type === 'FullyConstrained' ? 0 : (status.dof ?? null),
			regions: []
		};
		try {
			// The committed feature, through the same request `sketch_regions` sends (gears expanded).
			const committed = plain(getFeatureTree()?.features?.find((f) => f.id === featureId));
			const regionsMsg = await sendAgentMessage(await sketchRegionsRequest(committed, (m) => sendAgentMessage(m)));
			out.regions = (regionsMsg?.regions ?? []).map((r) => ({ profile_entity_ids: r.profile_entity_ids ?? null, area_m2: r.area }));
		} catch (err) {
			// The sketch is committed; a region query failure must not report the step as failed.
			out.regions_error = String(err?.message ?? err);
		}
		return toolOk({ ...out, ...delta });
	},

	async import_step(args, env) {
		// The engine records Import provenance itself; unlike importStepFromText
		// this opens no placement dialog (the identity placement stands).
		const { delta, featureId } = await applyStep(
			env,
			{ type: 'ImportStep', file_name: args.file_name, data: args.step_text },
			{ onError: args.on_error ?? 'rollback', fallback: 'FeatureRebuildFailed' }
		);
		return toolOk({ feature_id: featureId, ...delta });
	},

	async feature_add(args, env) {
		checkOperation(args.operation);
		const { delta, featureId } = await applyStep(
			env,
			{ type: 'AddFeature', operation: args.operation, provenance: agentProvenance(env) },
			{ onError: args.on_error ?? 'rollback', fallback: 'InvalidOperation' }
		);
		return toolOk({ feature_id: featureId, ...delta });
	},

	async feature_edit(args, env) {
		const feature = requireFeature(args.feature_id);
		const origin = provenanceOrigin(feature.id);
		if (origin === 'Derived') {
			throw fail('DerivedFeatureReadOnly', 'This feature is regenerated from a source and cannot be edited.', {
				feature_id: feature.id
			});
		}
		if (origin === 'Import' || feature.operation?.type === 'ImportedBody') {
			throw fail('UseImportTool', 'Imported features are placed through the import dialog, not feature_edit.', {
				feature_id: feature.id
			});
		}
		checkOperation(args.operation);
		if (feature.operation?.type !== args.operation.type) {
			throw fail('OperationKindMismatch', `Feature ${feature.id} is a ${feature.operation?.type}, not a ${args.operation.type}.`, {
				expected: feature.operation?.type ?? null,
				got: args.operation.type
			});
		}
		const { delta } = await applyStep(
			env,
			{ type: 'EditFeature', feature_id: feature.id, operation: args.operation, provenance: agentProvenance(env) },
			{ onError: args.on_error ?? 'rollback', fallback: 'InvalidOperation' }
		);
		return toolOk(delta);
	},

	async feature_delete(args, env) {
		requireFeature(args.feature_id);
		const { delta } = await applyStep(env, { type: 'DeleteFeature', feature_id: args.feature_id }, { onError: 'report' });
		return toolOk(delta);
	},

	async feature_suppress(args, env) {
		requireFeature(args.feature_id);
		const message = { type: 'SuppressFeature', feature_id: args.feature_id, suppressed: args.suppressed };
		const { delta } = await applyStep(env, message, { onError: 'report' });
		return toolOk(delta);
	},

	async feature_reorder(args, env) {
		requireFeature(args.feature_id);
		const message = { type: 'ReorderFeature', feature_id: args.feature_id, new_position: args.new_position };
		const { delta } = await applyStep(env, message, { onError: 'report' });
		return toolOk(delta);
	},

	async feature_rename(args, env) {
		requireFeature(args.feature_id);
		const message = { type: 'RenameFeature', feature_id: args.feature_id, new_name: args.new_name };
		const { delta } = await applyStep(env, message, { onError: 'report' });
		return toolOk(delta);
	},

	async body_rename(args, env) {
		requireBody(args.body_id);
		const message = { type: 'RenameBody', body_id: args.body_id, new_name: args.new_name };
		const { delta } = await applyStep(env, message, { onError: 'report' });
		return toolOk(delta);
	},

	async rollback_set(args, env) {
		const count = getFeatureTree()?.features?.length ?? 0;
		if (args.index != null && args.index >= count) {
			throw fail('FeatureNotFound', `Rollback index ${args.index} is past the last feature (the tree has ${count}).`, {
				index: args.index,
				feature_count: count
			});
		}
		const { delta } = await applyStep(env, { type: 'SetRollbackIndex', index: args.index }, { onError: 'report' });
		return toolOk(delta);
	},

	async parameters_set(args, env) {
		const current = new Map((getFeatureTree()?.parameters ?? []).map((p) => [p.id, p]));
		const parameters = args.parameters.map((p) => ({
			id: p.id ?? crypto.randomUUID(),
			name: p.name,
			expression: p.expression,
			value: typeof current.get(p.id)?.value === 'number' ? current.get(p.id).value : 0
		}));
		const { delta } = await applyStep(env, { type: 'SetParameters', parameters }, { onError: 'report' });
		const evaluated = (plain(getFeatureTree()?.parameters) ?? []).map((p) => {
			/** @type {Record<string, unknown>} */
			const row = { id: p.id, name: p.name, value_mm: p.error ? null : (p.value ?? null) };
			if (p.error) row.error = p.error;
			return row;
		});
		return toolOk({ parameters: evaluated, ...delta });
	},

	async undo(_args, env) {
		const { delta } = await applyStep(env, { type: 'Undo' }, { onError: 'report' });
		return toolOk(delta);
	},

	async redo(_args, env) {
		const { delta } = await applyStep(env, { type: 'Redo' }, { onError: 'report' });
		return toolOk(delta);
	}
};
