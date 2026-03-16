/**
 * Svelte action: drag-to-resize for mobile bottom sheets.
 *
 * Usage: <div class="header" use:bottomSheetResize={{ minVh: 20, maxVh: 90 }}>
 *
 * On mobile (viewport ≤768px), dragging the header vertically resizes the
 * parent element. On desktop, the action is inert.
 */
export function bottomSheetResize(node, opts = {}) {
	const MOBILE_MAX = 768;
	const minVh = opts.minVh ?? 20;
	const maxVh = opts.maxVh ?? 90;

	let startY = 0;
	let startHeight = 0;
	let dragging = false;

	function isMobile() {
		return window.innerWidth <= MOBILE_MAX;
	}

	function onPointerDown(e) {
		if (!isMobile() || !e.isPrimary) return;
		// Don't interfere with button clicks — only start drag tracking
		dragging = false;
		startY = e.clientY;
		startHeight = node.parentElement.offsetHeight;
		node.setPointerCapture(e.pointerId);
		document.body.style.userSelect = 'none';
	}

	function onPointerMove(e) {
		if (!e.isPrimary || startHeight === 0) return;
		dragging = true;
		const deltaY = startY - e.clientY; // up = positive = taller
		const vh = window.innerHeight / 100;
		const minPx = minVh * vh;
		const maxPx = maxVh * vh;
		const newHeight = Math.min(maxPx, Math.max(minPx, startHeight + deltaY));
		node.parentElement.style.height = newHeight + 'px';
	}

	function onPointerEnd(e) {
		if (!e.isPrimary) return;
		dragging = false;
		startHeight = 0;
		document.body.style.userSelect = '';
	}

	// Prevent scroll/pan interference on the drag handle
	node.style.touchAction = 'none';

	node.addEventListener('pointerdown', onPointerDown);
	node.addEventListener('pointermove', onPointerMove);
	node.addEventListener('pointerup', onPointerEnd);
	node.addEventListener('pointercancel', onPointerEnd);

	return {
		destroy() {
			node.removeEventListener('pointerdown', onPointerDown);
			node.removeEventListener('pointermove', onPointerMove);
			node.removeEventListener('pointerup', onPointerEnd);
			node.removeEventListener('pointercancel', onPointerEnd);
			node.style.touchAction = '';
		}
	};
}
