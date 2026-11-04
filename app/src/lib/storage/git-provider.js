/**
 * Git-backed document storage on any supported host
 * (`specs/waffle_v4_document_model.md` §7.1–7.4): one `.waffle` file per
 * document in a repository folder, plus an index file `.waffle-index.json`
 * mapping document id → path. Reads and writes go through the host adapters'
 * write-back API (`git/hosts.js`); the token comes from the per-host token
 * store. A document stored here has a git LOCATION
 * (`{remote, path, ref: Branch(branch)}`), which is what makes it linkable
 * (`getShareUrl` → the `/open` locator link) and what its `Relative` links
 * resolve against.
 *
 * Revisions (`rev`) are the host-reported git blob SHAs (`file.sha` from
 * `adapter.getFile`), checked explicitly against `options.base` before writing.
 *
 * @implements {import('./types.js').DocumentStore}
 */

import { base } from '$app/paths';
import { blobShaOf, gitBlobSha1 } from './git/hash.js';
import { GitHostError, adapterFor } from './git/hosts.js';
import { buildOpenLink } from './git/locator.js';
import { parseRemote } from './git/remote.js';
import { getHostToken } from './git/tokens.js';

const INDEX_FILE = '.waffle-index.json';
const WATCH_CHANNEL = 'waffle-iron-storage';

/**
 * Validate `put(doc, options)` options so missing `base` fails loudly.
 * @param {import('./types.js').DocumentPutOptions} options
 * @returns {string|null}
 */
function validatePutBase(options) {
	if (!options || typeof options !== 'object' || !('base' in options)) {
		throw new TypeError('GitProvider.put requires options { base: string | null }');
	}
	const { base: baseRev } = options;
	if (baseRev !== null && typeof baseRev !== 'string') {
		throw new TypeError('GitProvider.put base must be a revision string or null');
	}
	return baseRev;
}

export class GitProvider {
	/** @type {string} */
	id;
	/** @type {string} */
	label;
	canShare = true;

	#cfg;
	#token;
	#adapter;
	/** @type {Array|null} */
	#indexCache = null;
	/** @type {string|null} */
	#indexSha = null;
	/** @type {Set<(event: import('./types.js').DocumentChangeEvent) => void>} */
	#watchers = new Set();
	/** @type {BroadcastChannel|null} */
	#rxChannel = null;

	/**
	 * @param {import('./providers.js').GitProviderConfig} cfg
	 * @param {string|null} [token] - explicit token; else the host token store
	 */
	constructor(cfg, token = null) {
		this.#cfg = { ...cfg, folder: cfg.folder ?? '' };
		this.id = cfg.id;
		this.label = cfg.label;
		this.#token = token;
		this.#adapter = adapterFor({ remote: cfg.remote, host: cfg.kind });
	}

