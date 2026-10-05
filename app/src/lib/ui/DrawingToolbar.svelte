<script>
	/**
	 * The drawing tab's toolbar (`specs/drawings_and_mbd.md` §8, D4d) — the
	 * dimensioning tools, where the modelling toolbar's Sketch/Extrude group
	 * sits on a Part tab.
	 *
	 * It REPLACES `Toolbar.svelte` rather than sitting beside it, the way
	 * `AssemblyPanel` replaces `FeatureTree` and `DrawingSheet` replaces the
	 * viewport: a drawing has no sketch to enter and no solid to extrude, and
	 * two toolbars would cost a second row of the one piece of chrome the
	 * window has least of. It therefore carries the document actions too
	 * (Home, the name, Undo/Redo, Save/Open/Export, Settings) and keeps the
	 * same `data-testid="toolbar"`, so every width oracle and every
	 * `clickToolbarAction` helper reaches it unchanged.
	 *
	 * ## The tool list is DATA
	 *
	 * One array of descriptors (`DRAWING_TOOLS`), read by the inline group,
	 * the "More ▾" overflow and the collapse ladder alike. Adding a tool is
	 * one entry — which is the arrangement D4e needs: its `+ view` and
	 * projected-view tools are two more rows here and two more arms of the
	 * sheet's mode dispatch, with nothing to change in the modes that already
	 * work.
	 *
	 * ## The collapse ladder
	 *
	 * The same mechanism as `Toolbar.svelte`'s, and for the same reason
	 * (CLAUDE.md, "Chrome must scroll or collapse, never overflow"; the oracle
	 * is `tests/gui/layout-overflow.spec.js`). After every change to its
	 * content or width it MEASURES whether its in-flow items fit and collapses
	 * one rung at a time until they do. Rungs, in order:
	 *
	 *   1  file / export group        → the ⋮ overflow menu
	 *   2  hide the brand
	 *   3  hide the project name
	 *   4  Undo / Redo                → the ⋮ overflow menu
	 *   5… trailing tools, one per rung → a "More ▾" dropdown (priority+);
	 *      when every tool is in it, that IS the single-dropdown mobile layout
	 *
	 * The mobile breakpoint forces rung 1 and the full tool dropdown outright
	 * (touch targets, not width). Rung n+1 is never wider than rung n except
	 * the first "More ▾" step (the trigger costs more than one tool), which
	 * the ladder walks straight through — so it cannot oscillate.
	 */
	import {
		isEngineReady,
		getMobileLayout,
		getProjectName,
		setProjectName,
		getSheetMode,
		setSheetMode,
		getSheetPicks,
		getSheetSelection,
		getDrawingHistoryDepth,
		deleteSheetAnnotation,
		popSheetPick,
		undo,
		redo,
		saveProject,
		saveToStorage,
		loadProject,
		toggleExamplesBrowser,
		getAgentActivity,
		setToolHint,
		AGENT_WORKING_HINT
	} from '$lib/engine/store.svelte.js';
	import { TOOL_FLOW } from '$lib/drawings/pick.js';
	import SettingsModal from './SettingsModal.svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { onMount, flushSync } from 'svelte';

	/**
	 * The sheet's tools, in toolbar order.
	 *
	 * `mode` is the `sheetMode` the tool switches to, which is also the key
	 * into `TOOL_FLOW` (how many anchors it takes and whether it ends with a
	 * placement click) — one name for one tool across the toolbar, the pointer
	 * dispatch and the specs.
	 *
	 * There is one entry per authorable `DIMENSION_TAGS` kind. `Ordinate` is
	 * deliberately absent: it reads one raw view-plane coordinate measured
	 * from the view FRAME's origin, so its printed value cannot be read off
	 * the sheet and changes when the part moves in space (§7's open item). It
	 * joins when that closes, as one more row.
	 *
	 * @type {{ id: string, mode: string, label: string, title: string }[]}
	 */
	const DRAWING_TOOLS = [
		{
			id: 'select',
			mode: 'select',
			label: 'Select',
			title: 'Pick, drag and delete the dimensions already on the sheet'
		},
		{
			id: 'dim-distance',
			mode: 'dimension-distance',
			label: 'Dist',
			title: 'Aligned distance between two entities, measured along the line joining them (across two parallel edges)'
		},
		{
			id: 'dim-hdistance',
			mode: 'dimension-hdistance',
			label: 'Horiz',
			title: 'Horizontal distance between two entities, measured along the view u axis'
		},
		{
			id: 'dim-vdistance',
			mode: 'dimension-vdistance',
			label: 'Vert',
			title: 'Vertical distance between two entities, measured along the view v axis'
		},
		{
			id: 'dim-pointline',
			mode: 'dimension-pointline',
			label: 'Pt-Ln',
			title: 'Perpendicular distance from a point to a line: pick the point, then the line'
		},
		{
			id: 'dim-angle',
			mode: 'dimension-angle',
			label: 'Angle',
			title: 'Angle between two straight edges (the acute one — a projection carries no edge direction)'
		},
		{
			id: 'dim-radius',
			mode: 'dimension-radius',
			label: 'Radius',
			title: 'Radius of a circular rim'
		},
		{
			id: 'dim-diameter',
			mode: 'dimension-diameter',
			label: 'Diam',
			title: 'Diameter of a circular rim, drawn through the centre'
		},
		{
			id: 'note',
			mode: 'note',
			label: 'Note',
			title: 'A note with a leader: pick what it points at, then place the text'
		},
		{
			id: 'datum',
			mode: 'datum',
			label: 'Datum',
			title: 'A datum label: pick the feature, then place the boxed letter'
		}
	];

	let ready = $derived(isEngineReady());
	let agentBusy = $derived(getAgentActivity() !== null);
	let isMobile = $derived(getMobileLayout());
	let mode = $derived(getSheetMode());
	let picks = $derived(getSheetPicks());
	let selection = $derived(getSheetSelection());
	let name = $derived(getProjectName());
	let history = $derived(getDrawingHistoryDepth());

	/**
	 * The one-line prompt for where the active tool's flow has got to.
	 *
	 * Bounded text by construction (a count and a short verb), and the slot it
	 * sits in is width-capped as well — a toolbar whose width followed a
	 * message would re-walk the collapse ladder on every click.
	 */
	let prompt = $derived.by(() => {
		const flow = TOOL_FLOW[mode];
		if (!flow) {
			return selection ? `dimension ${selection.index + 1} selected` : 'select';
		}
		if (picks.length < flow.anchors) {
			return `pick ${picks.length + 1} of ${flow.anchors}`;
		}
		return flow.placement ? 'click to place' : 'placing…';
	});

	// ── Overflow-driven collapse (see the header comment) ────────────────
	const FIXED_RUNGS = 4;
	let collapseLevel = $state(0);
	let collapseMax = $derived(FIXED_RUNGS + DRAWING_TOOLS.length);
	let compactFile = $derived(isMobile || collapseLevel >= 1);
	let hideBrand = $derived(collapseLevel >= 2);
	let hideName = $derived(collapseLevel >= 3);
	let compactHistory = $derived(collapseLevel >= 4);
	let toolsHidden = $derived(
		Math.max(0, Math.min(DRAWING_TOOLS.length, collapseLevel - FIXED_RUNGS))
	);
	let compactTools = $derived(isMobile || toolsHidden >= DRAWING_TOOLS.length);
	let inlineTools = $derived(DRAWING_TOOLS.slice(0, DRAWING_TOOLS.length - toolsHidden));
	let overflowTools = $derived(DRAWING_TOOLS.slice(DRAWING_TOOLS.length - toolsHidden));

	let showMoreTools = $state(false);
	let showDrawingTools = $state(false);
	let showOverflow = $state(false);
	let settingsOpen = $state(false);
	let saving = $state(false);
	let editingName = $state(false);
	let nameInputValue = $state('');

	let dropdownPos = $state({ top: 0, left: 0 });
	let overflowPos = $state({ top: 0, right: 0 });

	/** @type {HTMLDivElement | null} */
	let toolbarEl = $state(null);
	let relayoutQueued = false;
	/** Counts completed ladder walks — see `Toolbar.svelte`'s own note: a test
	 *  that resizes the window must wait on this rather than on a fixed delay. */
	let relayoutSeq = $state(0);

	/** Does the in-flow content fit? Identical sum to `Toolbar.svelte`'s, and
	 *  to the one `layout-overflow.spec.js` recomputes to wait on. */
	function contentFits() {
		if (!toolbarEl) return true;
		const cs = getComputedStyle(toolbarEl);
		const avail = toolbarEl.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
		const gap = parseFloat(cs.columnGap) || 0;
		let needed = 0;
		let count = 0;
		for (const child of toolbarEl.children) {
			const ccs = getComputedStyle(child);
			if (ccs.display === 'none' || ccs.position === 'fixed' || ccs.position === 'absolute') continue;
			count++;
			if (child.classList.contains('toolbar-spacer')) continue;
			needed +=
				child.getBoundingClientRect().width +
				(parseFloat(ccs.marginLeft) || 0) +
				(parseFloat(ccs.marginRight) || 0);
		}
		needed += gap * Math.max(0, count - 1);
		return needed <= avail + 0.5;
	}

	function relayout() {
		if (!toolbarEl) return;
		while (collapseLevel > 0) {
			collapseLevel--;
			flushSync();
			if (!contentFits()) {
				collapseLevel++;
				flushSync();
				break;
			}
		}
		while (collapseLevel < collapseMax && !contentFits()) {
			collapseLevel++;
			flushSync();
		}
		relayoutSeq++;
	}

	function scheduleRelayout() {
		if (relayoutQueued) return;
		relayoutQueued = true;
		queueMicrotask(() => {
			relayoutQueued = false;
			relayout();
		});
	}

	$effect(() => {
		// Everything that changes the in-flow content re-measures. `prompt` is
		// read because the slot's text changes with it (its width is capped,
		// but the measure is cheap and a capped slot that is EMPTY is narrower).
		void [ready, agentBusy, isMobile, name, editingName, saving, prompt];
		scheduleRelayout();
	});

	$effect(() => {
		if (!toolbarEl) return;
		const ro = new ResizeObserver(() => scheduleRelayout());
		ro.observe(toolbarEl);
		document.fonts?.ready?.then(() => scheduleRelayout());
		return () => ro.disconnect();
	});

	function openDropdown(triggerEl, setState) {
		const rect = triggerEl.getBoundingClientRect();
		dropdownPos = { top: rect.bottom + 4, left: rect.left };
		setState();
	}

	function openOverflow(triggerEl) {
		const rect = triggerEl.getBoundingClientRect();
		const saiRight =
			parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--sai-right')) || 0;
		overflowPos = { top: rect.bottom + 4, right: Math.max(4, window.innerWidth - rect.right - saiRight) };
		showOverflow = !showOverflow;
	}

	function closeOverflow() {
		showOverflow = false;
	}

	function pickTool(tool) {
		if (agentBusy) {
			setToolHint(AGENT_WORKING_HINT);
			return;
		}
		setSheetMode(tool.mode);
	}

	onMount(() => {
		/** @param {KeyboardEvent} e */
		function onKeyDown(e) {
			if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
			if (!ready) return;
			if (e.ctrlKey || e.metaKey) {
				if (e.key === 's') {
					e.preventDefault();
					saveToStorage();
					return;
				}
				if (e.key === 'o') {
					e.preventDefault();
					loadProject();
					return;
				}
				if (e.key === 'z' && !e.shiftKey) {
					e.preventDefault();
					undo();
					return;
				}
				if ((e.key === 'z' && e.shiftKey) || e.key === 'Z') {
					e.preventDefault();
					redo();
					return;
				}
				return;
			}
			if (e.key === 'Escape') {
				// One pick at a time, then the tool itself — the same
				// back-out-one-step Escape the sketch tools have.
				e.preventDefault();
				popSheetPick();
				return;
			}
			if (e.key === 'Delete' || e.key === 'Backspace') {
				const sel = getSheetSelection();
				if (sel) {
					e.preventDefault();
					deleteSheetAnnotation(sel.viewId, sel.index);
				}
			}
		}
		window.addEventListener('keydown', onKeyDown);
		return () => window.removeEventListener('keydown', onKeyDown);
	});
