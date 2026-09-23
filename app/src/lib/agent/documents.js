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
	setProjectName,
	switchTab
} from '$lib/engine/store.svelte.js';
import { FORMAT_VERSION, fileTooNew } from '$lib/engine/format.js';
import { getActiveProvider, getProvider } from '$lib/storage/index.js';
import { newDocumentRecord, newUuid } from '$lib/storage/newDocument.js';
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

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** @param {unknown} s */
const isUuid = (s) => typeof s === 'string' && UUID_RE.test(s);

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

	async document_import(args) {
		const fileName = String(args.file_name);
		let parsed;
		try {
			parsed = JSON.parse(args.text);
		} catch (err) {
			throw fail('InvalidDocument', `${fileName} is not a .waffle document: ${reasonOf(err)}`, {
				file_name: fileName,
				reason: reasonOf(err)
			});
		}
		if (fileTooNew(parsed)) {
			throw fail('FormatTooNew', `${fileName} was saved by a newer version of Waffle Iron.`, {
				file_name: fileName,
				file_version: Math.max(parsed?.version ?? 0, parsed?.min_reader_version ?? 0),
				supported_version: FORMAT_VERSION
			});
		}
		confirmLeaving(args);
		const store = getActiveProvider();
		// The file's own identity is the storage key (v4 P2-5, the picker's
		// rule): re-importing an export of a stored document re-homes to that
		// record; a legacy file without one gets a fresh identity.
		const id = isUuid(parsed?.document?.id) ? parsed.document.id : newUuid();
		const now = Date.now();
		const created = Date.parse(parsed?.document?.created ?? '') || now;
		try {
			await store.put({ id, json: args.text, created, modified: now });
		} catch (err) {
			throw fail('SaveFailed', `Storing the document in ${store.label} failed: ${reasonOf(err)}`, {
				provider: store.id,
				reason: reasonOf(err)
			});
		}
		await open(id, args.text, null, store.id);
		// The file name wins over the stored name, as it does in the picker.
		const name = args.name ?? fileName.replace(/\.(waffle|json)$/i, '');
		if (name && name !== getDocumentInfo().name) {
			setProjectName(name);
			try {
				await saveDocumentOrThrow();
			} catch (err) {
				throw fail('SaveFailed', `Saving the imported document failed: ${reasonOf(err)}`, {
					provider: store.id,
					reason: reasonOf(err)
				});
			}
		}
		return toolOk(documentInfo());
	},

	async document_save({ allow_empty = false } = {}) {
		if (isDocumentReadOnly()) throw fail('DocumentReadOnly', READ_ONLY_MESSAGE, {});
		try {
			return toolOk(await saveDocumentOrThrow({ allowEmpty: allow_empty }));
		} catch (err) {
			if (err?.readOnly) throw fail('DocumentReadOnly', READ_ONLY_MESSAGE, {});
			if (err?.emptyDocument) {
				throw fail(
					'EmptyDocument',
					'The open document is empty (no features, instances or sources) — after a page reload this is the blank ' +
						'startup document, not your work. Call storage_list and document_open to get back to it, or pass allow_empty: true to store an empty document.',
					{}
				);
			}
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
