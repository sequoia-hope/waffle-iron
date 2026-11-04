<script>
	/**
	 * `/open?remote=&path=&ref=` (also `?url=` and the legacy `?src=`): the
	 * share-link entry point (specs/waffle_v4_document_model.md §7.4). Public
	 * repositories open without login; a private one asks for a token for
	 * that host, stored per host (§7.2). The document opens linked and
	 * read-only; "Fork to edit" in the editor makes an editable copy.
	 */
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import { parseOpenParams, describeLocator } from '$lib/storage/git/locator.js';
	import { inferHost, parseRemote } from '$lib/storage/git/remote.js';
	import { setHostToken } from '$lib/storage/git/tokens.js';
	import { openFromLocator } from '$lib/storage/open-link.js';

	/** @type {'resolving'|'token'|'error'} */
	let status = $state('resolving');
	let message = $state('');
	let locator = $state(null);
	let hostUrl = $state('');
	let tokenInput = $state('');
	let busy = $state(false);

	async function attempt() {
		status = 'resolving';
		busy = true;
		try {
			const { id, json, link, rev } = await openFromLocator(locator);
			sessionStorage.setItem('waffle-active-doc', id);
			sessionStorage.setItem('waffle-active-json', json);
			if (rev) {
				sessionStorage.setItem('waffle-active-rev', rev);
				sessionStorage.setItem('waffle-active-rev-provider', 'local');
			} else {
				sessionStorage.removeItem('waffle-active-rev');
				sessionStorage.removeItem('waffle-active-rev-provider');
			}
			sessionStorage.setItem('waffle-active-link', JSON.stringify(link));
			goto(`${base}/`, { replaceState: true });
		} catch (err) {
			const code = err?.code;
			if ((code === 'auth_required' || code === 'permission_denied') && locator?.type === 'Git') {
				hostUrl = parseRemote(locator.remote)?.origin ?? '';
				status = 'token';
				message = err.message;
			} else {
				status = 'error';
				message = err?.message || String(err);
			}
		} finally {
			busy = false;
		}
	}

	function submitToken(e) {
		e?.preventDefault?.();
		const token = tokenInput.trim();
		if (!token || !hostUrl) return;
		setHostToken(hostUrl, token, { kind: inferHost(locator.remote) });
		tokenInput = '';
		attempt();
	}

	onMount(() => {
		const parsed = parseOpenParams($page.url.searchParams);
		if ('error' in parsed) {
			status = 'error';
			message = parsed.error;
			return;
		}
		locator = parsed.locator;
		attempt();
	});
</script>

<div class="open-page" data-testid="open-page">
	<div class="open-card">
		<h1>Opening linked document</h1>
		{#if locator}
			<p class="locator" data-testid="open-locator">{describeLocator(locator)}</p>
		{/if}

		{#if status === 'resolving'}
			<p class="status" data-testid="open-status">Fetching…</p>
		{:else if status === 'token'}
			<form class="token-form" data-testid="open-token-form" onsubmit={submitToken}>
				<p class="status">This repository needs an access token for <strong>{hostUrl}</strong>.</p>
				<p class="hint">{message}</p>
				<label for="open-token">Personal access token (read access to the repository)</label>
				<input
					id="open-token"
					data-testid="open-token-input"
					type="password"
					bind:value={tokenInput}
					placeholder="token"
					autocomplete="off"
				/>
				<div class="actions">
					<button type="submit" data-testid="open-token-submit" disabled={busy || !tokenInput.trim()}>
						Use token
					</button>
					<a class="link" href="{base}/home">Cancel</a>
				</div>
			</form>
		{:else}
			<p class="error" data-testid="open-error">{message}</p>
			<a class="link" href="{base}/home">Back to documents</a>
		{/if}
	</div>
</div>

<style>
	.open-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-primary, #1e1e2e);
		color: var(--text-primary, #cdd6f4);
	}
	.open-card {
		max-width: 560px;
		padding: 32px;
		border: 1px solid var(--border-color, #45475a);
		border-radius: 8px;
		background: var(--bg-secondary, #181825);
	}
	h1 {
		font-size: 18px;
		margin: 0 0 12px;
	}
	.locator {
		font-family: monospace;
		font-size: 13px;
		color: var(--text-secondary, #a6adc8);
		word-break: break-all;
	}
	.status {
		margin: 12px 0 0;
	}
	.hint {
		font-size: 12px;
		color: var(--text-muted, #6c7086);
	}
	.error {
		color: var(--error, #f38ba8);
	}
	.token-form label {
		display: block;
		font-size: 12px;
		margin: 12px 0 4px;
	}
	.token-form input {
		width: 100%;
		box-sizing: border-box;
		padding: 8px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		background: var(--bg-primary, #1e1e2e);
		color: inherit;
	}
	.actions {
		display: flex;
		gap: 12px;
		align-items: center;
		margin-top: 12px;
	}
	button {
		padding: 8px 14px;
		border-radius: 4px;
		border: none;
		background: var(--accent, #89b4fa);
		color: var(--bg-primary, #1e1e2e);
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.link {
		color: var(--accent, #89b4fa);
	}
</style>
