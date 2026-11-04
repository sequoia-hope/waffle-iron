/**
 * Open-from-link (`specs/waffle_v4_document_model.md` §7.1, §7.4): a share
 * link is a locator, not a copy. Opening it fetches the document at the
 * RESOLVED commit, caches the bytes by content hash, and creates a **linked,
 * read-only** record in this browser's local store — the recipient cannot
 * overwrite the sender's file, and "fork" is the only path to editing.
 */

import { fileTooNew } from '$lib/engine/format.js';
import { cachePut } from './git/cache.js';
import { contentHashFromBlobSha, gitBlobSha1 } from './git/hash.js';
import { GitHostError, fetchGitLocator, fetchUrlLocator } from './git/hosts.js';
import { parseRemote } from './git/remote.js';
import { getHostToken } from './git/tokens.js';
import { getStore } from './index.js';
import { generateDocId } from './types.js';

/**
 * @typedef {Object} DocumentLink
 * @property {any} locator - the `Git` or `Url` locator the document was opened from
 * @property {{commit: string, at: string}|null} resolved - commit actually loaded (git only)
 * @property {string} contentHash - `git-blob-sha1:` hash of the loaded bytes
 * @property {boolean} readOnly - always true for a linked document
 * @property {string} name - display name (the file's basename)
 */

/** Two locators name the same content stream (remote+path+ref, or url). */
export function sameLocator(a, b) {
	if (!a || !b || a.type !== b.type) return false;
	if (a.type === 'Url') return a.url === b.url;
	if (a.type === 'Git') {
		return (
			a.remote === b.remote &&
			a.path === b.path &&
			JSON.stringify(a.ref) === JSON.stringify(b.ref)
		);
	}
	return false;
}

/**
 * Fetch a locator's `.waffle` text plus its provenance.
 * @param {any} locator
 * @returns {Promise<{text: string, resolved: {commit: string, at: string}|null, contentHash: string}>}
 */
export async function fetchDocumentAt(locator) {
	if (locator?.type === 'Git') {
		const origin = parseRemote(locator.remote)?.origin;
		const token = origin ? getHostToken(origin) : null;
		const { commit, text, blobSha } = await fetchGitLocator(locator, token);
		const contentHash = blobSha ? contentHashFromBlobSha(blobSha) : await gitBlobSha1(text);
		return { text, resolved: { commit, at: new Date().toISOString() }, contentHash };
	}
	if (locator?.type === 'Url') {
		const { text } = await fetchUrlLocator(locator.url);
		return { text, resolved: null, contentHash: await gitBlobSha1(text) };
	}
	throw new GitHostError(`cannot open a ${locator?.type ?? 'missing'} locator`, 'invalid');
}

/** Refuse anything that is not a `.waffle` document this build can read. */
export function assertWaffleDocument(text) {
	let parsed;
	try {
		parsed = JSON.parse(text);
	} catch {
		throw new GitHostError('the linked file is not a .waffle document (invalid JSON)', 'invalid');
	}
	if (parsed?.format !== 'waffle-iron') {
		throw new GitHostError('the linked file is not a .waffle document', 'invalid');
	}
	if (fileTooNew(parsed)) {
		throw new GitHostError('the linked document was saved by a newer version of Waffle Iron', 'invalid');
	}
	return parsed;
}

/**
 * Open a locator as a linked, read-only local document. Re-opening the same
 * link updates the existing linked record instead of piling up copies.
 * @param {any} locator
 * @returns {Promise<{id: string, json: string, link: DocumentLink, rev: string|null, documentName: string|null}>}
 */
export async function openFromLocator(locator) {
	// Persisted below: a plain object, never a Svelte `$state` proxy (IndexedDB
	// structured clone refuses proxies — "#<Object> could not be cloned").
	locator = JSON.parse(JSON.stringify(locator));
	const { text, resolved, contentHash } = await fetchDocumentAt(locator);
	const parsed = assertWaffleDocument(text);
	await cachePut(contentHash, text);

	const local = getStore();
	const existingSummary = (await local.list()).find((d) => d.link && sameLocator(d.link.locator, locator));
	const existingRecord = existingSummary ? await local.get(existingSummary.id) : null;
	const id = existingRecord?.doc.id ?? generateDocId();
	const now = Date.now();
	const name =
		locator.type === 'Git' ? locator.path.split('/').pop() : locator.url.split('/').pop() || 'linked';
	/** @type {DocumentLink} */
	const link = { locator, resolved, contentHash, readOnly: true, name };
	const putRes = await local.put(
		{
			id,
			json: text,
			created: existingRecord?.doc.created ?? now,
			modified: now,
			link
		},
		{ base: existingRecord?.rev ?? null }
	);
	if ('conflict' in putRes) {
		const theirs = await local.get(id);
		if (theirs) {
			let theirsName = null;
			try {
				theirsName = JSON.parse(theirs.doc.json)?.document?.name ?? null;
			} catch {
				theirsName = null;
			}
			return {
				id: theirs.doc.id,
				json: theirs.doc.json,
				link: theirs.doc.link ?? link,
				rev: theirs.rev,
				documentName: theirsName
			};
		}
		return { id, json: text, link, rev: null, documentName: parsed?.document?.name ?? null };
	}
	return { id, json: text, link, rev: putRes.rev, documentName: parsed?.document?.name ?? null };
}
