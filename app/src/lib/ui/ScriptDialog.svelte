<script>
	/**
	 * Custom feature script dialog (specs/custom_features_and_modeling_roadmap.md
	 * §A8, A-M4): pick a Script source of the document (or add one — a new
	 * script in the editor, or a built-in library script), and the fields are
	 * GENERATED from the script's `@param` header as the engine reports it
	 * (`CheckScript` → `interface`): numeric parameters take a measurement in
	 * the display unit or an expression over the design variables (mm-space,
	 * stored as `arg_exprs`), a `plane` takes a datum plane or the selected
	 * planar face, `body`/`face`/`edge` take a pick. Apply adds ONE Script
	 * node (or replaces the edited node's arguments).
	 */
	import {
		getScriptDialogState,
		hideScriptDialog,
		setScriptDialogSource,
		applyScript,
		getScriptSources,
		addScriptSource,
		showScriptEditor,
		checkScript,
		getBodies,
		getSelectedRefs,
		computeFacePlane,
		getFeatureTree,
		getFeatureErrors,
		evaluateExpression,
		getDocumentDisplayUnit,
		SCRIPT_LIBRARY
	} from '$lib/engine/store.svelte.js';
	import { showToast } from '$lib/ui/toast.svelte.js';
	import { log } from '$lib/engine/logger.js';
	import { getAllPlanes, resolvePlane } from '$lib/engine/planes.js';
	import { parseAndConvert, formatForInput, isPlainMeasurement, UNITS } from '$lib/units.js';

	let dialogState = $derived(getScriptDialogState());
	let sources = $derived(getScriptSources());
	let displayUnit = $derived(getDocumentDisplayUnit());
	let unitLabel = $derived(UNITS[displayUnit]?.label ?? displayUnit);
	let features = $derived(getFeatureTree()?.features ?? []);
	let bodies = $derived(getBodies());
	let featureErrors = $derived(getFeatureErrors());

	const NUMERIC = new Set(['int', 'number', 'length', 'angle']);
	const SELECTED_FACE = '__selected_face__';
	const CUSTOM_PLANE = '__custom__';

	// The interface of the selected source, from the engine.
	let iface = $state(null);
	let ifaceError = $state(null);
	let loadingIface = $state(false);
	let checkToken = 0;

	// Field state, keyed by parameter name.
	let values = $state({});
	// Expression evaluation per numeric field (mm-space).
	let evals = $state({});
	let evalTokens = {};

	// Datum planes (built-in + user), resolved.
	let allPlanes = $derived(
		getAllPlanes(features).map((p) => {
			let resolved;
			try {
				resolved = resolvePlane(p.definition, features, computeFacePlane);
			} catch {
				resolved = { origin: [0, 0, 0], normal: [0, 0, 1] };
			}
			return { id: p.id, label: p.name, origin: resolved.origin, normal: resolved.normal };
		})
	);
	let selectedFace = $derived(getSelectedRefs().find((r) => r?.kind?.type === 'Face') ?? null);
	let selectedEdge = $derived(getSelectedRefs().find((r) => r?.kind?.type === 'Edge') ?? null);
	let selectedFacePlane = $derived(selectedFace ? computeFacePlane(selectedFace) : null);

	let sourceId = $derived(dialogState?.sourceId ?? null);
	let editParams = $derived(dialogState?.editParams ?? null);
	let nodeError = $derived(dialogState?.editingFeatureId ? featureErrors.get(dialogState.editingFeatureId) ?? null : null);

	// Load the interface whenever the source changes (and on open).
	$effect(() => {
		const id = sourceId;
		const open = !!dialogState;
		if (!open) return;
		iface = null;
		ifaceError = null;
		if (!id) return;
		loadingIface = true;
		const token = ++checkToken;
		checkScript({ sourceId: id }).then((check) => {
			if (token !== checkToken) return;
			loadingIface = false;
			if (check.ok) {
				iface = check.interface;
				ifaceError = null;
				initValues(check.interface);
			} else {
				iface = null;
				ifaceError = check.error ?? { stage: 'check', reason: 'unknown' };
			}
		});
	});

	/** Initial field values: the edited node's arguments, else the header defaults. */
	function initValues(spec) {
		const next = {};
		const ep = editParams;
		for (const p of spec.params) {
			const expr = ep?.arg_exprs?.[p.name];
			const arg = ep?.args?.[p.name];
			if (NUMERIC.has(p.type)) {
				if (typeof expr === 'string') next[p.name] = expr;
				else if (typeof arg === 'number') next[p.name] = p.type === 'length' ? formatForInput(arg, displayUnit) : String(arg);
				else if (typeof p.default === 'number') next[p.name] = p.type === 'length' ? formatForInput(p.default, displayUnit) : String(p.default);
				else next[p.name] = '';
			} else if (p.type === 'bool') {
				next[p.name] = typeof arg === 'boolean' ? arg : !!p.default;
			} else if (p.type === 'string') {
				next[p.name] = typeof arg === 'string' ? arg : (p.default ?? '');
			} else if (p.type === 'plane') {
				next[p.name] = planeChoiceFor(arg);
			} else {
				// body / face / edge: a GeomRef or null
				next[p.name] = arg && typeof arg === 'object' ? JSON.parse(JSON.stringify(arg)) : null;
			}
		}
		values = next;
		evals = {};
	}

	/** The plane option an existing argument corresponds to. */
	function planeChoiceFor(arg) {
		if (typeof arg === 'string') return { id: arg };
		if (arg && typeof arg === 'object' && Array.isArray(arg.normal)) {
			const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-9);
			const match = allPlanes.find((p) => near(p.origin, arg.origin ?? [0, 0, 0]) && near(p.normal, arg.normal));
			if (match) return { id: match.id };
			return { id: CUSTOM_PLANE, origin: arg.origin ?? [0, 0, 0], normal: arg.normal };
		}
		return { id: allPlanes[0]?.id ?? CUSTOM_PLANE, origin: [0, 0, 0], normal: [0, 0, 1] };
	}

	function isExpr(name) {
		const v = values[name];
		return typeof v === 'string' && v.trim() !== '' && !isPlainMeasurement(v);
	}

	// Evaluate expression-valued numeric fields as they change.
	$effect(() => {
		const spec = iface;
		if (!spec) return;
		for (const p of spec.params) {
			if (!NUMERIC.has(p.type)) continue;
			const text = values[p.name];
			if (!isExpr(p.name)) {
				if (evals[p.name]) evals = { ...evals, [p.name]: null };
				continue;
			}
			const token = (evalTokens[p.name] = (evalTokens[p.name] ?? 0) + 1);
			evaluateExpression(text.trim()).then((result) => {
				if (token === evalTokens[p.name]) evals = { ...evals, [p.name]: result };
			});
		}
	});

	/** A numeric field's model-unit value (NaN when unparseable / unevaluated). */
	function numericValue(p) {
		const text = values[p.name] ?? '';
		if (isExpr(p.name)) {
			const ev = evals[p.name];
			if (!ev || ev.value == null) return NaN;
			return p.type === 'length' ? ev.value * 0.001 : ev.value;
		}
		if (p.type === 'length') return parseAndConvert(text, displayUnit);
		const n = parseFloat(text);
		return Number.isFinite(n) ? n : NaN;
	}

	function fieldProblem(p) {
		if (NUMERIC.has(p.type)) {
			const text = values[p.name] ?? '';
			if (text.trim() === '') return p.default === undefined ? 'required' : null;
			if (isExpr(p.name)) {
				const ev = evals[p.name];
				if (!ev) return 'evaluating…';
				if (ev.error) return ev.error;
			}
			const v = numericValue(p);
			if (!Number.isFinite(v)) return 'not a number';
			if (p.type === 'int' && !Number.isInteger(v)) return 'must be an integer';
			if (p.min != null && v < p.min) return `below minimum ${p.type === 'length' ? formatForInput(p.min, displayUnit) + ' ' + unitLabel : p.min}`;
			if (p.max != null && v > p.max) return `above maximum ${p.type === 'length' ? formatForInput(p.max, displayUnit) + ' ' + unitLabel : p.max}`;
			return null;
		}
		if (p.type === 'plane') {
			const c = values[p.name];
			if (!c) return 'required';
			if (c.id === SELECTED_FACE && !selectedFacePlane) return 'select a planar face';
			return null;
		}
		if (p.type === 'body' || p.type === 'face' || p.type === 'edge') {
			return values[p.name] ? null : 'required';
		}
		return null;
	}

	let problems = $derived(iface ? Object.fromEntries(iface.params.map((p) => [p.name, fieldProblem(p)])) : {});
	let canApply = $derived(!!iface && !!sourceId && Object.values(problems).every((x) => x == null));

	function bodyToGeomRef(body) {
		return {
			kind: { type: 'Solid' },
			anchor: { type: 'FeatureOutput', feature_id: body.featureId, output_key: body.outputKey ?? { type: 'Main' } },
			selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
			policy: { type: 'BestEffort' }
		};
	}

	function bodyKey(ref) {
		const a = ref?.anchor;
		return a?.feature_id ? `${a.feature_id}/${a.output_key?.type ?? 'Main'}${a.output_key?.index != null ? ':' + a.output_key.index : ''}${a.output_key?.name ? ':' + a.output_key.name : ''}` : '';
	}

	function bodyOptionKey(body) {
		return bodyKey(bodyToGeomRef(body));
	}

	/** Build `{ args, argExprs }` from the fields. */
	function collect() {
		const args = {};
		const argExprs = {};
		for (const p of iface.params) {
			const v = values[p.name];
			if (NUMERIC.has(p.type)) {
				if ((v ?? '').trim() === '') continue; // header default
				if (isExpr(p.name)) argExprs[p.name] = v.trim();
				else args[p.name] = p.type === 'int' ? Math.round(numericValue(p)) : numericValue(p);
			} else if (p.type === 'bool' || p.type === 'string') {
				args[p.name] = v;
			} else if (p.type === 'plane') {
				if (v.id === SELECTED_FACE) {
					args[p.name] = { origin: [...selectedFacePlane.origin], normal: [...selectedFacePlane.normal] };
				} else if (v.id === CUSTOM_PLANE) {
					args[p.name] = { origin: [...v.origin], normal: [...v.normal] };
				} else {
					const plane = allPlanes.find((pl) => pl.id === v.id);
					args[p.name] = plane ? { origin: [...plane.origin], normal: [...plane.normal] } : v.id;
				}
			} else {
				args[p.name] = v;
			}
		}
		return { args, argExprs };
	}

	function handleApply() {
		if (!canApply) return;
		const { args, argExprs } = collect();
		applyScript({ sourceId, entry: editParams?.entry ?? 'feature', args, argExprs }).catch((err) =>
			log('error', `Script dialog apply failed: ${err}`)
		);
	}

	function handleCancel() {
		hideScriptDialog();
	}

	async function handleSourceChange(e) {
		const choice = e.currentTarget.value;
		if (choice === '__new__') {
			e.currentTarget.value = sourceId ?? '';
			await showScriptEditor(null, { forDialog: true });
			return;
		}
		if (choice.startsWith('lib:')) {
			const library = choice.slice(4);
			const added = await addScriptSource({ library });
			if (added) setScriptDialogSource(added.source_id);
			else showToast('error', 'The library script could not be added');
			return;
		}
		setScriptDialogSource(choice || null);
	}

	function usePick(p) {
		const ref = p.type === 'face' ? selectedFace : p.type === 'edge' ? selectedEdge : null;
		if (!ref) return;
		values = { ...values, [p.name]: JSON.parse(JSON.stringify(ref)) };
	}

	$effect(() => {
		if (!dialogState) return;
		function onKeyDown(e) {
			if (e.key === 'Enter' && e.target?.tagName !== 'TEXTAREA') {
				e.preventDefault();
				e.stopPropagation();
				handleApply();
			} else if (e.key === 'Escape') {
				e.preventDefault();
				e.stopPropagation();
				handleCancel();
			}
		}
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});

	function paramLabel(p) {
		if (p.type === 'length') return `${p.name} (${unitLabel})`;
		if (p.type === 'angle') return `${p.name} (°)`;
		return p.name;
	}
