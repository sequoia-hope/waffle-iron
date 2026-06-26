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
		getSelectedConstraintIndex,
		deleteSelectedConstraint,
		toggleConstruction,
		addLocalConstraint,
		finishSketch,
		showExtrudeDialog,
		showRevolveDialog,
		showChamferDialog,
		showFilletDialog,
		showShellDialog,
		showBooleanDialog,
		showDatumPlaneDialog,
		saveProject,
		saveToStorage,
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
		toggleAssayBrowser,
		getShowDatumPlanes,
		getShowOriginTriad,
		toggleDatumPlanes,
		toggleOriginTriad,
		removeSketchEntities,
		getSketchSolveStatus,
		getSectionState,
		toggleSection,
		flipSection,
		setSectionOffset,
		clearSection,
		openConstraintModal
	} from '$lib/engine/store.svelte.js';
	import { isModalConstraint } from '$lib/sketch/constraintModalEngine.js';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { showToast } from '$lib/ui/toast.svelte.js';
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
	let section = $derived(getSectionState());
	let applicable = $derived(inSketch ? getApplicableConstraints(selection, entities, positions) : {});
	let solveStatus = $derived(inSketch ? getSketchSolveStatus() : null);

	// Portrait mode: collapse file/view actions into overflow menu
	let showOverflow = $state(false);
	let showConstraints = $state(false);
	let showSketchTools = $state(false);
	let showModelingTools = $state(false);

	let showDebugMenu = $state(false);

	// Fixed-position dropdown tracking for mobile (avoids overflow:hidden clipping)
	let dropdownPos = $state({ top: 0, left: 0, right: null });
	let overflowPos = $state({ top: 0, right: 0 });

	function openDropdown(triggerEl, setState) {
		const rect = triggerEl.getBoundingClientRect();
		dropdownPos = { top: rect.bottom + 4, left: rect.left, right: null };
		setState();
	}

	function openOverflow(triggerEl) {
		const rect = triggerEl.getBoundingClientRect();
		const saiRight = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sai-right')) || 0;
		overflowPos = { top: rect.bottom + 4, right: Math.max(4, window.innerWidth - rect.right - saiRight) };
		showOverflow = !showOverflow;
	}

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
		// Selection-first: if the current selection already satisfies the
		// constraint, apply it immediately (legacy behavior).
		const builder = applicable[id];
		if (builder) {
			addLocalConstraint(builder());
			return;
		}
		// Otherwise enter the constraint-first modal (keep picking geometry).
		// See /specs/constraint_modal.md.
		if (isModalConstraint(id)) openConstraintModal(id);
	}

	const modelingTools = [
		{ id: 'sketch', label: 'Sketch', shortcut: 'S' },
		{ id: 'extrude', label: 'Extrude', shortcut: 'E' },
		{ id: 'revolve', label: 'Revolve', shortcut: '' },
		{ id: 'fillet', label: 'Fillet', shortcut: '' },
		{ id: 'chamfer', label: 'Chamfer', shortcut: '' },
		{ id: 'shell', label: 'Shell', shortcut: '' },
		{ id: 'boolean', label: 'Boolean', shortcut: '' },
		{ id: 'datum-plane', label: 'Plane', shortcut: '' },
	];

	const sketchTools = [
		{ id: 'select', label: 'Select', shortcut: '' },
		{ id: 'point', label: 'Point', shortcut: '' },
		{ id: 'line', label: 'Line', shortcut: 'L' },
		{ id: 'polyline', label: 'Poly', shortcut: 'P' },
		{ id: 'rectangle', label: 'Rect', shortcut: 'R' },
		{ id: 'circle', label: 'Circle', shortcut: 'C' },
		{ id: 'arc', label: 'Arc', shortcut: 'A' },
		{ id: 'project', label: 'Proj', shortcut: 'J' },
		{ id: 'slot', label: 'Slot', shortcut: 'T' },
		{ id: 'gear', label: 'Gear', shortcut: 'G' },
		{ id: 'planetary', label: 'Planet', shortcut: '' },
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
						await enterSketchMode(plane.origin, plane.normal, refs[0]);
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
		if (toolId === 'datum-plane' && !inSketch) {
			showDatumPlaneDialog();
			return;
		}
		if (toolId === 'construction') {
			handleToggleConstruction();
			return;
		}
		setActiveTool(toolId);
	}

	function handleToggleSection() {
		// toggleSection captures the selected plane/face; it toasts a hint and
		// stays off if nothing suitable is selected.
		toggleSection();
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

	function handleViewportDebug() {
		const info = window.__waffle?.viewportDebug?.();
		if (!info) { showToast('error', 'Viewport debug not available'); return; }
		if (info.error) { showToast('error', info.error); return; }
		const cam = info.camera;
		const lines = [
			`type: ${cam.type ?? 'unknown'}`,
			`near: ${cam.near.toFixed(4)}, far: ${cam.far.toFixed(1)}`,
		];
		if (cam.fov != null) lines.push(`fov: ${cam.fov}`);
		if (cam.orthoFrustum) {
			const f = cam.orthoFrustum;
			lines.push(`frustum: L=${f.left.toFixed(3)} R=${f.right.toFixed(3)} T=${f.top.toFixed(3)} B=${f.bottom.toFixed(3)}`);
		}
		lines.push(
			`pos: [${cam.position.map(v => v.toFixed(2)).join(', ')}]`,
			`dist: ${info.cameraDistanceToAABB?.toFixed(2) ?? 'N/A'}`,
			`insideAABB: ${info.isInsideAABB}`,
		);
		if (info.sceneAABB) {
			const s = info.sceneAABB;
			lines.push(`AABB min: [${s.min.map(v => v.toFixed(2)).join(', ')}]`);
			lines.push(`AABB max: [${s.max.map(v => v.toFixed(2)).join(', ')}]`);
		}
		lines.push(`logDepth: ${info.rendererInfo?.logDepthBuffer ?? 'N/A'}`);
		showToast('info', lines.join('\n'), 8000);
	}

	function handleToggleWireframeDebug() {
		const count = window.__waffle?.toggleWireframe?.();
		showToast('info', count != null ? `Wireframe toggled on ${count} materials` : 'No meshes found');
	}

	function handleToggleShaderDebug() {
		if (!window.__waffle) return;
		window.__waffle.shaderDebug = !window.__waffle.shaderDebug;
		showToast('info', `Shader debug: ${window.__waffle.shaderDebug ? 'ON' : 'OFF'}`);
	}

	onMount(() => {
		/** @param {KeyboardEvent} e */
		function onKeyDown(e) {
			if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
			if (!ready) return;

			if (e.ctrlKey || e.metaKey) {
				if (e.key === 's') { e.preventDefault(); saveToStorage(); return; }
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
				case 'g': if (inSketch) setActiveTool('gear'); break;
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
						// A selected constraint badge takes priority over entities.
						if (getSelectedConstraintIndex() != null) {
							e.preventDefault();
							deleteSelectedConstraint();
							break;
						}
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

	<button
		class="toolbar-btn home-btn"
		data-testid="toolbar-btn-home"
		title="Home"
		onclick={() => goto(`${base}/home`)}
	>Home</button>

	<div class="project-name" data-testid="project-name">
		{#if editingName}
			<input
				class="name-input"
				bind:value={nameInputValue}
				onblur={() => { if (nameInputValue.trim()) { setProjectName(nameInputValue.trim()); saveToStorage(); } editingName = false; }}
				onkeydown={(e) => { if (e.key === 'Enter') { if (nameInputValue.trim()) { setProjectName(nameInputValue.trim()); saveToStorage(); } editingName = false; } else if (e.key === 'Escape') { editingName = false; } }}
			/>
		{:else}
			<button class="name-btn" ondblclick={() => { nameInputValue = name; editingName = true; }} title="Double-click to rename">
				{name}
			</button>
		{/if}
	</div>

	{#if inSketch}
		<!-- Sketch mode tools -->
		{#if isMobile}
			<!-- Mobile: sketch tools in dropdown -->
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-sketch-tools-dropdown"
					onclick={(e) => { openDropdown(e.currentTarget, () => { showSketchTools = !showSketchTools; showConstraints = false; }); }}
				>Tools ▾</button>
			</div>
			{#if showSketchTools}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="dropdown-backdrop" onclick={() => showSketchTools = false} onpointerdown={(e) => e.stopPropagation()}></div>
				<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="dropdown-panel dropdown-fixed" style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;" data-testid="sketch-tools-dropdown"
				onpointerdown={(e) => e.stopPropagation()}
			>
					<div class="dropdown-grid">
						{#each sketchTools as t}
							<button
								class="toolbar-btn"
								class:active={t.id !== 'construction' && tool === t.id}
								disabled={!ready}
								title="{t.label}{t.shortcut ? ` (${t.shortcut})` : ''}"
								data-testid="toolbar-btn-{t.id}"
								onclick={() => { t.id === 'construction' ? handleToggleConstruction() : setActiveTool(t.id); showSketchTools = false; }}
							>{t.label}</button>
						{/each}
					</div>
				</div>
			{/if}
		{:else}
			<!-- Desktop: sketch tools inline -->
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
		{/if}
		<div class="toolbar-sep"></div>
		<!-- Constraints dropdown (always a dropdown) -->
		{#if isMobile}
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-constraints-dropdown"
					onclick={(e) => { openDropdown(e.currentTarget, () => { showConstraints = !showConstraints; showSketchTools = false; }); }}
				>Constraints ▾</button>
			</div>
			{#if showConstraints}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="dropdown-backdrop" onclick={() => showConstraints = false} onpointerdown={(e) => e.stopPropagation()}></div>
				<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="dropdown-panel dropdown-fixed constraints-panel" style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;" data-testid="constraints-dropdown"
				onpointerdown={(e) => e.stopPropagation()}
			>
					<div class="dropdown-grid constraints-grid">
						{#each constraintButtons as cb}
							<button
								class="constraint-btn"
								disabled={!applicable[cb.id] && !isModalConstraint(cb.id)}
								title={cb.title}
								data-testid="toolbar-constraint-{cb.id}"
								onclick={() => { applyConstraint(cb.id); showConstraints = false; }}
							>{cb.label}</button>
						{/each}
					</div>
				</div>
			{/if}
		{:else}
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-constraints-dropdown"
					onclick={() => { showConstraints = !showConstraints; showSketchTools = false; }}
				>Constraints ▾</button>
				{#if showConstraints}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="dropdown-backdrop" onclick={() => showConstraints = false} onpointerdown={(e) => e.stopPropagation()}></div>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="dropdown-panel constraints-panel" data-testid="constraints-dropdown"
						onpointerdown={(e) => e.stopPropagation()}
					>
						<div class="dropdown-grid constraints-grid">
							{#each constraintButtons as cb}
								<button
									class="constraint-btn"
									disabled={!applicable[cb.id] && !isModalConstraint(cb.id)}
									title={cb.title}
									data-testid="toolbar-constraint-{cb.id}"
									onclick={() => { applyConstraint(cb.id); showConstraints = false; }}
								>{cb.label}</button>
							{/each}
						</div>
					</div>
				{/if}
			</div>
		{/if}
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
			class:dof-ok={solveStatus.dof === 0 && solveStatus.status === 'FullyConstrained'}
			class:dof-under={solveStatus.dof > 0 && solveStatus.status === 'UnderConstrained'}
			class:dof-over={solveStatus.status === 'OverConstrained'}
			class:dof-redundant={solveStatus.status === 'SolveFailed'}
			data-testid="dof-badge"
			title={solveStatus.status === 'OverConstrained' ? 'Over-constrained: conflicting constraints detected'
				: solveStatus.status === 'SolveFailed' ? 'Solver did not converge (possible redundant constraints)'
				: solveStatus.dof === 0 ? 'Fully constrained'
				: `${solveStatus.dof} degrees of freedom remaining`}
		>
			{#if solveStatus.status === 'OverConstrained'}
				Over-constrained
			{:else if solveStatus.status === 'SolveFailed'}
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
		{#if isMobile}
			<!-- Mobile: modeling tools in dropdown -->
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-modeling-dropdown"
					onclick={(e) => { openDropdown(e.currentTarget, () => { showModelingTools = !showModelingTools; }); }}
				>Model ▾</button>
			</div>
			{#if showModelingTools}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="dropdown-backdrop" onclick={() => showModelingTools = false} onpointerdown={(e) => e.stopPropagation()}></div>
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="dropdown-panel dropdown-fixed" style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;" data-testid="modeling-tools-dropdown"
					onpointerdown={(e) => e.stopPropagation()}
				>
					<div class="dropdown-grid">
						{#each modelingTools as t}
							<button
								class="toolbar-btn"
								class:active={tool === t.id}
								disabled={!ready}
								title="{t.label}{t.shortcut ? ` (${t.shortcut})` : ''}"
								data-testid="toolbar-btn-{t.id}"
								onclick={async () => { await handleToolClick(t.id); showModelingTools = false; }}
							>{t.label}</button>
						{/each}
					</div>
				</div>
			{/if}
		{:else}
			<!-- Desktop: modeling tools inline -->
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
			<div class="toolbar-sep"></div>
			<div class="toolbar-group">
				<button
					class="toolbar-btn"
					class:active={section.active}
					disabled={!ready}
					title="Section view — clip the model at the selected plane/face (capped)"
					data-testid="toolbar-btn-section"
					onclick={handleToggleSection}
				>Section</button>
				{#if section.active}
					<button
						class="toolbar-btn"
						title="Flip which half is kept"
						data-testid="toolbar-btn-section-flip"
						onclick={() => flipSection()}
					>Flip</button>
					<label class="section-offset" title="Move the cut along the plane normal">
						<input
							type="range"
							min="-0.1"
							max="0.1"
							step="0.001"
							value={section.offset}
							data-testid="section-offset"
							oninput={(e) => setSectionOffset(parseFloat(e.currentTarget.value))}
						/>
					</label>
					<button
						class="toolbar-btn"
						title="Exit section view"
						data-testid="toolbar-btn-section-clear"
						onclick={() => clearSection()}
					>Clear</button>
				{/if}
			</div>
		{/if}
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
			<button class="toolbar-btn overflow-trigger" title="More actions" onclick={(e) => openOverflow(e.currentTarget)}
				data-testid="toolbar-btn-overflow">
				&#x22EE;
			</button>
		</div>
		{#if showOverflow}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="overflow-backdrop" onclick={closeOverflow}></div>
			<div class="overflow-menu overflow-fixed" style="top: {overflowPos.top}px; right: {overflowPos.right}px;" data-testid="toolbar-overflow-menu">
					<button class="overflow-item" disabled={!ready || saving}
						data-testid="toolbar-btn-save"
						onclick={async () => { closeOverflow(); saving = true; try { await saveToStorage(); } finally { saving = false; } }}>
						{saving ? 'Saving...' : 'Save'}
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-export-waffle"
						onclick={async () => { closeOverflow(); await saveProject(); }}>
						Export .waffle
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
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-assay"
						onclick={() => { closeOverflow(); toggleAssayBrowser(); }}>Assay</button>
					<div class="overflow-separator"></div>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-viewport"
						onclick={() => { closeOverflow(); handleViewportDebug(); }}>Viewport Info</button>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-wireframe"
						onclick={() => { closeOverflow(); handleToggleWireframeDebug(); }}>Toggle Wireframe</button>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-shader"
						onclick={() => { closeOverflow(); handleToggleShaderDebug(); }}>Toggle Shader Debug</button>
					<div class="overflow-separator"></div>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-toggle-planes"
						onclick={() => { closeOverflow(); toggleDatumPlanes(); }}>
						{getShowDatumPlanes() ? '✓ ' : ''}Planes
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-toggle-axes"
						onclick={() => { closeOverflow(); toggleOriginTriad(); }}>
						{getShowOriginTriad() ? '✓ ' : ''}Axes
					</button>
					<div class="overflow-separator"></div>
					<button class="overflow-item"
						data-testid="toolbar-btn-reload"
						onclick={() => { closeOverflow(); location.reload(); }}>Reload</button>
				</div>
		{/if}
	{:else}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" disabled={!ready || saving} title="Save (Ctrl+S)"
				data-testid="toolbar-btn-save"
				onclick={async () => { saving = true; try { await saveToStorage(); } finally { saving = false; } }}>
				{saving ? 'Saving...' : 'Save'}
			</button>
			<button class="toolbar-btn" disabled={!ready} title="Open (Ctrl+O)"
				data-testid="toolbar-btn-open"
				onclick={() => loadProject()}>Open</button>
			<button class="toolbar-btn" disabled={!ready} title="Download a .waffle file (Save persists to the browser)"
				data-testid="toolbar-btn-export-waffle-main"
				onclick={async () => { await saveProject(); }}>Export .waffle</button>
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
			<button class="toolbar-btn" disabled={!ready} title="Assay Browser"
				data-testid="toolbar-btn-assay"
				onclick={() => toggleAssayBrowser()}>Assay</button>
			<div class="dropdown-container">
				<button class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-debug-dropdown"
					onclick={() => { showDebugMenu = !showDebugMenu; }}
				>Debug ▾</button>
				{#if showDebugMenu}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="dropdown-backdrop" onclick={() => showDebugMenu = false}></div>
					<div class="dropdown-panel" data-testid="debug-dropdown">
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-viewport"
							onclick={() => { showDebugMenu = false; handleViewportDebug(); }}>Viewport Info</button>
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-wireframe"
							onclick={() => { showDebugMenu = false; handleToggleWireframeDebug(); }}>Toggle Wireframe</button>
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-shader"
							onclick={() => { showDebugMenu = false; handleToggleShaderDebug(); }}>Toggle Shader Debug</button>
						<div class="build-info" data-testid="build-info"
							title="Bundle build provenance (vite define __BUILD_INFO__)">
							Build {__BUILD_INFO__.date} ({__BUILD_INFO__.commit})
						</div>
					</div>
				{/if}
			</div>
		</div>
	{/if}

	<div class="toolbar-spacer"></div>
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
		position: relative;
		display: flex;
		align-items: center;
		height: 100%;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-color);
		padding: 0 max(8px, env(safe-area-inset-right, 0px)) 0 max(8px, env(safe-area-inset-left, 0px));
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

	.home-btn {
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
		max-width: calc(100vw - 16px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px));
		padding: 4px 0;
	}

	.overflow-fixed {
		position: fixed;
	}

	.overflow-separator {
		height: 1px;
		background: var(--border-color);
		margin: 4px 0;
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

	/* Dropdown containers for collapsible toolbar sections */
	.dropdown-container {
		position: relative;
	}

	.dropdown-trigger {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	.dropdown-backdrop {
		position: fixed;
		inset: 0;
		z-index: 199;
	}

	.dropdown-panel {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 200;
		padding: 8px;
		max-width: calc(100vw - 16px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px));
	}

	.dropdown-fixed {
		position: fixed;
	}

	.dropdown-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 4px;
	}

	.dropdown-grid .toolbar-btn,
	.dropdown-grid .constraint-btn {
		min-width: 60px;
		text-align: center;
		padding: 6px 8px;
	}

	.constraints-panel {
		min-width: min(220px, calc(100vw - 16px - env(safe-area-inset-left, 0px) - env(safe-area-inset-right, 0px)));
	}

	.constraints-grid {
		grid-template-columns: repeat(3, 1fr);
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
	}

	@media (max-width: 480px) {
		.toolbar-brand {
			display: none;
		}

		.project-name {
			max-width: 80px;
			overflow: hidden;
		}

		.project-name .name-btn {
			font-size: 10px;
			padding: 2px 4px;
			white-space: nowrap;
			overflow: hidden;
			text-overflow: ellipsis;
			max-width: 80px;
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

	@media (max-width: 960px) and (orientation: landscape) {
		.toolbar-brand {
			display: none;
		}

		.project-name {
			display: none;
		}

		.toolbar-btn {
			padding: 4px 6px;
			font-size: 11px;
			min-height: 28px;
		}

		.constraint-btn {
			padding: 3px 5px;
			font-size: 10px;
			min-height: 28px;
		}
	}
	.section-offset {
		display: flex;
		align-items: center;
		padding: 0 4px;
	}

	.section-offset input[type='range'] {
		width: 80px;
	}

	.build-info {
		padding: 6px 12px;
		font-size: 11px;
		color: #8a8f98;
		border-top: 1px solid rgba(255, 255, 255, 0.08);
		margin-top: 4px;
		user-select: text;
	}
</style>
