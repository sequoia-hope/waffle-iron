/**
 * Git host adapters (`specs/waffle_v4_document_model.md` §7.2). Every adapter
 * implements:
 *
 *   resolveRef(remote, ref, token?)          → { commit }
 *   fetchBlob(remote, commit, path, token?)  → { text, blobSha }
 * and, for storage providers (write-back, §7.3):
 *   repoExists(remote, token?)               → boolean
 *   getFile(remote, branch, path, token?)    → { text, sha } | null
 *   putFile(remote, branch, path, text, message, token?, existingSha?) → { sha }
 *   deleteFile(remote, branch, path, message, token?, existingSha?)
 *
 * `ref` is a `GitRef` (`{type:'Commit', sha}` | `{type:'Branch', name}` |
 * `{type:'Tag', name}`). Content is always fetched at a RESOLVED commit, never
 * at a moving ref name, so `resolved.commit` and `content_hash` agree (§4
 * inv. 4). Public repositories need no token.
 */

import { inferHost, isCommitSha, parseRemote } from './remote.js';

export class GitHostError extends Error {
	/**
	 * @param {string} message
	 * @param {'not_found'|'auth_required'|'permission_denied'|'rate_limit'|'network'|'unresolvable'|'api_error'|'invalid'} code
	 * @param {object} [extra]
	 */
	constructor(message, code, extra = {}) {
		super(message);
		this.name = 'GitHostError';
		/** @type {string} */
		this.code = code;
		Object.assign(this, extra);
	}
}

/** Decode a base64 body (with or without line breaks) as UTF-8 text. */
export function decodeBase64Utf8(b64) {
	const bin = atob(String(b64).replace(/\s/g, ''));
	const bytes = new Uint8Array(bin.length);
	for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
	return new TextDecoder().decode(bytes);
}

/** Encode UTF-8 text as base64 (for write-back). */
export function encodeUtf8Base64(text) {
	const bytes = new TextEncoder().encode(text);
	let bin = '';
	for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
	return btoa(bin);
}

/** A repo path as URL segments. @param {string} path */
function encodePath(path) {
	return path.split('/').map(encodeURIComponent).join('/');
}

/** The ref name/sha a host API wants in a URL. @param {any} ref */
export function refToString(ref) {
	if (!ref || typeof ref !== 'object') return String(ref ?? '');
	if (ref.type === 'Commit') return ref.sha;
	return ref.name;
}

/**
 * Turn an HTTP response into a typed error.
 * @param {Response} res
 * @param {string} what
 */
async function failFor(res, what) {
	if (res.status === 401) return new GitHostError(`${what}: authentication required`, 'auth_required', { status: res.status });
	if (res.status === 403) {
		if (res.headers.get('X-RateLimit-Remaining') === '0') {
			return new GitHostError(`${what}: rate limited`, 'rate_limit', { status: res.status });
		}
		return new GitHostError(`${what}: permission denied (private repository?)`, 'permission_denied', { status: res.status });
	}
	if (res.status === 404) return new GitHostError(`${what}: not found`, 'not_found', { status: res.status });
	return new GitHostError(`${what}: HTTP ${res.status}`, 'api_error', { status: res.status });
}

/**
 * fetch with a network-error wrapper.
 * @param {string} url
 * @param {RequestInit} init
 */
async function doFetch(url, init) {
	try {
		return await fetch(url, init);
	} catch (err) {
		throw new GitHostError(`Network error: ${err?.message || err}`, 'network');
	}
}

// ----------------------------------------------------------------- GitHub

