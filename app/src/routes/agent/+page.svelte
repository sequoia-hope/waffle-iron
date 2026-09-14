<script>
	/**
	 * `/agent?relay=&code=&name=` — the agent-link consent screen
	 * (specs/waffle_mcp_server.md §2.2, P2-P5, P10, I7). Nothing connects until
	 * the user clicks Allow; Deny opens nothing. After `welcome` the tab goes to
	 * the editor with the link still open (the socket lives in `$lib/agent/link.js`).
	 */
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { page } from '$app/stores';
	import { getDocumentName } from '$lib/engine/store.svelte.js';
	import { connectWithCode } from '$lib/agent/link.js';

	const params = $page.url.searchParams;
	const relay = params.get('relay') ?? '';
	const code = params.get('code') ?? '';
	const agentName = params.get('name') || 'An unnamed agent';

	function relayIsValid(value) {
		try {
			const url = new URL(value);
			// A path is allowed: a TLS proxy (e.g. `tailscale serve`) may mount the relay under one.
			return (url.protocol === 'ws:' || url.protocol === 'wss:') && !url.search && !url.hash;
		} catch {
			return false;
		}
	}
	const linkError = !relayIsValid(relay)
		? 'This pairing link has no valid relay address.'
		: !code
			? 'This pairing link has no pairing code.'
			: null;

	/** @type {'consent' | 'connecting' | 'denied' | 'failed'} */
	let status = $state('consent');
	/** @type {{ kind: string, reason: string | null, message: string } | null} */
	let failure = $state(null);
	let documentName = $derived(getDocumentName());

	const BYE_MESSAGES = {
		invalid_code: 'This link has expired or was already used. Ask the agent to reconnect.',
		already_paired: 'Another tab is already connected to this agent.',
		revoked: 'The agent started a new pairing, which replaced this one. Use the newest link.',
		session_expired: 'The session expired. Ask the agent to reconnect.',
		protocol_mismatch:
			'The relay and this version of Waffle Iron speak different link protocols. Update the relay or the app.'
	};

	async function allow() {
		status = 'connecting';
		try {
			await connectWithCode({ relay, code, agentName });
			goto(`${base}/`, { replaceState: true });
		} catch (err) {
			failure = { kind: err?.kind ?? 'network_error', reason: err?.reason ?? null, message: String(err?.message ?? err) };
			status = 'failed';
		}
	}

	function deny() {
		status = 'denied';
	}
</script>

<div class="agent-page" data-testid="agent-consent-page">
	<div class="card">
		{#if linkError}
			<h1>Invalid agent link</h1>
			<p class="error" data-testid="agent-link-invalid">{linkError}</p>
			<a class="link" href="{base}/">Open Waffle Iron</a>
		{:else if status === 'denied'}
			<h1>Not connected</h1>
			<p data-testid="agent-denied">You denied the request. No connection was opened.</p>
			<a class="link" href="{base}/">Back to Waffle Iron</a>
		{:else}
			<h1>Allow <span data-testid="agent-consent-name">{agentName}</span> to edit documents in this tab?</h1>
			<dl>
				<dt>Relay</dt>
				<dd class="mono" data-testid="agent-consent-relay">{relay}</dd>
				<dt>Document</dt>
				<dd data-testid="agent-consent-document">{documentName || 'Untitled'}</dd>
			</dl>
			<p class="hint">
				The agent will read and change the document open in this tab, through the same engine and undo
				history as your own edits. You can disconnect at any time.
			</p>

			{#if status === 'failed' && failure}
				<div class="failure" data-testid="agent-consent-failure" data-error-class={failure.kind}>
					{#if failure.kind === 'bye'}
						<p class="error">{BYE_MESSAGES[failure.reason] ?? `The relay refused the link (${failure.reason}).`}</p>
					{:else}
						<p class="error">
							Could not reach the relay ({failure.kind === 'permission_denied'
								? 'local network access is blocked for this site'
								: failure.kind === 'permission_blocked'
									? 'this browser needs your permission to reach the local network'
									: failure.kind === 'security_error'
										? 'the browser blocked the address'
										: 'the connection failed or was blocked'}).
						</p>
						{#if failure.kind === 'permission_denied' || failure.kind === 'permission_blocked'}
							<div class="hint" data-testid="agent-consent-lna-help">
								<p>
									The relay runs on this computer, and this browser only lets a website reach it after you allow
									<strong>local network access</strong> for the site.
								</p>
								<ol>
									{#if failure.kind === 'permission_denied'}
										<li>Click the site controls icon at the left of the address bar and open <em>Site settings</em>.</li>
										<li>Set <em>Local network access</em> to <em>Allow</em>.</li>
									{:else}
										<li>Click <em>Allow</em> again; when the browser asks to let this site access devices on your local network, choose <em>Allow</em>.</li>
										<li>If no prompt appears, open the site controls at the left of the address bar and allow <em>Local network access</em>.</li>
									{/if}
									<li>Return here and click <em>Allow</em> again.</li>
								</ol>
							</div>
						{/if}
						<p class="hint">Fallbacks:</p>
						<ol class="hint">
							<li>Run Waffle Iron from the dev server on <code>localhost</code>.</li>
							<li>Run the relay with <code>--bind</code> and <code>--tls-cert</code>/<code>--tls-key</code> on a host name this browser trusts (for example a Tailscale <code>*.ts.net</code> certificate).</li>
						</ol>
					{/if}
				</div>
			{/if}

			<div class="actions">
				<button
					class="allow"
					data-testid="agent-consent-allow"
					onclick={allow}
					disabled={status === 'connecting'}
				>
					{status === 'connecting' ? 'Connecting…' : 'Allow'}
				</button>
				<button class="deny" data-testid="agent-consent-deny" onclick={deny} disabled={status === 'connecting'}>
					Deny
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.agent-page {
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
	h1 {
		font-size: 18px;
		margin: 0 0 16px;
	}
	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 6px 12px;
		margin: 0 0 12px;
		font-size: 13px;
	}
	dt {
		color: var(--text-secondary, #a6adc8);
	}
	dd {
		margin: 0;
		word-break: break-all;
	}
	.mono {
		font-family: monospace;
	}
	.hint {
		font-size: 12px;
		color: var(--text-muted, #6c7086);
	}
	.error {
		color: var(--error, #f38ba8);
	}
	.actions {
		display: flex;
		gap: 12px;
		margin-top: 16px;
	}
	button {
		padding: 8px 16px;
		border-radius: 4px;
		cursor: pointer;
	}
	.allow {
		border: none;
		background: var(--accent, #89b4fa);
		color: var(--bg-primary, #1e1e2e);
	}
	.deny {
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: inherit;
	}
	button:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.link {
		color: var(--accent, #89b4fa);
	}
</style>
