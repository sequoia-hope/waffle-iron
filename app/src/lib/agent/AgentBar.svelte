<script>
	/**
	 * "Agent connected" bar (specs/waffle_mcp_server.md §1, P12). Also the place a
	 * reloaded tab resumes its own consented session (P7) — never a new pairing.
	 */
	import { onMount } from 'svelte';
	import { agentLink, disconnectAgentLink, resumeAgentLink } from './link.js';

	onMount(() => {
		resumeAgentLink();
	});
</script>

{#if $agentLink.state === 'connected'}
	<div class="agent-bar" data-testid="agent-bar" role="status">
		<span class="label" data-testid="agent-bar-label">Agent connected — {$agentLink.agentName}</span>
		<button class="disconnect" data-testid="agent-disconnect" onclick={disconnectAgentLink}>
			Disconnect
		</button>
	</div>
{:else if $agentLink.state === 'disconnected' && $agentLink.reason !== 'user_disconnected'}
	<div class="agent-bar lost" data-testid="agent-bar-lost" role="status">
		<span class="label">Agent link closed ({$agentLink.reason}). Ask the agent to reconnect.</span>
	</div>
{/if}

<style>
	.agent-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 4px 12px;
		font-size: 12px;
		background: var(--bg-secondary, #181825);
		color: var(--text-primary, #cdd6f4);
		border-bottom: 1px solid var(--accent, #89b4fa);
	}
	.agent-bar.lost {
		border-bottom-color: var(--border-color, #45475a);
		color: var(--text-secondary, #a6adc8);
	}
	.label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.disconnect {
		padding: 2px 10px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: inherit;
		cursor: pointer;
	}
</style>
