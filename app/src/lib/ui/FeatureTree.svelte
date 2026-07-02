<script>
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
		toggleBodyVisibility
	} from '$lib/engine/store.svelte.js';
	import { BUILTIN_PLANES, makePlaneRef } from '$lib/engine/planes.js';
	import { longPressContextMenu } from './longPressContextMenu.js';

	let tree = $derived(getFeatureTree());
	let selectedId = $derived(getSelectedFeatureId());
	// Face→feature (Tier 1): the feature whose geometry is currently picked.
	let faceFeatureId = $derived(getSelectedRefFeatureId());
	let featureErrors = $derived(getFeatureErrors());
	let bodies = $derived(getBodies());
	let selectedBodyId = $derived(getSelectedBodyId());

	/** @type {{ x: number, y: number, featureId: string, featureName: string, suppressed: boolean, isSketch: boolean, operationType: string | null } | null} */
	let contextMenu = $state(null);

	/** @type {{ x: number, y: number, kind: 'plane' | 'axis', id: string, visible: boolean } | null} */
	let originContextMenu = $state(null);

	// Built-in axis definitions for the Origin section
	const ORIGIN_AXES = [
		{ id: 'x', name: 'X Axis', color: '#ff4444' },
		{ id: 'y', name: 'Y Axis', color: '#44cc44' },
		{ id: 'z', name: 'Z Axis', color: '#4488ff' },
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

	// Bodies section state
	let bodiesExpanded = $state(true);

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
		bodyRenaming = { bodyId: body.bodyId, value: body.name };
	}

	function commitBodyRename() {
		if (!bodyRenaming) return;
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
		const opType = feature.operation?.type;
		if (opType === 'Sketch') {
			enterSketchEditMode(feature.id);
		} else if (opType === 'Extrude' || opType === 'Revolve') {
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
			deleteFeature(selectedId);
			selectFeature(null);
		}
	}

	function handleDelete() {
		if (contextMenu) {
			deleteFeature(contextMenu.featureId);
			if (selectedId === contextMenu.featureId) selectFeature(null);
			closeContextMenu();
		}
	}

	function handleSuppress() {
		if (contextMenu) {
			suppressFeature(contextMenu.featureId, !contextMenu.suppressed);
			closeContextMenu();
		}
	}

	function handleEditSketch() {
		if (contextMenu && contextMenu.isSketch) {
			enterSketchEditMode(contextMenu.featureId);
			closeContextMenu();
		}
	}

	function handleEditFeature() {
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
			case 'Fillet': return '\u25CF';
			case 'Chamfer': return '\u25C6';
			case 'Shell': return '\u25A1';
			case 'BooleanCombine': return '\u2229';
			default: return '\u2022';
		}
	}

	// -- Drag and drop --

	function handleDragStart(e, feature) {
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
		if (dragFeatureId) {
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
		const val = parseInt(e.target.value);
		const index = val >= tree.features.length ? null : val;
		setRollbackIndex(index);
	}
</script>

<svelte:window onclick={closeContextMenu} onkeydown={handleKeyDown} />

<div class="feature-tree">
	<div class="panel-header">Features</div>
	<div class="tree-content" use:longPressContextMenu>
		<!-- Origin section -->
		<div class="origin-section">
			<button
				class="origin-header"
				onclick={() => originExpanded = !originExpanded}
				data-testid="origin-toggle"
			>
				<span class="expand-icon">{originExpanded ? '\u25BE' : '\u25B8'}</span>
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
		{#if contextMenu.operationType === 'Extrude' || contextMenu.operationType === 'Revolve'}
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
		background: rgba(0, 120, 212, 0.2);
		border-left: 2px solid var(--accent);
		padding-left: 10px;
	}

	.tree-item.selected.sketch-selected {
		background: rgba(255, 136, 0, 0.15);
		border-left-color: #ff8800;
	}

	/* Face→feature: the feature that created the currently-picked face. */
	.tree-item.face-source {
		background: rgba(68, 204, 136, 0.16);
		border-left: 2px solid #44cc88;
		padding-left: 10px;
	}

	.face-source-badge {
		margin-left: auto;
		font-size: 9px;
		color: #2e9e6a;
		background: rgba(68, 204, 136, 0.18);
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
		color: #ff6b6b;
		cursor: help;
		flex-shrink: 0;
	}

	.error-indicator-btn {
		margin-left: auto;
		font-size: 12px;
		color: #ff6b6b;
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