</script>

<div
	class="toolbar"
	data-testid="toolbar"
	data-toolbar="drawing"
	data-collapse-level={collapseLevel}
	data-relayout-seq={relayoutSeq}
	data-sheet-mode={mode}
	data-sheet-picks={picks.length}
	bind:this={toolbarEl}
>
	{#if !hideBrand}
		<div class="toolbar-brand">Waffle Iron</div>
	{/if}

	<button
		class="toolbar-btn home-btn"
		data-testid="toolbar-btn-home"
		title="Home"
		onclick={() => goto(`${base}/home`)}>Home</button
	>

	{#if !hideName}
		<div class="project-name" data-testid="project-name">
			{#if editingName}
				<input
					class="name-input"
					bind:value={nameInputValue}
					onblur={() => {
						if (nameInputValue.trim()) {
							setProjectName(nameInputValue.trim());
							saveToStorage();
						}
						editingName = false;
					}}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							if (nameInputValue.trim()) {
								setProjectName(nameInputValue.trim());
								saveToStorage();
							}
							editingName = false;
						} else if (e.key === 'Escape') {
							editingName = false;
						}
					}}
				/>
			{:else}
				<button
					class="name-btn"
					ondblclick={() => {
						nameInputValue = name;
						editingName = true;
					}}
					title="Double-click to rename">{name}</button
				>
			{/if}
		</div>
	{/if}

	{#if compactTools}
		<!-- Every tool in one dropdown: the mobile layout, and the last rung
		     of the ladder on a very narrow desktop window. -->
		<div class="dropdown-container">
			<button
				class="toolbar-btn dropdown-trigger"
				data-testid="toolbar-btn-drawing-tools-dropdown"
				onclick={(e) => openDropdown(e.currentTarget, () => (showDrawingTools = !showDrawingTools))}
				>Dimension ▾</button
			>
		</div>
		{#if showDrawingTools}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="dropdown-backdrop"
				onclick={() => (showDrawingTools = false)}
				onpointerdown={(e) => e.stopPropagation()}
			></div>
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div
				class="dropdown-panel dropdown-fixed"
				style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;"
				data-testid="drawing-tools-dropdown"
				onpointerdown={(e) => e.stopPropagation()}
			>
				<div class="dropdown-grid">
					{#each DRAWING_TOOLS as t (t.id)}
						<button
							class="toolbar-btn"
							class:active={mode === t.mode}
							disabled={!ready}
							title={t.title}
							data-testid="toolbar-btn-{t.id}"
							onclick={() => {
								pickTool(t);
								showDrawingTools = false;
							}}>{t.label}</button
						>
					{/each}
				</div>
			</div>
		{/if}
	{:else}
		<div class="toolbar-group">
			{#each inlineTools as t (t.id)}
				<button
					class="toolbar-btn"
					class:active={mode === t.mode}
					disabled={!ready}
					title={t.title}
					data-testid="toolbar-btn-{t.id}"
					onclick={() => pickTool(t)}>{t.label}</button
				>
			{/each}
			{#if overflowTools.length > 0}
				<div class="dropdown-container">
					<button
						class="toolbar-btn dropdown-trigger"
						data-testid="toolbar-btn-more-tools"
						title="More dimension tools"
						onclick={(e) => openDropdown(e.currentTarget, () => (showMoreTools = !showMoreTools))}
						>More ▾</button
					>
				</div>
				{#if showMoreTools}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="dropdown-backdrop"
						onclick={() => (showMoreTools = false)}
						onpointerdown={(e) => e.stopPropagation()}
					></div>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="dropdown-panel dropdown-fixed"
						style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;"
						data-testid="more-tools-dropdown"
						onpointerdown={(e) => e.stopPropagation()}
					>
						<div class="dropdown-grid">
							{#each overflowTools as t (t.id)}
								<button
									class="toolbar-btn"
									class:active={mode === t.mode}
									disabled={!ready}
									title={t.title}
									data-testid="toolbar-btn-{t.id}"
									onclick={() => {
										showMoreTools = false;
										pickTool(t);
									}}>{t.label}</button
								>
							{/each}
						</div>
					</div>
				{/if}
			{/if}
		</div>
	{/if}

	<div class="toolbar-sep"></div>
	<!-- Width-capped: the prompt's text changes on every pick, and a toolbar
	     whose width followed a message would re-walk the ladder each time. -->
	<span class="prompt-slot" data-testid="sheet-prompt" title="Where the active tool's flow has got to"
		>{prompt}</span
	>

	{#if !compactHistory}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button
				class="toolbar-btn"
				data-testid="toolbar-btn-undo"
				disabled={!ready || agentBusy}
				title="Undo (Ctrl+Z) — {history.undo} drawing edit{history.undo === 1 ? '' : 's'} on this tab"
				onclick={undo}>Undo</button
			>
			<button
				class="toolbar-btn"
				data-testid="toolbar-btn-redo"
				disabled={!ready || agentBusy}
				title="Redo (Ctrl+Shift+Z)"
				onclick={redo}>Redo</button
			>
		</div>
	{/if}

	{#if compactFile}
		<div class="toolbar-sep"></div>
		<div class="overflow-container">
			<button
				class="toolbar-btn overflow-trigger"
				title="More actions"
				data-testid="toolbar-btn-overflow"
				onclick={(e) => openOverflow(e.currentTarget)}>&#x22EE;</button
			>
		</div>
		{#if showOverflow}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="overflow-backdrop" onclick={closeOverflow}></div>
			<div
				class="overflow-menu overflow-fixed"
				style="top: {overflowPos.top}px; right: {overflowPos.right}px;"
				data-testid="toolbar-overflow-menu"
			>
				{#if compactHistory}
					<button
						class="overflow-item"
						disabled={!ready || agentBusy}
						data-testid="toolbar-btn-undo"
						title="Undo (Ctrl+Z)"
						onclick={() => {
							closeOverflow();
							undo();
						}}>Undo</button
					>
					<button
						class="overflow-item"
						disabled={!ready || agentBusy}
						data-testid="toolbar-btn-redo"
						title="Redo (Ctrl+Shift+Z)"
						onclick={() => {
							closeOverflow();
							redo();
						}}>Redo</button
					>
					<div class="overflow-separator"></div>
				{/if}
				<button
					class="overflow-item"
					disabled={!ready || saving}
					data-testid="toolbar-btn-save"
					onclick={async () => {
						closeOverflow();
						saving = true;
						try {
							await saveToStorage();
						} finally {
							saving = false;
						}
					}}>{saving ? 'Saving...' : 'Save'}</button
				>
				<button
					class="overflow-item"
					disabled={!ready}
					data-testid="toolbar-btn-export-waffle-main"
					onclick={async () => {
						closeOverflow();
						await saveProject();
					}}>Export .waffle</button
				>
				<button
					class="overflow-item"
					disabled={!ready || agentBusy}
					data-testid="toolbar-btn-open"
					onclick={() => {
						closeOverflow();
						loadProject();
					}}>Open</button
				>
				<button
					class="overflow-item"
					disabled={!ready}
					data-testid="toolbar-btn-examples"
					onclick={() => {
						closeOverflow();
						toggleExamplesBrowser();
					}}>Examples</button
				>
				<div class="overflow-separator"></div>
				<button
					class="overflow-item"
					data-testid="toolbar-btn-reload"
					onclick={() => {
						closeOverflow();
						location.reload();
					}}>Reload</button
				>
			</div>
		{/if}
	{:else}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button
				class="toolbar-btn"
				disabled={!ready || saving}
				title="Save (Ctrl+S)"
				data-testid="toolbar-btn-save"
				onclick={async () => {
					saving = true;
					try {
						await saveToStorage();
					} finally {
						saving = false;
					}
				}}>{saving ? 'Saving...' : 'Save'}</button
			>
			<button
				class="toolbar-btn"
				disabled={!ready || agentBusy}
				title="Open (Ctrl+O)"
				data-testid="toolbar-btn-open"
				onclick={() => loadProject()}>Open</button
			>
			<button
				class="toolbar-btn"
				disabled={!ready}
				title="Download a .waffle file (Save persists to the browser)"
				data-testid="toolbar-btn-export-waffle-main"
				onclick={async () => {
					await saveProject();
				}}>Export .waffle</button
			>
			<button
				class="toolbar-btn"
				disabled={!ready}
				title="Official example documents"
				data-testid="toolbar-btn-examples"
				onclick={() => toggleExamplesBrowser()}>Examples</button
			>
		</div>
	{/if}

	<div class="toolbar-spacer"></div>
	<button
		class="toolbar-btn settings-btn"
		title="Settings"
		aria-label="Settings"
		data-testid="toolbar-btn-settings"
		onclick={() => (settingsOpen = true)}>&#x2699;</button
	>
	<div class="toolbar-status">
		{#if ready}
			<span class="status-dot ready" data-testid="status-dot"></span>
		{:else}
			<span class="status-dot loading" data-testid="status-dot"></span>
		{/if}
	</div>
</div>

{#if settingsOpen}
	<SettingsModal onclose={() => (settingsOpen = false)} />
{/if}

<style>
	/* Deliberately the same rules as `Toolbar.svelte`'s, scoped to this
	   component: the two toolbars are one piece of chrome a user sees in two
	   modes, and a drawing tab whose buttons were a different size would read
	   as a different application. Only the slot below is new. */
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
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		border-color: var(--accent);
		color: var(--accent);
	}

	.toolbar-btn:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	/* The flow prompt. Hard-capped in width so the toolbar's layout never
	   follows its text — the same call as `Toolbar.svelte`'s fixed-width DOF
	   slot, made the other way round (a cap rather than a reservation,
	   because this one is never the widest thing in the bar). */
	.prompt-slot {
		font-size: 11px;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 140px;
	}

	.settings-btn {
		font-size: 16px;
		line-height: 1;
		padding: 0 8px;
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

	.status-dot.ready {
		background: var(--success);
	}

	.status-dot.loading {
		background: var(--warning);
		animation: pulse 1s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.3;
		}
	}

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

	.dropdown-grid .toolbar-btn {
		min-width: 60px;
		text-align: center;
		padding: 6px 8px;
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
	}
</style>
