<script>
	import { startDeviceFlow, pollForToken, fetchUser, saveAuth, disconnect as disconnectAuth, getClientId } from '$lib/storage/github-auth.js';
	import { GitHubStore } from '$lib/storage/github.js';
	import { registerProvider, unregisterProvider, setActiveProvider } from '$lib/storage/index.js';

	let { visible = false, onclose, onconnect, ondisconnect } = $props();

	// States: 'initial' | 'waiting' | 'connected'
	let state = $state('initial');
	let repoName = $state('waffle-iron-documents');
	let userCode = $state('');
	let verificationUri = $state('');
	let error = $state('');
	let connectedUser = $state('');
	let connectedRepo = $state('');
	let pollController = $state(null);

	function reset() {
		state = 'initial';
		repoName = 'waffle-iron-documents';
		userCode = '';
		verificationUri = '';
		error = '';
		connectedUser = '';
		connectedRepo = '';
		if (pollController) {
			pollController.abort();
			pollController = null;
		}
	}

	$effect(() => {
		if (!visible) reset();
	});

	async function handleConnect() {
		error = '';
		try {
			const flow = await startDeviceFlow();
			userCode = flow.user_code;
			verificationUri = flow.verification_uri;
			state = 'waiting';

			const accessToken = await pollForToken(flow.device_code, flow.interval);

			const user = await fetchUser(accessToken);
			saveAuth(accessToken, user.login, repoName);

			// Register and activate the GitHub provider
			const store = new GitHubStore(accessToken, user.login, repoName);
			await store.ensureRepo();
			registerProvider(store);
			setActiveProvider('github');

			connectedUser = user.login;
			connectedRepo = repoName;
			state = 'connected';

			onconnect?.({ token: accessToken, login: user.login, repo: repoName });
		} catch (err) {
			error = err.message || 'Failed to connect';
			state = 'initial';
		}
	}

	function handleCancel() {
		if (pollController) {
			pollController.abort();
			pollController = null;
		}
		state = 'initial';
	}

	async function handleDisconnect() {
		disconnectAuth();
		unregisterProvider('github');
		setActiveProvider('local');
		ondisconnect?.();
		onclose?.();
	}

	function handleClose() {
		handleCancel();
		onclose?.();
	}

	async function copyCode() {
		try {
			await navigator.clipboard.writeText(userCode);
		} catch {
			// Fallback: select text
		}
	}

	function handleKeydown(e) {
		if (e.key === 'Escape') {
			e.preventDefault();
			handleClose();
		}
	}

	function handleBackdropClick(e) {
		if (e.target === e.currentTarget) {
			handleClose();
		}
	}
</script>

