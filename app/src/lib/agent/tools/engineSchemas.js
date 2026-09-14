/**
 * Engine-type JSON Schemas for tool `inputSchema`s (specs/waffle_mcp_server.md §2.4).
 *
 * The definitions come from `docs/schema/waffle-v5.schema.json` (the CI-pinned
 * golden) through `engineSchemas.generated.js`, which
 * `app/scripts/gen-agent-manifest.mjs` regenerates; nothing is hand-copied. A
 * tool schema refers to them as `#/$defs/<Name>` and carries its own `$defs`
 * block, so MCP clients and the relay can resolve every reference without the
 * golden file. Plain ESM: loaded by the page and by the generator under Node.
 */
import { ENGINE_DEFS } from './engineSchemas.generated.js';

/**
 * The `$defs` a schema referring to `roots` needs: the roots and every
 * definition they reference, transitively.
 * @param {...string} roots
 * @returns {Record<string, object>}
 */
export function defsFor(...roots) {
	/** @type {Record<string, object>} */
	const out = {};
	const stack = [...roots];
	while (stack.length > 0) {
		const name = /** @type {string} */ (stack.pop());
		if (name in out) continue;
		const def = ENGINE_DEFS[name];
		if (!def) throw new Error(`engine schema has no definition ${name}`);
		out[name] = def;
		for (const match of JSON.stringify(def).matchAll(/"#\/\$defs\/([A-Za-z0-9_]+)"/g)) {
			stack.push(match[1]);
		}
	}
	return Object.fromEntries(Object.keys(out).sort().map((k) => [k, out[k]]));
}

/** `{ $ref }` to an engine definition. @param {string} name */
export function engineRef(name) {
	return { $ref: `#/$defs/${name}` };
}
