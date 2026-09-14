<script>
	/**
	 * "Agent connected" bar (specs/waffle_mcp_server.md §1, G4, P12, I12): the
	 * running tool, Pause/Resume and Disconnect. Also reports the page state to
	 * the relay (`status` frames) and resumes a reloaded tab's own consented
	 * session (P7) — never a new pairing.
	 */
	import { onMount } from 'svelte';
	import { getAgentActivity, getDocumentName, getUserBusyReason } from '$lib/engine/store.svelte.js';
	import {
		agentLink,
		agentPause,
		disconnectAgentLink,
		resumeAgentLink,
		sendAgentStatus,
		setAgentPaused
	} from './link.js';

	onMount(() => {
		resumeAgentLink();
	});

	let activity = $derived(getAgentActivity());

	$effect(() => {
		if ($agentLink.state !== 'connected') return;
		const busy = getUserBusyReason();
		const documentName = getDocumentName();
		sendAgentStatus({
			state: $agentPause.paused ? 'paused' : busy ? 'busy' : 'ready',
			reason: busy,
			document_name: documentName
		});
	});
</script>

{#if $agentLink.state === 'connected'}
	<div class="agent-bar" class:paused={$agentPause.paused} data-testid="agent-bar" role="status">
		<span class="label" data-testid="agent-bar-label">Agent connected — {$agentLink.agentName}</span>
		{#if activity}
			<span class="activity" data-testid="agent-bar-activity">running {activity.tool}…</span>
		{:else if $agentPause.paused}
			<span class="activity" data-testid="agent-bar-paused">
				paused{$agentPause.reason ? `: ${$agentPause.reason}` : ''}
			</span>
		{/if}
		{#if $agentPause.paused}
			<button class="bar-btn" data-testid="agent-resume" onclick={() => setAgentPaused(false)}>Resume</button>
		{:else}
			<button class="bar-btn" data-testid="agent-pause" onclick={() => setAgentPaused(true)}>Pause</button>
		{/if}
		<button class="bar-btn" data-testid="agent-disconnect" onclick={disconnectAgentLink}>Disconnect</button>
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
	.agent-bar.paused {
		border-bottom-color: var(--warning, #f9e2af);
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
	.activity {
		color: var(--text-secondary, #a6adc8);
		white-space: nowrap;
	}
	.bar-btn {
		padding: 2px 10px;
		border-radius: 4px;
		border: 1px solid var(--border-color, #45475a);
		background: transparent;
		color: inherit;
		cursor: pointer;
	}
</style>