{#if visible}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" onclick={handleBackdropClick} onkeydown={handleKeydown} data-testid="github-connect-dialog">
		<div class="dialog">
			<div class="dialog-header">
				<span class="dialog-title">
					{#if state === 'connected'}
						GitHub Connected
					{:else}
						Connect to GitHub
					{/if}
				</span>
				<button class="close-btn" onclick={handleClose}>&times;</button>
			</div>

			<div class="dialog-body">
				{#if state === 'initial'}
					<p class="description">Back up and share your documents via a GitHub repository.</p>
					<div class="field">
						<label for="github-repo">Repo name</label>
						<input
							id="github-repo"
							data-testid="github-repo-input"
							type="text"
							bind:value={repoName}
							placeholder="waffle-iron-documents"
						/>
					</div>
					{#if error}
						<p class="error">{error}</p>
					{/if}
					<button
						class="btn btn-primary"
						data-testid="github-connect-btn"
						onclick={handleConnect}
					>
						Connect with GitHub
					</button>

				{:else if state === 'waiting'}
					<p class="description">
						Go to <a href={verificationUri || 'https://github.com/login/device'} target="_blank" rel="noopener noreferrer">github.com/login/device</a>
					</p>
					<p class="description">and enter this code:</p>
					<div class="code-display">
						<code class="user-code" data-testid="github-user-code">{userCode}</code>
						<button class="btn btn-copy" data-testid="github-copy-code-btn" onclick={copyCode}>Copy</button>
					</div>
					<div class="waiting">
						<span class="spinner"></span>
						<span>Waiting for authorization...</span>
					</div>
					{#if error}
						<p class="error">{error}</p>
					{/if}
					<button
						class="btn btn-cancel"
						data-testid="github-cancel-btn"
						onclick={handleCancel}
					>
						Cancel
					</button>

				{:else if state === 'connected'}
					<div class="connected-info">
						<div class="info-row">
							<span class="info-label">User</span>
							<span class="info-value">{connectedUser}</span>
						</div>
						<div class="info-row">
							<span class="info-label">Repo</span>
							<span class="info-value">{connectedRepo}</span>
						</div>
					</div>
					<button
						class="btn btn-danger"
						data-testid="github-disconnect-btn"
						onclick={handleDisconnect}
					>
						Disconnect
					</button>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		backdrop-filter: blur(4px);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.dialog {
		background: var(--bg-primary, #1e1e2e);
		border: 1px solid var(--border-color, #45475a);
		border-radius: 8px;
		width: 380px;
		max-width: 90vw;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
	}

	.dialog-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border-color, #45475a);
	}

	.dialog-title {
		font-weight: 600;
		font-size: 15px;
		color: var(--text-primary, #cdd6f4);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary, #a6adc8);
		font-size: 20px;
		cursor: pointer;
		padding: 0 4px;
		line-height: 1;
	}

	.close-btn:hover {
		color: var(--text-primary, #cdd6f4);
	}

	.dialog-body {
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.description {
		font-size: 13px;
		color: var(--text-secondary, #a6adc8);
		margin: 0;
		line-height: 1.5;
	}

	.description a {
		color: var(--accent, #0078d4);
		text-decoration: none;
	}

	.description a:hover {
		text-decoration: underline;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field label {
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
		font-weight: 500;
	}

	.field input {
		background: var(--bg-secondary, #313244);
		border: 1px solid var(--border-color, #45475a);
		color: var(--text-primary, #cdd6f4);
		padding: 8px 12px;
		border-radius: 4px;
		font-size: 13px;
	}

	.field input:focus {
		outline: none;
		border-color: var(--accent, #0078d4);
	}

	.error {
		font-size: 12px;
		color: #f38ba8;
		margin: 0;
	}

	.btn {
		padding: 8px 16px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
		transition: opacity 0.15s;
	}

	.btn:hover {
		opacity: 0.9;
	}

	.btn-primary {
		background: var(--accent, #0078d4);
		color: white;
	}

	.btn-cancel {
		background: transparent;
		color: var(--text-secondary, #a6adc8);
		border-color: var(--border-color, #45475a);
	}

	.btn-cancel:hover {
		background: var(--bg-secondary, #313244);
	}

	.btn-danger {
		background: transparent;
		color: #f38ba8;
		border-color: #f38ba8;
	}

	.btn-danger:hover {
		background: rgba(243, 139, 168, 0.1);
	}

	.btn-copy {
		background: var(--bg-secondary, #313244);
		color: var(--text-primary, #cdd6f4);
		border-color: var(--border-color, #45475a);
		padding: 4px 12px;
		font-size: 12px;
	}

	.code-display {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 16px;
		background: var(--bg-secondary, #313244);
		border-radius: 6px;
	}

	.user-code {
		font-family: 'Courier New', Courier, monospace;
		font-size: 24px;
		font-weight: 700;
		letter-spacing: 4px;
		color: var(--text-primary, #cdd6f4);
	}

	.waiting {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 13px;
		color: var(--text-secondary, #a6adc8);
	}

	.spinner {
		width: 16px;
		height: 16px;
		border: 2px solid var(--border-color, #45475a);
		border-top-color: var(--accent, #0078d4);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.connected-info {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		background: var(--bg-secondary, #313244);
		border-radius: 6px;
	}

	.info-row {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.info-label {
		font-size: 12px;
		color: var(--text-secondary, #a6adc8);
		min-width: 40px;
	}

	.info-value {
		font-size: 13px;
		color: var(--text-primary, #cdd6f4);
		font-weight: 500;
	}
</style>
