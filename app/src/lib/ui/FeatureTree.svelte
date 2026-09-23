<script>
	import { AXIS_COLORS } from '$lib/config.js';
	import {
		getFeatureTree,
		getSelectedFeatureId,
		selectFeature,
		deleteFeature,
		suppressFeature,
		setRollbackIndex,
		reorderFeature,
		renameFeature,
		send,
		isEngineReady,
		selectRef,
		getSelectedRefs,
		geomRefEquals,
		isSketchVisible,
		toggleSketchVisibility,
		showAllSketches,
		hideAllSketches,
		isPlaneVisible,
		togglePlaneVisibility,
		showAllPlanes,
		hideAllPlanes,
		isAxisVisible,
		toggleAxisVisibility,
		showAllAxes,
		hideAllAxes,
		enterSketchEditMode,
		getFeatureErrors,
		getSelectedRefFeatureId,
		showEditFeatureDialog,
		getBodies,
		getSelectedBodyId,
		selectBody,
		setHoveredBodyId,
		renameBody,
		exportBodyStl,
		isBodyVisible,
		toggleBodyVisibility,
		getParameters,
		setParameters,
		getSources,
		setSourcePack,
		packAllSources,
		pinSource,
		updateSourceToTip,
		fetchSource,
		showScriptEditor,
		getAgentActivity,
		setToolHint,
		AGENT_WORKING_HINT
	} from '$lib/engine/store.svelte.js';
	import { BUILTIN_PLANES, makePlaneRef } from '$lib/engine/planes.js';
	import { describeLocator } from '$lib/storage/git/locator.js';
	import { longPressContextMenu } from './longPressContextMenu.js';

	let tree = $derived(getFeatureTree());
	let selectedId = $derived(getSelectedFeatureId());
	// Face→feature (Tier 1): the feature whose geometry is currently picked.
	let faceFeatureId = $derived(getSelectedRefFeatureId());
	let featureErrors = $derived(getFeatureErrors());
	let bodies = $derived(getBodies());
	let selectedBodyId = $derived(getSelectedBodyId());
	// An agent-link call is running: tree edits are refused, not queued (spec G8).
	let agentBusy = $derived(getAgentActivity() !== null);

	/** True (and shows the status hint) when an agent call blocks a tree edit. */
	function blockedByAgent() {
		if (!agentBusy) return false;
		setToolHint(AGENT_WORKING_HINT);
		return true;
	}

	/** @type {{ x: number, y: number, featureId: string, featureName: string, suppressed: boolean, isSketch: boolean, operationType: string | null } | null} */
	let contextMenu = $state(null);

	/** @type {{ x: number, y: number, kind: 'plane' | 'axis', id: string, visible: boolean } | null} */
	let originContextMenu = $state(null);

	// Built-in axis definitions for the Origin section
	const ORIGIN_AXES = [
		{ id: 'x', name: 'X Axis', color: AXIS_COLORS.x },
		{ id: 'y', name: 'Y Axis', color: AXIS_COLORS.y },
		{ id: 'z', name: 'Z Axis', color: AXIS_COLORS.z },
	];

	/** @type {{ featureId: string, value: string } | null} */
	let renaming = $state(null);

	// Drag-and-drop state
	/** @type {string | null} */
	let dragFeatureId = $state(null);
	/** @type {number | null} */
	let dropTargetIndex = $state(null);

	// Origin section state
	let originExpanded = $state(true);

	// Variables (design parameters) section state
	let variablesExpanded = $state(true);
	let parameters = $derived(getParameters());
	/** Inline edit state: null | { id: string|null, name: string, expression: string }.
	 *  id === null means a new row being created. */
	let editingVariable = $state(/** @type {any} */ (null));

	function startAddVariable(e) {
		e.stopPropagation();
		variablesExpanded = true;
		// Suggest the first free varN name.
		let n = 1;
		const names = new Set(parameters.map((p) => p.name));
		while (names.has(`var${n}`)) n++;
		editingVariable = { id: null, name: `var${n}`, expression: '10' };
	}

	function startEditVariable(param) {
		editingVariable = { id: param.id, name: param.name, expression: param.expression };
	}

	async function commitVariableEdit() {
		const edit = editingVariable;
		if (!edit) return;
		if (blockedByAgent()) {
			editingVariable = null;
			return;
		}
		editingVariable = null;
		const name = edit.name.trim();
		const expression = edit.expression.trim();
		if (!name || !expression) return;
		const list = parameters.map((p) => ({ ...p }));
		if (edit.id === null) {
			list.push({ name, expression });
		} else {
			const row = list.find((p) => p.id === edit.id);
			if (!row) return;
			row.name = name;
			row.expression = expression;
		}
		await setParameters(list);
	}

	function cancelVariableEdit() {
		editingVariable = null;
	}

	async function deleteVariable(e, param) {
		e.stopPropagation();
		if (blockedByAgent()) return;
		await setParameters(parameters.filter((p) => p.id !== param.id).map((p) => ({ ...p })));
	}

	function handleVariableKeydown(e) {
		e.stopPropagation();
		if (e.key === 'Enter') commitVariableEdit();
		else if (e.key === 'Escape') cancelVariableEdit();
	}

	/** Compact display of an evaluated value (mm-space number). */
	function formatVariableValue(param) {
		if (param.error) return '!';
		const v = param.value ?? 0;
		const rounded = Math.abs(v - Math.round(v)) < 1e-9 ? Math.round(v) : parseFloat(v.toFixed(4));
		return `${rounded}`;
	}

	// Bodies section state
	let bodiesExpanded = $state(true);

	// Sources section (v4 §2.3): the document's linked/embedded content.
	let sources = $derived(getSources());
	let sourcesExpanded = $state(true);
	let sourceBusy = $state(null);

	/** Short status: where the content comes from and at which commit. */
	function sourceStatus(s) {
		const loc = s.locator ?? {};
		let where;
		switch (loc.type) {
			case 'Git': {
				const r = loc.ref ?? {};
				const at = s.resolved?.commit ? s.resolved.commit.slice(0, 7) : '?';
				where = r.type === 'Commit' ? `pinned ${r.sha.slice(0, 7)}` : `${r.type === 'Tag' ? 'tag ' : ''}${r.name} @ ${at}`;
				break;
			}
			case 'Relative': where = `./${loc.path}`; break;
			case 'Url': where = 'url'; break;
			case 'Embedded': where = 'embedded'; break;
			case 'Local': where = 'this browser'; break;
			default: where = `${loc.type ?? '?'} (unsupported)`;
		}
		return s.available ? where : `missing · ${where}`;
	}

	function isFloatingGit(s) {
		return s.locator?.type === 'Git' && s.locator.ref?.type !== 'Commit';
	}

	async function withBusy(id, fn) {
		if (blockedByAgent()) return;
		sourceBusy = id;
		try {
			await fn();
		} finally {
			sourceBusy = null;
		}
	}

	/** Inline-rename state for the Bodies list. Keyed by bodyId so only the
	 * edited row shows an input even when one feature owns several bodies. Body
	 * rename is independent of feature rename (sends RenameBody). */
	/** @type {{ bodyId: string, value: string } | null} */
	let bodyRenaming = $state(null);

	/** @type {{ x: number, y: number, bodyId: string, name: string } | null} */
	let bodyContextMenu = $state(null);

	function handleBodyClick(bodyId) {
		selectBody(selectedBodyId === bodyId ? null : bodyId);
	}

	function handleBodyVisibilityToggle(e, bodyId) {
		e.stopPropagation();
		toggleBodyVisibility(bodyId);
	}

	function handleBodyContextMenu(e, body) {
		e.preventDefault();
		contextMenu = null;
		originContextMenu = null;
		const pos = clampMenuPosition(e.clientX, e.clientY);
		bodyContextMenu = { x: pos.x, y: pos.y, bodyId: body.bodyId, name: body.name };
	}

	function handleBodyExport() {
		if (bodyContextMenu) {
			exportBodyStl(bodyContextMenu.bodyId, bodyContextMenu.name);
			bodyContextMenu = null;
		}
	}

	function handleBodyDblClick(body) {
		if (blockedByAgent()) return;
		bodyRenaming = { bodyId: body.bodyId, value: body.name };
	}

	function commitBodyRename() {
		if (!bodyRenaming) return;
		if (blockedByAgent()) {
			bodyRenaming = null;
			return;
		}
		// Empty/whitespace clears the override (engine reverts to derived name).
		renameBody(bodyRenaming.bodyId, bodyRenaming.value.trim());
		bodyRenaming = null;
	}

	function handleBodyRename(e) {
		if (!bodyRenaming) return;
		if (e.key === 'Enter') {
			commitBodyRename();
		} else if (e.key === 'Escape') {
			bodyRenaming = null;
		}
	}

	function handleBodyRenameBlur() {
		commitBodyRename();
	}

	// Build plane refs once
	const planeRefs = BUILTIN_PLANES.map((p) => makePlaneRef(p.id));

	function isPlaneSelected(index) {
		return getSelectedRefs().some((r) => geomRefEquals(r, planeRefs[index]));
	}

	function handlePlaneClick(index) {
		selectRef(planeRefs[index]);
	}

	function handleClick(featureId) {
		selectFeature(featureId);
	}

	function handleDblClick(feature) {
		if (blockedByAgent()) return;
		const opType = feature.operation?.type;
		if (opType === 'Sketch') {
			enterSketchEditMode(feature.id);
		} else if (opType === 'Extrude' || opType === 'Revolve' || opType === 'Pipe' || opType === 'MateConnector' || opType === 'Script') {
			showEditFeatureDialog(feature.id);
		} else {
			renaming = { featureId: feature.id, value: feature.name };
		}
	}

	function clampMenuPosition(x, y, menuWidth = 160, menuHeight = 200) {
		const maxX = window.innerWidth - menuWidth - 8;
		const maxY = window.innerHeight - menuHeight - 8;
		return {
			x: Math.min(x, Math.max(0, maxX)),
			y: Math.min(y, Math.max(0, maxY))
		};
	}

	function handleContextMenu(e, feature) {
		e.preventDefault();
		if (blockedByAgent()) return;
		originContextMenu = null;
		const pos = clampMenuPosition(e.clientX, e.clientY);
		contextMenu = {
			x: pos.x,
			y: pos.y,
			featureId: feature.id,
			featureName: feature.name,
			suppressed: feature.suppressed,
			isSketch: feature.operation?.type === 'Sketch',
			operationType: feature.operation?.type ?? null
		};
	}

	function closeContextMenu() {
		contextMenu = null;
		originContextMenu = null;
		bodyContextMenu = null;
	}

	function handleRename(e) {
		if (!renaming) return;
		if (e.key === 'Enter') {
			if (blockedByAgent()) {
				renaming = null;
				return;
			}
			const trimmed = renaming.value.trim();
			if (trimmed) {
				renameFeature(renaming.featureId, trimmed);
			}
			renaming = null;
		} else if (e.key === 'Escape') {
			renaming = null;
		}
	}

	function handleRenameBlur() {
		if (!renaming) return;
		if (blockedByAgent()) {
			renaming = null;
			return;
		}
		const trimmed = renaming.value.trim();
		if (trimmed) {
			renameFeature(renaming.featureId, trimmed);
		}
		renaming = null;
	}

	function handleKeyDown(e) {
		if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
		if (renaming) return;
		if ((e.key === 'Delete' || e.key === 'Backspace') && selectedId) {
			if (blockedByAgent()) return;
			deleteFeature(selectedId);
			selectFeature(null);
		}
	}

	function handleDelete() {
		if (blockedByAgent()) return closeContextMenu();
		if (contextMenu) {
			deleteFeature(contextMenu.featureId);
			if (selectedId === contextMenu.featureId) selectFeature(null);
			closeContextMenu();
		}
	}

	function handleSuppress() {
		if (blockedByAgent()) return closeContextMenu();
		if (contextMenu) {
			suppressFeature(contextMenu.featureId, !contextMenu.suppressed);
			closeContextMenu();
		}
	}

	function handleEditSketch() {
		if (blockedByAgent()) return closeContextMenu();
		if (contextMenu && contextMenu.isSketch) {
			enterSketchEditMode(contextMenu.featureId);
			closeContextMenu();
		}
	}

	function handleEditFeature() {
		if (blockedByAgent()) return closeContextMenu();
		if (contextMenu) {
			showEditFeatureDialog(contextMenu.featureId);
			closeContextMenu();
		}
	}

	function handleRenameFromMenu() {
		if (contextMenu) {
			renaming = { featureId: contextMenu.featureId, value: contextMenu.featureName };
			closeContextMenu();
		}
	}

	function handleVisibilityToggle(e, featureId) {
		e.stopPropagation();
		toggleSketchVisibility(featureId);
	}

	function handlePlaneVisibilityToggle(e, planeId) {
		e.stopPropagation();
		togglePlaneVisibility(planeId);
	}

	function handleAxisVisibilityToggle(e, axisId) {
		e.stopPropagation();
		toggleAxisVisibility(axisId);
	}

	function handleOriginContextMenu(e, kind, id, visible) {
		e.preventDefault();
		e.stopPropagation();
		contextMenu = null;
		const pos = clampMenuPosition(e.clientX, e.clientY);
		originContextMenu = { x: pos.x, y: pos.y, kind, id, visible };
	}

	function handleShowAllPlanes() {
		showAllPlanes(BUILTIN_PLANES);
		closeContextMenu();
	}

	function handleHideAllPlanes() {
		hideAllPlanes(BUILTIN_PLANES);
		closeContextMenu();
	}

	function handleShowAllAxes() {
		showAllAxes();
		closeContextMenu();
	}

	function handleHideAllAxes() {
		hideAllAxes();
		closeContextMenu();
	}

	function handleShowAllSketches() {
		showAllSketches(tree.features);
		closeContextMenu();
	}

	function handleHideAllSketches() {
		hideAllSketches(tree.features);
		closeContextMenu();
	}

	function featureIcon(opType) {
		switch (opType) {
			case 'Sketch': return '\u270E';
			case 'Extrude': return '\u25A7';
			case 'Revolve': return '\u21BB';
			case 'Pipe': return '\u2312';
			case 'Fillet': return '\u25CF';
			case 'Chamfer': return '\u25C6';
			case 'Shell': return '\u25A1';
			case 'BooleanCombine': return '\u2229';
			case 'UnionAll': return '\u222A';
			case 'ImportedBody': return '\u2913';
			case 'MateConnector': return '\u2295';
			case 'PatternCircular': return '\u25CC';
			case 'PatternLinear': return '\u2237';
			case 'Script': return '\u2328';
			default: return '\u2022';
		}
	}

	// -- Drag and drop --

	function handleDragStart(e, feature) {
		if (blockedByAgent()) {
			e.preventDefault();
			return;
		}
		dragFeatureId = feature.id;
		e.dataTransfer.effectAllowed = 'move';
		e.dataTransfer.setData('text/plain', feature.id);
	}

	function handleDragOver(e, index) {
		e.preventDefault();
		e.dataTransfer.dropEffect = 'move';
		dropTargetIndex = index;
	}

	function handleDragLeave() {
		dropTargetIndex = null;
	}

	function handleDrop(e, targetIndex) {
		e.preventDefault();
		if (dragFeatureId && !blockedByAgent()) {
			reorderFeature(dragFeatureId, targetIndex);
		}
		dragFeatureId = null;
		dropTargetIndex = null;
	}

	function handleDragEnd() {
		dragFeatureId = null;
		dropTargetIndex = null;
	}

	// Rollback slider
	let rollbackValue = $derived(tree.active_index ?? tree.features.length);

	function handleRollback(e) {
		if (blockedByAgent()) {
			e.target.value = String(rollbackValue);
			return;
		}
		const val = parseInt(e.target.value);
		const index = val >= tree.features.length ? null : val;
		setRollbackIndex(index);
	}
