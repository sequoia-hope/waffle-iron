/**
 * Tool manifest canonicalization and hashing (specs/waffle_mcp_server.md §2.4).
 *
 * Plain ESM with no `$lib` or store imports: it is loaded both by the page and
 * by `app/scripts/gen-agent-manifest.mjs` under Node. The hash is SHA-256 over
 * `canonicalJson(tools)`; the relay computes the identical bytes with
 * `json.dumps(tools, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`
 * (relay/src/waffle_mcp_relay/manifest.py). Keys must stay ASCII so JS and
 * Python key ordering agree.
 */

export const LINK_PROTOCOL = 'waffle-agent-link/1';

/**
 * JSON with object keys sorted recursively and no whitespace.
 * @param {unknown} value
 * @returns {string}
 */
export function canonicalJson(value) {
	if (value === null || typeof value !== 'object') return JSON.stringify(value);
	if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
	const keys = Object.keys(value)
		.filter((k) => value[k] !== undefined)
		.sort();
	return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`).join(',')}}`;
}

/**
 * A deep copy with keys sorted, for deterministic pretty-printing.
 * @param {unknown} value
 * @returns {unknown}
 */
export function sortKeysDeep(value) {
	if (value === null || typeof value !== 'object') return value;
	if (Array.isArray(value)) return value.map(sortKeysDeep);
	/** @type {Record<string, unknown>} */
	const out = {};
	for (const k of Object.keys(value).sort()) {
		if (value[k] !== undefined) out[k] = sortKeysDeep(value[k]);
	}
	return out;
}

/**
 * The MCP-facing tool object for a definition (definitions are already MCP-shaped).
 * @param {{name: string, description: string, inputSchema: object, outputSchema?: object, annotations?: object}} def
 */
export function toManifestTool(def) {
	return {
		name: def.name,
		description: def.description,
		inputSchema: def.inputSchema,
		outputSchema: def.outputSchema,
		annotations: def.annotations
	};
}

/**
 * @param {Array<object>} definitions
 * @param {(text: string) => string | Promise<string>} sha256Hex
 * @returns {Promise<{protocol: string, manifest_hash: string, tools: object[]}>}
 */
export async function buildManifest(definitions, sha256Hex) {
	const tools = definitions.map(toManifestTool);
	const manifest_hash = await sha256Hex(canonicalJson(tools));
	return { protocol: LINK_PROTOCOL, manifest_hash, tools };
}

/**
 * SHA-256 hex digest with Web Crypto (secure contexts only: https or localhost).
 * @param {string} text
 */
export async function webSha256Hex(text) {
	const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
	return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}
