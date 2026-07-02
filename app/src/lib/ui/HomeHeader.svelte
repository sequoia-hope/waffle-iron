<script>
	import { getAllProviders, getActiveProvider, getActiveProviderId, setActiveProvider } from '$lib/storage/index.js';
	import GitHubConnectDialog from './GitHubConnectDialog.svelte';
	import ThemeSwitcher from './ThemeSwitcher.svelte';

	let { oncreate, onproviderchange } = $props();

	let dropdownOpen = $state(false);
	let githubDialogVisible = $state(false);

	let providers = $derived(getAllProviders());
	let activeId = $derived(getActiveProviderId());
	let activeLabel = $derived(getActiveProvider()?.label ?? 'This Browser');

	function toggleDropdown() {
		dropdownOpen = !dropdownOpen;
	}

	function selectProvider(id) {
		setActiveProvider(id);
		dropdownOpen = false;
		window.dispatchEvent(new CustomEvent('waffle-provider-changed', { detail: { id } }));
		onproviderchange?.(id);
	}

	function openGitHubDialog() {
		dropdownOpen = false;
		githubDialogVisible = true;
	}

	function handleClickOutside(e) {
		if (dropdownOpen && !e.target.closest('.provider-dropdown-container')) {
			dropdownOpen = false;
		}
	}

	$effect(() => {
		if (dropdownOpen) {
			const handler = (e) => handleClickOutside(e);
			document.addEventListener('pointerdown', handler);
			return () => document.removeEventListener('pointerdown', handler);
		}
	});
</script>

<header class="home-header" data-testid="home-header">
	<div class="brand-area">
		<h1 class="brand-title">Waffle Iron</h1>
		<p class="brand-tagline">Open-source parametric CAD</p>
	</div>

	<div class="header-actions">
		<ThemeSwitcher />
		<div class="provider-dropdown-container" data-testid="provider-dropdown">
			<button class="provider-trigger" onclick={toggleDropdown}>
				<span class="provider-label">{activeLabel}</span>
				<span class="provider-chevron" class:open={dropdownOpen}>&#9662;</span>
			</button>
			{#if dropdownOpen}
				<div class="provider-menu">
					{#each providers as provider}
						<button
							class="provider-item"
							class:active={provider.id === activeId}
							data-testid="provider-option-{provider.id}"
							onclick={() => selectProvider(provider.id)}
						>
							{provider.label}
							{#if provider.id === activeId}
								<span class="check-mark">&#10003;</span>
							{/if}
						</button>
					{/each}
					<div class="provider-divider"></div>
					<button
						class="provider-item connect-item"
						data-testid="provider-connect-github"
						onclick={openGitHubDialog}
					>
						Connect GitHub...
					</button>
				</div>
			{/if}
		</div>

		<button class="new-doc-btn" data-testid="new-document-btn" onclick={oncreate}>
			+ New Document
		</button>
	</div>
</header>

<GitHubConnectDialog
	visible={githubDialogVisible}
	onclose={() => { githubDialogVisible = false; }}
	onconnect={(info) => {
		githubDialogVisible = false;
		onproviderchange?.(info);
	}}
	ondisconnect={() => {
		onproviderchange?.('local');
	}}
/>

<style>
	.home-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 24px 32px;
		border-bottom: 1px solid var(--border-color, #45475a);
	}

	.brand-title {
		font-size: 22px;
		font-weight: 700;
		margin: 0;
		color: var(--text-primary, #cdd6f4);
	}

	.brand-tagline {
		font-size: 13px;
		color: var(--text-secondary, #a6adc8);
		margin: 4px 0 0;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.provider-dropdown-container {
		position: relative;
	}

	.provider-trigger {
		display: flex;
		align-items: center;
		gap: 6px;
		background: var(--bg-secondary, #313244);
		border: 1px solid var(--border-color, #45475a);
		color: var(--text-primary, #cdd6f4);
		padding: 8px 14px;
		border-radius: 6px;
		font-size: 13px;
		cursor: pointer;
		transition: border-color 0.15s;
	}

	.provider-trigger:hover {
		border-color: var(--accent, #0078d4);
	}

	.provider-label {
		font-weight: 500;
	}

	.provider-chevron {
		font-size: 10px;
		transition: transform 0.15s;
	}

	.provider-chevron.open {
		transform: rotate(180deg);
	}

	.provider-menu {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		min-width: 200px;
		background: var(--bg-primary, #1e1e2e);
		border: 1px solid var(--border-color, #45475a);
		border-radius: 6px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
		z-index: 100;
		padding: 4px 0;
	}

	.provider-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 8px 14px;
		background: none;
		border: none;
		color: var(--text-primary, #cdd6f4);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.provider-item:hover {
		background: var(--bg-secondary, #313244);
	}

	.provider-item.active {
		color: var(--accent, #0078d4);
	}

	.check-mark {
		font-size: 12px;
		color: var(--accent, #0078d4);
	}

	.provider-divider {
		height: 1px;
		background: var(--border-color, #45475a);
		margin: 4px 0;
	}

	.connect-item {
		color: var(--text-secondary, #a6adc8);
	}

	.new-doc-btn {
		background: var(--accent, #0078d4);
		color: var(--text-on-accent);
		border: none;
		padding: 10px 24px;
		border-radius: 6px;
		font-size: 14px;
		cursor: pointer;
		font-weight: 500;
		transition: opacity 0.15s;
	}

	.new-doc-btn:hover {
		opacity: 0.9;
	}
</style>