</script>

<svelte:window onclick={closeContextMenu} onkeydown={handleKeyDown} />

<div class="feature-tree">
	<div class="panel-header">Features</div>
	<div class="tree-content" use:longPressContextMenu>
		<!-- Variables (design parameters) section -->
		<div class="origin-section" data-testid="variables-section">
			<div class="origin-header variables-header">
				<button
					class="origin-header variables-toggle"
					onclick={() => variablesExpanded = !variablesExpanded}
					data-testid="variables-toggle"
				>
					<span class="expand-icon">{variablesExpanded ? '▾' : '▸'}</span>
					<span class="origin-label">Variables</span>
				</button>
				<button
					class="variable-add"
					title="Add variable (lengths in mm, angles in degrees; expressions may reference other variables, e.g. width / 2)"
					onclick={startAddVariable}
					data-testid="variable-add"
				>+</button>
			</div>
			{#if variablesExpanded}
				{#each parameters as param (param.id)}
					{#if editingVariable && editingVariable.id === param.id}
						<div class="variable-row variable-editing" data-testid="variable-edit-row">
							<!-- svelte-ignore a11y_autofocus -->
							<input
								class="variable-input variable-name-input"
								bind:value={editingVariable.name}
								onkeydown={handleVariableKeydown}
								data-testid="variable-name-input"
								autofocus
							/>
							<span class="variable-eq">=</span>
							<input
								class="variable-input variable-expr-input"
								bind:value={editingVariable.expression}
								onkeydown={handleVariableKeydown}
								onblur={commitVariableEdit}
								data-testid="variable-expr-input"
							/>
						</div>
					{:else}
						<div
							class="variable-row"
							class:variable-error={!!param.error}
							role="treeitem"
							tabindex="0"
							title={param.error ? param.error : `${param.name} = ${param.expression} → ${formatVariableValue(param)}`}
							onclick={() => startEditVariable(param)}
							onkeydown={(e) => { if (e.key === 'Enter') startEditVariable(param); }}
							data-testid="variable-row-{param.name}"
						>
							<span class="variable-name">{param.name}</span>
							<span class="variable-eq">=</span>
							<span class="variable-expr">{param.expression}</span>
							<span class="variable-value" data-testid="variable-value-{param.name}">{param.error ? '⚠' : formatVariableValue(param)}</span>
							<button
								class="variable-delete"
								title="Delete variable"
								onclick={(e) => deleteVariable(e, param)}
								data-testid="variable-delete-{param.name}"
							>×</button>
						</div>
					{/if}
				{/each}
				{#if editingVariable && editingVariable.id === null}
					<div class="variable-row variable-editing" data-testid="variable-edit-row">
						<!-- svelte-ignore a11y_autofocus -->
						<input
							class="variable-input variable-name-input"
							bind:value={editingVariable.name}
							onkeydown={handleVariableKeydown}
							data-testid="variable-name-input"
							autofocus
						/>
						<span class="variable-eq">=</span>
						<input
							class="variable-input variable-expr-input"
							bind:value={editingVariable.expression}
							onkeydown={handleVariableKeydown}
							onblur={commitVariableEdit}
							data-testid="variable-expr-input"
						/>
					</div>
				{/if}
				{#if parameters.length === 0 && !editingVariable}
					<div class="variable-empty">No variables — press + to add</div>
				{/if}
			{/if}
		</div>

		<!-- Origin section -->
		<div class="origin-section">
			<button
				class="origin-header"
				onclick={() => originExpanded = !originExpanded}
				data-testid="origin-toggle"
			>
				<span class="expand-icon">{originExpanded ? '▾' : '▸'}</span>
				<span class="origin-label">Origin</span>
			</button>
			{#if originExpanded}
				{#each BUILTIN_PLANES as plane, i (plane.id)}
					<div
						class="tree-item origin-item"
						class:selected={isPlaneSelected(i)}
						class:hidden-item={!isPlaneVisible(plane.id)}
						onclick={() => handlePlaneClick(i)}
						oncontextmenu={(e) => handleOriginContextMenu(e, 'plane', plane.id, isPlaneVisible(plane.id))}
						role="treeitem"
						tabindex="0"
						data-testid="origin-plane-{plane.name.toLowerCase()}"
					>
						<span class="tree-icon origin-icon">{'\u25C7'}</span>
						<span class="tree-label">{plane.name}</span>
						<button
							class="visibility-toggle"
							title={isPlaneVisible(plane.id) ? 'Hide plane' : 'Show plane'}
							onclick={(e) => handlePlaneVisibilityToggle(e, plane.id)}
							data-testid="plane-visibility-{plane.name.toLowerCase()}"
						>
							{isPlaneVisible(plane.id) ? '\u25C9' : '\u25CE'}
						</button>
					</div>
				{/each}
				{#each ORIGIN_AXES as axis (axis.id)}
					<div
						class="tree-item origin-item"
						class:hidden-item={!isAxisVisible(axis.id)}
						oncontextmenu={(e) => handleOriginContextMenu(e, 'axis', axis.id, isAxisVisible(axis.id))}
						role="treeitem"
						tabindex="0"
						data-testid="origin-axis-{axis.id}"
					>
						<span class="tree-icon origin-icon" style="color: {axis.color}">{'\u2502'}</span>
						<span class="tree-label">{axis.name}</span>
						<button
							class="visibility-toggle"
							title={isAxisVisible(axis.id) ? 'Hide axis' : 'Show axis'}
							onclick={(e) => handleAxisVisibilityToggle(e, axis.id)}
							data-testid="axis-visibility-{axis.id}"
						>
							{isAxisVisible(axis.id) ? '\u25C9' : '\u25CE'}
						</button>
					</div>
				{/each}
			{/if}
		</div>

		<!-- Feature list -->
		{#if tree.features.length === 0}
			<div class="empty-state">No features yet</div>
		{:else}
			{#each tree.features as feature, i (feature.id)}
				{@const isAfterRollback = tree.active_index !== null && i > tree.active_index}
				{@const isDragging = dragFeatureId === feature.id}
				{@const isSketch = feature.operation?.type === 'Sketch'}
				<div
					class="tree-item"
					class:selected={selectedId === feature.id}
					class:sketch-selected={selectedId === feature.id && isSketch}
					class:face-source={faceFeatureId === feature.id}
					class:suppressed={feature.suppressed}
					class:after-rollback={isAfterRollback}
					class:dragging={isDragging}
					class:drop-above={dropTargetIndex === i && dragFeatureId !== feature.id}
					data-testid="feature-item-{i}"
					draggable="true"
					onclick={() => handleClick(feature.id)}
					ondblclick={() => handleDblClick(feature)}
					oncontextmenu={(e) => handleContextMenu(e, feature)}
					ondragstart={(e) => handleDragStart(e, feature)}
					ondragover={(e) => handleDragOver(e, i)}
					ondragleave={handleDragLeave}
					ondrop={(e) => handleDrop(e, i)}
					ondragend={handleDragEnd}
					role="treeitem"
					tabindex="0"
				>
					<span class="tree-icon">{featureIcon(feature.operation?.type)}</span>
					{#if renaming && renaming.featureId === feature.id}
						<input
							class="rename-input"
							bind:value={renaming.value}
							onkeydown={handleRename}
							onblur={handleRenameBlur}
						/>
					{:else}
						<span class="tree-label">{feature.name}</span>
					{/if}
					{#if tree.provenance?.[feature.id]?.origin?.type === 'Agent'}
						<span
							class="agent-badge"
							data-testid="agent-badge-{i}"
							title="Last authored by agent {tree.provenance[feature.id].origin.name}"
						>agent</span>
					{/if}
					{#if faceFeatureId === feature.id}
						<span class="face-source-badge" title="The selected face was created by this feature">◀ face</span>
					{/if}
					{#if feature.suppressed}
						<span class="suppress-indicator" title="Suppressed">S</span>
					{/if}
					{#if isSketch}
						<button
							class="visibility-toggle"
							title={isSketchVisible(feature.id) ? 'Hide sketch' : 'Show sketch'}
							onclick={(e) => handleVisibilityToggle(e, feature.id)}
						>
							{isSketchVisible(feature.id) ? '\u25C9' : '\u25CE'}
						</button>
					{/if}
					{#if featureErrors.get(feature.id)}
						<button
							class="error-indicator-btn"
							title={featureErrors.get(feature.id)}
							data-testid="feature-error-{i}"
							onclick={(e) => {
								e.stopPropagation();
							}}
						>⚠</button>
					{/if}
				</div>
				{#if tree.active_index !== null && i === tree.active_index && tree.active_index < tree.features.length - 1}
					<div class="rollback-bar" data-testid="rollback-bar" title="Rollback point — features below are rolled back and hidden">
						<span class="rollback-bar-label">Rollback</span>
					</div>
				{/if}
			{/each}
		{/if}

		<!-- Sources section (v4 §2.3) -->
		{#if sources.length > 0}
			<div class="bodies-section sources-section">
				<button
					class="origin-header"
					onclick={() => sourcesExpanded = !sourcesExpanded}
					data-testid="sources-toggle"
				>
					<span class="expand-icon">{sourcesExpanded ? '▾' : '▸'}</span>
					<span class="origin-label">Sources ({sources.length})</span>
				</button>
				{#if sourcesExpanded}
					{#if sources.some((s) => !s.pack && s.available && s.locator?.type !== 'Embedded')}
						<div class="source-tools">
							<button
								class="src-action"
								data-testid="sources-pack-all"
								title="Embed every fetched source in the file so it opens anywhere without network"
								onclick={() => withBusy('*', packAllSources)}
								disabled={sourceBusy !== null}
							>pack all</button>
						</div>
					{/if}
					{#each sources as s, i (s.id)}
						<div class="source-item" data-testid="source-item-{i}" title={describeLocator(s.locator)}>
							<span class="tree-icon" class:src-missing={!s.available}>{s.available ? '⛁' : '⚠'}</span>
							<span class="tree-label src-name">{s.name}</span>
							<span class="src-meta" data-testid="source-status-{i}">{sourceStatus(s)}</span>
							{#if s.kind === 'Script' && s.available}
								<button class="src-action" data-testid="source-edit-{i}" title="Open the script in the editor" disabled={sourceBusy !== null} onclick={() => showScriptEditor(s.id)}>edit</button>
							{/if}
							{#if !s.available}
								<button class="src-action" data-testid="source-fetch-{i}" title="Fetch through the link" disabled={sourceBusy !== null} onclick={() => withBusy(s.id, () => fetchSource(s.id))}>fetch</button>
							{/if}
							{#if isFloatingGit(s)}
								<button class="src-action" data-testid="source-update-{i}" title="Re-resolve the branch/tag tip and fetch the content there" disabled={sourceBusy !== null} onclick={() => withBusy(s.id, () => updateSourceToTip(s.id))}>update</button>
								<button class="src-action" data-testid="source-pin-{i}" title="Pin to the commit currently resolved" disabled={sourceBusy !== null || !s.resolved?.commit} onclick={() => withBusy(s.id, () => pinSource(s.id))}>pin</button>
							{/if}
							<label class="src-pack" title="Embed the content in the file (self-contained)">
								<input
									type="checkbox"
									data-testid="source-pack-{i}"
									checked={s.pack}
									disabled={sourceBusy !== null || s.locator?.type === 'Embedded' || !s.available}
									onchange={(e) => withBusy(s.id, () => setSourcePack(s.id, e.currentTarget.checked))}
								/>
								pack
							</label>
						</div>
					{/each}
				{/if}
			</div>
		{/if}

		<!-- Bodies section -->
		{#if bodies.length > 0}
			<div class="bodies-section">
				<button
					class="origin-header"
					onclick={() => bodiesExpanded = !bodiesExpanded}
					data-testid="bodies-toggle"
				>
					<span class="expand-icon">{bodiesExpanded ? '▾' : '▸'}</span>
					<span class="origin-label">Bodies ({bodies.length})</span>
				</button>
				{#if bodiesExpanded}
					{#each bodies as body, i (body.bodyId)}
						<div
							class="body-item"
							class:selected={selectedBodyId === body.bodyId}
							class:hidden-item={!isBodyVisible(body.bodyId)}
							data-testid="body-item-{i}"
							onclick={() => handleBodyClick(body.bodyId)}
							ondblclick={() => handleBodyDblClick(body)}
							oncontextmenu={(e) => handleBodyContextMenu(e, body)}
							onmouseenter={() => setHoveredBodyId(body.bodyId)}
							onmouseleave={() => setHoveredBodyId(null)}
							role="treeitem"
							tabindex="0"
						>
							<span class="tree-icon">{'▣'}</span>
							{#if bodyRenaming && bodyRenaming.bodyId === body.bodyId}
								<input
									class="rename-input body-rename-input"
									bind:value={bodyRenaming.value}
									onkeydown={handleBodyRename}
									onblur={handleBodyRenameBlur}
								/>
							{:else}
								<span class="tree-label">{body.name}</span>
							{/if}
							<button
								class="visibility-toggle"
								title={isBodyVisible(body.bodyId) ? 'Hide body' : 'Show body'}
								data-testid="body-visibility-{i}"
								onclick={(e) => handleBodyVisibilityToggle(e, body.bodyId)}
							>
								{isBodyVisible(body.bodyId) ? '◉' : '◎'}
							</button>
						</div>
					{/each}
				{/if}
			</div>
		{/if}
	</div>

	{#if tree.features.length > 0}
		<div class="rollback-area">
			<label class="rollback-label">
				Rollback
				<input
					type="range"
					class="rollback-slider"
					data-testid="rollback-slider"
					min="0"
					max={tree.features.length}
					value={rollbackValue}
					oninput={handleRollback}
				/>
			</label>
		</div>
	{/if}
</div>

<!-- Feature Context Menu -->
{#if contextMenu}
	<div
		class="context-menu"
		style="left: {contextMenu.x}px; top: {contextMenu.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		{#if contextMenu.isSketch}
			<button class="ctx-item" data-testid="ft-ctx-edit-sketch" onclick={handleEditSketch}>Edit Sketch</button>
		{/if}
		{#if contextMenu.operationType === 'Extrude' || contextMenu.operationType === 'Revolve' || contextMenu.operationType === 'Pipe' || contextMenu.operationType === 'MateConnector'}
			<button class="ctx-item" data-testid="ft-ctx-edit-feature" onclick={handleEditFeature}>Edit Feature</button>
		{/if}
		<button class="ctx-item" data-testid="ft-ctx-rename" onclick={handleRenameFromMenu}>Rename</button>
		<button class="ctx-item" data-testid="ft-ctx-suppress" onclick={handleSuppress}>
			{contextMenu.suppressed ? 'Unsuppress' : 'Suppress'}
		</button>
		<button class="ctx-item danger" data-testid="ft-ctx-delete" onclick={handleDelete}>Delete</button>
		{#if contextMenu.isSketch}
			<div class="ctx-sep"></div>
			{#if isSketchVisible(contextMenu.featureId)}
				<button class="ctx-item" data-testid="ft-ctx-hide-all-sketches" onclick={handleHideAllSketches}>Hide All Sketches</button>
			{:else}
				<button class="ctx-item" data-testid="ft-ctx-show-all-sketches" onclick={handleShowAllSketches}>Show All Sketches</button>
			{/if}
		{/if}
	</div>
{/if}

<!-- Origin Context Menu (planes & axes) -->
{#if originContextMenu}
	<div
		class="context-menu"
		style="left: {originContextMenu.x}px; top: {originContextMenu.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		{#if originContextMenu.kind === 'plane'}
			{#if originContextMenu.visible}
				<button class="ctx-item" data-testid="ft-ctx-hide-all-planes" onclick={handleHideAllPlanes}>Hide All Planes</button>
			{:else}
				<button class="ctx-item" data-testid="ft-ctx-show-all-planes" onclick={handleShowAllPlanes}>Show All Planes</button>
			{/if}
		{:else}
			{#if originContextMenu.visible}
				<button class="ctx-item" data-testid="ft-ctx-hide-all-axes" onclick={handleHideAllAxes}>Hide All Axes</button>
			{:else}
				<button class="ctx-item" data-testid="ft-ctx-show-all-axes" onclick={handleShowAllAxes}>Show All Axes</button>
			{/if}
		{/if}
	</div>
{/if}

<!-- Body Context Menu -->
{#if bodyContextMenu}
	<div
		class="context-menu"
		style="left: {bodyContextMenu.x}px; top: {bodyContextMenu.y}px"
		onclick={(e) => e.stopPropagation()}
	>
		<button class="ctx-item" data-testid="body-ctx-export-stl" onclick={handleBodyExport}>
			Export STL
		</button>
	</div>
{/if}

<style>
	.feature-tree {
		height: 100%;
		background: var(--bg-secondary);
		display: flex;
		flex-direction: column;
	}

	.panel-header {
		padding: 6px 12px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--text-secondary);
		border-bottom: 1px solid var(--border-color);
		background: var(--bg-tertiary);
	}

	.tree-content {
		flex: 1;
		padding: 4px 0;
		overflow-y: auto;
	}

	.origin-section {
		border-bottom: 1px solid var(--border-color, #444);
		margin-bottom: 2px;
	}

	.origin-header {
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 3px 8px;
		background: none;
		border: none;
		color: var(--text-secondary, #aaa);
		font-size: 11px;
		cursor: pointer;
		text-align: left;
	}

	.origin-header:hover {
		background: var(--bg-hover, #333);
	}

	/* Variables (design parameters) */
	.variables-header {
		display: flex;
		align-items: center;
		padding: 0;
	}

	.variables-header .variables-toggle {
		flex: 1;
	}

	.variable-add {
		background: none;
		border: none;
		color: var(--text-secondary, #aaa);
		font-size: 14px;
		line-height: 1;
		padding: 2px 8px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.variable-add:hover {
		color: var(--text-primary, #eee);
		background: var(--bg-hover, #333);
	}

	.variable-row {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px 2px 22px;
		font-size: 11px;
		font-family: ui-monospace, monospace;
		cursor: pointer;
		color: var(--text-primary, #ddd);
	}

	.variable-row:hover {
		background: var(--bg-hover, #333);
	}

	.variable-row:hover .variable-delete {
		visibility: visible;
	}

	.variable-name {
		color: var(--accent-color, #58a6ff);
		white-space: nowrap;
	}

	.variable-eq {
		color: var(--text-secondary, #888);
	}

	.variable-expr {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.variable-value {
		color: var(--text-secondary, #999);
		white-space: nowrap;
	}

	.variable-error .variable-value,
	.variable-error .variable-name {
		color: var(--error-color, #f66);
	}

	.variable-delete {
		visibility: hidden;
		background: none;
		border: none;
		color: var(--text-secondary, #888);
		font-size: 12px;
		line-height: 1;
		padding: 0 2px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.variable-delete:hover {
		color: var(--error-color, #f66);
	}

	.variable-editing {
		cursor: default;
	}

	.variable-input {
		background: var(--bg-primary, #222);
		border: 1px solid var(--accent-color, #58a6ff);
		border-radius: 2px;
		color: var(--text-primary, #eee);
		font-size: 11px;
		font-family: ui-monospace, monospace;
		padding: 1px 4px;
		min-width: 0;
	}

	.variable-name-input {
		width: 34%;
		flex-shrink: 0;
	}

	.variable-expr-input {
		flex: 1;
	}

	.variable-empty {
		padding: 2px 8px 4px 22px;
		font-size: 10px;
		color: var(--text-secondary, #777);
		font-style: italic;
	}

	.expand-icon {
		width: 10px;
		font-size: 10px;
		flex-shrink: 0;
	}

	.origin-label {
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.3px;
	}

	.origin-item {
		padding-left: 22px;
		cursor: pointer;
	}

	.origin-icon {
		color: var(--text-muted, #666);
	}

	.bodies-section {
		border-top: 1px solid var(--border-color, #444);
		margin-top: 2px;
		padding-top: 2px;
	}

	.body-item {
		display: flex;
		align-items: center;
		padding: 3px 12px;
		gap: 6px;
		cursor: pointer;
		user-select: none;
	}

	.source-item {
		display: flex;
		align-items: center;
		padding: 3px 12px;
		gap: 6px;
		font-size: 12px;
		user-select: none;
	}
	.source-item .src-name {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.src-meta {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-secondary, #a6adc8);
		font-family: monospace;
		font-size: 11px;
	}
	.src-missing {
		color: var(--warning, #f9e2af);
	}
	.src-action {
		padding: 0 6px;
		border-radius: 3px;
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: var(--accent, #89b4fa);
		font-size: 11px;
		cursor: pointer;
	}
	.src-action:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.src-pack {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		color: var(--text-secondary, #a6adc8);
	}
	.source-tools {
		padding: 2px 12px;
	}

	.body-item:hover {
		background: var(--bg-hover);
	}

	.body-item.selected {
		background: rgba(0, 120, 212, 0.2);
		border-left: 2px solid var(--accent);
		padding-left: 10px;
	}

	.body-item.hidden-item {
		opacity: 0.45;
	}

	/* Rollback bar: a horizontal marker drawn just below the active feature.
	 * Features rendered below it are rolled back (greyed + hidden in the scene). */
	.rollback-bar {
		display: flex;
		align-items: center;
		height: 0;
		border-top: 2px solid var(--accent, #0078d4);
		margin: 3px 0;
		position: relative;
	}

	.rollback-bar-label {
		position: absolute;
		left: 8px;
		top: -8px;
		font-size: 9px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.4px;
		color: var(--text-on-accent);
		background: var(--accent, #0078d4);
		padding: 1px 5px;
		border-radius: 3px;
		line-height: 1.2;
		white-space: nowrap;
	}

	.empty-state {
		padding: 16px 12px;
		color: var(--text-muted);
		font-style: italic;
		font-size: 12px;
	}

	.tree-item {
		display: flex;
		align-items: center;
		padding: 3px 12px;
		cursor: grab;
		gap: 6px;
		user-select: none;
		transition: border-top 0.1s;
		border-top: 2px solid transparent;
	}

	.tree-item:hover {
		background: var(--bg-hover);
	}

	.tree-item.selected {
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		border-left: 2px solid var(--accent);
		padding-left: 10px;
	}

	.tree-item.selected.sketch-selected {
		background: color-mix(in srgb, var(--warning) 18%, transparent);
		border-left-color: var(--warning);
	}

	/* Face→feature: the feature that created the currently-picked face. */
	.tree-item.face-source {
		background: color-mix(in srgb, var(--success) 16%, transparent);
		border-left: 2px solid var(--success);
		padding-left: 10px;
	}

	.face-source-badge {
		margin-left: auto;
		font-size: 9px;
		color: var(--success);
		background: color-mix(in srgb, var(--success) 18%, transparent);
		padding: 0 4px;
		border-radius: 3px;
		flex-shrink: 0;
		white-space: nowrap;
	}

	.tree-item.origin-item.selected {
		padding-left: 20px;
	}

	.tree-item.suppressed {
		opacity: 0.4;
		text-decoration: line-through;
	}

	.tree-item.hidden-item {
		opacity: 0.4;
	}

	.tree-item.after-rollback {
		opacity: 0.3;
	}

	.tree-item.dragging {
		opacity: 0.4;
	}

	.tree-item.drop-above {
		border-top: 2px solid var(--accent);
	}

	.tree-icon {
		width: 16px;
		text-align: center;
		font-size: 12px;
		color: var(--text-secondary);
		flex-shrink: 0;
	}

	.tree-label {
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.agent-badge {

		font-size: 9px;

		padding: 0 4px;

		border-radius: 6px;

		border: 1px solid var(--accent, #89b4fa);

		color: var(--accent, #89b4fa);

		line-height: 14px;

		flex-shrink: 0;

	}


	.suppress-indicator {
		margin-left: auto;
		font-size: 9px;
		color: var(--text-muted);
		background: var(--bg-tertiary);
		padding: 0 3px;
		border-radius: 2px;
	}

	.visibility-toggle {
		margin-left: auto;
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 11px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
		opacity: 0.6;
	}

	.visibility-toggle:hover {
		opacity: 1;
		color: var(--text-primary);
	}

	.error-indicator {
		margin-left: auto;
		font-size: 12px;
		color: var(--error);
		cursor: help;
		flex-shrink: 0;
	}

	.error-indicator-btn {
		margin-left: auto;
		font-size: 12px;
		color: var(--error);
		cursor: pointer;
		flex-shrink: 0;
		background: none;
		border: none;
		padding: 0 4px;
		border-radius: 3px;
	}

	.error-indicator-btn:hover {
		background: rgba(255, 107, 107, 0.15);
	}

	.rename-input {
		background: var(--bg-primary);
		border: 1px solid var(--accent);
		color: var(--text-primary);
		font-size: 12px;
		padding: 1px 4px;
		outline: none;
		flex: 1;
		min-width: 0;
	}

	.rollback-area {
		padding: 6px 12px;
		border-top: 1px solid var(--border-color);
		background: var(--bg-tertiary);
	}

	.rollback-label {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 10px;
		color: var(--text-secondary);
	}

	.rollback-slider {
		flex: 1;
		height: 4px;
		accent-color: var(--accent);
	}

	.context-menu {
		position: fixed;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 4px 0;
		z-index: 1000;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
		min-width: 120px;
	}

	.ctx-item {
		display: block;
		width: 100%;
		background: transparent;
		border: none;
		color: var(--text-primary);
		font-size: 12px;
		padding: 5px 16px;
		cursor: pointer;
		text-align: left;
	}

	.ctx-item:hover {
		background: var(--accent);
		color: var(--text-on-accent);
	}

	.ctx-item.danger:hover {
		background: var(--error);
	}

	.ctx-sep {
		height: 1px;
		background: var(--border-color, #444);
		margin: 4px 0;
	}
</style>
