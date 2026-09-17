/**
 * Document and storage tool implementations (specs/waffle_mcp_server.md §2.5,
 * §3.4 S1–S4). They call the same store flows as the Home screen, the Save
 * button and the tab bar (I1, I2).
 */
import {
	addTab,
	cancelPendingAutoSave,
	getDocumentInfo,
	getDocumentTabs,
	getSources,
	hasPendingAutoSave,
	isDocumentReadOnly,
	moveTab,
	openDocumentRecord,
	renameTab,
	saveDocumentOrThrow,
	switchTab
} from '$lib/engine/store.svelte.js';
import { getActiveProvider, getProvider } from '$lib/storage/index.js';
import { newDocumentRecord } from '$lib/storage/newDocument.js';
import { fail, plain, toolOk } from './results.js';

const READ_ONLY_MESSAGE = 'The open document is linked read-only; the user must fork it to allow edits and saves.';

function documentInfo() {
	const info = getDocumentInfo();
	const provider = getActiveProvider();
	return {
		document_id: info.documentId ?? null,
		storage_id: info.storageId ?? null,
		name: info.name ?? '',
		storage_provider: { id: provider.id, label: provider.label },
		tabs: plain(info.tabs),
		active_tab: info.activeTab ?? null,
		read_only: info.readOnly,
		sources: (plain(getSources()) ?? []).map((s) => ({ id: s.id, name: s.name, kind: s.kind, available: s.available })),
		unsaved: hasPendingAutoSave()
	};
}

/** @param {string | undefined} id */
function providerFor(id) {
	if (id == null) return getActiveProvider();
	const provider = getProvider(id);
	if (!provider) throw fail('ProviderNotFound', `No storage provider "${id}" is connected in this tab.`, { provider: id });
	return provider;
}

/** @param {any} err */
const reasonOf = (err) => String(err?.message ?? err);

/** @param {string} tab_id */
function requireTab(tab_id) {
	const tab = getDocumentTabs().find((t) => t.id === tab_id);
	if (!tab) throw fail('TabNotFound', `The document has no tab with id ${tab_id}.`, { tab_id });
	return tab;
}

/** G5 for the tab-list edits: a linked read-only document's tabs are not the agent's to change. */
function requireEditable() {
	if (isDocumentReadOnly()) throw fail('DocumentReadOnly', READ_ONLY_MESSAGE, {});
}

/**
 * S3: leaving a document whose latest changes are not stored yet.
 * @param {{ discard_unsaved?: boolean }} args
 */
function confirmLeaving(args) {
	if (!hasPendingAutoSave()) return;
	if (!args.discard_unsaved) {
		throw fail(
			'UnsavedChanges',
			'The open document has changes that are not saved yet. Call document_save first, or pass discard_unsaved: true.',
			{}
		);
	}
	const accepted = window.confirm(
		'An agent wants to leave this document. Its latest changes are not saved yet. Discard them?'
	);
	if (!accepted) throw fail('UserDeclined', 'The user declined to discard the unsaved changes.', {});
	cancelPendingAutoSave();
}

/**
 * @param {string} storageId
 * @param {string} json
 * @param {any} link
 * @param {string} provider
 */
async function open(storageId, json, link, provider) {
	try {
		await openDocumentRecord(storageId, json, link);
	} catch (err) {
		throw fail('StorageFailed', `The document did not load: ${reasonOf(err)}`, { provider, reason: reasonOf(err) });
	}
}

/** @type {Record<string, { run: (args: any) => any }>} */
export const DOCUMENT_QUERIES = {
	document_info: { run: () => toolOk(documentInfo()) },

	storage_list: {
		run: async ({ provider }) => {
			const store = providerFor(provider);
			let docs;
			try {
				docs = await store.list();
			} catch (err) {
				throw fail('StorageFailed', `Listing ${store.label} failed: ${reasonOf(err)}`, { provider: store.id, reason: reasonOf(err) });
			}
			return toolOk({
				provider: store.id,
				documents: docs.map((d) => ({
					id: d.id,
					name: d.name,
					created: d.created,
					modified: d.modified,
					tab_count: d.tabCount,
					linked: !!d.link
				}))
			});
		}
	}
};

/** @type {Record<string, (args: any) => Promise<object>>} */
export const DOCUMENT_COMMANDS = {
	async document_open(args) {
		const store = providerFor(args.provider);
		let doc = null;
		try {
			doc = await store.get(args.id);
		} catch (err) {
			throw fail('StorageFailed', `Reading ${store.label} failed: ${reasonOf(err)}`, { provider: store.id, reason: reasonOf(err) });
		}
		if (!doc) throw fail('DocumentNotFound', `No document "${args.id}" in ${store.label}.`, { provider: store.id, id: args.id });
		confirmLeaving(args);
		await open(doc.id, doc.json, doc.link ?? null, store.id);
		return toolOk(documentInfo());
	},

	async document_new(args) {
		confirmLeaving(args);
		const store = getActiveProvider();
		const record = newDocumentRecord({ name: args.name ?? 'Untitled' });
		try {
			await store.put(record);
		} catch (err) {
			throw fail('SaveFailed', `Creating the document in ${store.label} failed: ${reasonOf(err)}`, {
				provider: store.id,
				reason: reasonOf(err)
			});
		}
		await open(record.id, record.json, null, store.id);
		return toolOk(documentInfo());
	},

	async document_save() {
		if (isDocumentReadOnly()) throw fail('DocumentReadOnly', READ_ONLY_MESSAGE, {});
		try {
			return toolOk(await saveDocumentOrThrow());
		} catch (err) {
			if (err?.readOnly) throw fail('DocumentReadOnly', READ_ONLY_MESSAGE, {});
			const provider = getActiveProvider().id;
			throw fail('SaveFailed', `Saving failed: ${reasonOf(err)}`, { provider, reason: reasonOf(err) });
		}
	},

	async tab_switch({ tab_id }) {
		const tab = requireTab(tab_id);
		const kind = tab.kind?.type ?? 'Part';
		// Part tabs take the feature tools, Assembly tabs the assembly tools;
		// anything else (a kind from a newer build) has no tool to work it.
		if (kind !== 'Part' && kind !== 'Assembly') {
			throw fail('TabKindNotSupported', `Tab ${tab.name} is a ${kind} tab; agents work on Part and Assembly tabs.`, { kind });
		}
		await switchTab(tab_id);
		return toolOk(documentInfo());
	},

	async tab_add({ kind = 'Part', name, activate = true }) {
		requireEditable();
		// The engine mints the tab (S2 C3); the store follows it.
		const id = await addTab(kind);
		if (!id) throw fail('Internal', 'The engine did not add the tab.', { kind });
		if (name) await renameTab(id, name);
		if (activate) await switchTab(id);
		return toolOk({ tab_id: id, ...documentInfo() });
	},

	async tab_move({ tab_id, index }) {
		requireEditable();
		requireTab(tab_id);
		await moveTab(tab_id, index);
		return toolOk(documentInfo());
	},

	async tab_rename({ tab_id, name }) {
		requireEditable();
		requireTab(tab_id);
		await renameTab(tab_id, name);
		return toolOk(documentInfo());
	}
};