	/** @returns {import('./providers.js').GitProviderConfig} */
	get config() {
		return { ...this.#cfg };
	}

	get kind() {
		return this.#cfg.kind;
	}

	get remote() {
		return this.#cfg.remote;
	}

	get branch() {
		return this.#cfg.branch;
	}

	get token() {
		if (this.#token) return this.#token;
		const origin = parseRemote(this.#cfg.remote)?.origin;
		return origin ? getHostToken(origin) : null;
	}

	/**
	 * Notify in-tab watchers and broadcast to other same-origin tabs.
	 * @param {import('./types.js').DocumentChangeEvent} event
	 */
	#emitChange(event) {
		for (const fn of this.#watchers) {
			try {
				fn(event);
			} catch {
				// Ignore listener errors.
			}
		}
		if (typeof BroadcastChannel !== 'undefined') {
			try {
				const txChannel = new BroadcastChannel(WATCH_CHANNEL);
				txChannel.postMessage(event);
				txChannel.close();
			} catch {
				// BroadcastChannel unavailable.
			}
		}
	}

	/**
	 * Subscribe to changes committed to this provider (in this tab or another tab).
	 * @param {(event: import('./types.js').DocumentChangeEvent) => void} fn
	 * @returns {() => void}
	 */
	watch(fn) {
		this.#watchers.add(fn);
		if (!this.#rxChannel && typeof BroadcastChannel !== 'undefined') {
			try {
				this.#rxChannel = new BroadcastChannel(WATCH_CHANNEL);
				/** @type {any} */ (this.#rxChannel).unref?.();
				this.#rxChannel.onmessage = (msg) => {
					const event = msg?.data;
					if (!event || event.provider !== this.id) return;
					this.invalidate();
					for (const listener of this.#watchers) {
						try {
							listener(event);
						} catch {
							// Ignore listener errors.
						}
					}
				};
			} catch {
				this.#rxChannel = null;
			}
		}
		return () => {
			this.#watchers.delete(fn);
			if (this.#watchers.size === 0 && this.#rxChannel) {
				this.#rxChannel.close();
				this.#rxChannel = null;
			}
		};
	}

	/** Throw unless the repository exists and the token can see it. */
	async ensureRepo() {
		const exists = await this.#adapter.repoExists(this.#cfg.remote, this.token);
		if (!exists) {
			throw new GitHostError(
				`${this.#cfg.remote}: repository not found — create it on the host first (or check the token)`,
				'not_found'
			);
		}
	}

	#path(name) {
		return `${this.#cfg.folder}${name}`;
	}

	/**
	 * @returns {Promise<import('./types.js').DocumentSummary[]>}
	 */
	async list() {
		const index = await this.#loadIndex();
		return index.map((entry) => ({
			id: entry.id,
			name: entry.name,
			created: entry.created,
			modified: entry.modified,
			displayUnit: entry.displayUnit || null,
			tabCount: entry.tabCount || 1,
			provider: this.id,
			previewMesh: entry.previewMesh || null,
			link: null
		}));
	}

	/**
	 * @param {string} docId
	 * @returns {Promise<import('./types.js').DocumentRecord|null>}
	 */
	async get(docId) {
		const hadCache = this.#indexCache !== null;
		let index = await this.#loadIndex();
		let entry = index.find((e) => e.id === docId);
		if (!entry && hadCache) {
			this.invalidate();
			index = await this.#loadIndex();
			entry = index.find((e) => e.id === docId);
		}
		if (!entry) return null;
		let file = await this.#adapter.getFile(
			this.#cfg.remote,
			this.#cfg.branch,
			this.#path(entry.filename),
			this.token
		);
		if (!file && hadCache) {
			// Another client may have renamed the document in `.waffle-index.json`.
			this.invalidate();
			index = await this.#loadIndex();
			entry = index.find((e) => e.id === docId);
			if (!entry) return null;
			file = await this.#adapter.getFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(entry.filename),
				this.token
			);
		}
		if (!file) return null;
		return {
			doc: {
				id: entry.id,
				json: file.text,
				created: entry.created,
				modified: entry.modified
			},
			rev: file.sha
		};
	}

	/**
	 * Create or update a document at `options.base`. Fetches the current remote
	 * `sha` before writing and refuses (never writes or deletes) when it does not
	 * match `options.base`.
	 *
	 * @param {import('./types.js').StoredDocument} doc
	 * @param {import('./types.js').DocumentPutOptions} options
	 * @returns {Promise<import('./types.js').DocumentPutResult>}
	 */
	async put(doc, options) {
		const baseRev = validatePutBase(options);
		const parsed = JSON.parse(doc.json);
		const name = parsed.document?.name || parsed.project?.name || parsed.name || 'Untitled';
		const baseSlug = buildSlug(name);

		// Reload the remote index before writing so cross-tab creations/renames are visible.
		this.invalidate();
		const index = await this.#loadIndex();
		const existing = index.find((e) => e.id === doc.id);
		// Keep an existing entry's filename stable unless THIS document was renamed,
		// so share links / Relative locators never flip when sibling documents change
		// and legacy shared-filename entries do not migrate on an ordinary save.
		let filename;
		if (existing && existing.name === name) {
			filename = existing.filename;
		} else {
			const candidateFilename = `${baseSlug}.waffle`;
			const filenameTakenByOther = index.some(
				(e) => e.id !== doc.id && e.filename === candidateFilename
			);
			filename = filenameTakenByOther
				? `${baseSlug}-${String(doc.id).slice(0, 8)}.waffle`
				: candidateFilename;
		}

		let existingSha = null;
		/** @type {{ filename: string, sha: string } | null} */
		let oldFileToDelete = null;
		if (existing) {
			const storedFile = await this.#adapter.getFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(existing.filename),
				this.token
			);
			const storedRev = storedFile?.sha ?? null;
			if (baseRev !== storedRev) {
				return { conflict: { theirs_rev: storedRev } };
			}
			if (existing.filename !== filename) {
				// Null-base check on the rename target: refuse before any write or
				// delete if a file already occupies `filename` (including unindexed files).
				const currentTarget = await this.#adapter.getFile(
					this.#cfg.remote,
					this.#cfg.branch,
					this.#path(filename),
					this.token
				);
				if (currentTarget) {
					return { conflict: { theirs_rev: currentTarget.sha } };
				}
				existingSha = null;
				if (storedFile) {
					oldFileToDelete = { filename: existing.filename, sha: storedFile.sha };
				}
			} else {
				existingSha = storedRev;
			}
		} else {
			const current = await this.#adapter.getFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(filename),
				this.token
			);
			const storedRev = current?.sha ?? null;
			if (baseRev !== storedRev) {
				return { conflict: { theirs_rev: storedRev } };
			}
			existingSha = storedRev;
		}

		let putRes;
		try {
			putRes = await this.#adapter.putFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(filename),
				doc.json,
				`Save ${name}`,
				this.token,
				existingSha
			);
		} catch (err) {
			if (err?.status === 409 || err?.status === 412) {
				const latest = await this.#adapter
					.getFile(this.#cfg.remote, this.#cfg.branch, this.#path(filename), this.token)
					.catch(() => null);
				return { conflict: { theirs_rev: latest?.sha ?? null } };
			}
			if (
				(this.#cfg.kind === 'gitlab' && err?.status === 400) ||
				(existingSha === null && err?.status === 422)
			) {
				const latest = await this.#adapter
					.getFile(this.#cfg.remote, this.#cfg.branch, this.#path(filename), this.token)
					.catch(() => null);
				const latestSha = latest?.sha ?? null;
				if (latestSha !== existingSha) {
					return { conflict: { theirs_rev: latestSha } };
				}
			}
			throw err;
		}

		let nextRev = putRes?.sha ?? null;
		if (!nextRev) {
			const after = await this.#adapter.getFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(filename),
				this.token
			);
			nextRev = after?.sha ?? blobShaOf(await gitBlobSha1(doc.json));
		}

		const entry = {
			id: doc.id,
			name,
			filename,
			created: existing ? existing.created : doc.created,
			modified: doc.modified,
			displayUnit: parsed.document?.display_unit || parsed.displayUnit || null,
			tabCount: parsed.tabs ? parsed.tabs.length : 1,
			previewMesh: null
		};
		const committedIndex = await this.#mutateIndex((entries) => {
			const idx = entries.findIndex((e) => e.id === doc.id);
			if (idx >= 0) entries[idx] = entry;
			else entries.push(entry);
			return entries;
		});

		if (
			oldFileToDelete &&
			!committedIndex.some(
				(e) => e.id !== doc.id && e.filename === oldFileToDelete.filename
			)
		) {
			try {
				await this.#adapter.deleteFile(
					this.#cfg.remote,
					this.#cfg.branch,
					this.#path(oldFileToDelete.filename),
					`Rename ${oldFileToDelete.filename} to ${filename}`,
					this.token,
					oldFileToDelete.sha
				);
			} catch (err) {
				if (err?.status !== 404) throw err;
			}
		}

		this.#emitChange({
			type: 'changed',
			id: doc.id,
			rev: nextRev,
			provider: this.id
		});
		return { rev: nextRev };
	}

	/** @param {string} docId */
	async delete(docId) {
		this.invalidate();
		const index = await this.#loadIndex();
		const entry = index.find((e) => e.id === docId);
		if (!entry) return;
		const remaining = await this.#mutateIndex((entries) =>
			entries.filter((e) => e.id !== docId)
		);
		if (!remaining.some((e) => e.filename === entry.filename)) {
			const file = await this.#adapter.getFile(
				this.#cfg.remote,
				this.#cfg.branch,
				this.#path(entry.filename),
				this.token
			);
			if (file) {
				await this.#adapter.deleteFile(
					this.#cfg.remote,
					this.#cfg.branch,
					this.#path(entry.filename),
					`Delete ${entry.name}`,
					this.token,
					file.sha
				);
			}
		}
		this.#emitChange({
			type: 'deleted',
			id: docId,
			rev: null,
			provider: this.id
		});
	}

	/** The document's git locator (its location, §7.1), or null. @param {string} docId */
	async getLocator(docId) {
		const index = await this.#loadIndex();
		const entry = index.find((e) => e.id === docId);
		if (!entry) return null;
		return {
			type: 'Git',
			remote: this.#cfg.remote,
			path: this.#path(entry.filename),
			ref: { type: 'Branch', name: this.#cfg.branch },
			host: this.#cfg.kind
		};
	}

	/**
	 * A share link is a locator (§7.4): this repo, this file, the branch tip.
	 * @param {string} docId
	 */
	async getShareUrl(docId) {
		const loc = await this.getLocator(docId);
		if (!loc) return null;
		return buildOpenLink(loc, `${window.location.origin}${base}`);
	}

	async #loadIndex() {
		if (this.#indexCache) return this.#indexCache;
		const file = await this.#adapter.getFile(this.#cfg.remote, this.#cfg.branch, this.#path(INDEX_FILE), this.token);
		if (!file) {
			this.#indexCache = [];
			this.#indexSha = null;
			return this.#indexCache;
		}
		this.#indexCache = parseIndexEntries(file.text);
		this.#indexSha = file.sha;
		return this.#indexCache;
	}

	/**
	 * Apply a single-document mutation (`mutator`) to `.waffle-index.json` with
	 * compare-and-swap semantics on `#indexSha`. If another tab or client updated
	 * `.waffle-index.json` concurrently (either before `putFile` or racing with
	 * `putFile`), re-read their latest index, merge our entry mutation onto their
	 * entries, and retry with their `sha` as base.
	 *
	 * @param {(entries: Array<any>) => Array<any>} mutator
	 * @returns {Promise<Array<any>>}
	 */
	async #mutateIndex(mutator) {
		const MAX_ATTEMPTS = 6;
		let lastConflictErr = null;
		for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
			if (!this.#indexCache) {
				await this.#loadIndex();
			} else {
				const remote = await this.#adapter.getFile(
					this.#cfg.remote,
					this.#cfg.branch,
					this.#path(INDEX_FILE),
					this.token
				);
				const remoteSha = remote?.sha ?? null;
				if (remoteSha !== this.#indexSha) {
					this.#indexCache = remote ? parseIndexEntries(remote.text) : [];
					this.#indexSha = remoteSha;
				}
			}

			const baseSha = this.#indexSha;
			const nextEntries = mutator(this.#indexCache.map((e) => ({ ...e })));
			try {
				const putRes = await this.#adapter.putFile(
					this.#cfg.remote,
					this.#cfg.branch,
					this.#path(INDEX_FILE),
					JSON.stringify(nextEntries, null, 2),
					'Update document index',
					this.token,
					baseSha
				);
				let nextSha = putRes?.sha ?? null;
				if (!nextSha) {
					const after = await this.#adapter.getFile(
						this.#cfg.remote,
						this.#cfg.branch,
						this.#path(INDEX_FILE),
						this.token
					);
					nextSha = after?.sha ?? null;
				}
				this.#indexCache = nextEntries;
				this.#indexSha = nextSha;
				return nextEntries;
			} catch (err) {
				if (err?.status === 409 || err?.status === 412) {
					lastConflictErr = err;
					this.invalidate();
					continue;
				}
				if (
					(baseSha === null && err?.status === 422) ||
					(this.#cfg.kind === 'gitlab' && err?.status === 400)
				) {
					const latest = await this.#adapter
						.getFile(this.#cfg.remote, this.#cfg.branch, this.#path(INDEX_FILE), this.token)
						.catch(() => null);
					const latestSha = latest?.sha ?? null;
					if (latestSha !== baseSha) {
						lastConflictErr = err;
						this.#indexCache = latest ? parseIndexEntries(latest.text) : [];
						this.#indexSha = latestSha;
						continue;
					}
				}
				throw err;
			}
		}
		throw (
			lastConflictErr ??
			new Error('Updating .waffle-index.json failed after repeated concurrent conflicts')
		);
	}

	/** Drop the cached index (e.g. after an external change). */
	invalidate() {
		this.#indexCache = null;
		this.#indexSha = null;
	}
}

/**
 * Parse `.waffle-index.json` text into its array of document entries.
 * @param {string} text
 * @returns {Array<any>}
 */
function parseIndexEntries(text) {
	try {
		const parsed = JSON.parse(text);
		if (Array.isArray(parsed)) return parsed;
		if (Array.isArray(parsed?.documents)) return parsed.documents;
		return [];
	} catch {
		return [];
	}
}

/**
 * Build a URL-safe slug from a document name.
 * @param {string} name
 */
export function buildSlug(name) {
	return (
		String(name)
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '') || 'untitled'
	);
}
