/**
 * Client API for assay test case browsing.
 * Tries the Vite dev server plugin first (/api/assay-cases),
 * falls back to static files (/assay/) for production builds.
 */

import { base } from '$app/paths';

const DEV_BASE = '/api/assay-cases';
const STATIC_BASE = `${base}/assay`;

/**
 * Fetch with dev-server-first fallback to static files.
 * @param {string} devPath - Path under /api/assay-cases
 * @param {string} staticPath - Path under /assay/
 * @returns {Promise<Response>}
 */
async function fetchWithFallback(devPath, staticPath) {
	const devRes = await fetch(devPath);
	if (devRes.ok) return devRes;
	// Dev server not available — try static
	const staticRes = await fetch(staticPath);
	return staticRes;
}

/**
 * Fetch the assay manifest (list of all generated cases).
 */
export async function fetchAssayManifest() {
	const res = await fetchWithFallback(DEV_BASE, `${STATIC_BASE}/manifest.json`);
	if (!res.ok) throw new Error(`Failed to fetch assay manifest: ${res.status}`);
	return res.json();
}

/**
 * Fetch a single assay case's .waffle data.
 * @param {string} id
 * @returns {Promise<string>} Raw JSON string
 */
export async function fetchAssayCase(id) {
	const res = await fetchWithFallback(`${DEV_BASE}/${id}`, `${STATIC_BASE}/${id}.waffle`);
	if (!res.ok) throw new Error(`Failed to fetch assay case ${id}: ${res.status}`);
	return res.text();
}

/**
 * Fetch a single assay case's oracle metadata.
 * @param {string} id
 * @returns {Promise<object>}
 */
export async function fetchAssayMeta(id) {
	const res = await fetchWithFallback(`${DEV_BASE}/${id}/meta`, `${STATIC_BASE}/${id}.meta.json`);
	if (!res.ok) throw new Error(`Failed to fetch assay meta ${id}: ${res.status}`);
	return res.json();
}

/**
 * Fetch assay results (pass/fail/error status for each case).
 * Returns null if results.json doesn't exist yet.
 */
export async function fetchAssayResults() {
	try {
		const res = await fetchWithFallback(`${DEV_BASE}/results`, `${STATIC_BASE}/results.json`);
		if (!res.ok) return null;
		const data = await res.json();
		if (!data.results || data.results.length === 0) return null;
		return data;
	} catch {
		return null;
	}
}
