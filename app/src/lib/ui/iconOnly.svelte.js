/**
 * Whether toolbar buttons show icons only (ui-chrome M13, user decision
 * 2026-10-30: icons + labels on desktop, "only icons on mobile").
 *
 * Wider than the mobile LAYOUT switch (`max-width: 768px` in +page.svelte):
 * a phone held landscape is ~950 px wide and keeps the desktop layout, but it
 * is still a phone. A coarse pointer tells it from a narrow desktop window.
 *
 * The query is read and watched at module load, never inside the getter:
 * the getter is called from `$derived`, where writing `$state` throws
 * (`state_unsafe_mutation`).
 */
const ICON_ONLY_QUERY = '(max-width: 768px), (pointer: coarse) and (max-width: 960px)';

const mql =
	typeof window !== 'undefined' && window.matchMedia ? window.matchMedia(ICON_ONLY_QUERY) : null;

let iconOnly = $state(mql?.matches ?? false);
mql?.addEventListener('change', (e) => {
	iconOnly = e.matches;
});

/** Reactive: true on a phone (either orientation). */
export function isIconOnly() {
	return iconOnly;
}
