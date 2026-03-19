/**
 * GitHub-backed document storage using the GitHub Contents API.
 * Stores .waffle files in a user's repo with a .waffle-index.json manifest.
 *
 * @implements {import('./types.js').DocumentStore}
 */

export class GitHubStorageError extends Error {
	/** @param {string} message @param {string} code */
	constructor(message, code) {
		super(message);
		this.name = 'GitHubStorageError';
		/** @type {string} */
		this.code = code;
	}
}

export class GitHubStore {
	id = 'github';
	label = 'GitHub';
	canShare = true;

	#token;
	#owner;
	#repo;
	/** @type {Array|null} */
	#indexCache = null;

	/**
	 * @param {string} token - GitHub access token
	 * @param {string} owner - GitHub username
	 * @param {string} [repo='waffle-iron-documents'] - Repository name
	 */
	constructor(token, owner, repo = 'waffle-iron-documents') {
		this.#token = token;
		this.#owner = owner;
		this.#repo = repo;
	}

	/**
	 * List all documents.
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
			provider: 'github',
			previewMesh: entry.previewMesh || null
		}));
	}

	/**
	 * Get a document by ID.
	 * @param {string} docId
	 * @returns {Promise<import('./types.js').StoredDocument|null>}
	 */
	async get(docId) {
		const index = await this.#loadIndex();
		const entry = index.find((e) => e.id === docId);
		if (!entry) return null;

		try {
			const { content } = await this.#getFile(entry.filename);
			return {
				id: entry.id,
				json: content,
				created: entry.created,
				modified: entry.modified
			};
		} catch (err) {
			if (err instanceof GitHubStorageError && err.code === 'not_found') return null;
			throw err;
		}
	}

	/**
	 * Create or update a document.
	 * @param {import('./types.js').StoredDocument} doc
	 */
	async put(doc) {
		const parsed = JSON.parse(doc.json);
		const name = parsed.name || 'Untitled';
		const filename = buildSlug(name) + '.waffle';

		const index = await this.#loadIndex();
		const existing = index.find((e) => e.id === doc.id);

		// Put the .waffle file
		let sha;
		if (existing && existing.filename === filename) {
			// Same filename — get SHA for update
			try {
				const file = await this.#getFile(existing.filename);
				sha = file.sha;
			} catch {
				// File may have been deleted externally
			}
		} else if (existing && existing.filename !== filename) {
			// Filename changed — delete old file, create new
			try {
				const oldFile = await this.#getFile(existing.filename);
				await this.#deleteFile(existing.filename, oldFile.sha, `Rename ${existing.filename} to ${filename}`);
			} catch {
				// Old file may not exist
			}
		}

		await this.#putFile(filename, doc.json, `Save ${name}`, sha);

		// Update index
		const entry = {
			id: doc.id,
			name,
			filename,
			created: existing ? existing.created : doc.created,
			modified: doc.modified,
			displayUnit: parsed.displayUnit || null,
			tabCount: parsed.tabs ? parsed.tabs.length : 1,
			previewMesh: null
		};

		if (existing) {
			const idx = index.indexOf(existing);
			index[idx] = entry;
		} else {
			index.push(entry);
		}

		await this.#saveIndex(index);
	}

	/**
	 * Delete a document by ID.
	 * @param {string} docId
	 */
	async delete(docId) {
		const index = await this.#loadIndex();
		const entry = index.find((e) => e.id === docId);
		if (!entry) return;

		try {
			const file = await this.#getFile(entry.filename);
			await this.#deleteFile(entry.filename, file.sha, `Delete ${entry.name}`);
		} catch {
			// File may already be gone
		}

		const updated = index.filter((e) => e.id !== docId);
		await this.#saveIndex(updated);
	}

	/**
	 * Get a shareable URL for a document.
	 * @param {string} docId
	 * @returns {Promise<string|null>}
	 */
	async getShareUrl(docId) {
		const index = await this.#loadIndex();
		const entry = index.find((e) => e.id === docId);
		if (!entry) return null;

		const rawUrl = `https://raw.githubusercontent.com/${this.#owner}/${this.#repo}/main/${entry.filename}`;
		return `${window.location.origin}?src=${encodeURIComponent(rawUrl)}`;
	}

