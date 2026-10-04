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
		getFeatureWarnings,
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
		measureBodyMass,
		measureBodyGeometry,
		getDocumentDisplaySettings,
		getMaterials,
		setBodyMaterial,
		getParameters,
		setParameters,
		getDocumentParameters,
		setDocumentParameters,
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
	import { showImportLinkDialog } from '$lib/engine/store.svelte.js';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { BUILTIN_PLANES, makePlaneRef } from '$lib/engine/planes.js';
	import { internalToDisplay, UNITS } from '$lib/units.js';
	import { describeLocator } from '$lib/storage/git/locator.js';
	import { longPressContextMenu } from './longPressContextMenu.js';

	let tree = $derived(getFeatureTree());
	let selectedId = $derived(getSelectedFeatureId());
	// Face→feature (Tier 1): the feature whose geometry is currently picked.
	let faceFeatureId = $derived(getSelectedRefFeatureId());
	let featureErrors = $derived(getFeatureErrors());
	let featureWarnings = $derived(getFeatureWarnings());
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

	// Variables (design parameters) section state. TWO scopes since P2
	// (`specs/agent_mechanical_design.md` §6): the tab's own table and the
	// DOCUMENT table above it, which every tab resolves through after its
	// own. One set of handlers drives both — the scope travels on the edit —
	// because the row shape, the rename rule and the delete are identical and
	// a second copy would be a second place for them to drift.
	let variablesExpanded = $state(true);
	let documentVariablesExpanded = $state(false);
	let parameters = $derived(getParameters());
	let documentParameters = $derived(getDocumentParameters());
	/** Tab-local names, so a shadowed document row can be marked as such. */
	let shadowedNames = $derived(new Set(parameters.map((p) => p.name)));
	/** Inline edit state: null | { id: string|null, name, expression, scope }.
	 *  id === null means a new row being created; scope is 'tab' | 'document'. */
	let editingVariable = $state(/** @type {any} */ (null));

	/** The rows of one scope. */
	function rowsOf(scope) {
		return scope === 'document' ? documentParameters : parameters;
	}

	function startAddVariable(e, scope = 'tab') {
		e.stopPropagation();
		if (scope === 'document') documentVariablesExpanded = true;
		else variablesExpanded = true;
		// Suggest the first free varN name, free in BOTH scopes: a document
		// name a tab also declares is legal but shadowed, which is not what
		// someone pressing + is asking for.
		let n = 1;
		const names = new Set([
			...parameters.map((p) => p.name),
			...documentParameters.map((p) => p.name)
		]);
		while (names.has(`var${n}`)) n++;
		editingVariable = { id: null, name: `var${n}`, expression: '10', scope };
	}

	function startEditVariable(param, scope = 'tab') {
		editingVariable = { id: param.id, name: param.name, expression: param.expression, scope };
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
		const scope = edit.scope ?? 'tab';
		const list = rowsOf(scope).map((p) => ({ ...p }));
		/** @type {Array<[string, string]>} */
		const renames = [];
		if (edit.id === null) {
			list.push({ name, expression });
		} else {
			const row = list.find((p) => p.id === edit.id);
			if (!row) return;
			// A changed name on the same row is a RENAME, and its dependents
			// have to follow — other parameters and every feature field that
			// reads it. Sending only the new table would leave them reading a
			// name the document no longer has, and they would hold their
			// last-good geometry while saying so (P5).
			if (row.name !== name) renames.push([row.name, name]);
			row.name = name;
			row.expression = expression;
		}
		if (scope === 'document') {
			// A DOCUMENT rename is refused by the engine, because the rewrite
			// would have to reach every tab's expressions and this message
			// carries one table (P2). Say so here rather than sending an edit
			// that silently keeps the old name.
			if (renames.length > 0) {
				showToast(
					'error',
					`A document variable cannot be renamed in place: '${renames[0][0]}' is read by every ` +
						`tab. Add '${name}' alongside it, repoint the expressions that read ` +
						`'${renames[0][0]}', then delete it.`
				);
				return;
			}
			await setDocumentParameters(list);
			return;
		}
		await setParameters(list, renames);
	}

	function cancelVariableEdit() {
		editingVariable = null;
	}

	async function deleteVariable(e, param, scope = 'tab') {
		e.stopPropagation();
		if (blockedByAgent()) return;
		const kept = rowsOf(scope).filter((p) => p.id !== param.id).map((p) => ({ ...p }));
		if (scope === 'document') await setDocumentParameters(kept);
		else await setParameters(kept);
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

	// ─────────────────────────── Body properties (M1, specs/drawings_and_mbd.md §9)
	//
	// "The properties panel shows mass and centre of mass." There is no
	// properties panel: bodies are listed here, so the properties are a
	// disclosure on the body's own row — the same shape the assembly panel's
	// per-instance `pos` disclosure has, and for the same reason: the numbers
	// belong to one row, and a separate panel would need a selection model of
	// its own to say which row it is showing.
	//
	// Measured on OPEN rather than on every rebuild. `MeasureMass` is an
	// integration over a body's faces; running it for every body on every
	// rebuild would cost the whole tree's geometry to show numbers nobody
	// asked to see.

	/** Which bodies' properties are disclosed. @type {Set<string>} */
	let propsOpen = $state(new Set());
	/**
	 * The last `MassMeasured` per body, or `{ error }`. Keyed by bodyId so a
	 * body whose row moved keeps its own numbers.
	 * @type {Record<string, any>}
	 */
	let bodyProps = $state({});

	async function handleBodyPropsToggle(e, bodyId) {
		e.stopPropagation();
		const next = new Set(propsOpen);
		if (next.has(bodyId)) {
			next.delete(bodyId);
			propsOpen = next;
			return;
		}
		next.add(bodyId);
		propsOpen = next;
		// Re-measured on each open: the body may have been rebuilt since.
		const { [bodyId]: _drop, ...rest } = bodyProps;
		bodyProps = rest;
		bodyProps = { ...bodyProps, [bodyId]: await readProps(bodyId) };
	}

	/**
	 * One body's properties, by one measurement where possible.
	 *
	 * `MeasureMass` integrates everything at once, so its numbers are
	 * guaranteed to be at one tier. When it refuses — a body with no material
	 * has no mass, and the engine says so rather than defaulting the density —
	 * the geometry is still measurable: `MeasureBody` fills the volume and
	 * area rows and the mass row names what is missing. Losing the volume
	 * because the mass is unavailable would be the worse answer.
	 */
	async function readProps(bodyId) {
		try {
			const measured = await measureBodyMass(bodyId);
			if (measured) return measured;
		} catch (err) {
			const refusal = String(err?.message ?? err);
			try {
				const geometry = await measureBodyGeometry(bodyId);
				if (geometry) {
					return {
						volume_m3: geometry.volume_m3?.value,
						surface_area_m2: geometry.surface_area_m2?.value,
						method: geometry.volume_m3?.method ?? 'mesh',
						chord_bound_m: 0,
						centroid: null,
						massRefusal: refusal
					};
				}
			} catch {
				// Both refused: report the FIRST refusal, which is the one
				// about the measurement that was asked for.
			}
			// Named, not swallowed: a body the kernel cannot integrate is a
			// capability gap worth seeing, not an empty row.
			return { error: refusal };
		}
		return { error: 'the engine did not measure this body' };
	}

	/** The engine's material table, for the per-body picker. */
	let materials = $derived(getMaterials());
	/** The body whose material assignment is in flight. @type {string|null} */
	let busyBody = $state(null);

	async function assignMaterial(bodyId, name) {
		busyBody = bodyId;
		try {
			await setBodyMaterial(bodyId, name || null);
			// The assignment changes the density, so the mass is a different
			// number: re-measure rather than leaving the old one on screen.
			const measured = await measureBodyMass(bodyId);
			if (measured) bodyProps = { ...bodyProps, [bodyId]: measured };
		} catch (err) {
			bodyProps = { ...bodyProps, [bodyId]: { error: String(err?.message ?? err) } };
		} finally {
			busyBody = null;
		}
	}

	/** Six significant digits is a measured quantity's honest width here. */
	function sig(x, digits = 6) {
		if (!Number.isFinite(x)) return '—';
		return String(parseFloat(x.toPrecision(digits)));
	}

	/**
	 * The NAME of the material a body is made of, or null.
	 *
	 * Read off the feature tree's own `body_materials` side table — the
	 * engine's assignment, mirrored into this store already, so there is no
	 * material list in JavaScript and no second answer to "what is this body
	 * made of". `MeasureMass` reports the DENSITY it used but not the name,
	 * which is why the name comes from here and the number from there.
	 *
	 * @param {string} bodyId
	 */
	function materialOf(bodyId) {
		const name = tree?.body_materials?.[bodyId];
		return typeof name === 'string' && name.length > 0 ? name : null;
	}

	/** Whether this body carries a material. */
	function hasMaterial(p, bodyId) {
		if (materialOf(bodyId)) return true;
		// The ENGINE decides, and it says so explicitly: since M1's review a
		// body with no material comes back with `density_kg_m3` and `mass_kg`
		// null and `mass_unavailable` naming the remedy, rather than measured
		// at a default density of 1 where `mass_kg` would be the volume in m³.
		// So a density present AT ALL means the mass is a real one — which
		// also covers a density supplied some other way (a caller's explicit
		// one), where there is a real mass but no assignment on the tree.
		//
		// `typeof` and not `Number.isFinite(Number(x))`: `Number(null)` is
		// ZERO, which is finite, so the coercing form reads the engine's
		// explicit "no density" as a density of 0 and calls it a material.
		return typeof p?.density_kg_m3 === 'number' && Number.isFinite(p.density_kg_m3);
	}

	function propMethod(p) {
		return p?.method === 'exact' ? 'exact' : `mesh (±${sig(lenOf(p?.chord_bound_m), 3)})`;
	}
	function propMethodTitle(p) {
		return p?.method === 'exact'
			? 'Integrated over the analytic faces: exact.'
			: 'Integrated over the TESSELLATION. Every number below carries the ' +
					`chord band ${sig(p?.chord_bound_m, 3)} m, so none of them is exact.`;
	}

	/** A length in meters, in the document's display unit. */
	function lenOf(meters) {
		const s = getDocumentDisplaySettings();
		return internalToDisplay(Number(meters), s.unit);
	}
	function unitLabel() {
		const s = getDocumentDisplaySettings();
		return UNITS[s.unit]?.label ?? s.unit;
	}
	/** The display unit's own factor, for the squared and cubed quantities. */
	function perMeter() {
		const s = getDocumentDisplaySettings();
		return UNITS[s.unit]?.fromMeters ?? 1;
	}

	function propVolume(p) {
		const k = perMeter();
		return `${sig(Number(p?.volume_m3) * k * k * k)} ${unitLabel()}³`;
	}
	function propArea(p) {
		const k = perMeter();
		return `${sig(Number(p?.surface_area_m2) * k * k)} ${unitLabel()}²`;
	}
	function propMaterial(p, bodyId) {
		const name = materialOf(bodyId);
		if (name) return `${name} (${sig(p?.density_kg_m3)} kg/m³)`;
		if (hasMaterial(p, bodyId)) return `${sig(p?.density_kg_m3)} kg/m³`;
		return 'none';
	}
	function propMass(p, bodyId) {
		// The engine refused the whole measurement. Its own words, because
		// the reason is the kernel's to give.
		if (p?.massRefusal) return p.massRefusal;
		// Or it measured the geometry and reported that there is no MASS:
		// `mass_unavailable` is the engine saying this body has no material,
		// which it does instead of handing back a density of 1 (where
		// `mass_kg` would be numerically the volume in m³ — a number that is
		// not the mass of anything).
		if (p?.mass_unavailable || !hasMaterial(p, bodyId)) return 'no material assigned';
		const kg = Number(p?.mass_kg);
		if (!Number.isFinite(kg)) return '—';
		return kg < 1 ? `${sig(kg * 1000)} g` : `${sig(kg)} kg`;
	}
	function propMassTitle(p, bodyId) {
		// The engine's own sentence when it has one — it names the remedy
		// (`body_material_set`) more precisely than this component can.
		if (p?.mass_unavailable) return p.mass_unavailable;
		return hasMaterial(p, bodyId)
			? `At ${sig(p?.density_kg_m3)} kg/m³.`
			: 'Assign a material to this body and its mass follows from the volume.';
	}
	function propCentroid(p) {
		const c = p?.centroid;
		// The geometry-only fallback has no centroid to report; `MeasureBody`
		// does not compute one, and inventing the bbox centre instead would
		// be a different quantity under the same label.
		if (!Array.isArray(c) || c.length !== 3) return '—';
		const s = getDocumentDisplaySettings();
		return `${c.map((v) => fixedAt(lenOf(v), s.precision)).join(', ')} ${unitLabel()}`;
	}
	function fixedAt(x, places) {
		if (!Number.isFinite(x)) return '—';
		const t = x.toFixed(places);
		return Number(t) === 0 ? (0).toFixed(places) : t;
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
			case 'Sketch3d': return '\u2727';
			case 'Extrude': return '\u25A7';
			case 'Revolve': return '\u21BB';
			case 'Pipe': return '\u2312';
			case 'Sweep': return '\u2933';
			case 'Fillet': return '\u25CF';
			case 'Chamfer': return '\u25C6';
			case 'Shell': return '\u25A1';
			case 'BooleanCombine': return '\u2229';
			case 'UnionAll': return '\u222A';
			case 'ImportedBody': return '\u2913';
			case 'MateConnector': return '\u2295';
			case 'PatternCircular': return '\u25CC';
			case 'PatternLinear': return '\u2237';
			case 'PatternMirror': return '\u2AEB';
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
	{#snippet variableRows(rows, scope)}
		{#each rows as param (param.id)}
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
					class:variable-shadowed={scope === 'document' && shadowedNames.has(param.name)}
					role="treeitem"
					tabindex="0"
					title={param.error
						? param.error
						: scope === 'document' && shadowedNames.has(param.name)
							? `${param.name} = ${param.expression} → ${formatVariableValue(param)} — SHADOWED: this tab declares its own '${param.name}', which is what its expressions read`
							: `${param.name} = ${param.expression} → ${formatVariableValue(param)}`}
					onclick={() => startEditVariable(param, scope)}
					onkeydown={(e) => { if (e.key === 'Enter') startEditVariable(param, scope); }}
					data-testid={scope === 'document'
						? `document-variable-row-${param.name}`
						: `variable-row-${param.name}`}
				>
					<span class="variable-name">{param.name}</span>
					<span class="variable-eq">=</span>
					<span class="variable-expr">{param.expression}</span>
					<span
						class="variable-value"
						data-testid={scope === 'document'
							? `document-variable-value-${param.name}`
							: `variable-value-${param.name}`}
					>{param.error ? '⚠' : formatVariableValue(param)}</span>
					<button
						class="variable-delete"
						title="Delete variable"
						onclick={(e) => deleteVariable(e, param, scope)}
						data-testid={scope === 'document'
							? `document-variable-delete-${param.name}`
							: `variable-delete-${param.name}`}
					>×</button>
				</div>
			{/if}
		{/each}
		{#if editingVariable && editingVariable.id === null && (editingVariable.scope ?? 'tab') === scope}
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
		{#if rows.length === 0 && !(editingVariable && (editingVariable.scope ?? 'tab') === scope)}
			<div class="variable-empty">
				{scope === 'document' ? 'No document variables — press + to add' : 'No variables — press + to add'}
			</div>
		{/if}
	{/snippet}

		<!-- Document variables (P2): above the tabs, read by every tab after
		     its own table. Collapsed by default — a document with none should
		     not grow a second empty section in front of the tree. -->
		<div class="origin-section" data-testid="document-variables-section">
			<div class="origin-header variables-header">
				<button
					class="origin-header variables-toggle"
					onclick={() => documentVariablesExpanded = !documentVariablesExpanded}
					data-testid="document-variables-toggle"
				>
					<span class="expand-icon">{documentVariablesExpanded ? '▾' : '▸'}</span>
					<span class="origin-label">Document variables</span>
					{#if documentParameters.length > 0 && !documentVariablesExpanded}
						<span class="variable-count">{documentParameters.length}</span>
					{/if}
				</button>
				<button
					class="variable-add"
					title="Add a DOCUMENT variable — every tab resolves it after its own table, so one value can drive two Parts. Not an undo step."
					onclick={(e) => startAddVariable(e, 'document')}
					data-testid="document-variable-add"
				>+</button>
			</div>
			{#if documentVariablesExpanded}
				{@render variableRows(documentParameters, 'document')}
			{/if}
		</div>

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
					onclick={(e) => startAddVariable(e, 'tab')}
					data-testid="variable-add"
				>+</button>
			</div>
			{#if variablesExpanded}
				{@render variableRows(parameters, 'tab')}
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
					{:else if featureWarnings.get(feature.id)}
						<!--
							N2 (`specs/agent_mechanical_design.md` §5.3 item 4): a
							reference that rebound by geometry, or a sketch whose face
							has moved, is persistent state about THIS feature — a toast
							scrolls away, so it goes on the row. The same affordance as
							the error glyph, in the warning colour; an error wins,
							because a feature that failed has nothing to warn about.
						-->
						<button
							class="error-indicator-btn warning-indicator-btn"
							title={featureWarnings.get(feature.id).join('\n')}
							data-testid="feature-warning-{i}"
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
					<div class="source-tools">
						<button
							class="src-action"
							data-testid="sources-link-kicad"
							title="Link a .kicad_pcb from GitHub, GitLab or Gitea: the board becomes an exact solid, its footprints an assembly"
							onclick={() => showImportLinkDialog('kicad')}
							disabled={sourceBusy !== null}
						>link KiCad…</button>
					</div>
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
							class:with-props={propsOpen.has(body.bodyId)}
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
								title="Volume, mass and centre of mass"
								data-testid="body-props-toggle-{i}"
								onclick={(e) => handleBodyPropsToggle(e, body.bodyId)}
							>
								{propsOpen.has(body.bodyId) ? '▾' : '▸'}&#8201;ƒ
							</button>
							<button
								class="visibility-toggle"
								title={isBodyVisible(body.bodyId) ? 'Hide body' : 'Show body'}
								data-testid="body-visibility-{i}"
								onclick={(e) => handleBodyVisibilityToggle(e, body.bodyId)}
							>
								{isBodyVisible(body.bodyId) ? '◉' : '◎'}
							</button>
						</div>
						{#if propsOpen.has(body.bodyId)}
							<!-- The body's measured properties (M1). Every number here
							     came from the engine's one integration, and the row
							     says which TIER it came from: an exact answer and a
							     mesh approximation must never look alike. -->
							<dl class="body-props" data-testid="body-props-{i}">
								{#if bodyProps[body.bodyId]?.error}
									<div class="prop-error" data-testid="body-props-error-{i}">
										{bodyProps[body.bodyId].error}
									</div>
								{:else if !bodyProps[body.bodyId]}
									<div class="prop-pending" data-testid="body-props-pending-{i}">measuring…</div>
								{:else}
									{@const p = bodyProps[body.bodyId]}
									<div class="prop">
										<dt>method</dt>
										<dd data-testid="body-prop-method-{i}" title={propMethodTitle(p)}>
											{propMethod(p)}
										</dd>
									</div>
									<div class="prop">
										<dt>volume</dt>
										<dd data-testid="body-prop-volume-{i}">{propVolume(p)}</dd>
									</div>
									<div class="prop">
										<dt>area</dt>
										<dd data-testid="body-prop-area-{i}">{propArea(p)}</dd>
									</div>
									<div class="prop">
										<dt>material</dt>
										<dd data-testid="body-prop-material-{i}">
											{#if materials.length > 0}
												<!-- The options are the ENGINE's material table off the
												     feature tree, never a list invented here. With an
												     empty table there is nothing to choose between, so
												     the row reads rather than offers. -->
												<select
													class="material-select"
													data-testid="body-material-select-{i}"
													disabled={busyBody === body.bodyId}
													value={materialOf(body.bodyId) ?? ''}
													onclick={(e) => e.stopPropagation()}
													onchange={(e) => assignMaterial(body.bodyId, e.currentTarget.value)}
												>
													<option value="">none</option>
													{#each materials as m (m.name)}
														<option value={m.name}>{m.name}</option>
													{/each}
												</select>
											{:else}
												{propMaterial(p, body.bodyId)}
											{/if}
										</dd>
									</div>
									<div class="prop">
										<dt>mass</dt>
										<dd data-testid="body-prop-mass-{i}" title={propMassTitle(p, body.bodyId)}>
											{propMass(p, body.bodyId)}
										</dd>
									</div>
									<div class="prop">
										<dt>centre of mass</dt>
										<dd data-testid="body-prop-centroid-{i}">{propCentroid(p)}</dd>
									</div>
								{/if}
							</dl>
						{/if}
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

	/* A document row this tab redeclares. Shown struck through rather than
	   hidden: "why is my document variable not driving this" is answered by
	   seeing it listed and visibly overridden. */
	.variable-shadowed .variable-name,
	.variable-shadowed .variable-expr,
	.variable-shadowed .variable-value {
		opacity: 0.55;
		text-decoration: line-through;
	}

	/* How many document variables a collapsed section is hiding. */
	.variable-count {
		margin-left: 4px;
		padding: 0 4px;
		border-radius: 6px;
		background: var(--bg-tertiary, #333);
		color: var(--text-secondary, #999);
		font-size: 9px;
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
	/* An open disclosure keeps its own row highlighted, so the numbers below
	   read as belonging to it rather than to the next body down. */
	.body-item.with-props {
		background: var(--bg-hover);
	}
	/* The body's measured properties (M1). Scrolls rather than overflows if a
	   narrow panel cannot fit a value, per CLAUDE.md's chrome rule. */
	.body-props {
		margin: 0;
		padding: 2px 12px 6px 30px;
		font-size: 11px;
		color: var(--text-secondary);
		overflow-x: auto;
	}
	.body-props .prop {
		display: flex;
		gap: 6px;
		justify-content: space-between;
		white-space: nowrap;
	}
	.body-props dt {
		opacity: 0.75;
	}
	.body-props dd {
		margin: 0;
		font-variant-numeric: tabular-nums;
	}
	.body-props .material-select {
		font: inherit;
		color: inherit;
		background: var(--bg-primary);
		border: 1px solid var(--border);
		border-radius: 3px;
		max-width: 140px;
	}
	.body-props .prop-pending {
		opacity: 0.7;
	}
	.body-props .prop-error {
		color: var(--error, #d33);
		white-space: normal;
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

	.warning-indicator-btn {
		color: var(--warning, #e0a040);
	}

	.warning-indicator-btn:hover {
		background: rgba(224, 160, 64, 0.15);
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
