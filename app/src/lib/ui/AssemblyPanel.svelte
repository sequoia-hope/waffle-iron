<script>
	/**
	 * Left panel for an open Assembly tab (v4 Phase 3c): instances of this
	 * document's Part tabs, mate connectors on instance faces or edges (or
	 * explicit frames) with their adjustments (anchor on the axis, flip,
	 * turn, offset — `specs/assembly_connector_adjustments.md`), mates, and
	 * the evaluation's problems. Edits go through the store, which
	 * re-evaluates the assembly in the engine (`OpenAssembly`) and autosaves
	 * the tab.
	 */
	import {
		getAssembly,
		getAssemblyStatus,
		getDocumentTabs,
		getPartParameters,
		addInstance,
		updateInstance,
		removeInstance,
		addConnector,
		updateConnector,
		removeConnector,
		CONNECTOR_ANCHORS,
		addMate,
		updateMate,
		removeMate,
		getSelectedInstanceId,
		getSelectedInstancePath,
		getSelectedRefs,
		getAssemblyConnectorFrames,
		getAssemblyPartConnectors,
		getConnectorRefusal,
		getSources,
		getSourceTabs,
		getActiveTabId,
		openPartInContext,
		isInstanceVisible,
		toggleInstanceVisibility,
		showAllInstances,
		hiddenInstanceCount,
		MATE_KINDS
	} from '$lib/engine/store.svelte.js';
	import { eulerDegToQuat, quatToEulerDeg } from '$lib/engine/rotation.js';

	let asm = $derived(getAssembly());
	let status = $derived(getAssemblyStatus());
	let partTabs = $derived(getDocumentTabs().filter((t) => t.kind?.type === 'Part'));
	let assemblyTabs = $derived(getDocumentTabs().filter((t) => t.kind?.type === 'Assembly' && t.id !== getActiveTabId()));
	/** Instance sources: this document's parts and assemblies, then linked documents' tabs. */
	let sourceOptions = $derived.by(() => {
		const opts = [];
		for (const t of partTabs) opts.push({ key: `tab:${t.id}`, tabId: t.id, sourceId: null, label: t.name, group: 'Parts' });
		for (const t of assemblyTabs) opts.push({ key: `tab:${t.id}`, tabId: t.id, sourceId: null, label: `${t.name} (assembly)`, group: 'Assemblies' });
		const tabsBySource = getSourceTabs();
		for (const s of getSources()) {
			for (const t of tabsBySource[s.id] ?? []) {
				if (t.kind !== 'Part' && t.kind !== 'Assembly') continue;
				opts.push({ key: `src:${s.id}:${t.id}`, tabId: t.id, sourceId: s.id, label: `${s.name} › ${t.name}${t.kind === 'Assembly' ? ' (assembly)' : ''}`, group: 'Linked' });
			}
		}
		return opts;
	});
	/**
	 * Instances whose position row is open. The transform numbers are six
	 * fields per instance — the panel is unreadable with them all showing, and
	 * they are an occasional edit, so each row reveals its own.
	 * @type {Set<string>}
	 */
	let positionsOpen = $state(new Set());
	function togglePositions(id) {
		const next = new Set(positionsOpen);
		if (!next.delete(id)) next.add(id);
		positionsOpen = next;
	}

	/**
	 * Instances whose parameter-override row is open (P2,
	 * `specs/agent_mechanical_design.md` §6). Same reason as `positionsOpen`:
	 * one field per parameter of the part, revealed per row.
	 * @type {Set<string>}
	 */
	let overridesOpen = $state(new Set());
	function toggleOverrides(id) {
		const next = new Set(overridesOpen);
		if (!next.delete(id)) next.add(id);
		overridesOpen = next;
	}

	/**
	 * The parameters of the PART an instance is of, so the row can offer one
	 * field per parameter rather than asking for a name.
	 *
	 * Read from the document's tabs: only the OPEN tab's tree is live, and an
	 * assembly tab is the open one here, so every part's table comes from the
	 * session's tab list. A linked source's part is not reachable from there,
	 * which is why such an instance shows the names it already overrides and
	 * nothing more.
	 */
	function partParameters(inst) {
		// From `ModelUpdated.document.part_parameters`, NOT from the tab list:
		// only the OPEN tab's tree is on the wire, and the open tab is the
		// assembly whenever this panel is showing, so every part tab's entry
		// in `documentTabs` is a placeholder with an empty table. A linked
		// source's part is not in the document's tabs at all.
		if (inst.source?.source_id) return null;
		return getPartParameters()[inst.source?.tab_id] ?? null;
	}

	/**
	 * The override rows to show for an instance: one per parameter of the
	 * part, or — when the part's table is not reachable — one per name the
	 * instance already overrides, so an existing override is never hidden by
	 * the panel's inability to list the part.
	 */
	function overrideRows(inst) {
		const declared = partParameters(inst);
		const overrides = inst.parameter_overrides ?? {};
		if (declared) {
			return declared.map((p) => ({
				name: p.name,
				fallback: p.value ?? 0,
				override: overrides[p.name]
			}));
		}
		return Object.keys(overrides)
			.sort()
			.map((name) => ({ name, fallback: null, override: overrides[name] }));
	}

	/**
	 * Set or clear one override. An empty field CLEARS it, so the instance
	 * goes back to the part's own value for that parameter — which is why the
	 * placeholder shows what the part says.
	 */
	function setOverride(inst, name, raw) {
		const text = String(raw ?? '').trim();
		const patch = { ...(inst.parameter_overrides ?? {}) };
		if (text === '') {
			delete patch[name];
		} else {
			const value = Number(text);
			if (!Number.isFinite(value)) return;
			patch[name] = value;
		}
		run(() => updateInstance(inst.id, { parameter_overrides: patch }));
	}

	/** How many parameters this instance pins, for the collapsed label. */
	function overrideCount(inst) {
		return Object.keys(inst.parameter_overrides ?? {}).length;
	}

	let selectedInstance = $derived(getSelectedInstanceId());
	let selectedPath = $derived(getSelectedInstancePath());
	/** A same-document Part instance can be edited in this assembly's context. */
	function editableInContext(inst) {
		return !inst.source?.source_id && partTabs.some((t) => t.id === inst.source?.tab_id);
	}
	/**
	 * The pick a connector would be placed on: a face or an edge of the
	 * clicked instance. An edge is what a Revolute mate usually wants (a
	 * hole's rim), and a cylindrical face is what it wants when the rim is
	 * hidden — the engine derives the axis from either.
	 */
	let selectedPick = $derived(
		getSelectedRefs().find((r) => r?.kind?.type === 'Face' || r?.kind?.type === 'Edge') ?? null
	);
	let pickLabel = $derived(selectedPick?.kind?.type === 'Edge' ? 'edge' : 'face');
	/** What each connector's frame was derived from, by connector id. */
	let derivedKinds = $derived.by(() => {
		const by = {};
		for (const f of getAssemblyConnectorFrames()) if (f.kind) by[f.id] = f.kind;
		return by;
	});
	let refusal = $derived(getConnectorRefusal());
	/**
	 * The named connectors of the instances' parts (`specs/part_mate_connectors.md`)
	 * that no assembly connector uses yet: each can be taken as a connector,
	 * or picked straight into a new mate.
	 */
	let unusedPartConnectors = $derived(
		getAssemblyPartConnectors().filter(
			(pc) =>
				!(asm?.connectors ?? []).some(
					(c) => c.part_connector === pc.feature_id && (c.instance_path ?? []).join() === pc.instance_path.join()
				)
		)
	);
	function partConnectorLabel(pc) {
		return `${pathLabel(pc.instance_path)} › ${pc.name}`;
	}
	/** A connector on a rotational face has an axial extent to anchor along. */
	function hasAxialExtent(kind) {
		return /cylindrical|conical|toroidal|axial/.test(kind ?? '');
	}
	const ANCHOR_LABELS = { middle: 'middle', positive_end: '+z end', negative_end: '−z end' };

	let newInstanceKey = $state('');
	let mateA = $state('');
	let mateB = $state('');
	let mateKind = $state('Fastened');
	let mateFlip = $state(true);
	let mateRotation = $state(0);
	let busy = $state(false);

	function partName(inst) {
		if (inst.source?.source_id) {
			const s = getSources().find((x) => x.id === inst.source.source_id);
			const t = (getSourceTabs()[inst.source.source_id] ?? []).find((x) => x.id === inst.source.tab_id);
			return `${s?.name ?? 'linked'} › ${t?.name ?? inst.source.tab_id}`;
		}
		const tab = getDocumentTabs().find((t) => t.id === inst.source?.tab_id);
		if (!tab) return inst.source?.tab_id ?? '?';
		return tab.kind?.type === 'Assembly' ? `${tab.name} (assembly)` : tab.name;
	}

	function pathLabel(path) {
		if (!path?.length) return '?';
		const top = instanceName(path[0]);
		return path.length > 1 ? `${top} › member` : top;
	}

	function instanceName(id) {
		return asm?.instances.find((i) => i.id === id)?.name ?? '?';
	}

	function connectorName(id) {
		return asm?.connectors?.find((c) => c.id === id)?.name ?? '?';
	}

	const MM = 1000;
	function mm(v) {
		return Math.round((v ?? 0) * MM * 1000) / 1000;
	}

	async function run(fn) {
		busy = true;
		try {
			await fn();
		} finally {
			busy = false;
		}
	}

	async function handleAddInstance() {
		const opt = sourceOptions.find((o) => o.key === newInstanceKey) ?? sourceOptions[0];
		if (!opt) return;
		await run(() => addInstance({ tabId: opt.tabId, sourceId: opt.sourceId }));
	}

	function eulerOf(inst) {
		return quatToEulerDeg(inst.transform?.rotation_quat ?? [0, 0, 0, 1]);
	}

	async function setRotation(inst, axis, valueDeg) {
		const t = JSON.parse(JSON.stringify(inst.transform ?? { translation_m: [0, 0, 0], rotation_quat: [0, 0, 0, 1] }));
		const v = Number(valueDeg);
		if (!Number.isFinite(v)) return;
		const e = eulerOf(inst);
		e[axis] = v;
		t.rotation_quat = eulerDegToQuat(e);
		await run(() => updateInstance(inst.id, { transform: t }));
	}

	async function setTranslation(inst, axis, valueMm) {
		const t = JSON.parse(JSON.stringify(inst.transform ?? { translation_m: [0, 0, 0], rotation_quat: [0, 0, 0, 1] }));
		const v = Number(valueMm);
		if (!Number.isFinite(v)) return;
		t.translation_m[axis] = v / MM;
		await run(() => updateInstance(inst.id, { transform: t }));
	}

	async function handleAddConnectorFromPick() {
		if (!selectedPath?.length || !selectedPick) return;
		await run(() => addConnector({ instancePath: selectedPath, geomRef: selectedPick }));
	}

	async function handleAddOriginConnector(inst) {
		await run(() => addConnector({ instanceId: inst.id, name: `${inst.name} origin` }));
	}

	async function setConnectorOffset(c, axis, valueMm) {
		const v = Number(valueMm);
		if (!Number.isFinite(v)) return;
		const offsetMm = [0, 1, 2].map((k) => mm(c.offset_m?.[k]));
		offsetMm[axis] = v;
		await run(() => updateConnector(c.id, { offsetMm }));
	}

	async function handleUsePartConnector(pc) {
		await run(() => addConnector({ instancePath: pc.instance_path, partConnector: pc.feature_id }));
	}

	/** A mate pick: an assembly connector id, or `pc:<i>` for an unused part connector. */
	function pickedPartConnector(value) {
		return value.startsWith('pc:') ? unusedPartConnectors[Number(value.slice(3))] ?? null : null;
	}

	async function handleAddMate() {
		if (!mateA || !mateB || mateA === mateB) return;
		// Resolve both picks before creating anything: taking the first part
		// connector changes which ones are still unused.
		const picks = [mateA, mateB].map((v) => ({ id: v, pc: pickedPartConnector(v) }));
		await run(async () => {
			const ids = [];
			for (const p of picks) {
				ids.push(p.pc ? await addConnector({ instancePath: p.pc.instance_path, partConnector: p.pc.feature_id }) : p.id);
			}
			if (!ids[0] || !ids[1]) return;
			await addMate({ a: ids[0], b: ids[1], kind: mateKind, flip: mateFlip, rotationDeg: Number(mateRotation) || 0 });
		});
		mateA = '';
		mateB = '';
	}
