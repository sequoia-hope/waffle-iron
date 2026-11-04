/**
 * Document and storage tool implementations (specs/waffle_mcp_server.md §2.5,
 * §3.4 S1–S4). They call the same store flows as the Home screen, the Save
 * button and the tab bar (I1, I2).
 */
import {
	cancelPendingAutoSave,
	getDocumentInfo,
	getSources,
	hasPendingAutoSave,
	isDocumentReadOnly,
	openDocumentRecord,
	saveDocumentOrThrow,
	setProjectName
} from '$lib/engine/store.svelte.js';
import { FORMAT_VERSION, fileTooNew } from '$lib/engine/format.js';
import { getActiveProvider, getProvider } from '$lib/storage/index.js';
import { newDocumentRecord, newUuid } from '$lib/storage/newDocument.js';
import { fail, plain, toolOk } from './results.js';

const READ_ONLY_MESSAGE = 'The open document is linked read-only; the user must fork it to allow edits and saves.';

/**
 * The open document as `document_info` answers it. Also what the executor
 * overlays on a tab tool's engine answer (the engine knows the session's
 * share; the storage record, provider, read-only and unsaved are this page's).
 */
export function documentInfo() {
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
 * @param {string | null} [rev]
 */
async function open(storageId, json, link, provider, rev = null) {
	try {
		await openDocumentRecord(storageId, json, link, rev, provider);
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
		let found = null;
		try {
			found = await store.get(args.id);
		} catch (err) {
			throw fail('StorageFailed', `Reading ${store.label} failed: ${reasonOf(err)}`, { provider: store.id, reason: reasonOf(err) });
		}
		if (!found) throw fail('DocumentNotFound', `No document "${args.id}" in ${store.label}.`, { provider: store.id, id: args.id });
		confirmLeaving(args);
		const { doc, rev } = found;
		await open(doc.id, doc.json, doc.link ?? null, store.id, rev);
		return toolOk(documentInfo());
	},

	async document_new(args) {
		confirmLeaving(args);
		const store = getActiveProvider();
		const record = newDocumentRecord({ name: args.name ?? 'Untitled' });
		let putRes;
		try {
			putRes = await store.put(record, { base: null });
		} catch (err) {
			throw fail('SaveFailed', `Creating the document in ${store.label} failed: ${reasonOf(err)}`, {
				provider: store.id,
				reason: reasonOf(err)
			});
		}
		if ('conflict' in putRes) {
			throw fail('SaveFailed', `Creating the document in ${store.label} conflicted with revision ${putRes.conflict.theirs_rev}`, {
				provider: store.id,
				theirs_rev: putRes.conflict.theirs_rev
			});
		}
		await open(record.id, record.json, null, store.id, putRes.rev);
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
		// rule): re-importing an export of a stored document targets that
		// record (including an older 8-char-keyed record carrying `fileUuid`),
		// ignoring `/open` read-only link caches stored under another key.
		// A legacy file without an identity gets a fresh one.
		const hasFileUuid = isUuid(parsed?.document?.id);
		const fileUuid = hasFileUuid ? parsed.document.id : newUuid();
		const now = Date.now();
		let existing = null;
		if (hasFileUuid) {
			try {
				existing = await store.get(fileUuid);
			} catch (err) {
				throw fail('SaveFailed', `Reading ${store.label} failed: ${reasonOf(err)}`, {
					provider: store.id,
					reason: reasonOf(err)
				});
			}
		}
		const ownExisting =
			existing && (!existing.doc?.link || existing.doc.id === fileUuid) ? existing : null;
		const id = ownExisting ? ownExisting.doc.id : fileUuid;
		let base = null;
		if (ownExisting) {
			if (!args.replace) {
				throw fail(
					'SaveFailed',
					`A document with id "${id}" already exists in ${store.label} (revision ${ownExisting.rev}). Pass replace: true to ask the user to replace it.`,
					{
						provider: store.id,
						id,
						theirs_rev: ownExisting.rev
					}
				);
			}
			let existingName = id;
			try {
				const existingParsed = JSON.parse(ownExisting.doc.json);
				existingName =
					existingParsed?.document?.name || existingParsed?.project?.name || existingName;
			} catch {
				// Fall back to storage id when stored JSON lacks a readable name.
			}
			const accepted = window.confirm(
				`An agent wants to replace the stored document "${existingName}" with imported file "${fileName}". Replace it?`
			);
			if (!accepted) {
				throw fail('UserDeclined', 'The user declined to replace the stored document.', {
					provider: store.id,
					id
				});
			}
			base = ownExisting.rev;
		}
		const created = ownExisting?.doc?.created ?? (Date.parse(parsed?.document?.created ?? '') || now);
		let putRes;
		try {
			putRes = await store.put(
				{ id, json: args.text, created, modified: now, link: null },
				{ base }
			);
		} catch (err) {
			throw fail('SaveFailed', `Storing the document in ${store.label} failed: ${reasonOf(err)}`, {
				provider: store.id,
				reason: reasonOf(err)
			});
		}
		if ('conflict' in putRes) {
			throw fail('SaveFailed', `Storing the document in ${store.label} conflicted with revision ${putRes.conflict.theirs_rev}`, {
				provider: store.id,
				id,
				theirs_rev: putRes.conflict.theirs_rev
			});
		}
		await open(id, args.text, null, store.id, putRes.rev);
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
			if (err?.conflict) {
				throw fail('SaveFailed', `Saving failed due to a revision conflict: ${reasonOf(err)}`, {
					provider,
					theirs_rev: err.conflict.theirs_rev ?? null,
					reason: reasonOf(err)
				});
			}
			throw fail('SaveFailed', `Saving failed: ${reasonOf(err)}`, { provider, reason: reasonOf(err) });
		}
	}
};
// (The four tab tools ran here until 2026-09-23; they are engine commands
// now — `crates/wasm-bridge/src/tools/tabs.rs` — and the executor overlays
// `documentInfo()` on their answers.)
