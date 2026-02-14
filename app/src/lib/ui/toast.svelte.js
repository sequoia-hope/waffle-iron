/**
 * Reactive toast notification state for Waffle Iron.
 *
 * Uses Svelte 5 $state runes for reactivity.
 * Auto-dismisses toasts after a duration based on level.
 */

import { onLog } from '$lib/engine/logger.js';

/** @type {Array<{ id: number, level: string, message: string, timer: ReturnType<typeof setTimeout> }>} */
let toasts = $state([]);

let nextId = 1;

const AUTO_DISMISS_MS = {
	error: 6000,
	warning: 4000,
	info: 3000,
	success: 2500,
};

/**
 * Show a toast notification.
 * @param {'error'|'warning'|'info'|'success'} level
 * @param {string} message
 * @param {number} [durationMs] - Override auto-dismiss duration
 * @returns {number} Toast ID
 */
export function showToast(level, message, durationMs) {
	const id = nextId++;
	const ms = durationMs ?? AUTO_DISMISS_MS[level] ?? 3000;
	const timer = setTimeout(() => dismissToast(id), ms);
	toasts = [...toasts, { id, level, message, timer }];
	return id;
}

/**
 * Dismiss a toast by ID.
 * @param {number} id
 */
export function dismissToast(id) {
	const idx = toasts.findIndex(t => t.id === id);
	if (idx < 0) return;
	clearTimeout(toasts[idx].timer);
	toasts = [...toasts.slice(0, idx), ...toasts.slice(idx + 1)];
}

/**
 * Get the current toasts (reactive).
 * @returns {Array<{ id: number, level: string, message: string }>}
 */
export function getToasts() {
	return toasts;
}

/**
 * Subscribe to logger errors and auto-show toasts.
 * Call once at startup.
 */
export function initLoggerToasts() {
	onLog((entry) => {
		if (entry.category === 'error') {
			showToast('error', entry.message);
		}
	});
}