const github = {
	kind: 'github',
	/** @param {string} remote */
	apiBase(remote) {
		const p = parseRemote(remote);
		if (!p) throw new GitHostError(`bad remote ${remote}`, 'invalid');
		const api = p.host === 'github.com' ? 'https://api.github.com' : `${p.origin}/api/v3`;
		return { api, repo: `${p.owner}/${p.repo}` };
	},
	headers(token) {
		const h = { Accept: 'application/vnd.github+json' };
		if (token) h.Authorization = `Bearer ${token}`;
		return h;
	},
	async resolveRef(remote, ref, token) {
		if (ref?.type === 'Commit') return { commit: ref.sha.toLowerCase() };
		const { api, repo } = this.apiBase(remote);
		const name = refToString(ref);
		const res = await doFetch(`${api}/repos/${repo}/commits/${encodeURIComponent(name)}`, {
			headers: this.headers(token)
		});
		if (!res.ok) throw await failFor(res, `resolve ${name} on ${remote}`);
		const data = await res.json();
		if (!data?.sha) throw new GitHostError('resolve: no sha in response', 'api_error');
		return { commit: String(data.sha).toLowerCase() };
	},
	async fetchBlob(remote, commit, path, token) {
		const { api, repo } = this.apiBase(remote);
		const url = `${api}/repos/${repo}/contents/${path.split('/').map(encodeURIComponent).join('/')}?ref=${encodeURIComponent(commit)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (!res.ok) throw await failFor(res, `fetch ${path}@${commit.slice(0, 7)} from ${remote}`);
		const data = await res.json();
		if (data?.encoding === 'base64' && typeof data.content === 'string') {
			return { text: decodeBase64Utf8(data.content), blobSha: String(data.sha).toLowerCase() };
		}
		// Files over 1 MiB come back without content; take the raw media type.
		// `no-store`: GitHub gives both replies the same ETag, so a revalidation
		// gets a 304 and the browser would hand back the JSON it just cached.
		const raw = await doFetch(url, {
			cache: 'no-store',
			headers: { ...this.headers(token), Accept: 'application/vnd.github.raw+json' }
		});
		if (!raw.ok) throw await failFor(raw, `fetch ${path} (raw) from ${remote}`);
		return { text: await raw.text(), blobSha: data?.sha ? String(data.sha).toLowerCase() : null };
	},
	// --- write-back (storage providers, §7.3) ---
	async repoExists(remote, token) {
		const { api, repo } = this.apiBase(remote);
		const res = await doFetch(`${api}/repos/${repo}`, { headers: this.headers(token) });
		if (res.status === 404) return false;
		if (!res.ok) throw await failFor(res, `check ${remote}`);
		return true;
	},
	async getFile(remote, branch, path, token) {
		const { api, repo } = this.apiBase(remote);
		const url = `${api}/repos/${repo}/contents/${encodePath(path)}?ref=${encodeURIComponent(branch)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (res.status === 404) return null;
		if (!res.ok) throw await failFor(res, `read ${path} on ${remote}`);
		const data = await res.json();
		if (!data?.sha) throw new GitHostError(`read ${path} on ${remote}: no sha in response`, 'api_error');
		return { text: decodeBase64Utf8(data.content), sha: String(data.sha).toLowerCase() };
	},
	async putFile(remote, branch, path, text, message, token, existingSha = null) {
		const { api, repo } = this.apiBase(remote);
		const body = { message, content: encodeUtf8Base64(text), branch };
		if (existingSha) body.sha = existingSha;
		const res = await doFetch(`${api}/repos/${repo}/contents/${encodePath(path)}`, {
			method: 'PUT',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify(body)
		});
		if (!res.ok) throw await failFor(res, `write ${path} on ${remote}`);
		const data = await res.json().catch(() => ({}));
		return { sha: data?.content?.sha ? String(data.content.sha).toLowerCase() : null };
	},
	async deleteFile(remote, branch, path, message, token, existingSha) {
		const { api, repo } = this.apiBase(remote);
		const res = await doFetch(`${api}/repos/${repo}/contents/${encodePath(path)}`, {
			method: 'DELETE',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify({ message, sha: existingSha, branch })
		});
		if (!res.ok && res.status !== 404) throw await failFor(res, `delete ${path} on ${remote}`);
	}
};

// ----------------------------------------------------------------- GitLab

const gitlab = {
	kind: 'gitlab',
	apiBase(remote) {
		const p = parseRemote(remote);
		if (!p) throw new GitHostError(`bad remote ${remote}`, 'invalid');
		return { api: `${p.origin}/api/v4/projects/${encodeURIComponent(p.repoPath)}` };
	},
	headers(token) {
		const h = { Accept: 'application/json' };
		if (token) h.Authorization = `Bearer ${token}`;
		return h;
	},
	async resolveRef(remote, ref, token) {
		if (ref?.type === 'Commit') return { commit: ref.sha.toLowerCase() };
		const { api } = this.apiBase(remote);
		const name = refToString(ref);
		// /repository/commits/{ref} accepts branch names, tag names and shas.
		const res = await doFetch(`${api}/repository/commits/${encodeURIComponent(name)}`, {
			headers: this.headers(token)
		});
		if (!res.ok) throw await failFor(res, `resolve ${name} on ${remote}`);
		const data = await res.json();
		if (!data?.id) throw new GitHostError('resolve: no commit id in response', 'api_error');
		return { commit: String(data.id).toLowerCase() };
	},
	async fetchBlob(remote, commit, path, token) {
		const { api } = this.apiBase(remote);
		const url = `${api}/repository/files/${encodeURIComponent(path)}?ref=${encodeURIComponent(commit)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (!res.ok) throw await failFor(res, `fetch ${path}@${commit.slice(0, 7)} from ${remote}`);
		const data = await res.json();
		if (typeof data?.content !== 'string') throw new GitHostError('fetch: no content', 'api_error');
		return { text: decodeBase64Utf8(data.content), blobSha: data.blob_id ? String(data.blob_id).toLowerCase() : null };
	},
	// --- write-back (storage providers, §7.3) ---
	async repoExists(remote, token) {
		const { api } = this.apiBase(remote);
		const res = await doFetch(api, { headers: this.headers(token) });
		if (res.status === 404) return false;
		if (!res.ok) throw await failFor(res, `check ${remote}`);
		return true;
	},
	async getFile(remote, branch, path, token) {
		const { api } = this.apiBase(remote);
		const url = `${api}/repository/files/${encodeURIComponent(path)}?ref=${encodeURIComponent(branch)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (res.status === 404) return null;
		if (!res.ok) throw await failFor(res, `read ${path} on ${remote}`);
		const data = await res.json();
		const rev = data?.last_commit_id || data?.blob_id;
		if (!rev) {
			throw new GitHostError(
				`read ${path} on ${remote}: no last_commit_id or blob_id in response`,
				'api_error'
			);
		}
		return { text: decodeBase64Utf8(data.content), sha: String(rev).toLowerCase() };
	},
	async putFile(remote, branch, path, text, message, token, existingSha = null) {
		const { api } = this.apiBase(remote);
		/** @type {Record<string, unknown>} */
		const body = { branch, content: text, commit_message: message };
		if (existingSha) body.last_commit_id = existingSha;
		const res = await doFetch(`${api}/repository/files/${encodeURIComponent(path)}`, {
			method: existingSha ? 'PUT' : 'POST',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify(body)
		});
		if (!res.ok) throw await failFor(res, `write ${path} on ${remote}`);
		return { sha: null };
	},
	async deleteFile(remote, branch, path, message, token, existingSha = null) {
		const { api } = this.apiBase(remote);
		/** @type {Record<string, unknown>} */
		const body = { branch, commit_message: message };
		if (existingSha) body.last_commit_id = existingSha;
		const res = await doFetch(`${api}/repository/files/${encodeURIComponent(path)}`, {
			method: 'DELETE',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify(body)
		});
		if (!res.ok && res.status !== 404) throw await failFor(res, `delete ${path} on ${remote}`);
	}
};

// ------------------------------------------------------------ Gitea / Forgejo

const gitea = {
	kind: 'gitea',
	apiBase(remote) {
		const p = parseRemote(remote);
		if (!p) throw new GitHostError(`bad remote ${remote}`, 'invalid');
		return { api: `${p.origin}/api/v1/repos/${p.owner}/${p.repo}` };
	},
	headers(token) {
		const h = { Accept: 'application/json' };
		if (token) h.Authorization = `token ${token}`;
		return h;
	},
	async resolveRef(remote, ref, token) {
		if (ref?.type === 'Commit') return { commit: ref.sha.toLowerCase() };
		const { api } = this.apiBase(remote);
		const name = refToString(ref);
		const tryUrl = async (url, pick) => {
			const res = await doFetch(url, { headers: this.headers(token) });
			if (res.status === 404) return null;
			if (!res.ok) throw await failFor(res, `resolve ${name} on ${remote}`);
			return pick(await res.json());
		};
		const order =
			ref?.type === 'Tag'
				? [
						[`${api}/tags/${encodeURIComponent(name)}`, (d) => d?.commit?.sha],
						[`${api}/branches/${encodeURIComponent(name)}`, (d) => d?.commit?.id]
					]
				: [
						[`${api}/branches/${encodeURIComponent(name)}`, (d) => d?.commit?.id],
						[`${api}/tags/${encodeURIComponent(name)}`, (d) => d?.commit?.sha]
					];
		for (const [url, pick] of order) {
			const sha = await tryUrl(url, pick);
			if (sha) return { commit: String(sha).toLowerCase() };
		}
		if (isCommitSha(name)) return { commit: name.toLowerCase() };
		throw new GitHostError(`resolve ${name} on ${remote}: not found`, 'not_found');
	},
	async fetchBlob(remote, commit, path, token) {
		const { api } = this.apiBase(remote);
		const url = `${api}/contents/${path.split('/').map(encodeURIComponent).join('/')}?ref=${encodeURIComponent(commit)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (!res.ok) throw await failFor(res, `fetch ${path}@${commit.slice(0, 7)} from ${remote}`);
		const data = await res.json();
		if (typeof data?.content !== 'string') throw new GitHostError('fetch: no content', 'api_error');
		return { text: decodeBase64Utf8(data.content), blobSha: data.sha ? String(data.sha).toLowerCase() : null };
	},
	// --- write-back (storage providers, §7.3) ---
	async repoExists(remote, token) {
		const { api } = this.apiBase(remote);
		const res = await doFetch(api, { headers: this.headers(token) });
		if (res.status === 404) return false;
		if (!res.ok) throw await failFor(res, `check ${remote}`);
		return true;
	},
	async getFile(remote, branch, path, token) {
		const { api } = this.apiBase(remote);
		const url = `${api}/contents/${encodePath(path)}?ref=${encodeURIComponent(branch)}`;
		const res = await doFetch(url, { headers: this.headers(token) });
		if (res.status === 404) return null;
		if (!res.ok) throw await failFor(res, `read ${path} on ${remote}`);
		const data = await res.json();
		if (!data?.sha) throw new GitHostError(`read ${path} on ${remote}: no sha in response`, 'api_error');
		return { text: decodeBase64Utf8(data.content), sha: String(data.sha).toLowerCase() };
	},
	async putFile(remote, branch, path, text, message, token, existingSha = null) {
		const { api } = this.apiBase(remote);
		const body = { content: encodeUtf8Base64(text), message, branch };
		if (existingSha) body.sha = existingSha;
		const res = await doFetch(`${api}/contents/${encodePath(path)}`, {
			method: existingSha ? 'PUT' : 'POST',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify(body)
		});
		if (!res.ok) throw await failFor(res, `write ${path} on ${remote}`);
		const data = await res.json().catch(() => ({}));
		return { sha: data?.content?.sha ? String(data.content.sha).toLowerCase() : null };
	},
	async deleteFile(remote, branch, path, message, token, existingSha) {
		const { api } = this.apiBase(remote);
		const res = await doFetch(`${api}/contents/${encodePath(path)}`, {
			method: 'DELETE',
			headers: { ...this.headers(token), 'Content-Type': 'application/json' },
			body: JSON.stringify({ message, sha: existingSha, branch })
		});
		if (!res.ok && res.status !== 404) throw await failFor(res, `delete ${path} on ${remote}`);
	}
};

// ---------------------------------------------------------------- generic

const generic = {
	kind: 'generic',
	async resolveRef(remote, ref) {
		if (ref?.type === 'Commit') return { commit: ref.sha.toLowerCase() };
		throw new GitHostError(
			`${remote}: no API adapter for this host — pin the link to a commit or set \`host\``,
			'unresolvable'
		);
	},
	async fetchBlob(remote) {
		throw new GitHostError(`${remote}: no API adapter for this host`, 'unresolvable');
	},
	async repoExists(remote) {
		throw new GitHostError(`${remote}: no API adapter for this host`, 'unresolvable');
	},
	async getFile(remote) {
		throw new GitHostError(`${remote}: no API adapter for this host`, 'unresolvable');
	},
	async putFile(remote) {
		throw new GitHostError(`${remote}: no API adapter for this host`, 'unresolvable');
	},
	async deleteFile(remote) {
		throw new GitHostError(`${remote}: no API adapter for this host`, 'unresolvable');
	}
};

const ADAPTERS = { github, gitlab, gitea, generic };

/**
 * The adapter for a locator: its explicit `host`, else inferred from the remote.
 * @param {{remote: string, host?: string}} locator
 */
export function adapterFor(locator) {
	const kind = locator.host && ADAPTERS[locator.host] ? locator.host : inferHost(locator.remote);
	return ADAPTERS[kind] ?? generic;
}

/**
 * Resolve a `Git` locator to a commit and fetch its content there.
 * @param {{remote: string, path: string, ref: any, host?: string}} locator
 * @param {string|null} [token]
 * @returns {Promise<{commit: string, text: string, blobSha: string|null}>}
 */
export async function fetchGitLocator(locator, token = null) {
	const adapter = adapterFor(locator);
	const { commit } = await adapter.resolveRef(locator.remote, locator.ref, token);
	const { text, blobSha } = await adapter.fetchBlob(locator.remote, commit, locator.path, token);
	return { commit, text, blobSha };
}

/**
 * Plain fetch of a `Url` locator. No ref resolution; the caller hashes.
 * @param {string} url
 * @returns {Promise<{text: string}>}
 */
export async function fetchUrlLocator(url) {
	if (!/^https:\/\//i.test(url)) throw new GitHostError(`Url locator must be https: ${url}`, 'invalid');
	const res = await doFetch(url, {});
	if (!res.ok) throw await failFor(res, `fetch ${url}`);
	return { text: await res.text() };
}
