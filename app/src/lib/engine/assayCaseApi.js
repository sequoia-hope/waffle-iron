/**
 * Client API for assay test case browsing.
 * Communicates with the Vite dev server plugin at /api/assay-cases.
 */

const BASE = '/api/assay-cases';

/**
 * Fetch the assay manifest (list of all generated cases).
 * @returns {Promise<{ master_seed: number, count: number, generator_version: number, cases: Array<{ id: string, filename: string, meta_filename: string, description: string }> }>}
 */
export async function fetchAssayManifest() {
	const res = await fetch(BASE);
	if (!res.ok) throw new Error(`Failed to fetch assay manifest: ${res.status}`);
	return res.json();
}

/**
 * Fetch a single assay case's .waffle data.
 * @param {string} id
 * @returns {Promise<string>} Raw JSON string
 */
export async function fetchAssayCase(id) {
	const res = await fetch(`${BASE}/${id}`);
	if (!res.ok) throw new Error(`Failed to fetch assay case ${id}: ${res.status}`);
	return res.text();
}

/**
 * Fetch a single assay case's oracle metadata.
 * @param {string} id
 * @returns {Promise<object>} Meta JSON with oracles, operations, scale, etc.
 */
export async function fetchAssayMeta(id) {
	const res = await fetch(`${BASE}/${id}/meta`);
	if (!res.ok) throw new Error(`Failed to fetch assay meta ${id}: ${res.status}`);
	return res.json();
}

/**
 * Fetch assay results (pass/fail/error status for each case).
 * Returns null if results.json doesn't exist yet.
 * @returns {Promise<{ total: number, passed: number, failed: number, errored: number, results: Array<{ id: string, status: string, category: string, detail: string }> } | null>}
 */
export async function fetchAssayResults() {
	try {
		const res = await fetch(`${BASE}/results`);
		if (!res.ok) return null;
		const data = await res.json();
		if (!data.results || data.results.length === 0) return null;
		return data;
	} catch {
		return null;
	}
}
