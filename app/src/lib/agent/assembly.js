/**
 * Assembly tool implementations (specs/waffle_mcp_server.md §2.5 Assemblies).
 * They call the store flows the Assembly panel uses — `addInstance`,
 * `addConnector`, `addMate` and their edit/remove partners — so an agent's
 * assembly is exactly the one a user would have built (I1, I2). Every edit
 * goes through the store's `EditAssembly`, which re-solves the open tab; the
 * state it returns is read back from the store after that.
 */
import {
	addConnector,
	addInstance,
	addMate,
	ConnectorRefused,
	getActiveTabId,
	getAssembly,
	getAssemblyStatus,
	getDocumentTabs,
	getSources,
	getSourceTabs,
	isDocumentReadOnly,
	removeConnector,
	removeInstance,
	removeMate,
	updateConnector,
	updateInstance,
	updateMate
} from '$lib/engine/store.svelte.js';
import { eulerDegToQuat } from '$lib/engine/rotation.js';
import { fail, plain, toolOk } from './results.js';

const IDENTITY = () => ({ translation_m: [0, 0, 0], rotation_quat: [0, 0, 0, 1] });

/** The active Assembly tab, or a G7-shaped refusal (the inverse of the Part-tab gate). */
function requireAssemblyTab() {
	const tab = getDocumentTabs().find((t) => t.id === getActiveTabId());
	const kind = tab?.kind?.type ?? 'Part';
	if (kind !== 'Assembly') {
		throw fail(
			'TabKindNotSupported',
			`The active tab is a ${kind} tab; assembly tools need an Assembly tab (tab_add kind:"Assembly" or tab_switch).`,
			{ kind }
		);
	}
	return tab;
}

function requireEditable() {
	if (isDocumentReadOnly()) {
		throw fail('DocumentReadOnly', 'The open document is linked read-only; the user must fork it to allow edits.', {});
	}
}

/** @param {string} id */
function requireInstance(id) {
	const inst = getAssembly()?.instances.find((i) => i.id === id);
	if (!inst) throw fail('InstanceNotFound', `The open assembly has no instance ${id}.`, { instance_id: id });
	return inst;
}

/** @param {string} id */
function requireConnector(id) {
	const c = (getAssembly()?.connectors ?? []).find((x) => x.id === id);
	if (!c) throw fail('ConnectorNotFound', `The open assembly has no connector ${id}.`, { connector_id: id });
	return c;
}

/** @param {string} id */
function requireMate(id) {
	const m = (getAssembly()?.mates ?? []).find((x) => x.id === id);
	if (!m) throw fail('MateNotFound', `The open assembly has no mate ${id}.`, { mate_id: id });
	return m;
}

/**
 * Every tab an instance can be made of: this document's Part tabs and the
 * tabs of each available linked source.
 */
function availableParts() {
	// The Assembly panel's source options: this document's Part tabs, its
	// other Assembly tabs (sub-assemblies), then linked documents' tabs.
	const active = getActiveTabId();
	const own = getDocumentTabs()
		.map((t) => ({ tab_id: t.id, name: t.name, kind: t.kind?.type ?? 'Part', source_id: null, source_name: null }))
		.filter((t) => t.kind === 'Part' || (t.kind === 'Assembly' && t.tab_id !== active));
	const linked = [];
	const sources = plain(getSources()) ?? [];
	for (const [sourceId, tabs] of Object.entries(plain(getSourceTabs()) ?? {})) {
		const source = sources.find((s) => s.id === sourceId);
		for (const t of tabs) {
			if (t.kind !== 'Part' && t.kind !== 'Assembly') continue;
			linked.push({ tab_id: t.id, name: t.name, kind: t.kind, source_id: sourceId, source_name: source?.name ?? null });
		}
	}
	return [...own, ...linked];
}

/** The instance's part tab name, for reading. @param {any} inst */
function partName(inst) {
	const sourceId = inst.source?.source_id ?? null;
	if (sourceId) {
		const t = (getSourceTabs()[sourceId] ?? []).find((x) => x.id === inst.source.tab_id);
		return t?.name ?? inst.source.tab_id;
	}
	return getDocumentTabs().find((t) => t.id === inst.source?.tab_id)?.name ?? inst.source?.tab_id ?? '';
}

