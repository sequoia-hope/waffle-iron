<script>
	/**
	 * Custom feature script editor (specs/custom_features_and_modeling_roadmap.md
	 * §A8, A-M4): a monospace textarea over a Script source's text, Check
	 * (the engine parses the header, compiles, confirms the entry function —
	 * no second parser in the page) with the failing line, and Save, which
	 * replaces the source and regenerates every node using it (a new script
	 * becomes a Script source of the document). Not an undo step: sources
	 * are assets; the textarea keeps its own undo while open.
	 */
	import {
		getScriptEditorState,
		hideScriptEditor,
		saveScriptEditor,
		checkScript,
		getFeatureTree,
		getFeatureErrors
	} from '$lib/engine/store.svelte.js';
	import { log } from '$lib/engine/logger.js';

	let editorState = $derived(getScriptEditorState());
	let features = $derived(getFeatureTree()?.features ?? []);
	let featureErrors = $derived(getFeatureErrors());

	let name = $state('');
	let text = $state('');
	let savedText = $state('');
	let check = $state(null);
	let checking = $state(false);
	let saving = $state(false);
	let textarea = $state(null);
	let checkToken = 0;
	let debounce = null;

	let wasOpen = false;
	$effect(() => {
		const s = editorState;
		if (s && !wasOpen) {
			name = s.name ?? '';
			text = s.text ?? '';
			savedText = s.text ?? '';
			check = null;
			runCheck();
		}
		wasOpen = !!s;
	});

	let dirty = $derived(text !== savedText);
	let sourceId = $derived(editorState?.sourceId ?? null);
	let users = $derived(
		sourceId ? features.filter((f) => f.operation?.type === 'Script' && f.operation.params?.source_id === sourceId) : []
	);
	let userErrors = $derived(users.map((f) => ({ id: f.id, name: f.name, error: featureErrors.get(f.id) ?? null })).filter((u) => u.error));
	let lineCount = $derived(text.split('\n').length);

	/** The line an error message names: `line N:` (header) or `(line N, position M)` (parse). */
	function errorLine(err) {
		if (!err?.reason) return null;
		const m = err.reason.match(/\(line (\d+), position (\d+)\)/) ?? err.reason.match(/^line (\d+):/);
		return m ? { line: parseInt(m[1], 10), col: m[2] ? parseInt(m[2], 10) : 1 } : null;
	}
	let errorAt = $derived(check && !check.ok ? errorLine(check.error) : null);

	async function runCheck() {
		const token = ++checkToken;
		checking = true;
		const result = await checkScript({ text });
		if (token !== checkToken) return;
		checking = false;
		check = result;
	}

	function onInput() {
		if (debounce) clearTimeout(debounce);
		debounce = setTimeout(() => {
			debounce = null;
			runCheck();
		}, 400);
	}

	function goToLine(where) {
		if (!textarea || !where) return;
		const lines = text.split('\n');
		let offset = 0;
		for (let i = 0; i < where.line - 1 && i < lines.length; i++) offset += lines[i].length + 1;
		const end = offset + (lines[where.line - 1]?.length ?? 0);
		textarea.focus();
		textarea.setSelectionRange(offset, end);
	}

	async function handleSave(close = false) {
		if (saving) return;
		saving = true;
		try {
			const id = await saveScriptEditor({ name, text });
			if (id) {
				savedText = text;
				log('ui', 'Script saved', { sourceId: id });
				if (close) hideScriptEditor();
			}
		} finally {
			saving = false;
		}
	}

	function handleClose() {
		if (dirty && !confirm('Discard unsaved changes to this script?')) return;
		hideScriptEditor();
	}

	function onKeyDown(e) {
		if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			handleClose();
		} else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			e.stopPropagation();
			runCheck();
		} else if ((e.ctrlKey || e.metaKey) && e.key === 's') {
			e.preventDefault();
			e.stopPropagation();
			handleSave(false);
		} else if (e.key === 'Tab' && e.target === textarea) {
			// Indent instead of leaving the field.
			e.preventDefault();
			const start = textarea.selectionStart;
			const end = textarea.selectionEnd;
			text = text.slice(0, start) + '    ' + text.slice(end);
			requestAnimationFrame(() => textarea.setSelectionRange(start + 4, start + 4));
			onInput();
		}
	}

	$effect(() => {
		if (!editorState) return;
		window.addEventListener('keydown', onKeyDown, { capture: true });
		return () => window.removeEventListener('keydown', onKeyDown, { capture: true });
	});
</script>

