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
		toggleConstruction,
		finishSketch,
		showExtrudeDialog,
		showRevolveDialog,
		saveProject,
		loadProject
	} from '$lib/engine/store.svelte.js';
	import { resetTool } from '$lib/sketch/tools.js';
	import { onMount } from 'svelte';

	let ready = $derived(isEngineReady());
	let tool = $derived(getActiveTool());
	let inSketch = $derived(getSketchMode()?.active ?? false);

	const modelingTools = [
		{ id: 'sketch', label: 'Sketch', shortcut: 'S' },
		{ id: 'extrude', label: 'Extrude', shortcut: 'E' },
		{ id: 'revolve', label: 'Revolve', shortcut: '' },
		{ id: 'fillet', label: 'Fillet', shortcut: '' },
		{ id: 'chamfer', label: 'Chamfer', shortcut: '' },
		{ id: 'shell', label: 'Shell', shortcut: '' },
	];

	const sketchTools = [
		{ id: 'select', label: 'Select', shortcut: '' },
		{ id: 'line', label: 'Line', shortcut: 'L' },
		{ id: 'rectangle', label: 'Rect', shortcut: 'R' },
		{ id: 'circle', label: 'Circle', shortcut: 'C' },
		{ id: 'arc', label: 'Arc', shortcut: 'A' },
		{ id: 'construction', label: 'Constr', shortcut: 'X' },
	];

	function handleToolClick(toolId) {
		if (toolId === 'sketch') {
			if (inSketch) {
				handleFinishSketch();
			} else {
				enterSketchMode([0, 0, 0], [0, 0, 1]);
				setActiveTool('line');
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

	function handleFinishSketch() {
		finishSketch().catch(() => {});
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
				return;
			}

			switch (e.key) {
				case 's': handleToolClick('sketch'); break;
				case 'e': handleToolClick('extrude'); break;
				case 'l': if (inSketch) setActiveTool('line'); break;
				case 'r': if (inSketch) setActiveTool('rectangle'); break;
				case 'c': if (inSketch) setActiveTool('circle'); break;
				case 'a': if (inSketch) setActiveTool('arc'); break;
				case 'x': if (inSketch) handleToggleConstruction(); break;
				case 'Escape':
					if (inSketch) {
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
					// Handled by feature tree
					break;
			}
		}
		window.addEventListener('keydown', onKeyDown);
		return () => window.removeEventListener('keydown', onKeyDown);
	});
</script>

<div class="toolbar">
	<div class="toolbar-brand">Waffle Iron</div>

	{#if inSketch}
		<!-- Sketch mode tools -->
		<div class="toolbar-group">
			{#each sketchTools as t}
				<button
					class="toolbar-btn"
					class:active={t.id !== 'construction' && tool === t.id}
					disabled={!ready}
					title="{t.label}{t.shortcut ? ` (${t.shortcut})` : ''}"
					onclick={() => t.id === 'construction' ? handleToggleConstruction() : setActiveTool(t.id)}
				>{t.label}</button>
			{/each}
		</div>
		<div class="toolbar-sep"></div>
		<button class="toolbar-btn finish-btn" onclick={handleFinishSketch}>
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
					onclick={() => handleToolClick(t.id)}
				>{t.label}</button>
			{/each}
		</div>
	{/if}

	<div class="toolbar-sep"></div>
	<div class="toolbar-group">
		<button class="toolbar-btn" disabled={!ready} title="Undo (Ctrl+Z)" onclick={undo}>Undo</button>
		<button class="toolbar-btn" disabled={!ready} title="Redo (Ctrl+Shift+Z)" onclick={redo}>Redo</button>
	</div>
	<div class="toolbar-sep"></div>
	<div class="toolbar-group">
		<button class="toolbar-btn" disabled={!ready} title="Save (Ctrl+S)" onclick={() => saveProject()}>Save</button>
		<button class="toolbar-btn" disabled={!ready} title="Open (Ctrl+O)" onclick={() => loadProject()}>Open</button>
	</div>

	<div class="toolbar-spacer"></div>
	<div class="toolbar-status">
		{#if ready}
			<span class="status-dot ready"></span>
		{:else}
			<span class="status-dot loading"></span>
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
		padding: 0 8px;
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
</style>
