/**
 * GitHub Device Flow authentication for Waffle Iron.
 * Uses a Cloudflare Worker proxy to avoid CORS issues with GitHub's OAuth endpoints.
 */

const CLIENT_ID = 'Iv23li9ESWnQ4cDauvL8';

/**
 * Cloudflare Worker proxy URL for GitHub OAuth endpoints.
 * Deploy worker/github-oauth-proxy/ and set this to its URL.
 */
const OAUTH_PROXY = 'https://waffle-iron-github-oauth.tlalexander.workers.dev';

const LS_TOKEN = 'waffle-github-token';
const LS_USER = 'waffle-github-user';
const LS_REPO = 'waffle-github-repo';

export class GitHubAuthError extends Error {
	/** @param {string} message @param {string} code */
	constructor(message, code) {
		super(message);
		this.name = 'GitHubAuthError';
		/** @type {string} */
		this.code = code;
	}
}

/**
 * Returns the configured GitHub App client ID.
 * @returns {string}
 */
export function getClientId() {
	return CLIENT_ID;
}

/**
 * Start the GitHub device flow via the CORS proxy.
 * @returns {Promise<{device_code: string, user_code: string, verification_uri: string, expires_in: number, interval: number}>}
 */
export async function startDeviceFlow() {
	const res = await fetch(`${OAUTH_PROXY}/login/device/code`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
			Accept: 'application/json'
		},
		body: JSON.stringify({ client_id: CLIENT_ID })
	});

	if (!res.ok) {
		throw new GitHubAuthError(`Device flow request failed: ${res.status}`, 'device_flow_failed');
	}

	const data = await res.json();
	return {
		device_code: data.device_code,
		user_code: data.user_code,
		verification_uri: data.verification_uri,
		expires_in: data.expires_in,
		interval: data.interval
	};
}

/**
 * Poll for an access token via the CORS proxy.
 * @param {string} deviceCode
 * @param {number} interval - Polling interval in seconds
 * @returns {Promise<string>} Access token
 */
export async function pollForToken(deviceCode, interval) {
	let pollInterval = interval;

	while (true) {
		await new Promise((resolve) => setTimeout(resolve, pollInterval * 1000));

		const res = await fetch(`${OAUTH_PROXY}/login/oauth/access_token`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				Accept: 'application/json'
			},
			body: JSON.stringify({
				client_id: CLIENT_ID,
				device_code: deviceCode,
				grant_type: 'urn:ietf:params:oauth:grant-type:device_code'
			})
		});

		if (!res.ok) {
			throw new GitHubAuthError(`Token request failed: ${res.status}`, 'token_request_failed');
		}

		const data = await res.json();

		if (data.access_token) {
			return data.access_token;
		}

		switch (data.error) {
			case 'authorization_pending':
				break;
			case 'slow_down':
				pollInterval += 5;
				break;
			case 'expired_token':
				throw new GitHubAuthError('Device code expired. Please restart the flow.', 'expired_token');
			case 'access_denied':
				throw new GitHubAuthError('User denied authorization.', 'access_denied');
			default:
				throw new GitHubAuthError(
					`Unexpected error: ${data.error}`,
					data.error || 'unknown'
				);
		}
	}
}

/**
 * Fetch the authenticated user's profile.
 * This goes direct to api.github.com (CORS supported).
 * @param {string} token
 * @returns {Promise<{login: string, avatarUrl: string}>}
 */
export async function fetchUser(token) {
	const res = await fetch('https://api.github.com/user', {
		headers: {
			Authorization: `Bearer ${token}`,
			Accept: 'application/vnd.github+json'
		}
	});

	if (!res.ok) {
		throw new GitHubAuthError(`Failed to fetch user: ${res.status}`, 'auth_failed');
	}

	const data = await res.json();
	return { login: data.login, avatarUrl: data.avatar_url };
}

/**
 * Save auth credentials to localStorage.
 * @param {string} token
 * @param {string} login
 * @param {string} repo
 */
export function saveAuth(token, login, repo) {
	localStorage.setItem(LS_TOKEN, token);
	localStorage.setItem(LS_USER, login);
	localStorage.setItem(LS_REPO, repo);
}

/**
 * Load saved auth from localStorage.
 * @returns {{token: string, login: string, repo: string}|null}
 */
export function loadAuth() {
	if (typeof localStorage === 'undefined') return null;
	const token = localStorage.getItem(LS_TOKEN);
	const login = localStorage.getItem(LS_USER);
	const repo = localStorage.getItem(LS_REPO);
	if (!token || !login) return null;
	return { token, login, repo: repo || 'waffle-iron-documents' };
}

/**
 * Clear saved GitHub auth from localStorage.
 */
export function disconnect() {
	localStorage.removeItem(LS_TOKEN);
	localStorage.removeItem(LS_USER);
	localStorage.removeItem(LS_REPO);
}