{#if editorState}
	<div class="editor-backdrop" data-testid="script-editor">
		<div class="editor-panel">
			<div class="editor-header">
				<span class="editor-title">{sourceId ? 'Edit script' : 'New script'}</span>
				<input
					class="name-input"
					data-testid="script-editor-name"
					type="text"
					bind:value={name}
					placeholder={check?.ok && check.interface?.name ? check.interface.name : 'name'}
					title="Source name (defaults to the header's @feature name)"
				/>
				<span class="editor-meta" data-testid="script-editor-meta">
					{#if sourceId}{users.length} feature{users.length === 1 ? '' : 's'}{/if}
					{#if dirty}<span class="dirty" title="Unsaved changes">●</span>{/if}
				</span>
				<button class="close-btn" data-testid="script-editor-close" onclick={handleClose}>&times;</button>
			</div>
			<div class="editor-body">
				<div class="gutter" aria-hidden="true">
					{#each Array(lineCount) as _, i}
						<div class="gutter-line" class:error-line={errorAt?.line === i + 1}>{i + 1}</div>
					{/each}
				</div>
				<textarea
					class="source"
					data-testid="script-editor-text"
					bind:this={textarea}
					bind:value={text}
					oninput={onInput}
					spellcheck="false"
					wrap="off"
				></textarea>
			</div>
			<div class="editor-status" data-testid="script-editor-status" class:ok={check?.ok} class:bad={check && !check.ok}>
				{#if checking && !check}
					checking…
				{:else if check?.ok}
					✓ {check.interface.name} v{check.interface.version} — {check.interface.params.length} parameter{check.interface.params.length === 1 ? '' : 's'}, {check.interface.outputs.length} output{check.interface.outputs.length === 1 ? '' : 's'}
				{:else if check}
					<button class="error-link" data-testid="script-editor-error" onclick={() => goToLine(errorAt)} title={errorAt ? `Go to line ${errorAt.line}` : ''}>
						✗ {check.error?.stage}: {check.error?.reason}
					</button>
				{/if}
				{#each userErrors as u (u.id)}
					<div class="node-error" data-testid="script-editor-node-error">⚠ {u.name}: {u.error}</div>
				{/each}
			</div>
			<div class="editor-footer">
				<span class="hint">Ctrl+Enter check · Ctrl+S save</span>
				<button class="btn btn-cancel" data-testid="script-editor-check" onclick={runCheck} disabled={checking}>Check</button>
				<button class="btn btn-cancel" data-testid="script-editor-save" onclick={() => handleSave(false)} disabled={saving || (!dirty && !!sourceId)}>Save</button>
				<button class="btn btn-apply" data-testid="script-editor-save-close" onclick={() => handleSave(true)} disabled={saving}>Save &amp; close</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.editor-backdrop {
		position: fixed;
		inset: 0;
		z-index: 160;
		background: rgba(0, 0, 0, 0.35);
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: auto;
	}

	.editor-panel {
		width: min(760px, calc(100vw - 24px));
		height: min(80vh, 720px);
		display: flex;
		flex-direction: column;
		background: var(--bg-tertiary, #2d2d2d);
		border: 1px solid var(--border-color, #444);
		border-radius: 6px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
	}

	.editor-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-color, #444);
	}

	.editor-title {
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary, #eee);
		white-space: nowrap;
	}

	.name-input {
		flex: 1;
		min-width: 80px;
		background: var(--bg-primary, #1e1e1e);
		border: 1px solid var(--border-color, #444);
		color: var(--text-primary, #eee);
		padding: 3px 8px;
		border-radius: 3px;
		font-size: 12px;
	}

	.editor-meta {
		font-size: 11px;
		color: var(--text-secondary, #aaa);
		white-space: nowrap;
	}

	.dirty {
		color: var(--accent, #89b4fa);
		margin-left: 4px;
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

	.editor-body {
		flex: 1;
		display: flex;
		min-height: 0;
		background: var(--bg-primary, #1e1e1e);
		font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
		font-size: 12px;
		line-height: 18px;
	}

	.gutter {
		width: 40px;
		padding: 8px 0;
		text-align: right;
		color: var(--text-muted, #666);
		user-select: none;
		overflow: hidden;
		border-right: 1px solid var(--border-color, #444);
	}

	.gutter-line {
		padding-right: 8px;
		height: 18px;
	}

	.gutter-line.error-line {
		color: var(--error-color, #f66);
		font-weight: 700;
	}

	.source {
		flex: 1;
		resize: none;
		border: none;
		outline: none;
		padding: 8px 10px;
		background: transparent;
		color: var(--text-primary, #eee);
		font: inherit;
		line-height: inherit;
		tab-size: 4;
		white-space: pre;
	}

	.editor-status {
		padding: 6px 12px;
		font-size: 11px;
		font-family: ui-monospace, monospace;
		color: var(--text-secondary, #aaa);
		border-top: 1px solid var(--border-color, #444);
		max-height: 96px;
		overflow-y: auto;
	}

	.editor-status.ok {
		color: var(--success-color, #8c8);
	}

	.editor-status.bad,
	.node-error {
		color: var(--error-color, #f66);
	}

	.error-link {
		background: none;
		border: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
		padding: 0;
		text-align: left;
		white-space: pre-wrap;
	}

	.node-error {
		margin-top: 2px;
		white-space: pre-wrap;
	}

	.editor-footer {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		border-top: 1px solid var(--border-color, #444);
	}

	.hint {
		flex: 1;
		font-size: 10px;
		color: var(--text-muted, #888);
	}

	.btn {
		padding: 5px 14px;
		border-radius: 3px;
		font-size: 12px;
		cursor: pointer;
		border: 1px solid transparent;
	}

	.btn-cancel {
		background: transparent;
		color: var(--text-secondary, #aaa);
		border-color: var(--border-color, #444);
	}

	.btn-cancel:hover:not(:disabled) {
		background: var(--bg-hover, #333);
	}

	.btn-apply {
		background: var(--accent, #0078d4);
		color: var(--text-on-accent);
		border-color: var(--accent, #0078d4);
	}

	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