	/**
	 * Ensure the target repository exists, creating it if needed.
	 */
	async ensureRepo() {
		try {
			await this.#apiCall(`/repos/${this.#owner}/${this.#repo}`);
		} catch (err) {
			if (err instanceof GitHubStorageError && err.code === 'not_found') {
				await this.#apiCall('/user/repos', {
					method: 'POST',
					body: JSON.stringify({
						name: this.#repo,
						description: 'Waffle Iron CAD documents',
						private: false,
						auto_init: true
					})
				});
				// Clear index cache since repo is fresh
				this.#indexCache = null;
			} else {
				throw err;
			}
		}
	}

	// --- Internal helpers ---

	/**
	 * Make an authenticated GitHub API call.
	 * @param {string} endpoint
	 * @param {RequestInit} [options]
	 * @returns {Promise<any>}
	 */
	async #apiCall(endpoint, options = {}) {
		const url = `https://api.github.com${endpoint}`;
		const headers = {
			Authorization: `Bearer ${this.#token}`,
			Accept: 'application/vnd.github+json',
			...options.headers
		};

		let res;
		try {
			res = await fetch(url, { ...options, headers });
		} catch (err) {
			throw new GitHubStorageError(`Network error: ${err.message}`, 'network');
		}

		if (res.status === 401) {
			throw new GitHubStorageError('Authentication failed', 'auth_failed');
		}
		if (res.status === 403) {
			const resetHeader = res.headers.get('X-RateLimit-Reset');
			throw new GitHubStorageError(
				`Rate limited${resetHeader ? ` (resets at ${resetHeader})` : ''}`,
				'rate_limit'
			);
		}
		if (res.status === 404) {
			throw new GitHubStorageError('Not found', 'not_found');
		}
		if (!res.ok) {
			throw new GitHubStorageError(`GitHub API error: ${res.status}`, 'api_error');
		}

		// 204 No Content (e.g., DELETE)
		if (res.status === 204) return null;

		return res.json();
	}

	/**
	 * Get a file's content and SHA from the repo.
	 * @param {string} path
	 * @returns {Promise<{content: string, sha: string}>}
	 */
	async #getFile(path) {
		const data = await this.#apiCall(`/repos/${this.#owner}/${this.#repo}/contents/${path}`);
		// GitHub base64 content has line breaks every 60 chars
		const content = atob(data.content.replace(/\n/g, ''));
		return { content, sha: data.sha };
	}

	/**
	 * Create or update a file in the repo.
	 * @param {string} path
	 * @param {string} content - Raw string content
	 * @param {string} message - Commit message
	 * @param {string} [sha] - SHA of existing file (for updates)
	 */
	async #putFile(path, content, message, sha) {
		const body = {
			message,
			content: btoa(content)
		};
		if (sha) body.sha = sha;

		try {
			await this.#apiCall(`/repos/${this.#owner}/${this.#repo}/contents/${path}`, {
				method: 'PUT',
				body: JSON.stringify(body)
			});
		} catch (err) {
			if (err instanceof GitHubStorageError && err.code === 'not_found') {
				await this.ensureRepo();
				await this.#apiCall(`/repos/${this.#owner}/${this.#repo}/contents/${path}`, {
					method: 'PUT',
					body: JSON.stringify(body)
				});
			} else {
				throw err;
			}
		}
	}

	/**
	 * Delete a file from the repo.
	 * @param {string} path
	 * @param {string} sha
	 * @param {string} message
	 */
	async #deleteFile(path, sha, message) {
		await this.#apiCall(`/repos/${this.#owner}/${this.#repo}/contents/${path}`, {
			method: 'DELETE',
			body: JSON.stringify({ message, sha })
		});
	}

	/**
	 * Load the document index from the repo.
	 * @returns {Promise<Array>}
	 */
	async #loadIndex() {
		if (this.#indexCache) return this.#indexCache;

		try {
			const { content } = await this.#getFile('.waffle-index.json');
			this.#indexCache = JSON.parse(content);
			return this.#indexCache;
		} catch (err) {
			if (err instanceof GitHubStorageError && err.code === 'not_found') {
				this.#indexCache = [];
				return this.#indexCache;
			}
			throw err;
		}
	}

	/**
	 * Save the document index to the repo.
	 * @param {Array} index
	 */
	async #saveIndex(index) {
		this.#indexCache = index;
		const content = JSON.stringify(index, null, 2);

		let sha;
		try {
			const existing = await this.#getFile('.waffle-index.json');
			sha = existing.sha;
		} catch {
			// Index doesn't exist yet
		}

		await this.#putFile('.waffle-index.json', content, 'Update document index', sha);
	}
}

/**
 * Build a URL-safe slug from a document name.
 * @param {string} name
 * @returns {string}
 */
export function buildSlug(name) {
	return name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
}
