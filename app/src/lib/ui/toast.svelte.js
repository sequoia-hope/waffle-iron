/**
 * Reactive toast notification state for Waffle Iron.
 *
 * Uses Svelte 5 $state runes for reactivity.
 * Auto-dismisses toasts after a duration based on level.
 *
 * Rate limiting: an identical (level, message) pair never stacks a duplicate
 * while visible, and is suppressed for TOAST_REPEAT_SUPPRESS_MS after it was
 * last shown — so a repeat-firing problem toasts once per window, not once
 * per firing. A burst of distinct toasts past TOAST_STACK_MAX auto-clears the
 * older ones, keeping only the newest.
 */

import { onLog } from '$lib/engine/logger.js';
import { TOAST_DISMISS_MS, TOAST_REPEAT_SUPPRESS_MS, TOAST_STACK_MAX } from '$lib/config.js';

/** @type {Array<{ id: number, level: string, message: string, timer: ReturnType<typeof setTimeout> }>} */
let toasts = $state([]);

let nextId = 1;

/** Wall-clock of the last time each `${level}\n${message}` was shown. */
let lastShownAt = new Map();

/**
 * Show a toast notification.
 * @param {'error'|'warning'|'info'|'success'} level
 * @param {string} message
 * @param {number} [durationMs] - Override auto-dismiss duration
 * @returns {number} Toast ID; the visible duplicate's ID when one is already
 *   showing; 0 when suppressed by the repeat window (nothing shown).
 */
export function showToast(level, message, durationMs) {
	// An identical toast is already visible — don't stack a duplicate.
	const visible = toasts.find((t) => t.level === level && t.message === message);
	if (visible) return visible.id;

	// Recently shown (even if since dismissed) — suppress the repeat.
	const key = `${level}\n${message}`;
	const now = Date.now();
	const last = lastShownAt.get(key);
	if (last !== undefined && now - last < TOAST_REPEAT_SUPPRESS_MS) return 0;
	if (lastShownAt.size > 64) {
		for (const [k, t] of lastShownAt) {
			if (now - t >= TOAST_REPEAT_SUPPRESS_MS) lastShownAt.delete(k);
		}
	}
	lastShownAt.set(key, now);

	// A burst past the cap reads as noise — clear the stack, keep the newest.
	if (toasts.length >= TOAST_STACK_MAX) {
		for (const t of toasts) clearTimeout(t.timer);
		toasts = [];
	}

	const id = nextId++;
	const ms = durationMs ?? TOAST_DISMISS_MS[level] ?? 3000;
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
 * Dismiss every toast and reset the repeat-suppression window. An explicit
 * clear acknowledges everything shown, so the same message may show again
 * immediately (unlike per-toast dismissal, which keeps the window).
 */
export function dismissAllToasts() {
	for (const t of toasts) clearTimeout(t.timer);
	toasts = [];
	lastShownAt = new Map();
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
