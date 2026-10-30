<script>
	import {
		isEngineReady,
		getActiveTool,
		setActiveTool,
		getRectMode,
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
		showPipeDialog,
		showScriptDialog,
		showChamferDialog,
		showFilletDialog,
		showShellDialog,
		showBooleanDialog,
		showDatumPlaneDialog,
		showMateConnectorDialog,
		importStep,
		showImportLinkDialog,
		saveProject,
		saveToStorage,
		loadProject,
		exportStl,
		exportStep,
		getSelectedRefs,
		clearSelection,
		clearSketchSessionSelection,
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
		toggleExamplesBrowser,
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
		openConstraintModal,
		getAgentActivity,
		setToolHint,
		AGENT_WORKING_HINT
	} from '$lib/engine/store.svelte.js';
	import { importKicad } from '$lib/engine/store.svelte.js';
	import { isModalConstraint } from '$lib/sketch/constraintModalEngine.js';
	import SettingsModal from './SettingsModal.svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { getApplicableConstraints } from '$lib/sketch/constraintLogic.js';
	import Icon from '$lib/ui/icons/Icon.svelte';
	import { resetTool, projectRef, isProjectableRef, seedOffsetFromSelection } from '$lib/sketch/tools.js';
	import { onMount, flushSync } from 'svelte';

	let ready = $derived(isEngineReady());
	// An agent-link call is running: modeling commands are refused, not queued (spec G8).
	let agentBusy = $derived(getAgentActivity() !== null);
	let tool = $derived(getActiveTool());
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let planeSelecting = $derived(getSketchPlaneSelectionMode());
	let selection = $derived(getSketchSelection());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());

	let isMobile = $derived(getMobileLayout());
	// Icons only, labels visually hidden (ui-chrome M13, user decision
	// 2026-10-30: "only icons on mobile"). Wider than the mobile LAYOUT
	// switch (max-width 768px): a phone held landscape is ~950 px wide and
	// keeps the desktop layout, but it is still a phone. A coarse pointer
	// tells a phone from a narrow desktop window.
	const ICON_ONLY_QUERY = '(max-width: 768px), (pointer: coarse) and (max-width: 960px)';
	let iconOnly = $state(false);
	$effect(() => {
		const mql = window.matchMedia(ICON_ONLY_QUERY);
		iconOnly = mql.matches;
		const onChange = (/** @type {MediaQueryListEvent} */ e) => { iconOnly = e.matches; scheduleRelayout(); };
		mql.addEventListener('change', onChange);
		return () => mql.removeEventListener('change', onChange);
	});
	let section = $derived(getSectionState());
	let applicable = $derived(inSketch ? getApplicableConstraints(selection, entities, positions) : {});
	let solveStatus = $derived(inSketch ? getSketchSolveStatus() : null);

	// ── Overflow-driven collapse ─────────────────────────────────────────
	// The toolbar never lets content fall off the screen: after every change
	// to its content or width it MEASURES whether the in-flow items fit and
	// collapses, one rung at a time, until they do. Rungs, in order:
	//   1  file / export / debug group  → the ⋮ overflow menu
	//   2  Planes / Axes / Section      → the ⋮ overflow menu
	//   3  hide the brand
	//   4  hide the project name
	//   5  Undo / Redo                  → the ⋮ overflow menu
	//   6… trailing tools, one per rung → a "More ▾" dropdown (priority+);
	//      when every tool is in it, that IS the single-dropdown mobile layout
	// The mobile breakpoint forces rungs 1, 2 and the full tool dropdown
	// outright (touch targets, not width). Rung n+1 is never wider than rung
	// n except the first "More ▾" step (the trigger costs more than one tool),
	// which the ladder walks straight through — so it cannot oscillate.
	const FIXED_RUNGS = 5;
	let collapseLevel = $state(0);
	let activeTools = $derived(inSketch ? sketchTools : modelingTools);
	let collapseMax = $derived(FIXED_RUNGS + activeTools.length);
	let compactFile = $derived(isMobile || collapseLevel >= 1);
	let compactView = $derived(isMobile || collapseLevel >= 2);
	let hideBrand = $derived(collapseLevel >= 3);
	let hideName = $derived(collapseLevel >= 4);
	let compactHistory = $derived(collapseLevel >= 5);
	/** How many trailing tools have moved into "More ▾". */
	let toolsHidden = $derived(Math.max(0, Math.min(activeTools.length, collapseLevel - FIXED_RUNGS)));
	let compactTools = $derived(isMobile || toolsHidden >= activeTools.length);
	let inlineTools = $derived(activeTools.slice(0, activeTools.length - toolsHidden));
	let overflowTools = $derived(activeTools.slice(activeTools.length - toolsHidden));
	let showMoreTools = $state(false);

	/** @type {HTMLDivElement | null} */
	let toolbarEl = $state(null);
	let relayoutQueued = false;

	/** Does the toolbar's in-flow content fit its content box? Positioned
	 *  descendants (dropdown panels, backdrops) and the flexible spacer are
	 *  not content. Items never shrink (nowrap), so summing their boxes is
	 *  exact. */
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
			needed += child.getBoundingClientRect().width
				+ (parseFloat(ccs.marginLeft) || 0) + (parseFloat(ccs.marginRight) || 0);
		}
		needed += gap * Math.max(0, count - 1);
		return needed <= avail + 0.5;
	}

	/** Walk the ladder: expand while there is room, collapse while there is
	 *  not. Each step is flushed synchronously, so no intermediate layout is
	 *  ever painted. Must not run inside an effect (flushSync is forbidden
	 *  there) — schedule it with scheduleRelayout(). */
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

	/**
	 * Counts completed ladder walks (`data-relayout-seq`). The walk runs in a
	 * microtask after a ResizeObserver tick, so a test that resizes the window
	 * must wait for the count to move before measuring the toolbar — a fixed
	 * delay lost that race on a starved CI runner (layout-overflow.spec.js).
	 */
	let relayoutSeq = $state(0);

	function scheduleRelayout() {
		if (relayoutQueued) return;
		relayoutQueued = true;
		queueMicrotask(() => {
			relayoutQueued = false;
			relayout();
		});
	}

	$effect(() => {
		// Everything that changes the toolbar's in-flow content re-measures it.
		// collapseLevel itself is deliberately NOT read here.
		void [ready, agentBusy, inSketch, isMobile, section.active, solveStatus?.status,
			solveStatus?.dof, name, editingName, saving, exportingStl, exportingStep];
		scheduleRelayout();
	});

	$effect(() => {
		if (!toolbarEl) return;
		const ro = new ResizeObserver(() => scheduleRelayout());
		ro.observe(toolbarEl);
		document.fonts?.ready?.then(() => scheduleRelayout());
		return () => ro.disconnect();
	});

	// Portrait mode: collapse file/view actions into overflow menu
	let showOverflow = $state(false);
	let showConstraints = $state(false);
	let showSketchTools = $state(false);
	let showRectMenu = $state(false);
	let rectMode = $derived(getRectMode());

	// The two rectangle construction variants offered by the Rect split button.
	const rectVariants = [
		{ id: 'rectangle', icon: 'rectangle', label: 'Corner Rectangle', hint: 'Two opposite corners' },
		{ id: 'rectangle-center', icon: 'rectangle-center', label: 'Center Rectangle', hint: 'Center, then a corner' },
	];
	let showModelingTools = $state(false);

	let showDebugMenu = $state(false);
	let settingsOpen = $state(false);

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
		{ id: 'horizontal', icon: 'constraint-horizontal', label: 'H', title: 'Horizontal' },
		{ id: 'vertical', icon: 'constraint-vertical', label: 'V', title: 'Vertical' },
		{ id: 'coincident', icon: 'constraint-coincident', label: 'Co', title: 'Coincident' },
		{ id: 'perpendicular', icon: 'constraint-perpendicular', label: 'Perp', title: 'Perpendicular' },
		{ id: 'parallel', icon: 'constraint-parallel', label: 'Par', title: 'Parallel' },
		{ id: 'equal', icon: 'constraint-equal', label: 'Eq', title: 'Equal' },
		{ id: 'tangent', icon: 'constraint-tangent', label: 'Tan', title: 'Tangent' },
		{ id: 'midpoint', icon: 'constraint-midpoint', label: 'Mid', title: 'Midpoint' },
		{ id: 'fix', icon: 'constraint-fix', label: 'Fix', title: 'Fix Point' },
		{ id: 'angle', icon: 'constraint-angle', label: 'Ang', title: 'Angle' },
		{ id: 'symmetricH', icon: 'constraint-symmetric-h', label: 'SH', title: 'Symmetric Horizontal' },
		{ id: 'symmetricV', icon: 'constraint-symmetric-v', label: 'SV', title: 'Symmetric Vertical' },
		{ id: 'pointOnLine', icon: 'constraint-point-on-line', label: 'OnL', title: 'Point on Line' },
		{ id: 'hDistance', icon: 'constraint-h-distance', label: 'HD', title: 'Horizontal Distance' },
		{ id: 'vDistance', icon: 'constraint-v-distance', label: 'VD', title: 'Vertical Distance' },
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
		{ id: 'sketch', icon: 'sketch', name: 'Sketch', label: 'Sketch', shortcut: 'S' },
		{ id: 'extrude', icon: 'extrude', name: 'Extrude', label: 'Extrude', shortcut: 'E' },
		{ id: 'revolve', icon: 'revolve', name: 'Revolve', label: 'Revolve', shortcut: '' },
		{ id: 'pipe', icon: 'pipe', name: 'Pipe', label: 'Pipe', shortcut: '' },
		{ id: 'script', icon: 'script', name: 'Script', label: 'Script', shortcut: '' },
		{ id: 'fillet', icon: 'fillet', name: 'Fillet', label: 'Fillet', shortcut: '', deferred: true },
		{ id: 'chamfer', icon: 'chamfer', name: 'Chamfer', label: 'Chamfer', shortcut: '', deferred: true },
		{ id: 'shell', icon: 'shell', name: 'Shell', label: 'Shell', shortcut: '', deferred: true },
		{ id: 'boolean', icon: 'boolean', name: 'Boolean', label: 'Boolean', shortcut: '' },
		{ id: 'datum-plane', icon: 'datum-plane', name: 'Datum plane', label: 'Plane', shortcut: '' },
		{ id: 'mate-connector', icon: 'mate-connector', name: 'Mate connector', label: 'Connector', shortcut: '' },
		{ id: 'import-step', icon: 'import-step', name: 'Import STEP', label: 'Import', shortcut: '' },
		{ id: 'import-link', icon: 'import-link', name: 'Link STEP', label: 'Link STEP', shortcut: '' },
		{ id: 'import-kicad', icon: 'import-kicad', name: 'Import KiCad board', label: 'KiCad', shortcut: '' },
		{ id: 'link-kicad', icon: 'link-kicad', name: 'Link KiCad board', label: 'Link KiCad', shortcut: '' },
	];

	const sketchTools = [
		{ id: 'select', icon: 'mouse-pointer-2', name: 'Select', label: 'Select', shortcut: '' },
		{ id: 'point', icon: 'point', name: 'Point', label: 'Point', shortcut: '' },
		{ id: 'line', icon: 'line', name: 'Line', label: 'Line', shortcut: 'L' },
		{ id: 'polyline', icon: 'polyline', name: 'Polyline', label: 'Poly', shortcut: 'P' },
		{ id: 'rectangle', icon: 'rectangle', name: 'Rectangle', label: 'Rect', shortcut: 'R' },
		{ id: 'circle', icon: 'circle', name: 'Circle', label: 'Circle', shortcut: 'C' },
		{ id: 'arc', icon: 'arc', name: 'Arc', label: 'Arc', shortcut: 'A' },
		{ id: 'project', icon: 'project', name: 'Project geometry', label: 'Proj', shortcut: 'J' },
		{ id: 'offset', icon: 'offset', name: 'Offset', label: 'Offset', shortcut: 'O' },
		{ id: 'slot', icon: 'slot', name: 'Slot', label: 'Slot', shortcut: 'T' },
		{ id: 'gear', icon: 'gear', name: 'Gear', label: 'Gear', shortcut: 'G' },
		{ id: 'sprocket', icon: 'sprocket', name: 'Sprocket', label: 'Sprkt', shortcut: 'K' },
		{ id: 'planetary', icon: 'planetary', name: 'Planetary gear', label: 'Planet', shortcut: '' },
		{ id: 'trim', icon: 'trim', name: 'Trim', label: 'Trim', shortcut: '' },
		{ id: 'sketch-fillet', icon: 'sketch-fillet', name: 'Sketch fillet', label: 'Fillet', shortcut: 'F' },
		{ id: 'construction', icon: 'construction', name: 'Construction', label: 'Constr', shortcut: 'X' },
	];

	/** Tooltip: the tool's full name and shortcut; a deferred tool says so. */
	function toolTitle(t) {
		const base = `${t.name ?? t.label}${t.shortcut ? ` (${t.shortcut})` : ''}`;
		return t.deferred ? `${base} — not yet available` : base;
	}

	async function handleToolClick(toolId) {
		if (agentBusy) {
			setToolHint(AGENT_WORKING_HINT);
			return;
		}
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
						setActiveTool('select');
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
		if (toolId === 'pipe' && !inSketch) {
			showPipeDialog();
			return;
		}
		if (toolId === 'script' && !inSketch) {
			showScriptDialog();
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
		if (toolId === 'mate-connector' && !inSketch) {
			showMateConnectorDialog();
			return;
		}
		if (toolId === 'import-step' && !inSketch) {
			importStep();
			return;
		}
		if (toolId === 'import-link' && !inSketch) {
			showImportLinkDialog();
			return;
		}
		if (toolId === 'import-kicad' && !inSketch) {
			importKicad();
			return;
		}
		if (toolId === 'link-kicad' && !inSketch) {
			showImportLinkDialog('kicad');
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

	/**
	 * Proj button / J shortcut. Cycle-2 select-first: only when the Select tool is
	 * active AND body geometry is already selected do we project it all now and
	 * stay in Select. Under any drawing tool (or with nothing selected) we fall
	 * through to activating the Project tool — a stale body selection left over
	 * from Select must NOT silently project once a drawing tool is active (branch
	 * table row "Any drawing tool → activate project tool"). See the Cycle-2
	 * branch table in specs/projected_sketch_geometry.md.
	 */
	function activateOrRunProject() {
		if (inSketch && getActiveTool() === 'select') {
			const all = getSelectedRefs();
			const projectable = all.filter(isProjectableRef);
			if (projectable.length > 0) {
				for (const r of projectable) projectRef(r);
				if (projectable.length < all.length) {
					showToast('info', 'Some selected items cannot be projected');
				}
				clearSelection();
				return;
			}
		}
		setActiveTool('project');
	}

	/**
	 * Offset button / O shortcut. Select-first: with the Select tool active and
	 * sketch entities already selected, seed the offset chain from the selection
	 * (a lone entity expands to its chain; a multi-selection is offset as-is).
	 * Otherwise just activate the tool for click-first picking. Mirrors
	 * activateOrRunProject; see specs/sketch_chain_offset.md branch 12.
	 */
	function activateOrRunOffset() {
		if (inSketch && getActiveTool() === 'select' && getSketchSelection().size > 0) {
			setActiveTool('offset');
			// SketchInteraction resets tool state in an $effect on tool change;
			// flush it NOW so the reset runs before seeding, not after (which
			// would silently disarm the seeded chain).
			flushSync();
			seedOffsetFromSelection();
			return;
		}
		setActiveTool('offset');
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
			if (agentBusy && !((e.ctrlKey || e.metaKey) && e.key === 's')) {
				// G8: shortcuts other than Save are ignored (nothing queued) while an agent call runs.
				if (e.ctrlKey || e.metaKey || e.key.length === 1 || e.key === 'Delete' || e.key === 'Backspace') {
					e.preventDefault();
					setToolHint(AGENT_WORKING_HINT);
				}
				return;
			}

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
				case 'r': if (inSketch) setActiveTool(getRectMode()); break;
				case 'p': if (inSketch) setActiveTool('polyline'); break;
				case 'c': if (inSketch) setActiveTool('circle'); break;
				case 'a': if (inSketch) setActiveTool('arc'); break;
				case 'x': if (inSketch) handleToggleConstruction(); break;
				case 'j': if (inSketch) activateOrRunProject(); break;
				case 'o': if (inSketch) activateOrRunOffset(); break;
				case 't': if (inSketch) setActiveTool('slot'); break;
				case 'f': if (inSketch) setActiveTool('sketch-fillet'); break;
				case 'd': if (inSketch) setActiveTool('dimension'); break;
				case 'g': if (inSketch) setActiveTool('gear'); break;
				case 'k': if (inSketch) setActiveTool('sprocket'); break;
				case 'Escape':
					if (planeSelecting) {
						exitSketchPlaneSelection();
					} else if (inSketch) {
						// Escape never leaves the sketch (finishing is the
						// Finish button's job): it backs out one level — the
						// active tool to Select, then the selection to empty —
						// so repeated taps are harmless.
						if (tool !== 'select') {
							resetTool();
							setActiveTool('select');
						} else {
							clearSketchSessionSelection();
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

<!-- A toolbar button's face: icon + label. On a phone (`iconOnly`) the label is visually
     hidden (icons only) but stays the button's accessible name and text. -->
{#snippet tb(icon, label)}
	<Icon name={icon} size={iconOnly ? 20 : 16} /><span class="tb-label" class:sr-only={iconOnly}>{label}</span>
{/snippet}

<div class="toolbar" data-testid="toolbar" data-collapse-level={collapseLevel} data-relayout-seq={relayoutSeq} bind:this={toolbarEl}>
	{#if !hideBrand}
		<div class="toolbar-brand">Waffle Iron</div>
	{/if}

	<button
		class="toolbar-btn home-btn"
		data-testid="toolbar-btn-home"
		title="Home"
		onclick={() => goto(`${base}/home`)}
	>{@render tb('house', 'Home')}</button>

	{#if !hideName}
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
	{/if}

	{#if inSketch}
		<!-- Sketch mode tools -->
		{#if compactTools}
			<!-- Compact: sketch tools in dropdown -->
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-sketch-tools-dropdown"
					onclick={(e) => { openDropdown(e.currentTarget, () => { showSketchTools = !showSketchTools; showConstraints = false; }); }}
				>{@render tb('line', 'Tools ▾')}</button>
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
								title={toolTitle(t)}
								data-testid="toolbar-btn-{t.id}"
								onclick={() => { t.id === 'construction' ? handleToggleConstruction() : t.id === 'project' ? activateOrRunProject() : t.id === 'offset' ? activateOrRunOffset() : setActiveTool(t.id); showSketchTools = false; }}
							>{@render tb(t.icon, t.label)}</button>
							{#if t.id === 'rectangle'}
								<!-- Center-rectangle variant as its own entry on mobile. -->
								<button
									class="toolbar-btn"
									class:active={tool === 'rectangle-center'}
									disabled={!ready}
									title="Center Rectangle"
									data-testid="toolbar-btn-rectangle-center"
									onclick={() => { setActiveTool('rectangle-center'); showSketchTools = false; }}
								>{@render tb('rectangle-center', 'Center Rect')}</button>
							{/if}
						{/each}
					</div>
				</div>
			{/if}
		{:else}
			<!-- Expanded: sketch tools inline; trailing ones that do not fit
			     live in the "More ▾" dropdown at the end of the group. -->
			<div class="toolbar-group">
				{#each inlineTools as t}
					{#if t.id === 'rectangle'}
						<!-- Rectangle split button: main face selects the current rect
						     mode; the corner arrow opens the variant menu. -->
						<div class="split-btn dropdown-container">
							<button
								class="toolbar-btn split-btn-main"
								class:active={tool === 'rectangle' || tool === 'rectangle-center'}
								disabled={!ready}
								title="{rectMode === 'rectangle-center' ? 'Center Rectangle' : 'Corner Rectangle'} (R)"
								data-testid="toolbar-btn-rectangle"
								onclick={() => setActiveTool(rectMode)}
							>{@render tb(rectMode, 'Rect')}</button>
							<button
								class="split-btn-arrow"
								disabled={!ready}
								title="Rectangle type"
								aria-label="Rectangle type"
								data-testid="toolbar-btn-rectangle-menu"
								onclick={() => { showRectMenu = !showRectMenu; showSketchTools = false; showConstraints = false; }}
							>▾</button>
							{#if showRectMenu}
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div class="dropdown-backdrop" onclick={() => showRectMenu = false} onpointerdown={(e) => e.stopPropagation()}></div>
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div class="dropdown-panel rect-menu" data-testid="rectangle-menu" onpointerdown={(e) => e.stopPropagation()}>
									{#each rectVariants as rv}
										<button
											class="rect-menu-item"
											class:active={rectMode === rv.id}
											data-testid="rect-variant-{rv.id}"
											title={rv.hint}
											onclick={() => { setActiveTool(rv.id); showRectMenu = false; }}
										><Icon name={rv.icon} />{rv.label}</button>
									{/each}
								</div>
							{/if}
						</div>
					{:else}
						<button
							class="toolbar-btn"
							class:active={t.id !== 'construction' && tool === t.id}
							disabled={!ready}
							title={toolTitle(t)}
							data-testid="toolbar-btn-{t.id}"
							onclick={() => t.id === 'construction' ? handleToggleConstruction() : t.id === 'project' ? activateOrRunProject() : t.id === 'offset' ? activateOrRunOffset() : setActiveTool(t.id)}
						>{@render tb(t.icon, t.label)}</button>
					{/if}
				{/each}
				{#if overflowTools.length > 0}
					<div class="dropdown-container">
						<button
							class="toolbar-btn dropdown-trigger"
							data-testid="toolbar-btn-more-tools"
							title="More sketch tools"
							onclick={(e) => { openDropdown(e.currentTarget, () => { showMoreTools = !showMoreTools; showConstraints = false; }); }}
						>{@render tb('chevrons-right', 'More ▾')}</button>
					</div>
					{#if showMoreTools}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="dropdown-backdrop" onclick={() => showMoreTools = false} onpointerdown={(e) => e.stopPropagation()}></div>
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="dropdown-panel dropdown-fixed" style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;" data-testid="more-tools-dropdown"
							onpointerdown={(e) => e.stopPropagation()}
						>
							<div class="dropdown-grid">
								{#each overflowTools as t}
									<button
										class="toolbar-btn"
										class:active={t.id !== 'construction' && (tool === t.id || (t.id === 'rectangle' && tool === 'rectangle-center'))}
										disabled={!ready}
										title={toolTitle(t)}
										data-testid="toolbar-btn-{t.id}"
										onclick={() => { t.id === 'construction' ? handleToggleConstruction() : t.id === 'project' ? activateOrRunProject() : t.id === 'offset' ? activateOrRunOffset() : t.id === 'rectangle' ? setActiveTool(rectMode) : setActiveTool(t.id); showMoreTools = false; }}
									>{@render tb(t.icon, t.label)}</button>
									{#if t.id === 'rectangle'}
										<button
											class="toolbar-btn"
											class:active={tool === 'rectangle-center'}
											disabled={!ready}
											title="Center Rectangle"
											data-testid="toolbar-btn-rectangle-center"
											onclick={() => { setActiveTool('rectangle-center'); showMoreTools = false; }}
										>{@render tb('rectangle-center', 'Center Rect')}</button>
									{/if}
								{/each}
							</div>
						</div>
					{/if}
				{/if}
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
				>{@render tb('constraint-perpendicular', 'Constraints ▾')}</button>
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
							><Icon name={cb.icon} size={20} /><span class="cb-label" class:sr-only={iconOnly}>{cb.label}</span></button>
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
				>{@render tb('constraint-perpendicular', 'Constraints ▾')}</button>
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
								><Icon name={cb.icon} size={20} /><span class="cb-label" class:sr-only={iconOnly}>{cb.label}</span></button>
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
		>{@render tb('dimension', 'Dim')}</button>
		<div class="toolbar-sep"></div>
		<!-- The badge slot has a fixed width so the solve status changing (or
		     first appearing) never re-lays-out the toolbar mid-sketch. -->
		<span class="dof-slot">
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
		</span>
		<div class="toolbar-sep"></div>
		<button class="toolbar-btn finish-btn" data-testid="toolbar-btn-finish-sketch" title="Finish Sketch" onclick={handleFinishSketch}>
			<Icon name="check" size={iconOnly ? 20 : 16} /><span class="tb-label">{iconOnly ? 'Finish' : 'Finish Sketch'}</span>
		</button>
	{:else}
		<!-- Modeling tools -->
		{#if compactTools}
			<!-- Compact: modeling tools in dropdown -->
			<div class="dropdown-container">
				<button
					class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-modeling-dropdown"
					onclick={(e) => { openDropdown(e.currentTarget, () => { showModelingTools = !showModelingTools; }); }}
				>{@render tb('extrude', 'Model ▾')}</button>
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
								class:deferred={t.deferred}
								disabled={!ready || agentBusy}
								title={toolTitle(t)}
								data-testid="toolbar-btn-{t.id}"
								onclick={async () => { await handleToolClick(t.id); showModelingTools = false; }}
							>{@render tb(t.icon, t.label)}</button>
						{/each}
					</div>
				</div>
			{/if}
		{:else}
			<!-- Expanded: modeling tools inline; trailing ones that do not fit
			     live in the "More ▾" dropdown at the end of the group. -->
			<div class="toolbar-group">
				{#each inlineTools as t}
					<button
						class="toolbar-btn"
						class:active={tool === t.id}
						class:deferred={t.deferred}
						disabled={!ready || agentBusy}
						title={toolTitle(t)}
						data-testid="toolbar-btn-{t.id}"
						onclick={async () => { await handleToolClick(t.id); }}
					>{@render tb(t.icon, t.label)}</button>
				{/each}
				{#if overflowTools.length > 0}
					<div class="dropdown-container">
						<button
							class="toolbar-btn dropdown-trigger"
							data-testid="toolbar-btn-more-tools"
							title="More modeling tools"
							onclick={(e) => { openDropdown(e.currentTarget, () => { showMoreTools = !showMoreTools; }); }}
						>{@render tb('chevrons-right', 'More ▾')}</button>
					</div>
					{#if showMoreTools}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="dropdown-backdrop" onclick={() => showMoreTools = false} onpointerdown={(e) => e.stopPropagation()}></div>
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="dropdown-panel dropdown-fixed" style="top: {dropdownPos.top}px; left: {dropdownPos.left}px;" data-testid="more-tools-dropdown"
							onpointerdown={(e) => e.stopPropagation()}
						>
							<div class="dropdown-grid">
								{#each overflowTools as t}
									<button
										class="toolbar-btn"
										class:active={tool === t.id}
										class:deferred={t.deferred}
										disabled={!ready || agentBusy}
										title={toolTitle(t)}
										data-testid="toolbar-btn-{t.id}"
										onclick={async () => { showMoreTools = false; await handleToolClick(t.id); }}
									>{@render tb(t.icon, t.label)}</button>
								{/each}
							</div>
						</div>
					{/if}
				{/if}
			</div>
		{/if}
		{#if !compactView}
			<div class="toolbar-sep"></div>
			<div class="toolbar-group">
				<button
					class="toolbar-btn"
					class:active={getShowDatumPlanes()}
					title="Toggle Datum Planes"
					data-testid="toolbar-btn-toggle-planes"
					onclick={() => toggleDatumPlanes()}
				>{@render tb('toggle-planes', 'Planes')}</button>
				<button
					class="toolbar-btn"
					class:active={getShowOriginTriad()}
					title="Toggle Origin Axes"
					data-testid="toolbar-btn-toggle-axes"
					onclick={() => toggleOriginTriad()}
				>{@render tb('toggle-axes', 'Axes')}</button>
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
				>{@render tb('section', 'Section')}</button>
				{#if section.active}
					<button
						class="toolbar-btn"
						title="Flip which half is kept"
						data-testid="toolbar-btn-section-flip"
						onclick={() => flipSection()}
					>{@render tb('section-flip', 'Flip')}</button>
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
					>{@render tb('section-clear', 'Clear')}</button>
				{/if}
			</div>
		{/if}
	{/if}

	{#if !compactHistory}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" data-testid="toolbar-btn-undo" disabled={!ready || agentBusy} title="Undo (Ctrl+Z)" onclick={undo}>{@render tb('undo-2', 'Undo')}</button>
			<button class="toolbar-btn" data-testid="toolbar-btn-redo" disabled={!ready || agentBusy} title="Redo (Ctrl+Shift+Z)" onclick={redo}>{@render tb('redo-2', 'Redo')}</button>
		</div>
	{/if}
	{#if compactFile}
		<!-- Compact: file/export/test actions live in the ⋮ overflow menu -->
		<div class="toolbar-sep"></div>
		<div class="overflow-container">
			<button class="toolbar-btn overflow-trigger" title="More actions" onclick={(e) => openOverflow(e.currentTarget)}
				aria-label="More actions" data-testid="toolbar-btn-overflow">
				<Icon name="ellipsis-vertical" size={iconOnly ? 20 : 16} />
			</button>
		</div>
		{#if showOverflow}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="overflow-backdrop" onclick={closeOverflow}></div>
			<div class="overflow-menu overflow-fixed" style="top: {overflowPos.top}px; right: {overflowPos.right}px;" data-testid="toolbar-overflow-menu">
					{#if compactHistory}
						<button class="overflow-item" disabled={!ready || agentBusy}
							data-testid="toolbar-btn-undo" title="Undo (Ctrl+Z)"
							onclick={() => { closeOverflow(); undo(); }}><Icon name="undo-2" />Undo</button>
						<button class="overflow-item" disabled={!ready || agentBusy}
							data-testid="toolbar-btn-redo" title="Redo (Ctrl+Shift+Z)"
							onclick={() => { closeOverflow(); redo(); }}><Icon name="redo-2" />Redo</button>
						<div class="overflow-separator"></div>
					{/if}
					<button class="overflow-item" disabled={!ready || saving}
						data-testid="toolbar-btn-save"
						onclick={async () => { closeOverflow(); saving = true; try { await saveToStorage(); } finally { saving = false; } }}>
						<Icon name="save" />{saving ? 'Saving...' : 'Save'}
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-export-waffle-main"
						onclick={async () => { closeOverflow(); await saveProject(); }}>
						<Icon name="export-waffle" />Export .waffle
					</button>
					<button class="overflow-item" disabled={!ready || agentBusy}
						data-testid="toolbar-btn-open"
						onclick={() => { closeOverflow(); loadProject(); }}><Icon name="folder-open" />Open</button>
					<button class="overflow-item" disabled={!ready || exportingStl}
						data-testid="toolbar-btn-export-stl"
						onclick={async () => { closeOverflow(); exportingStl = true; try { await exportStl(); } finally { exportingStl = false; } }}>
						<Icon name="export-stl" />{exportingStl ? 'Exporting...' : 'Export STL'}
					</button>
					<button class="overflow-item" disabled={!ready || exportingStep}
						data-testid="toolbar-btn-export-step"
						onclick={async () => { closeOverflow(); exportingStep = true; try { await exportStep(); } finally { exportingStep = false; } }}>
						<Icon name="export-step" />{exportingStep ? 'Exporting...' : 'Export STEP'}
					</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-tests"
						onclick={() => { closeOverflow(); toggleTestCaseBrowser(); }}><Icon name="flask-conical" />Tests</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-examples"
						onclick={() => { closeOverflow(); toggleExamplesBrowser(); }}><Icon name="book-open" />Examples</button>
					<button class="overflow-item" disabled={!ready}
						data-testid="toolbar-btn-assay"
						onclick={() => { closeOverflow(); toggleAssayBrowser(); }}><Icon name="microscope" />Assay</button>
					<div class="overflow-separator"></div>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-viewport"
						onclick={() => { closeOverflow(); handleViewportDebug(); }}><Icon name="info" />Viewport Info</button>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-wireframe"
						onclick={() => { closeOverflow(); handleToggleWireframeDebug(); }}><Icon name="wireframe" />Toggle Wireframe</button>
					<button class="overflow-item"
						data-testid="toolbar-btn-debug-shader"
						onclick={() => { closeOverflow(); handleToggleShaderDebug(); }}><Icon name="palette" />Toggle Shader Debug</button>
					{#if compactView && !inSketch}
						<div class="overflow-separator"></div>
						<button class="overflow-item" disabled={!ready}
							data-testid="toolbar-btn-toggle-planes"
							role="menuitemcheckbox" aria-checked={getShowDatumPlanes()}
							onclick={() => { closeOverflow(); toggleDatumPlanes(); }}>
							<Icon name="toggle-planes" />Planes{#if getShowDatumPlanes()}<span class="overflow-check"><Icon name="check" size={14} /></span>{/if}
						</button>
						<button class="overflow-item" disabled={!ready}
							data-testid="toolbar-btn-toggle-axes"
							role="menuitemcheckbox" aria-checked={getShowOriginTriad()}
							onclick={() => { closeOverflow(); toggleOriginTriad(); }}>
							<Icon name="toggle-axes" />Axes{#if getShowOriginTriad()}<span class="overflow-check"><Icon name="check" size={14} /></span>{/if}
						</button>
						<button class="overflow-item" disabled={!ready}
							data-testid="toolbar-btn-section"
							title="Section view — clip the model at the selected plane/face (capped)"
							role="menuitemcheckbox" aria-checked={section.active}
							onclick={() => { closeOverflow(); handleToggleSection(); }}>
							<Icon name="section" />Section{#if section.active}<span class="overflow-check"><Icon name="check" size={14} /></span>{/if}
						</button>
						{#if section.active}
							<button class="overflow-item"
								data-testid="toolbar-btn-section-flip"
								onclick={() => { closeOverflow(); flipSection(); }}><Icon name="section-flip" />Flip section</button>
							<label class="overflow-item overflow-range" title="Move the cut along the plane normal">
								<span>Offset</span>
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
							<button class="overflow-item"
								data-testid="toolbar-btn-section-clear"
								onclick={() => { closeOverflow(); clearSection(); }}><Icon name="section-clear" />Clear section</button>
						{/if}
					{/if}
					<div class="overflow-separator"></div>
					<button class="overflow-item"
						data-testid="toolbar-btn-reload"
						onclick={() => { closeOverflow(); location.reload(); }}><Icon name="rotate-cw" />Reload</button>
				</div>
		{/if}
	{:else}
		<div class="toolbar-sep"></div>
		<div class="toolbar-group">
			<button class="toolbar-btn" disabled={!ready || saving} title="Save (Ctrl+S)"
				data-testid="toolbar-btn-save"
				onclick={async () => { saving = true; try { await saveToStorage(); } finally { saving = false; } }}>
				{@render tb('save', saving ? 'Saving...' : 'Save')}
			</button>
			<button class="toolbar-btn" disabled={!ready || agentBusy} title="Open (Ctrl+O)"
				data-testid="toolbar-btn-open"
				onclick={() => loadProject()}>{@render tb('folder-open', 'Open')}</button>
			<button class="toolbar-btn" disabled={!ready} title="Download a .waffle file (Save persists to the browser)"
				data-testid="toolbar-btn-export-waffle-main"
				onclick={async () => { await saveProject(); }}>{@render tb('export-waffle', 'Export .waffle')}</button>
			<button class="toolbar-btn" disabled={!ready || exportingStl} title="Export STL"
				data-testid="toolbar-btn-export-stl"
				onclick={async () => { exportingStl = true; try { await exportStl(); } finally { exportingStl = false; } }}>
				{@render tb('export-stl', exportingStl ? 'Exporting...' : 'Export STL')}
			</button>
			<button class="toolbar-btn" disabled={!ready || exportingStep} title="Export STEP"
				data-testid="toolbar-btn-export-step"
				onclick={async () => { exportingStep = true; try { await exportStep(); } finally { exportingStep = false; } }}>
				{@render tb('export-step', exportingStep ? 'Exporting...' : 'Export STEP')}
			</button>
			<button class="toolbar-btn" disabled={!ready} title="Test Cases (Ctrl+Shift+T)"
				data-testid="toolbar-btn-tests"
				onclick={() => toggleTestCaseBrowser()}>{@render tb('flask-conical', 'Tests')}</button>
			<button class="toolbar-btn" disabled={!ready} title="Official example documents"
				data-testid="toolbar-btn-examples"
				onclick={() => toggleExamplesBrowser()}>{@render tb('book-open', 'Examples')}</button>
			<button class="toolbar-btn" disabled={!ready} title="Assay Browser"
				data-testid="toolbar-btn-assay"
				onclick={() => toggleAssayBrowser()}>{@render tb('microscope', 'Assay')}</button>
			<div class="dropdown-container">
				<button class="toolbar-btn dropdown-trigger"
					data-testid="toolbar-btn-debug-dropdown"
					onclick={() => { showDebugMenu = !showDebugMenu; }}
				>{@render tb('bug', 'Debug ▾')}</button>
				{#if showDebugMenu}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="dropdown-backdrop" onclick={() => showDebugMenu = false}></div>
					<div class="dropdown-panel" data-testid="debug-dropdown">
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-viewport"
							onclick={() => { showDebugMenu = false; handleViewportDebug(); }}><Icon name="info" />Viewport Info</button>
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-wireframe"
							onclick={() => { showDebugMenu = false; handleToggleWireframeDebug(); }}><Icon name="wireframe" />Toggle Wireframe</button>
						<button class="overflow-item"
							data-testid="toolbar-btn-debug-shader"
							onclick={() => { showDebugMenu = false; handleToggleShaderDebug(); }}><Icon name="palette" />Toggle Shader Debug</button>
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
	<button
		class="toolbar-btn settings-btn"
		title="Settings"
		aria-label="Settings"
		data-testid="toolbar-btn-settings"
		onclick={() => (settingsOpen = true)}
	><Icon name="settings" size={iconOnly ? 20 : 16} /></button>
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
	.settings-btn {
		font-size: 16px;
		line-height: 1;
		padding: 0 8px;
	}

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
		padding: 4px 6px;
		border-radius: 3px;
		cursor: pointer;
		font-size: 12px;
		white-space: nowrap;
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}

	/* Fillet / chamfer / shell: deferred indefinitely — they open a dialog
	   whose Apply is disabled, so the button reads as unavailable too. */
	.toolbar-btn.deferred,
	.toolbar-btn.deferred:hover:not(:disabled) {
		color: var(--text-muted);
		opacity: 0.6;
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

	/* Rectangle split button: a main face + a small bottom-right dropdown arrow. */
	.split-btn {
		display: inline-flex;
		align-items: stretch;
	}

	.split-btn-main {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
	}

	.split-btn-arrow {
		display: flex;
		align-items: flex-end;
		justify-content: center;
		background: transparent;
		border: 1px solid transparent;
		border-top-left-radius: 0;
		border-bottom-left-radius: 0;
		border-top-right-radius: 3px;
		border-bottom-right-radius: 3px;
		color: var(--text-primary);
		cursor: pointer;
		font-size: 9px;
		line-height: 1;
		padding: 4px 3px 3px 1px;
		margin-left: -1px;
	}

	.split-btn-arrow:hover:not(:disabled) {
		background: var(--bg-hover);
		border-color: var(--border-color);
	}

	.split-btn-arrow:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	.rect-menu {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 160px;
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
		z-index: 200;
		padding: 6px;
	}

	.rect-menu-item {
		background: transparent;
		border: 1px solid transparent;
		border-radius: 3px;
		color: var(--text-primary);
		cursor: pointer;
		font-size: 12px;
		padding: 6px 8px;
		text-align: left;
		white-space: nowrap;
	}

	.rect-menu-item:hover {
		background: var(--bg-hover);
		border-color: var(--border-color);
	}

	.rect-menu-item.active {
		background: color-mix(in srgb, var(--accent) 20%, transparent);
		border-color: var(--accent);
		color: var(--accent);
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
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
	}

	.cb-label {
		font-size: 10px;
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

	/* Reserved width for the widest status ("Fully constrained") so the
	   toolbar's layout is independent of the solve result. */
	.dof-slot {
		display: inline-flex;
		justify-content: center;
		min-width: 112px;
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
		background: color-mix(in srgb, var(--success) 15%, transparent);
	}

	.dof-under {
		color: var(--warning);
		background: color-mix(in srgb, var(--warning) 15%, transparent);
	}

	.dof-over {
		color: var(--error);
		background: color-mix(in srgb, var(--error) 15%, transparent);
	}

	/* Redundant sits between "under" and "over": the warning hue, but pushed
	   toward the error one so the two badges stay distinguishable. */
	.dof-redundant {
		color: color-mix(in srgb, var(--warning) 65%, var(--error));
		background: color-mix(in srgb, var(--warning) 15%, transparent);
	}

	.finish-btn {
		color: var(--success);
		font-weight: 600;
	}

	.finish-btn:hover {
		background: color-mix(in srgb, var(--success) 15%, transparent);
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
		display: flex;
		align-items: center;
		gap: 10px;
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

	/* A view toggle that is on: a trailing check. */
	.overflow-check {
		margin-left: auto;
		display: inline-flex;
		color: var(--accent);
	}

	.overflow-item:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.overflow-item:disabled {
		color: var(--text-muted);
		cursor: default;
	}

	.overflow-range {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.overflow-range input[type='range'] {
		flex: 1;
		min-width: 80px;
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

	/* Tiles in the tool dropdowns: icon above its label. */
	.dropdown-grid .toolbar-btn {
		flex-direction: column;
		justify-content: center;
		gap: 2px;
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
		color: var(--text-muted);
		border-top: 1px solid var(--border-color);
		margin-top: 4px;
		user-select: text;
	}
</style>