</script>

{#if dialogState}
	<div class="script-panel" data-testid="script-dialog">
		<div class="dialog-header">
			<span class="dialog-title">{dialogState.editingFeatureId ? 'Edit script feature' : 'Script feature'}</span>
			<button class="close-btn" onclick={handleCancel}>&times;</button>
		</div>
		<div class="dialog-body">
			<div class="field">
				<label for="script-source">Script</label>
				<select id="script-source" data-testid="script-source" value={sourceId ?? ''} onchange={handleSourceChange}>
					{#if !sourceId}<option value="">— choose —</option>{/if}
					{#each sources as s (s.id)}
						<option value={s.id}>{s.name}</option>
					{/each}
					<option value="__new__">New script…</option>
					{#each SCRIPT_LIBRARY as lib (lib.id)}
						<option value={'lib:' + lib.id}>Add built-in: {lib.label}</option>
					{/each}
				</select>
			</div>
			{#if sourceId}
				<div class="field">
					<span class="field-hint" data-testid="script-iface-name">{iface ? `${iface.name} v${iface.version}` : loadingIface ? 'reading…' : ''}</span>
					<button class="btn btn-small" data-testid="script-edit-source" onclick={() => showScriptEditor(sourceId)}>Edit script…</button>
				</div>
			{/if}
			{#if ifaceError}
				<div class="expr-hint expr-error" data-testid="script-iface-error">script {ifaceError.stage}: {ifaceError.reason}</div>
			{/if}
			{#if iface}
				{#each iface.params as p (p.name)}
					<div class="field param" data-testid="script-param-{p.name}">
						<label for="script-param-{p.name}">{paramLabel(p)}</label>
						{#if NUMERIC.has(p.type)}
							<input
								id="script-param-{p.name}"
								data-testid="script-input-{p.name}"
								type="text"
								bind:value={values[p.name]}
								placeholder={p.default != null ? String(p.type === 'length' ? formatForInput(p.default, displayUnit) : p.default) : 'required'}
							/>
						{:else if p.type === 'bool'}
							<input id="script-param-{p.name}" data-testid="script-input-{p.name}" type="checkbox" bind:checked={values[p.name]} />
						{:else if p.type === 'string'}
							<input id="script-param-{p.name}" data-testid="script-input-{p.name}" type="text" bind:value={values[p.name]} />
						{:else if p.type === 'plane'}
							<select
								id="script-param-{p.name}"
								data-testid="script-input-{p.name}"
								value={values[p.name]?.id ?? ''}
								onchange={(e) => {
									const id = e.currentTarget.value;
									values = { ...values, [p.name]: id === CUSTOM_PLANE ? values[p.name] : { id } };
								}}
							>
								{#each allPlanes as pl (pl.id)}
									<option value={pl.id}>{pl.label}</option>
								{/each}
								<option value={SELECTED_FACE} disabled={!selectedFacePlane}>Selected face{selectedFacePlane ? '' : ' (none)'}</option>
								{#if values[p.name]?.id === CUSTOM_PLANE}
									<option value={CUSTOM_PLANE}>Custom (n = {values[p.name].normal.map((c) => +c.toFixed(3)).join(', ')})</option>
								{/if}
							</select>
						{:else if p.type === 'body'}
							<select
								id="script-param-{p.name}"
								data-testid="script-input-{p.name}"
								value={values[p.name] ? bodyKey(values[p.name]) : ''}
								onchange={(e) => {
									const key = e.currentTarget.value;
									const body = bodies.find((b) => bodyOptionKey(b) === key);
									values = { ...values, [p.name]: body ? bodyToGeomRef(body) : null };
								}}
							>
								<option value="">— choose a body —</option>
								{#each bodies as b (b.bodyId)}
									<option value={bodyOptionKey(b)}>{b.name}</option>
								{/each}
							</select>
						{:else}
							<span class="pick-row">
								<span class="field-hint" data-testid="script-pick-{p.name}">{values[p.name] ? `selected ${p.type}` : 'none'}</span>
								<button
									class="btn btn-small"
									data-testid="script-use-pick-{p.name}"
									disabled={!(p.type === 'face' ? selectedFace : selectedEdge)}
									onclick={() => usePick(p)}
								>use selection</button>
							</span>
						{/if}
						{#if NUMERIC.has(p.type) && isExpr(p.name)}
							<div class="expr-hint" class:expr-error={!!evals[p.name]?.error} data-testid="script-eval-{p.name}">
								{evals[p.name]?.error ? evals[p.name].error : evals[p.name]?.value != null ? `= ${parseFloat(evals[p.name].value.toFixed(4))}${p.type === 'length' ? ' mm' : ''}` : '…'}
							</div>
						{:else if problems[p.name]}
							<div class="expr-hint expr-error" data-testid="script-problem-{p.name}">{problems[p.name]}</div>
						{/if}
					</div>
				{/each}
				{#if iface.params.length === 0}
					<div class="pick-empty">This script takes no parameters.</div>
				{/if}
			{/if}
			{#if nodeError}
				<div class="expr-hint expr-error node-error" data-testid="script-node-error">{nodeError}</div>
			{/if}
		</div>
		<div class="dialog-footer">
			<button class="btn btn-cancel" data-testid="script-cancel" onclick={handleCancel}>Cancel</button>
			<button class="btn btn-apply" data-testid="script-apply" disabled={!canApply} onclick={handleApply}>Apply</button>
		</div>
	</div>
{/if}

<style>
	.script-panel {
		position: absolute;
		top: 12px;
		right: max(12px, env(safe-area-inset-right, 0px));
		width: 280px;
		max-height: calc(100vh - 80px);
		display: flex;
		flex-direction: column;
		z-index: 50;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		pointer-events: auto;
	}

	@media (max-width: 768px) {
		.script-panel {
			position: fixed;
			top: auto;
			right: 0;
			bottom: 0;
			left: 0;
			width: 100%;
			max-height: 60vh;
			border-radius: 12px 12px 0 0;
			z-index: 150;
			padding-bottom: env(safe-area-inset-bottom, 0px);
		}
	}

	.dialog-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-color, #444);
	}

	.dialog-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary, #eee);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 18px;
		cursor: pointer;
		padding: 0 2px;
		line-height: 1;
	}

	.close-btn:hover {
		color: var(--text-primary, #eee);
	}

	.dialog-body {
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		overflow-y: auto;
	}

	.field {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		flex-wrap: wrap;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #aaa);
		min-width: 50px;
		font-family: ui-monospace, monospace;
	}

	.field-hint {
		font-size: 11px;
		color: var(--text-secondary, #aaa);
	}

	.field input[type='text'],
	.field select {
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 4px 8px;
		border-radius: 3px;
		font-size: 12px;
		width: 130px;
	}

	.field input:focus,
	.field select:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.pick-row {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	.expr-hint {
		width: 100%;
		margin-top: 2px;
		font-size: 10px;
		font-family: ui-monospace, monospace;
		color: var(--text-secondary, #8a8);
	}

	.expr-error {
		color: var(--error-color, #f66);
	}

	.node-error {
		white-space: pre-wrap;
		word-break: break-word;
	}

	.pick-empty {
		font-size: 11px;
		color: var(--text-muted, #888);
		font-style: italic;
		padding: 4px 0;
	}

	.dialog-footer {
		display: flex;
		justify-content: flex-end;
		gap: 6px;
		padding: 8px 12px;
		border-top: 1px solid var(--border-color, #444);
	}

	.btn {
		padding: 5px 14px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid transparent;
	}

	.btn-small {
		padding: 2px 8px;
		font-size: 11px;
		background: transparent;
		color: var(--accent, #89b4fa);
		border-color: var(--border-color, #444);
	}

	.btn-small:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.btn-cancel {
		background: transparent;
		color: var(--text-secondary, #aaa);
		border-color: var(--border-color, #444);
	}

	.btn-cancel:hover {
		background: var(--bg-hover, #333);
	}

	.btn-apply {
		background: var(--accent, #0078d4);
		color: var(--text-on-accent);
		border-color: var(--accent, #0078d4);
	}

	.btn-apply:hover:not(:disabled) {
		filter: brightness(1.1);
	}

	.btn-apply:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