/** The open assembly as the tools return it (`assemblyStateSchema`). */
function assemblyState() {
	const tab = requireAssemblyTab();
	const asm = plain(tab.kind.assembly) ?? { instances: [] };
	const status = plain(getAssemblyStatus());
	const placements = status?.placements ?? asm.placements ?? {};
	const frames = new Map((status?.connectors ?? []).map((f) => [f.id, f]));
	return {
		tab_id: tab.id,
		name: tab.name,
		instances: (asm.instances ?? []).map((i) => ({
			id: i.id,
			name: i.name,
			source: { tab_id: i.source?.tab_id ?? '', source_id: i.source?.source_id ?? null },
			part_name: partName(i),
			transform: i.transform ?? IDENTITY(),
			fixed: !!i.fixed,
			suppressed: !!i.suppressed,
			placement: placements[i.id] ?? null
		})),
		connectors: (asm.connectors ?? []).map((c) => {
			const f = frames.get(c.id);
			return {
				id: c.id,
				name: c.name,
				instance_path: c.instance_path ?? [],
				part_connector: c.part_connector ?? null,
				geom_ref: c.geom_ref ?? null,
				frame: c.frame ?? { origin: [0, 0, 0], z_axis: [0, 0, 1], x_axis: [0, 0, 0] },
				anchor: c.anchor ?? 'middle',
				flip_z: !!c.flip_z,
				rotation_deg: c.rotation_deg ?? 0,
				offset_m: c.offset_m ?? [0, 0, 0],
				world_frame: f
					? { kind: f.kind ?? null, origin: f.origin, x_axis: f.x_axis, y_axis: f.y_axis, z_axis: f.z_axis }
					: null
			};
		}),
		mates: (asm.mates ?? []).map((m) => ({
			id: m.id,
			name: m.name,
			kind: m.kind ?? { type: 'Fastened' },
			connectors: [...m.connectors],
			suppressed: !!m.suppressed
		})),
		part_connectors: (status?.part_connectors ?? []).map((p) => ({
			feature_id: p.feature_id,
			name: p.name,
			instance_path: p.instance_path ?? [],
			kind: p.kind ?? null,
			origin: p.origin,
			x_axis: p.x_axis,
			y_axis: p.y_axis,
			z_axis: p.z_axis
		})),
		available_parts: availableParts(),
		errors: status?.errors ?? [],
		warnings: status?.warnings ?? []
	};
}

/**
 * The document Transform for a tool's transform argument over `current`.
 * @param {any} input
 * @param {any} current
 */
function transformFrom(input, current) {
	const t = plain(current) ?? IDENTITY();
	if (!input) return t;
	if (input.rotation_quat && input.rotation_euler_deg) {
		throw fail('InvalidArguments', 'Give rotation_quat or rotation_euler_deg, not both.', { field: 'transform' });
	}
	if (input.translation_m) t.translation_m = [...input.translation_m];
	if (input.rotation_quat) {
		const n = Math.hypot(...input.rotation_quat);
		if (!(n > 0)) throw fail('InvalidArguments', 'rotation_quat must be a non-zero quaternion.', { field: 'transform.rotation_quat' });
		t.rotation_quat = input.rotation_quat.map((v) => v / n);
	}
	if (input.rotation_euler_deg) t.rotation_quat = eulerDegToQuat(input.rotation_euler_deg);
	return t;
}

/** @param {any} err the store's `EditAssembly` failure */
function editFailed(err) {
	if (err instanceof ConnectorRefused) {
		return fail('ConnectorRefused', `Cannot put a connector here: ${err.reason}`, { reason: err.reason });
	}
	return fail('AssemblyEditFailed', `The assembly edit failed: ${err?.message ?? String(err)}`, {
		reason: String(err?.message ?? err)
	});
}

/**
 * Run one store edit; `null` from the store means the tab was not an
 * Assembly (checked above) or the engine refused — which now throws.
 * @template T
 * @param {() => Promise<T>} edit
 */
async function run(edit) {
	try {
		return await edit();
	} catch (err) {
		throw editFailed(err);
	}
}

/** @type {Record<string, { run: (args: any) => any }>} */
export const ASSEMBLY_QUERIES = {
	assembly_get: { run: () => toolOk(assemblyState()) }
};

