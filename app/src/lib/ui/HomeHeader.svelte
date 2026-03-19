<script>
	let { oncreate } = $props();
</script>

<header class="home-header" data-testid="home-header">
	<div class="brand-area">
		<h1 class="brand-title">Waffle Iron</h1>
		<p class="brand-tagline">Open-source parametric CAD</p>
	</div>
	<button class="new-doc-btn" data-testid="new-document-btn" onclick={oncreate}>
		+ New Document
	</button>
</header>

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

	.new-doc-btn {
		background: var(--accent, #0078d4);
		color: white;
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