</script>

{#if asm}
	<div class="assembly-panel" data-testid="assembly-panel">
		<div class="section">
			<div class="section-header">
				Instances ({asm.instances.length})
				{#if hiddenInstanceCount() > 0}
					<button class="act" data-testid="asm-show-all-instances" title="Show every instance hidden from the viewport" onclick={() => showAllInstances()}>show all ({hiddenInstanceCount()})</button>
				{/if}
			</div>
			{#each asm.instances as inst, i (inst.id)}
				<div class="row instance" class:selected={selectedInstance === inst.id} class:hidden-item={!isInstanceVisible(inst.id)} data-testid="asm-instance-{i}">
					<div class="row-main">
						<input
							class="name"
							value={inst.name}
							data-testid="asm-instance-name-{i}"
							onchange={(e) => run(() => updateInstance(inst.id, { name: e.currentTarget.value }))}
						/>
						<span class="meta" data-testid="asm-instance-part-{i}">{partName(inst)}</span>
						<button
							class="act visibility-toggle"
							title={isInstanceVisible(inst.id) ? 'Hide from the viewport (display only)' : 'Show in the viewport'}
							data-testid="asm-instance-visibility-{i}"
							onclick={() => toggleInstanceVisibility(inst.id)}
						>
							{isInstanceVisible(inst.id) ? '◉' : '◎'}
						</button>
						{#if editableInContext(inst)}
							<button class="act" title="Edit this part in the context of the assembly (the other instances show as ghosts)" data-testid="asm-instance-edit-context-{i}" disabled={busy} onclick={() => run(() => openPartInContext([inst.id]))}>edit</button>
						{/if}
						<button class="act" title="Remove instance" data-testid="asm-instance-remove-{i}" disabled={busy} onclick={() => run(() => removeInstance(inst.id))}>×</button>
					</div>
					<div class="row-sub">
						<label title="Grounded: never moved by mates"><input type="checkbox" data-testid="asm-instance-fixed-{i}" checked={!!inst.fixed} disabled={busy} onchange={(e) => run(() => updateInstance(inst.id, { fixed: e.currentTarget.checked }))} /> fixed</label>
						<label title="Suppressed: out of the model — no mate solve, saved with the document"><input type="checkbox" data-testid="asm-instance-suppressed-{i}" checked={!!inst.suppressed} disabled={busy} onchange={(e) => run(() => updateInstance(inst.id, { suppressed: e.currentTarget.checked }))} /> suppress</label>
						<button
							class="act"
							title={positionsOpen.has(inst.id) ? 'Hide position' : 'Show position and rotation'}
							data-testid="asm-instance-position-toggle-{i}"
							onclick={() => togglePositions(inst.id)}
						>
							{positionsOpen.has(inst.id) ? '▾' : '▸'} pos
						</button>
						<button
							class="act"
							title={overridesOpen.has(inst.id)
								? 'Hide the design-parameter overrides'
								: "Override this instance's design parameters, so it builds its own size from the same part"}
							data-testid="asm-instance-overrides-toggle-{i}"
							onclick={() => toggleOverrides(inst.id)}
						>
							{overridesOpen.has(inst.id) ? '▾' : '▸'} vars{overrideCount(inst) > 0 ? ` (${overrideCount(inst)})` : ''}
						</button>
						<button class="act" title="Connector at this instance's origin" data-testid="asm-instance-origin-connector-{i}" disabled={busy} onclick={() => handleAddOriginConnector(inst)}>+ frame</button>
					</div>
					{#if overridesOpen.has(inst.id)}
						<div class="row-sub overrides" data-testid="asm-instance-overrides-{i}">
							{#if overrideRows(inst).length === 0}
								<span class="meta">
									{partParameters(inst)
										? 'the part declares no variables'
										: "a linked part's variables are not listed here"}
								</span>
							{:else}
								{#each overrideRows(inst) as row (row.name)}
									<label class="override" title="Blank = use the part's own value ({row.fallback ?? '?'}). Working space: mm for a length, degrees for an angle.">
										<span class="override-name">{row.name}</span>
										<input
											class="num"
											type="number"
											step="any"
											placeholder={row.fallback === null ? '' : String(row.fallback)}
											data-testid="asm-instance-override-{row.name}-{i}"
											value={row.override ?? ''}
											disabled={busy}
											onchange={(e) => setOverride(inst, row.name, e.currentTarget.value)}
										/>
									</label>
								{/each}
							{/if}
						</div>
					{/if}
					{#if positionsOpen.has(inst.id)}
						<div class="row-sub" data-testid="asm-instance-position-{i}">
							<span class="xyz">
								{#each ['x', 'y', 'z'] as axis, k}
									<input class="num" type="number" step="0.1" title="{axis} (mm)" data-testid="asm-instance-t{axis}-{i}" value={mm(inst.transform?.translation_m?.[k])} disabled={busy} onchange={(e) => setTranslation(inst, k, e.currentTarget.value)} />
								{/each}
								<span class="unit">mm</span>
							</span>
							<span class="xyz" title="rotation, XYZ Euler (°)">
								{#each ['x', 'y', 'z'] as axis, k}
									<input class="num" type="number" step="5" title="rotate about {axis} (°)" data-testid="asm-instance-r{axis}-{i}" value={eulerOf(inst)[k]} disabled={busy} onchange={(e) => setRotation(inst, k, e.currentTarget.value)} />
								{/each}
								<span class="unit">°</span>
							</span>
						</div>
					{/if}
				</div>
			{/each}
			{#if selectedPath && selectedPath.length > 1}
				<div class="row">
					<span class="meta">selected: {pathLabel(selectedPath)}</span>
					<button class="act" title="Edit the selected member's part in the context of this assembly" data-testid="asm-edit-selected-context" disabled={busy} onclick={() => run(() => openPartInContext(selectedPath))}>edit in context</button>
				</div>
			{/if}
			<div class="row add">
				<select data-testid="asm-add-instance-part" bind:value={newInstanceKey} disabled={sourceOptions.length === 0}>
					{#each ['Parts', 'Assemblies', 'Linked'] as group}
						{#if sourceOptions.some((o) => o.group === group)}
							<optgroup label={group}>
								{#each sourceOptions.filter((o) => o.group === group) as o}
									<option value={o.key}>{o.label}</option>
								{/each}
							</optgroup>
						{/if}
					{/each}
				</select>
				<button class="act primary" data-testid="asm-add-instance" disabled={busy || sourceOptions.length === 0} onclick={handleAddInstance}>+ instance</button>
			</div>
		</div>

		<div class="section">
			<div class="section-header">Mate connectors ({asm.connectors?.length ?? 0})</div>
			{#each asm.connectors ?? [] as c, i (c.id)}
				<div class="row connector" data-testid="asm-connector-{i}">
					<div class="row-main">
						<input
							class="name"
							value={c.name}
							data-testid="asm-connector-name-{i}"
							onchange={(e) => run(() => updateConnector(c.id, { name: e.currentTarget.value }))}
						/>
						<span class="meta" data-testid="asm-connector-kind-{i}"
							>{pathLabel(c.instance_path)} · {derivedKinds[c.id] ??
								(c.geom_ref ? 'unresolved' : 'explicit frame')}</span
						>
						<button class="act" title="Remove connector" data-testid="asm-connector-remove-{i}" disabled={busy} onclick={() => run(() => removeConnector(c.id))}>×</button>
					</div>
					<div class="row-sub">
						{#if !c.part_connector && hasAxialExtent(derivedKinds[c.id])}
							<select
								data-testid="asm-connector-anchor-{i}"
								title="Where on the axis the frame sits: the middle of the face, or the end its z axis points toward (+z) or away from (−z)"
								value={c.anchor ?? 'middle'}
								disabled={busy}
								onchange={(e) => run(() => updateConnector(c.id, { anchor: e.currentTarget.value }))}
							>
								{#each CONNECTOR_ANCHORS as a}<option value={a}>{ANCHOR_LABELS[a]}</option>{/each}
							</select>
						{/if}
						<label title="Reverse the z axis (the triad's blue arrow)"><input type="checkbox" data-testid="asm-connector-flip-{i}" checked={!!c.flip_z} disabled={busy} onchange={(e) => run(() => updateConnector(c.id, { flipZ: e.currentTarget.checked }))} /> flip z</label>
						<label title="Turn about z (°) — moves the x axis, a Fastened mate's in-plane alignment">turn <input class="num" type="number" step="15" data-testid="asm-connector-rotation-{i}" value={c.rotation_deg ?? 0} disabled={busy} onchange={(e) => run(() => updateConnector(c.id, { rotationDeg: e.currentTarget.value }))} />°</label>
						<span class="xyz" title="Offset along the connector's own x, y, z (mm)">
							{#each ['x', 'y', 'z'] as axis, k}
								<input class="num" type="number" step="0.5" title="offset along the connector's {axis} (mm)" data-testid="asm-connector-o{axis}-{i}" value={mm(c.offset_m?.[k])} disabled={busy} onchange={(e) => setConnectorOffset(c, k, e.currentTarget.value)} />
							{/each}
							<span class="unit">mm</span>
						</span>
					</div>
				</div>
			{/each}
			{#each unusedPartConnectors as pc, i (pc.instance_path.join() + pc.feature_id)}
				<div class="row part-connector" data-testid="asm-part-connector-{i}">
					<div class="row-main">
						<span class="name-static" title="A named mate connector of the part">{partConnectorLabel(pc)}</span>
						<span class="meta">part · {pc.kind ?? 'explicit frame'}</span>
						<button class="act" title="Use this part connector in the assembly" data-testid="asm-part-connector-use-{i}" disabled={busy} onclick={() => handleUsePartConnector(pc)}>use</button>
					</div>
				</div>
			{/each}
			<div class="row add">
				<button
					class="act primary"
					data-testid="asm-add-connector-face"
					title={selectedInstance && selectedPick
						? `Connector on the selected ${pickLabel} (a planar face, a cylindrical/conical/spherical face, or a circular or straight edge)`
						: 'Select a face or an edge of an instance in the viewport first'}
					disabled={busy || !selectedInstance || !selectedPick}
					onclick={handleAddConnectorFromPick}
				>+ connector on selected {selectedPick ? pickLabel : 'face/edge'}</button>
			</div>
			{#if refusal}
				<div class="row"><span class="err" data-testid="asm-connector-refusal">{refusal}</span></div>
			{/if}
		</div>

		<div class="section">
			<div class="section-header">Mates ({asm.mates?.length ?? 0})</div>
			{#each asm.mates ?? [] as m, i (m.id)}
				<div class="row mate" data-testid="asm-mate-{i}">
					<div class="row-main">
						<span class="name-static">{m.name}</span>
						<span class="meta">{m.kind?.type ?? '?'} · {connectorName(m.connectors?.[0])} → {connectorName(m.connectors?.[1])}</span>
						<button class="act" title="Remove mate" data-testid="asm-mate-remove-{i}" disabled={busy} onclick={() => run(() => removeMate(m.id))}>×</button>
					</div>
					{#if MATE_KINDS.includes(m.kind?.type)}
						<div class="row-sub">
							<select data-testid="asm-mate-kind-{i}" value={m.kind.type} disabled={busy} onchange={(e) => run(() => updateMate(m.id, { kind: e.currentTarget.value }))}>
								{#each MATE_KINDS as k}<option value={k}>{k}</option>{/each}
							</select>
							{#if m.kind.type !== 'Ball'}
								<label><input type="checkbox" data-testid="asm-mate-flip-{i}" checked={!!m.kind.flip} disabled={busy} onchange={(e) => run(() => updateMate(m.id, { flip: e.currentTarget.checked }))} /> flip</label>
							{/if}
							{#if m.kind.type === 'Fastened'}
								<label>rotate <input class="num" type="number" step="15" data-testid="asm-mate-rotation-{i}" value={m.kind.rotation_deg ?? 0} disabled={busy} onchange={(e) => run(() => updateMate(m.id, { rotationDeg: e.currentTarget.value }))} />°</label>
							{/if}
							<label><input type="checkbox" data-testid="asm-mate-suppressed-{i}" checked={!!m.suppressed} disabled={busy} onchange={(e) => run(() => updateMate(m.id, { suppressed: e.currentTarget.checked }))} /> off</label>
						</div>
					{/if}
				</div>
			{/each}
			{#if (asm.connectors?.length ?? 0) + unusedPartConnectors.length >= 2}
				<div class="row add mate-add">
					{#each [['asm-mate-a', 'connector A'], ['asm-mate-b', 'connector B']] as [testid, placeholder], side}
						<select
							data-testid={testid}
							value={side === 0 ? mateA : mateB}
							onchange={(e) => (side === 0 ? (mateA = e.currentTarget.value) : (mateB = e.currentTarget.value))}
						>
							<option value="">{placeholder}</option>
							{#each asm.connectors ?? [] as c}<option value={c.id}>{c.name}</option>{/each}
							{#if unusedPartConnectors.length}
								<optgroup label="Part connectors">
									{#each unusedPartConnectors as pc, i}<option value="pc:{i}">{partConnectorLabel(pc)}</option>{/each}
								</optgroup>
							{/if}
						</select>
					{/each}
					<select data-testid="asm-mate-new-kind" bind:value={mateKind} title="Mate kind">
						{#each MATE_KINDS as k}<option value={k}>{k}</option>{/each}
					</select>
					<label><input type="checkbox" data-testid="asm-mate-new-flip" bind:checked={mateFlip} /> flip</label>
					<input class="num" type="number" step="15" data-testid="asm-mate-new-rotation" bind:value={mateRotation} title="rotation about z (°, Fastened)" />
					<button class="act primary" data-testid="asm-add-mate" disabled={busy || !mateA || !mateB || mateA === mateB} onclick={handleAddMate}>mate</button>
				</div>
			{/if}
		</div>

		{#if status?.errors?.length || status?.warnings?.length}
			<div class="section status">
				{#each status.errors ?? [] as e}
					<div class="err" data-testid="asm-error">{e}</div>
				{/each}
				{#each status.warnings ?? [] as w}
					<div class="warn" data-testid="asm-warning">{w}</div>
				{/each}
			</div>
		{/if}
	</div>
{/if}

<style>
	.assembly-panel {
		font-size: 12px;
		color: var(--text-primary, #cdd6f4);
	}
	.section {
		border-bottom: 1px solid var(--border-color, #444);
		padding: 4px 0;
	}
	.section-header {
		padding: 4px 12px;
		font-weight: 600;
		color: var(--text-secondary, #a6adc8);
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}
	.row {
		padding: 3px 12px;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.row.selected {
		background: rgba(0, 120, 212, 0.15);
	}
	.row-main,
	.row-sub,
	.row.add {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}
	.row-sub {
		color: var(--text-secondary, #a6adc8);
		font-size: 11px;
	}
	.name {
		flex: 1 1 60px;
		min-width: 0;
		background: transparent;
		border: 1px solid transparent;
		color: inherit;
		padding: 1px 4px;
		border-radius: 3px;
	}
	.name:hover,
	.name:focus {
		border-color: var(--border-color, #45475a);
	}
	.name-static {
		flex: 1 1 60px;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		color: var(--text-secondary, #a6adc8);
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.xyz {
		display: inline-flex;
		gap: 2px;
		align-items: center;
	}
	.num {
		width: 52px;
		background: var(--bg-primary, #1e1e2e);
		border: 1px solid var(--border-color, #45475a);
		color: inherit;
		border-radius: 3px;
		padding: 1px 3px;
		font-size: 11px;
	}
	.unit {
		font-size: 10px;
	}
	/* The per-instance override row: wraps rather than overflowing, because
	   a part with eight variables must not push the panel past the window
	   (`tests/gui/layout-overflow.spec.js` is the oracle for that). */
	.overrides {
		flex-wrap: wrap;
		gap: 4px 8px;
	}
	.override {
		display: inline-flex;
		gap: 3px;
		align-items: center;
	}
	.override-name {
		font-size: 10px;
		color: var(--text-secondary, #999);
		max-width: 80px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.act {
		padding: 0 6px;
		border-radius: 3px;
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: var(--accent, #89b4fa);
		font-size: 11px;
		cursor: pointer;
	}
	.act.primary {
		border-color: var(--accent, #89b4fa);
	}
	/* Hidden from the viewport — the row is still fully editable. */
	.row.instance.hidden-item .name,
	.row.instance.hidden-item .meta,
	.row.instance.hidden-item .row-sub {
		opacity: 0.4;
	}
	/* The eye reads as state, not as a button: no chrome around it. */
	.visibility-toggle {
		border-color: transparent;
		padding: 0 2px;
		font-size: 12px;
	}
	.act:disabled {
		opacity: 0.5;
		cursor: default;
	}
	select {
		background: var(--bg-primary, #1e1e2e);
		color: inherit;
		border: 1px solid var(--border-color, #45475a);
		border-radius: 3px;
		font-size: 11px;
		max-width: 120px;
	}
	.status {
		padding: 4px 12px;
	}
	.err {
		color: var(--error, #f38ba8);
	}
	.warn {
		color: var(--warning, #f9e2af);
	}
</style>
