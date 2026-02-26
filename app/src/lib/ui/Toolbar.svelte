<script>
	import {
		isEngineReady,
		getActiveTool,
		setActiveTool,
		getSketchMode,
		enterSketchMode,
		undo,
		redo,
		getSketchSelection,
		getSketchEntities,
		getSketchPositions,
		toggleConstruction,
		addLocalConstraint,
		finishSketch,
		showExtrudeDialog,
		showRevolveDialog,
		showChamferDialog,
		showFilletDialog,
		showShellDialog,
		showBooleanDialog,
		saveProject,
		loadProject,
		exportStl,
		exportStep,
		getSelectedRefs,
		computeFacePlane,
		enterSketchPlaneSelection,
		exitSketchPlaneSelection,
		getSketchPlaneSelectionMode,
		getMobileLayout,
		toggleMobilePanel,
		getProjectName,
		setProjectName,
		toggleTestCaseBrowser,
		getShowDatumPlanes,
		getShowOriginTriad,
		toggleDatumPlanes,
		toggleOriginTriad,
		removeSketchEntities,
		getSketchSolveStatus
	} from '$lib/engine/store.svelte.js';
	import { getApplicableConstraints } from '$lib/sketch/constraintLogic.js';
	import { resetTool } from '$lib/sketch/tools.js';
	import { onMount } from 'svelte';

	let ready = $derived(isEngineReady());
	let tool = $derived(getActiveTool());
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let planeSelecting = $derived(getSketchPlaneSelectionMode());
	let selection = $derived(getSketchSelection());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());

	let isMobile = $derived(getMobileLayout());
	let applicable = $derived(inSketch ? getApplicableConstraints(selection, entities, positions) : {});
	let solveStatus = $derived(inSketch ? getSketchSolveStatus() : null);

	// Portrait mode: collapse file/view actions into overflow menu
	let showOverflow = $state(false);

	function toggleOverflow() {
		showOverflow = !showOverflow;
	}

	function closeOverflow() {
		showOverflow = false;
	}

	let name = $derived(getProjectName());
	let editingName = $state(false);
	let nameInputValue = $state('');

	let saving = $state(false);
	let exportingStl = $state(false);
	let exportingStep = $state(false);

	const constraintButtons = [
		{ id: 'horizontal', label: 'H', title: 'Horizontal' },
		{ id: 'vertical', label: 'V', title: 'Vertical' },
		{ id: 'coincident', label: 'Co', title: 'Coincident' },
		{ id: 'perpendicular', label: 'Perp', title: 'Perpendicular' },
		{ id: 'parallel', label: 'Par', title: 'Parallel' },
		{ id: 'equal', label: 'Eq', title: 'Equal' },
		{ id: 'tangent', label: 'Tan', title: 'Tangent' },
		{ id: 'midpoint', label: 'Mid', title: 'Midpoint' },
		{ id: 'fix', label: 'Fix', title: 'Fix Point' },
		{ id: 'angle', label: 'Ang', title: 'Angle' },
		{ id: 'symmetricH', label: 'SH', title: 'Symmetric Horizontal' },
		{ id: 'symmetricV', label: 'SV', title: 'Symmetric Vertical' },
		{ id: 'pointOnLine', label: 'OnL', title: 'Point on Line' },
		{ id: 'hDistance', label: 'HD', title: 'Horizontal Distance' },
		{ id: 'vDistance', label: 'VD', title: 'Vertical Distance' },
	];

	function applyConstraint(id) {
		const builder = applicable[id];
		if (builder) addLocalConstraint(builder());
	}

	const modelingTools = [
		{ id: 'sketch', label: 'Sketch', shortcut: 'S' },
		{ id: 'extrude', label: 'Extrude', shortcut: 'E' },
		{ id: 'revolve', label: 'Revolve', shortcut: '' },
		{ id: 'fillet', label: 'Fillet', shortcut: '' },
		{ id: 'chamfer', label: 'Chamfer', shortcut: '' },
		{ id: 'shell', label: 'Shell', shortcut: '' },
		{ id: 'boolean', label: 'Boolean', shortcut: '' },
	];

	const sketchTools = [
		{ id: 'select', label: 'Select', shortcut: '' },
		{ id: 'line', label: 'Line', shortcut: 'L' },
		{ id: 'polyline', label: 'Poly', shortcut: 'P' },
		{ id: 'rectangle', label: 'Rect', shortcut: 'R' },
		{ id: 'circle', label: 'Circle', shortcut: 'C' },
		{ id: 'arc', label: 'Arc', shortcut: 'A' },
		{ id: 'project', label: 'Proj', shortcut: 'J' },
		{ id: 'slot', label: 'Slot', shortcut: 'T' },
		{ id: 'trim', label: 'Trim', shortcut: '' },
		{ id: 'sketch-fillet', label: 'Fillet', shortcut: 'F' },
		{ id: 'construction', label: 'Constr', shortcut: 'X' },
	];

	async function handleToolClick(toolId) {
		console.log('[waffle-toolbar] handleToolClick:', toolId, { ready, inSketch });
		if (toolId === 'sketch') {
			if (inSketch) {
				handleFinishSketch();
			} else {
				const refs = getSelectedRefs();
				if (refs.length > 0) {
					const plane = computeFacePlane(refs[0]);
					if (plane) {
						await enterSketchMode(plane.origin, plane.normal);
						setActiveTool('line');
						return;
					}
				}
				// No face selected — show plane selection dialog
				enterSketchPlaneSelection();
				return;
			}
			return;
		}
		if (toolId === 'extrude' && !inSketch) {
			showExtrudeDialog();
			return;
		}
		if (toolId === 'revolve' && !inSketch) {
			showRevolveDialog();
			return;
		}
		if (toolId === 'chamfer' && !inSketch) {
			showChamferDialog();
			return;
		}
		if (toolId === 'fillet' && !inSketch) {
			showFilletDialog();
			return;
		}
		if (toolId === 'shell' && !inSketch) {
			showShellDialog();
			return;
		}
		if (toolId === 'boolean' && !inSketch) {
			showBooleanDialog();
			return;
		}
		if (toolId === 'construction') {
			handleToggleConstruction();
			return;
		}
		setActiveTool(toolId);
	}

	function handleToggleConstruction() {
		const sel = getSketchSelection();
		for (const id of sel) {
			toggleConstruction(id);
		}
	}

	async function handleFinishSketch() {
		try {
			await finishSketch();
		} catch (err) {
			console.error('Finish sketch error:', err);
		}
	}

	onMount(() => {
		/** @param {KeyboardEvent} e */
		function onKeyDown(e) {
			if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
			if (!ready) return;

			if (e.ctrlKey || e.metaKey) {
				if (e.key === 's') { e.preventDefault(); saveProject(); return; }
				if (e.key === 'o') { e.preventDefault(); loadProject(); return; }
				if (e.key === 'z' && !e.shiftKey) { e.preventDefault(); undo(); }
				if (e.key === 'z' && e.shiftKey) { e.preventDefault(); redo(); }
				if (e.key === 'Z') { e.preventDefault(); redo(); }
				if (e.key === 'T' && e.shiftKey) { e.preventDefault(); toggleTestCaseBrowser(); return; }
				return;
			}

			switch (e.key) {
				case 's': handleToolClick('sketch'); break;
				case 'e': handleToolClick('extrude'); break;
				case 'l': if (inSketch) setActiveTool('line'); break;
				case 'r': if (inSketch) setActiveTool('rectangle'); break;
				case 'p': if (inSketch) setActiveTool('polyline'); break;
				case 'c': if (inSketch) setActiveTool('circle'); break;
				case 'a': if (inSketch) setActiveTool('arc'); break;
				case 'x': if (inSketch) handleToggleConstruction(); break;
				case 'j': if (inSketch) setActiveTool('project'); break;
				case 't': if (inSketch) setActiveTool('slot'); break;
				case 'f': if (inSketch) setActiveTool('sketch-fillet'); break;
				case 'd': if (inSketch) setActiveTool('dimension'); break;
				case 'g': if (inSketch) handleToggleConstruction(); break;
				case 'Escape':
					if (planeSelecting) {
						exitSketchPlaneSelection();
					} else if (inSketch) {
						if (tool !== 'select') {
							resetTool();
							setActiveTool('select');
						} else {
							handleFinishSketch();
						}
					} else {
						setActiveTool('select');
					}
					break;
				case 'Delete':
				case 'Backspace':
					if (inSketch) {
						const sel = getSketchSelection();
						if (sel.size > 0) {
							e.preventDefault();
							removeSketchEntities(sel);
						}
					}
					// Outside sketch: handled by feature tree
					break;
			}
		}
		window.addEventListener('keydown', onKeyDown);
		return () => window.removeEventListener('keydown', onKeyDown);
	});
