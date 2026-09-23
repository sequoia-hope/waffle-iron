<script>
	/**
	 * `/view?host=&code=` — the viewer (specs/waffle_server_mode.md §4.10):
	 * the editor's viewport and tree panel over a model streamed from a relay
	 * running `--kernel host`, with no engine in this browser. The root layout
	 * does not start the worker on this route; `$lib/viewer/link.js` holds the
	 * socket and the cache, so a reload paints the last snapshot at once.
	 *
	 * The code in the link is spent on the first attach; the address is then
	 * rewritten without it, so a reload resumes the stored session instead.
	 */
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import Viewport from '$lib/viewport/Viewport.svelte';
	import FeatureTree from '$lib/ui/FeatureTree.svelte';
	import ToastContainer from '$lib/ui/ToastContainer.svelte';
	import { getActiveTabId, getDocumentTabs } from '$lib/engine/store.svelte.js';
	import { startViewer, viewerLink } from '$lib/viewer/link.js';

	const params = $page.url.searchParams;
	const host = (params.get('host') ?? '').replace(/\s+/g, '');
	const code = (params.get('code') ?? '').replace(/\s+/g, '');

	function hostIsValid(value) {
		try {
			const url = new URL(value);
			return (url.protocol === 'ws:' || url.protocol === 'wss:') && !url.search && !url.hash;
		} catch {
			return false;
		}
	}
	const linkError = hostIsValid(host) ? null : 'This viewer link has no valid host address.';

	let tabs = $derived(getDocumentTabs());
	let activeTab = $derived(tabs.find((t) => t.id === getActiveTabId()) ?? null);
	let activeIsAssembly = $derived(activeTab?.kind?.type === 'Assembly');
	let assembly = $derived(activeIsAssembly ? activeTab.kind.assembly : null);

	const STATE_LABEL = {
		idle: 'Idle',
		connecting: 'Connecting…',
		attached: 'Live',
		reconnecting: 'Reconnecting…',
		failed: 'Not connected'
	};
	const REASONS = {
		invalid_code: 'This link has expired or was already used. Ask the agent for a new viewer link.',
		session_expired: 'The viewing session expired. Ask the agent for a new viewer link.',
		protocol_mismatch: 'The relay and this version of Waffle Iron speak different viewer protocols.',
		no_session: 'This browser has no viewing session for that host. Open a viewer link from the agent.'
	};

	onMount(() => {
		if (linkError) return;
		startViewer({ host, code: code || null });
		if (code) {
			// Spent on first use: a reload must resume, not re-present it.
			const url = new URL(window.location.href);
			url.searchParams.delete('code');
			history.replaceState(history.state, '', url);
		}
	});
</script>

{#if linkError}
	<div class="viewer-page" data-testid="viewer-page">
		<div class="card">
			<h1>Invalid viewer link</h1>
			<p class="error" data-testid="viewer-link-invalid">{linkError}</p>
			<p class="hint">Address received: <code>{$page.url.href}</code></p>
			<a class="link" href="{base}/">Open Waffle Iron</a>
		</div>
	</div>
{:else}
	<div class="viewer-shell" data-testid="viewer-page">
		<header class="bar">
			<span class="name" data-testid="viewer-document">{$viewerLink.documentName ?? 'Waffle Iron viewer'}</span>
			<span class="tabs">
				{#each tabs as tab (tab.id)}
					<span class="tab" class:active={tab.id === activeTab?.id}>{tab.name}</span>
				{/each}
			</span>
			<span
				class="status"
				data-testid="viewer-status"
				data-state={$viewerLink.state}
				data-stale={$viewerLink.stale}
				data-revision={$viewerLink.revision ?? ''}
			>
				{STATE_LABEL[$viewerLink.state]}{$viewerLink.stale ? ' · showing last known model' : ''}
				{#if $viewerLink.state === 'attached'}· {$viewerLink.bodies} {$viewerLink.bodies === 1 ? 'body' : 'bodies'}{/if}
			</span>
		</header>
		{#if $viewerLink.state === 'failed' && $viewerLink.reason}
			<p class="failure" data-testid="viewer-failure">{REASONS[$viewerLink.reason] ?? `Not connected (${$viewerLink.reason}).`}</p>
		{/if}
		<div class="main">
			<aside class="panel">
				{#if activeIsAssembly}
					<ul class="instances" data-testid="viewer-instances">
						{#each assembly?.instances ?? [] as inst (inst.id)}
							<li class:suppressed={inst.suppressed}>{inst.name}</li>
						{/each}
					</ul>
				{:else}
					<FeatureTree />
				{/if}
			</aside>
			<div class="viewport-slot">
				<Viewport />
			</div>
		</div>
	</div>
	<ToastContainer />
{/if}

<style>
	.viewer-shell {
		height: 100vh;
		display: grid;
		grid-template-rows: auto auto 1fr;
		background: var(--bg-primary, #1e1e2e);
		color: var(--text-primary, #cdd6f4);
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 6px 12px;
		border-bottom: 1px solid var(--border-color, #45475a);
		background: var(--bg-secondary, #181825);
		font-size: 13px;
		min-width: 0;
	}
	.name {
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tabs {
		display: flex;
		gap: 6px;
		overflow-x: auto;
		flex: 1;
	}
	.tab {
		padding: 2px 8px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		color: var(--text-secondary, #a6adc8);
		white-space: nowrap;
	}
	.tab.active {
		border-color: var(--accent, #89b4fa);
		color: var(--text-primary, #cdd6f4);
	}
	.status {
		white-space: nowrap;
		color: var(--text-secondary, #a6adc8);
	}
	.status[data-state='attached'] {
		color: var(--success, #a6e3a1);
	}
	.status[data-stale='true'] {
		color: var(--warning, #f9e2af);
	}
	.failure {
		margin: 0;
		padding: 6px 12px;
		color: var(--error, #f38ba8);
		font-size: 13px;
	}
	.main {
		display: grid;
		grid-template-columns: minmax(180px, 260px) 1fr;
		min-height: 0;
	}
	.panel {
		border-right: 1px solid var(--border-color, #45475a);
		overflow: auto;
		min-height: 0;
	}
	.viewport-slot {
		position: relative;
		min-height: 0;
	}
	.instances {
		list-style: none;
		margin: 0;
		padding: 8px;
		font-size: 13px;
	}
	.instances li {
		padding: 3px 4px;
	}
	.instances li.suppressed {
		opacity: 0.5;
	}
	@media (max-width: 640px) {
		.main {
			grid-template-rows: 1fr minmax(120px, 30vh);
			grid-template-columns: 1fr;
		}
		.panel {
			order: 2;
			border-right: none;
			border-top: 1px solid var(--border-color, #45475a);
		}
	}
	.viewer-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 16px;
		background: var(--bg-primary, #1e1e2e);
		color: var(--text-primary, #cdd6f4);
	}
	.card {
		max-width: 560px;
		width: 100%;
		padding: 32px;
		border: 1px solid var(--border-color, #45475a);
		border-radius: 8px;
		background: var(--bg-secondary, #181825);
	}
	.error {
		color: var(--error, #f38ba8);
	}
	.hint {
		font-size: 12px;
		color: var(--text-muted, #6c7086);
	}
	.link {
		color: var(--accent, #89b4fa);
	}
</style>
