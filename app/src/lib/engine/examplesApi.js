/**
 * Client API for the official examples (`app/static/examples/`).
 *
 * Reads go to the static files, which exist in development and in a build
 * alike. Writing ("save the open document as an example") is a development
 * affordance served by the Vite plugin (`vite-plugins/testCaseApi.js`,
 * `/api/examples`); `examplesWritable()` reports whether it is there.
 */

import { base } from '$app/paths';

const DEV_BASE = '/api/examples';
const STATIC_BASE = `${base}/examples`;

/**
 * Fetch the examples manifest: `{ examples: [{ id, name, filename, generator?,
 * description, features?, tabs?, built? }] }`.
 */
export async function fetchExamplesManifest() {
	const res = await fetch(`${STATIC_BASE}/manifest.json`, { cache: 'no-cache' });
	if (!res.ok) throw new Error(`Failed to fetch examples manifest: ${res.status}`);
	return res.json();
}

/**
 * Fetch an example's `.waffle` text by its manifest entry.
 * @param {{ filename: string }} entry
 * @returns {Promise<string>}
 */
export async function fetchExampleDocument(entry) {
	const res = await fetch(`${STATIC_BASE}/${entry.filename}`, { cache: 'no-cache' });
	if (!res.ok) throw new Error(`Failed to fetch example ${entry.filename}: ${res.status}`);
	return res.text();
}

/** The URL an example's generator is served at (for the panel's download link). */
export function exampleGeneratorUrl(entry) {
	return entry.generator ? `${STATIC_BASE}/${entry.generator}` : null;
}

/**
 * Whether the development write endpoint is available.
 *
 * A static host answers an unknown path with the SPA's own index.html and a
 * 200, so `res.ok` alone would put a Save button in a production build that
 * could never work. The endpoint must actually say so, in JSON.
 */
export async function examplesWritable() {
	try {
		const res = await fetch(DEV_BASE, { method: 'OPTIONS' });
		if (!res.ok) return false;
		const body = await res.json();
		return body?.writable === true;
	} catch {
		return false;
	}
}

/**
 * Save a document as a new official example (development only).
 * @param {{ name: string, description: string, waffleData: string }} body
 */
export async function createExample(body) {
	const res = await fetch(DEV_BASE, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(body)
	});
	const data = await res.json().catch(() => ({}));
	if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
	return data;
}
