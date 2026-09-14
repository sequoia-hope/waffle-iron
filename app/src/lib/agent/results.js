/**
 * Tool result shapes for the agent link (specs/waffle_mcp_server.md §2.3 `result`,
 * I10). Pure.
 */

/** A refusal or failure that becomes an `isError` result with `{code, message, details}`. */
export class ToolFailure extends Error {
	/**
	 * @param {string} code - one of the closed set in spec §6.1
	 * @param {string} message
	 * @param {Record<string, unknown>} [details]
	 */
	constructor(code, message, details = {}) {
		super(`${code}: ${message}`);
		this.code = code;
		this.detail = message;
		this.details = details;
	}
}

/**
 * @param {string} code
 * @param {string} message
 * @param {Record<string, unknown>} [details]
 */
export function fail(code, message, details = {}) {
	return new ToolFailure(code, message, details);
}

/**
 * An `isError` tool result carrying `structuredContent.error = {code, message, details}` (I10).
 * @param {string} code
 * @param {string} message
 * @param {Record<string, unknown>} [details]
 */
export function toolError(code, message, details = {}) {
	return {
		content: [{ type: 'text', text: `${code}: ${message}` }],
		structuredContent: { error: { code, message, details } },
		isError: true
	};
}

/** @param {Record<string, unknown>} structured */
export function toolOk(structured) {
	return {
		content: [{ type: 'text', text: JSON.stringify(structured) }],
		structuredContent: structured,
		isError: false
	};
}

/**
 * A detached plain-JSON copy ($state proxies do not survive postMessage or JSON reliably).
 * @template T
 * @param {T} value
 * @returns {T}
 */
export function plain(value) {
	return value === undefined ? value : JSON.parse(JSON.stringify(value));
}