</script>

<div class="toolbar" data-testid="toolbar">
	<div class="toolbar-brand">Waffle Iron</div>

	<div class="project-name" data-testid="project-name">
		{#if editingName}
			<input
				class="name-input"
				bind:value={nameInputValue}
				onblur={() => { if (nameInputValue.trim()) setProjectName(nameInputValue.trim()); editingName = false; }}
				onkeydown={(e) => { if (e.key === 'Enter') { if (nameInputValue.trim()) setProjectName(nameInputValue.trim()); editingName = false; } else if (e.key === 'Escape') { editingName = false; } }}
			/>
		{:else}
			<button class="name-btn" ondblclick={() => { nameInputValue = name; editingName = true; }} title="Double-click to rename">
				{name}
			</button>
		{/if}
	</div>

	{#if inSketch}
		<!-- Sketch mode tools -->
		<div class="toolbar-group">
			{#each sketchTools as t}
				<button
					class="toolbar-btn"
					class:active={t.id !== 'construction' && tool === t.id}
					disabled={!ready}
					title="{t.label}{t.shortcut ? ` (${t.shortcut})` : ''}"
					data-testid="toolbar-btn-{t.id}"
					onclick={() => t.id === 'construction' ? handleToggleConstruction() : setActiveTool(t.id)}
				>{t.label}</button>
			{/each}
		</div>
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			{#each constraintButtons as cb}
				<button
					class="constraint-btn"
					disabled={!applicable[cb.id]}
					title={cb.title}
					data-testid="toolbar-constraint-{cb.id}"
					onclick={() => applyConstraint(cb.id)}
				>{cb.label}</button>
			{/each}
		</div>
		<div class="toolbar-sep"></div>
		<button
			class="toolbar-btn"
			class:active={tool === 'dimension'}
			title="Smart Dimension (D)"
			data-testid="toolbar-btn-dimension"
			onclick={() => setActiveTool('dimension')}
		>Dim</button>
		<div class="toolbar-sep"></div>
		{#if solveStatus}
			<span
				class="dof-badge"
				class:dof-ok={solveStatus.dof === 0 && solveStatus.status === 'okay'}
				class:dof-under={solveStatus.dof > 0 && solveStatus.status === 'okay'}
				class:dof-over={solveStatus.status === 'inconsistent'}
				class:dof-redundant={solveStatus.status === 'didnt_converge'}
				data-testid="dof-badge"
				title={solveStatus.status === 'inconsistent' ? 'Over-constrained: conflicting constraints detected'
					: solveStatus.status === 'didnt_converge' ? 'Solver did not converge (possible redundant constraints)'
					: solveStatus.dof === 0 ? 'Fully constrained'
					: `${solveStatus.dof} degrees of freedom remaining`}
			>
				{#if solveStatus.status === 'inconsistent'}
					Over-constrained
				{:else if solveStatus.status === 'didnt_converge'}
					Redundant?
				{:else if solveStatus.dof === 0}
					Fully constrained
				{:else}
					{solveStatus.dof} DOF
				{/if}
			</span>
		{/if}
		<div class="toolbar-sep"></div>
		<button class="toolbar-btn finish-btn" data-testid="toolbar-btn-finish-sketch" onclick={handleFinishSketch}>
			Finish Sketch
		</button>
	{:else}
		<!-- Modeling tools -->
		<div class="toolbar-group">
			{#each modelingTools as t}
				<button
					class="toolbar-btn"
					class:active={tool === t.id}
					disabled={!ready}
					title="{t.label}{t.shortcut ? ` (${t.shortcut})` : ''}"
					data-testid="toolbar-btn-{t.id}"
					onclick={async () => { await handleToolClick(t.id); }}
				>{t.label}</button>
			{/each}
		</div>
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button
				class="toolbar-btn"
				class:active={getShowDatumPlanes()}
				title="Toggle Datum Planes"
				data-testid="toolbar-btn-toggle-planes"
				onclick={() => toggleDatumPlanes()}
			>Planes</button>
			<button
				class="toolbar-btn"
				class:active={getShowOriginTriad()}
				title="Toggle Origin Axes"
				data-testid="toolbar-btn-toggle-axes"
				onclick={() => toggleOriginTriad()}
			>Axes</button>
		</div>
	{/if}

	<div class="toolbar-sep"></div>
	<div class="toolbar-group">
		<button class="toolbar-btn" data-testid="toolbar-btn-undo" disabled={!ready} title="Undo (Ctrl+Z)" onclick={undo}>Undo</button>
		<button class="toolbar-btn" data-testid="toolbar-btn-redo" disabled={!ready} title="Redo (Ctrl+Shift+Z)" onclick={redo}>Redo</button>
	</div>
	{#if isMobile}
		<!-- Mobile: collapse file/export/test actions into overflow menu -->
		<div class="toolbar-sep"></div>
		<div class="overflow-container">
			<button class="toolbar-btn overflow-trigger" title="More actions" onclick={toggleOverflow}
				data-testid="toolbar-btn-overflow">
				&#x22EE;
			</button>
			{#if showOverflow}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="overflow-backdrop" onclick={closeOverflow}></div>
				<div class="overflow-menu" data-testid="toolbar-overflow-menu">
					<button class="overflow-item" disabled={!ready || saving}
						data-testid="toolbar-btn-save"
						onclick={async () => { closeOverflow(); saving = true; try { await saveProject(); } finally { saving = false; } }}>
						{saving ? 'Saving...' : 'Save'}
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-open"
						onclick={() => { closeOverflow(); loadProject(); }}>Open</button>
					<button class="overflow-item" disabled={!ready || exportingStl}
						data-testid="toolbar-btn-export-stl"
						onclick={async () => { closeOverflow(); exportingStl = true; try { await exportStl(); } finally { exportingStl = false; } }}>
						{exportingStl ? 'Exporting...' : 'Export STL'}
					</button>
					<button class="overflow-item" disabled={!ready || exportingStep}
						data-testid="toolbar-btn-export-step"
						onclick={async () => { closeOverflow(); exportingStep = true; try { await exportStep(); } finally { exportingStep = false; } }}>
						{exportingStep ? 'Exporting...' : 'Export STEP'}
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-tests"
						onclick={() => { closeOverflow(); toggleTestCaseBrowser(); }}>Tests</button>
				</div>
			{/if}
		</div>
	{:else}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" disabled={!ready || saving} title="Save (Ctrl+S)"
				data-testid="toolbar-btn-save"
				onclick={async () => { saving = true; try { await saveProject(); } finally { saving = false; } }}>
				{saving ? 'Saving...' : 'Save'}
			</button>
			<button class="toolbar-btn" disabled={!ready} title="Open (Ctrl+O)"
				data-testid="toolbar-btn-open"
				onclick={() => loadProject()}>Open</button>
			<button class="toolbar-btn" disabled={!ready || exportingStl} title="Export STL"
				data-testid="toolbar-btn-export-stl"
				onclick={async () => { exportingStl = true; try { await exportStl(); } finally { exportingStl = false; } }}>
				{exportingStl ? 'Exporting...' : 'Export STL'}
			</button>
			<button class="toolbar-btn" disabled={!ready || exportingStep} title="Export STEP"
				data-testid="toolbar-btn-export-step"
				onclick={async () => { exportingStep = true; try { await exportStep(); } finally { exportingStep = false; } }}>
				{exportingStep ? 'Exporting...' : 'Export STEP'}
			</button>
			<button class="toolbar-btn" disabled={!ready} title="Test Cases (Ctrl+Shift+T)"
				data-testid="toolbar-btn-tests"
				onclick={() => toggleTestCaseBrowser()}>Tests</button>
		</div>
	{/if}

	<div class="toolbar-spacer"></div>
	{#if isMobile}
		<button class="toolbar-btn mobile-toggle" title="Feature Tree" onclick={() => toggleMobilePanel('left')}>Tree</button>
		<button class="toolbar-btn mobile-toggle" title="Properties" onclick={() => toggleMobilePanel('right')}>Props</button>
	{/if}
	<div class="toolbar-status">
		{#if ready}
			<span class="status-dot ready" data-testid="status-dot"></span>
		{:else}
			<span class="status-dot loading" data-testid="status-dot"></span>
		{/if}
	</div>
</div>

<style>
	.toolbar {
		display: flex;
		align-items: center;
		height: 100%;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-color);
		padding: 0 max(8px, env(safe-area-inset-left, 0px));
		gap: 4px;
	}

	.toolbar-brand {
		font-weight: 600;
		font-size: 14px;
		color: var(--text-primary);
		padding-right: 12px;
		border-right: 1px solid var(--border-color);
		margin-right: 4px;
	}

	.project-name {
		display: flex;
		align-items: center;
		margin-right: 4px;
	}

	.name-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 12px;
		cursor: default;
		padding: 2px 6px;
		border-radius: 3px;
	}

	.name-btn:hover {
		background: var(--bg-hover);
	}

	.name-input {
		background: var(--bg-primary);
		border: 1px solid var(--accent);
		color: var(--text-primary);
		font-size: 12px;
		padding: 2px 6px;
		border-radius: 3px;
		width: 120px;
		outline: none;
	}

	.toolbar-group {
		display: flex;
		gap: 1px;
	}

	.toolbar-sep {
		width: 1px;
		height: 20px;
		background: var(--border-color);
		margin: 0 4px;
	}

	.toolbar-btn {
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-primary);
		padding: 4px 8px;
		border-radius: 3px;
		cursor: pointer;
		font-size: 12px;
		white-space: nowrap;
	}

	.toolbar-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		border-color: var(--border-color);
	}

	.toolbar-btn.active {
		background: rgba(0, 120, 212, 0.2);
		border-color: var(--accent);
		color: var(--accent);
	}

	.toolbar-btn:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	.constraint-btn {
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-primary);
		padding: 3px 5px;
		border-radius: 3px;
		cursor: pointer;
		font-size: 11px;
		white-space: nowrap;
	}

	.constraint-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		border-color: var(--border-color);
	}

	.constraint-btn:disabled {
		color: var(--text-muted);
		cursor: default;
		opacity: 0.4;
	}

	.dof-badge {
		font-size: 11px;
		padding: 2px 8px;
		border-radius: 3px;
		white-space: nowrap;
		font-weight: 500;
	}

	.dof-ok {
		color: var(--success);
		background: rgba(78, 201, 176, 0.15);
	}

	.dof-under {
		color: var(--warning, #e8a838);
		background: rgba(232, 168, 56, 0.15);
	}

	.dof-over {
		color: var(--error, #f44);
		background: rgba(255, 68, 68, 0.15);
	}

	.dof-redundant {
		color: #e89038;
		background: rgba(232, 144, 56, 0.15);
	}

	.finish-btn {
		color: var(--success);
		font-weight: 600;
	}

	.finish-btn:hover {
		background: rgba(78, 201, 176, 0.15);
		border-color: var(--success);
	}

	.toolbar-spacer {
		flex: 1;
	}

	.toolbar-status {
		display: flex;
		align-items: center;
	}

	.status-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.status-dot.ready { background: var(--success); }
	.status-dot.loading {
		background: var(--warning);
		animation: pulse 1s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.3; }
	}

	.mobile-toggle {
		display: none;
	}

	/* Overflow menu for mobile */
	.overflow-container {
		position: relative;
	}

	.overflow-trigger {
		font-size: 18px;
		font-weight: bold;
		letter-spacing: 1px;
		padding: 4px 8px;
	}

	.overflow-backdrop {
		position: fixed;
		inset: 0;
		z-index: 199;
	}

	.overflow-menu {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 200;
		min-width: 160px;
		padding: 4px 0;
	}

	.overflow-item {
		display: block;
		width: 100%;
		background: none;
		border: none;
		color: var(--text-primary);
		padding: 10px 16px;
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		white-space: nowrap;
	}

	.overflow-item:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.overflow-item:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	@media (max-width: 768px) {
		.toolbar {
			overflow-x: auto;
			scrollbar-width: none;
		}

		.toolbar::-webkit-scrollbar {
			display: none;
		}

		.toolbar-btn {
			padding: 8px 12px;
			min-height: 36px;
		}

		.constraint-btn {
			padding: 6px 8px;
			min-height: 36px;
		}

		.mobile-toggle {
			display: inline-flex;
		}
	}

	@media (max-width: 480px) {
		.toolbar-brand {
			display: none;
		}

		.project-name {
			display: none;
		}

		.toolbar-btn {
			padding: 6px 8px;
			font-size: 11px;
			min-height: 40px;
		}

		.constraint-btn {
			padding: 4px 6px;
			font-size: 10px;
			min-height: 40px;
		}

		.toolbar-sep {
			margin: 0 2px;
		}
	}
</style>
