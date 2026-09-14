/**
 * A new, empty `.waffle` storage record: one empty Part tab, keyed by the
 * document's own identity (v4 P2-5, specs/waffle_v4_document_model.md §2.1).
 * Shared by the Home screen's New and the agent link's `document_new`.
 */
import { FORMAT_VERSION, MIN_READER_VERSION } from '$lib/engine/format.js';

function newUuid() {
	return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
		? crypto.randomUUID()
		: ([1e7] + -1e3 + -4e3 + -8e3 + -1e11).replace(/[018]/g, (c) =>
				(c ^ (crypto.getRandomValues(new Uint8Array(1))[0] & (15 >> (c / 4)))).toString(16)
			);
}

/**
 * @param {{ name?: string, now?: number }} [opts]
 * @returns {import('./types.js').StoredDocument}
 */
export function newDocumentRecord({ name = 'Untitled', now = Date.now() } = {}) {
	const id = newUuid();
	const tabId = newUuid();
	return {
		id,
		json: JSON.stringify({
			format: 'waffle-iron',
			version: FORMAT_VERSION,
			min_reader_version: MIN_READER_VERSION,
			document: {
				id,
				name,
				created: new Date(now).toISOString(),
				modified: new Date(now).toISOString()
			},
			sources: [],
			tabs: [
				{
					id: tabId,
					name: 'Part 1',
					kind: { type: 'Part', features: { features: [], active_index: null } }
				}
			],
			active_tab: tabId
		}),
		created: now,
		modified: now
	};
}
