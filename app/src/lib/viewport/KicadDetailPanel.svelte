<script>
	/**
	 * Board data on click (specs/kicad_board_link.md §1 goal 4, C4): the full
	 * record for the clicked body — the component (reference, value,
	 * footprint, side, datasheet, every pad with its net) or the board (title
	 * block, layers, thickness, nets) — and a link to the source file at the
	 * commit it was fetched from.
	 */
	import { getEntityDetail, hideEntityDetail } from '$lib/engine/store.svelte.js';

	let detail = $derived(getEntityDetail());
	let component = $derived(detail?.component ?? null);
	let board = $derived(detail?.board ?? null);
	let source = $derived(detail?.source ?? null);

	/** The file on its host at the commit it was fetched from, or null. */
	let sourceUrl = $derived.by(() => {
		const loc = source?.locator;
		if (!loc) return null;
		if (loc.type === 'Url') return loc.url;
		if (loc.type !== 'Git') return null;
		const at = source.resolved?.commit ?? loc.ref?.sha ?? loc.ref?.name ?? 'HEAD';
		const remote = loc.remote.replace(/\/$/, '');
		const host = loc.host ?? (remote.includes('gitlab') ? 'gitlab' : remote.includes('github') ? 'github' : 'gitea');
		if (host === 'gitlab') return `${remote}/-/blob/${at}/${loc.path}`;
		if (host === 'github') return `${remote}/blob/${at}/${loc.path}`;
		return `${remote}/src/commit/${at}/${loc.path}`;
	});

	let sourceLabel = $derived(source ? `${source.name}${source.resolved?.commit ? ' @ ' + source.resolved.commit.slice(0, 8) : ''}` : '');
</script>

{#if detail && (component || board)}
	<div class="kicad-panel" data-testid="kicad-detail-panel">
		<div class="head">
			<h3>
				{#if component}
					<span data-testid="kicad-detail-reference">{component.reference}</span>
					<span class="value">{component.value}</span>
				{:else}
					<span data-testid="kicad-detail-board">{board.title || 'Board'}</span>
				{/if}
			</h3>
			<button class="close" data-testid="kicad-detail-close" title="Close" onclick={hideEntityDetail}>×</button>
		</div>

		{#if component}
			<dl>
				<dt>Footprint</dt><dd class="mono">{component.footprint}</dd>
				<dt>Side</dt><dd>{component.side}</dd>
				{#if component.datasheet}
					<dt>Datasheet</dt>
					<dd><a href={component.datasheet} target="_blank" rel="noopener noreferrer" data-testid="kicad-detail-datasheet">{component.datasheet}</a></dd>
				{/if}
				{#if component.attrs.length}
					<dt>Attributes</dt><dd>{component.attrs.join(', ')}</dd>
				{/if}
				<dt>UUID</dt><dd class="mono dim">{component.footprint_uuid}</dd>
			</dl>
			{#if component.pads.length}
				<table data-testid="kicad-detail-pads">
					<thead><tr><th>Pad</th><th>Net</th></tr></thead>
					<tbody>
						{#each component.pads as pad}
							<tr><td class="mono">{pad.number || '—'}</td><td class="mono">{pad.net_name || '—'}</td></tr>
						{/each}
					</tbody>
				</table>
			{/if}
		{:else}
			<dl>
				{#if board.rev}<dt>Revision</dt><dd>{board.rev}</dd>{/if}
				{#if board.date}<dt>Date</dt><dd>{board.date}</dd>{/if}
				{#if board.company}<dt>Company</dt><dd>{board.company}</dd>{/if}
				<dt>Copper layers</dt><dd>{board.copper_layers}</dd>
				<dt>Thickness</dt><dd>{(board.thickness_m * 1000).toFixed(3)} mm</dd>
				<dt>Nets</dt><dd>{board.net_count}</dd>
				<dt>Footprints</dt><dd>{board.footprint_count}</dd>
				{#each board.comments as c, i}
					<dt>Comment {i + 1}</dt><dd>{c}</dd>
				{/each}
			</dl>
		{/if}

		{#if source}
			<div class="source">
				{#if sourceUrl}
					<a href={sourceUrl} target="_blank" rel="noopener noreferrer" data-testid="kicad-detail-source">{sourceLabel}</a>
				{:else}
					<span data-testid="kicad-detail-source">{sourceLabel}</span>
				{/if}
			</div>
		{/if}
	</div>
{/if}

<style>
	.kicad-panel {
		position: absolute;
		right: 12px;
		bottom: 12px;
		z-index: 120;
		width: min(320px, calc(100% - 24px));
		max-height: 60%;
		overflow: auto;
		padding: 10px 12px;
		border-radius: 8px;
		border: 1px solid var(--border-color, #45475a);
		background: var(--bg-secondary, #181825);
		color: var(--text-primary, #cdd6f4);
		box-shadow: 0 6px 24px rgba(0, 0, 0, 0.4);
		font-size: 12px;
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
	}
	h3 {
		margin: 0 0 6px;
		font-size: 14px;
	}
	.value {
		margin-left: 8px;
		font-weight: 400;
		color: var(--text-secondary, #a6adc8);
	}
	.close {
		border: none;
		background: transparent;
		color: inherit;
		font-size: 16px;
		cursor: pointer;
		line-height: 1;
	}
	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 2px 10px;
		margin: 0 0 8px;
	}
	dt {
		color: var(--text-secondary, #a6adc8);
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.mono {
		font-family: monospace;
		font-size: 11px;
	}
	.dim {
		color: var(--text-secondary, #a6adc8);
	}
	table {
		width: 100%;
		border-collapse: collapse;
		margin-bottom: 8px;
	}
	th,
	td {
		text-align: left;
		padding: 2px 4px;
		border-bottom: 1px solid var(--border-color, #45475a);
	}
	th {
		color: var(--text-secondary, #a6adc8);
		font-weight: 500;
	}
	.source {
		font-size: 11px;
		color: var(--text-secondary, #a6adc8);
	}
	a {
		color: var(--accent, #89b4fa);
	}
</style>
