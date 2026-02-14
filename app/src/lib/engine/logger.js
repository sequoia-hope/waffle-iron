/**
 * Structured session logger for Waffle Iron.
 *
 * Ring buffer of log entries with subscribe/filter/export support.
 * All entries are mirrored to browser console with [waffle:category] prefix.
 */

const MAX_ENTRIES = 2000;
const MAX_STRING_LEN = 200;
const MAX_ARRAY_LEN = 10;
const MAX_DEPTH = 3;

/** @type {Array<{ ts: number, category: string, message: string, data?: any }>} */
const entries = [];

/** @type {Set<(entry: { ts: number, category: string, message: string, data?: any }) => void>} */
const listeners = new Set();

/**
 * Sanitize data to prevent bloat in the log buffer.
 * @param {any} value
 * @param {number} depth
 * @returns {any}
 */
function sanitizeData(value, depth = 0) {
	if (value == null) return value;
	if (depth > MAX_DEPTH) return '[depth limit]';

	if (typeof value === 'string') {
		return value.length > MAX_STRING_LEN ? value.slice(0, MAX_STRING_LEN) + '...' : value;
	}
	if (typeof value === 'number' || typeof value === 'boolean') return value;

	if (ArrayBuffer.isView(value)) {
		return `${value.constructor.name}(${value.length})`;
	}

	if (Array.isArray(value)) {
		const sliced = value.slice(0, MAX_ARRAY_LEN).map(v => sanitizeData(v, depth + 1));
		if (value.length > MAX_ARRAY_LEN) sliced.push(`...+${value.length - MAX_ARRAY_LEN}`);
		return sliced;
	}

	if (typeof value === 'object') {
		const out = {};
		for (const [k, v] of Object.entries(value)) {
			out[k] = sanitizeData(v, depth + 1);
		}
		return out;
	}

	return String(value);
}

/**
 * Log a structured entry.
 * @param {'action'|'error'|'engine'|'sketch'|'ui'|'system'} category
 * @param {string} message
 * @param {any} [data]
 */
export function log(category, message, data) {
	const entry = { ts: Date.now(), category, message };
	if (data !== undefined) {
		entry.data = sanitizeData(data);
	}

	entries.push(entry);
	if (entries.length > MAX_ENTRIES) {
		entries.splice(0, entries.length - MAX_ENTRIES);
	}

	// Mirror to console
	const prefix = `[waffle:${category}]`;
	if (category === 'error') {
		console.error(prefix, message, data !== undefined ? data : '');
	} else {
		console.log(prefix, message, data !== undefined ? data : '');
	}

	// Notify listeners synchronously
	for (const fn of listeners) {
		try { fn(entry); } catch { /* listener error — ignore */ }
	}
}

/**
 * Subscribe to new log entries.
 * @param {(entry: { ts: number, category: string, message: string, data?: any }) => void} fn
 * @returns {() => void} Unsubscribe function
 */
export function onLog(fn) {
	listeners.add(fn);
	return () => listeners.delete(fn);
}

/**
 * Get log entries, optionally filtered.
 * @param {{ category?: string, limit?: number, since?: number }} [filter]
 * @returns {Array<{ ts: number, category: string, message: string, data?: any }>}
 */
export function getLogs(filter) {
	let result = entries;
	if (filter?.category) {
		result = result.filter(e => e.category === filter.category);
	}
	if (filter?.since) {
		result = result.filter(e => e.ts >= filter.since);
	}
	if (filter?.limit) {
		result = result.slice(-filter.limit);
	}
	return result;
}

/**
 * Export logs as formatted text for copy-pasting.
 * @param {{ category?: string, limit?: number, since?: number }} [filter]
 * @returns {string}
 */
export function exportLogs(filter) {
	const logs = getLogs(filter);
	const lines = [
		'Waffle Iron Session Log',
		`Exported: ${new Date().toISOString()}`,
		`Entries: ${logs.length}`,
		'---'
	];
	for (const entry of logs) {
		const ts = new Date(entry.ts).toISOString();
		let line = `[${ts}] [${entry.category}] ${entry.message}`;
		if (entry.data !== undefined) {
			line += ' ' + JSON.stringify(entry.data);
		}
		lines.push(line);
	}
	return lines.join('\n');
}

/**
 * Clear all log entries.
 */
export function clearLogs() {
	entries.length = 0;
}