/** @type {Record<string, (args: any) => Promise<object>>} */
export const ASSEMBLY_COMMANDS = {
	async instance_add({ tab_id, source_id = null, name, transform, fixed = false }) {
		requireEditable();
		requireAssemblyTab();
		const part = availableParts().find((p) => p.tab_id === tab_id && (p.source_id ?? null) === source_id);
		if (!part) {
			throw fail(
				'TabNotFound',
				`No Part or Assembly tab ${tab_id}${source_id ? ` in source ${source_id}` : ''} to place (an assembly cannot contain itself).`,
				{ tab_id, source_id }
			);
		}
		const t = transformFrom(transform, IDENTITY());
		const id = await run(() => addInstance({ tabId: tab_id, sourceId: source_id, name, transform: t, fixed }));
		if (!id) throw fail('Internal', 'The store did not add the instance.', { tab_id });
		return toolOk({ instance_id: id, ...assemblyState() });
	},

	async instance_edit({ instance_id, ...patch }) {
		requireEditable();
		requireAssemblyTab();
		const inst = requireInstance(instance_id);
		/** @type {Record<string, unknown>} */
		const update = {};
		if ('name' in patch) update.name = patch.name;
		if ('fixed' in patch) update.fixed = !!patch.fixed;
		if ('suppressed' in patch) update.suppressed = !!patch.suppressed;
		if ('transform' in patch) update.transform = transformFrom(patch.transform, inst.transform);
		await run(() => updateInstance(instance_id, update));
		return toolOk(assemblyState());
	},

	async instance_delete({ instance_id }) {
		requireEditable();
		requireAssemblyTab();
		requireInstance(instance_id);
		await run(() => removeInstance(instance_id));
		return toolOk(assemblyState());
	},

	async connector_add({ instance_path, part_connector = null, geom_ref = null, frame = null, name }) {
		requireEditable();
		requireAssemblyTab();
		requireInstance(instance_path[0]);
		const given = [part_connector, geom_ref, frame].filter((v) => v != null).length;
		if (given > 1) {
			throw fail('InvalidArguments', 'Give one of part_connector, geom_ref or frame, not several.', {});
		}
		if (part_connector) {
			const known = (getAssemblyStatus()?.part_connectors ?? []).some(
				(p) => p.feature_id === part_connector && p.instance_path.join() === instance_path.join()
			);
			if (!known) {
				throw fail(
					'ConnectorNotFound',
					`The part of instance ${instance_path.join('/')} has no evaluated MateConnector feature ${part_connector} (see assembly_get.part_connectors).`,
					{ part_connector, instance_path }
				);
			}
		}
		const id = await run(() =>
			addConnector({
				instancePath: instance_path,
				geomRef: geom_ref,
				partConnector: part_connector,
				frame: frame ? { origin: frame.origin ?? [0, 0, 0], z_axis: frame.z_axis ?? [0, 0, 1], x_axis: frame.x_axis ?? [0, 0, 0] } : null,
				name
			})
		);
		if (!id) throw fail('Internal', 'The store did not add the connector.', { instance_path });
		return toolOk({ connector_id: id, ...assemblyState() });
	},

	async connector_edit({ connector_id, ...patch }) {
		requireEditable();
		requireAssemblyTab();
		requireConnector(connector_id);
		/** @type {Record<string, unknown>} */
		const update = {};
		if ('name' in patch) update.name = patch.name;
		if ('anchor' in patch) update.anchor = patch.anchor;
		if ('flip_z' in patch) update.flipZ = !!patch.flip_z;
		if ('rotation_deg' in patch) update.rotationDeg = patch.rotation_deg;
		if ('offset_m' in patch) update.offsetM = patch.offset_m;
		await run(() => updateConnector(connector_id, update));
		return toolOk(assemblyState());
	},

	async connector_delete({ connector_id }) {
		requireEditable();
		requireAssemblyTab();
		requireConnector(connector_id);
		await run(() => removeConnector(connector_id));
		return toolOk(assemblyState());
	},

	async mate_add({ a, b, kind = 'Fastened', flip = true, rotation_deg = 0, name }) {
		requireEditable();
		requireAssemblyTab();
		requireConnector(a);
		requireConnector(b);
		if (a === b) throw fail('InvalidArguments', 'A mate needs two different connectors.', { a, b });
		const id = await run(() => addMate({ a, b, kind, flip, rotationDeg: rotation_deg, name }));
		if (!id) throw fail('Internal', 'The store did not add the mate.', { a, b });
		return toolOk({ mate_id: id, ...assemblyState() });
	},

	async mate_edit({ mate_id, ...patch }) {
		requireEditable();
		requireAssemblyTab();
		requireMate(mate_id);
		/** @type {Record<string, unknown>} */
		const update = {};
		if ('name' in patch) update.name = patch.name;
		if ('kind' in patch) update.kind = patch.kind;
		if ('flip' in patch) update.flip = !!patch.flip;
		if ('rotation_deg' in patch) update.rotationDeg = patch.rotation_deg;
		if ('suppressed' in patch) update.suppressed = !!patch.suppressed;
		await run(() => updateMate(mate_id, update));
		return toolOk(assemblyState());
	},

	async mate_delete({ mate_id }) {
		requireEditable();
		requireAssemblyTab();
		requireMate(mate_id);
		await run(() => removeMate(mate_id));
		return toolOk(assemblyState());
	}
};
