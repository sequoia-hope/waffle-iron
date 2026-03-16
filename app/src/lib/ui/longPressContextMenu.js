import { LONG_PRESS_DURATION_MS, LONG_PRESS_THRESHOLD_PX } from '$lib/config.js';

/**
 * Svelte action: long-press on touch dispatches a synthetic contextmenu event.
 * Usage: <div use:longPressContextMenu> or <div use:longPressContextMenu={{ duration: 600 }}>
 */
export function longPressContextMenu(node, opts = {}) {
	const duration = opts.duration ?? LONG_PRESS_DURATION_MS;
	const threshold = opts.threshold ?? LONG_PRESS_THRESHOLD_PX;

	let timer = null;
	let startX = 0;
	let startY = 0;
	let activePointerId = null;

	function clear() {
		if (timer !== null) {
			clearTimeout(timer);
			timer = null;
		}
		activePointerId = null;
	}

	function onPointerDown(e) {
		// Only react to primary touch
		if (e.pointerType !== 'touch') return;

		if (!e.isPrimary) {
			// Second finger — cancel (two-finger gesture)
			clear();
			return;
		}

		startX = e.clientX;
		startY = e.clientY;
		activePointerId = e.pointerId;

		const target = e.target;
		const cx = e.clientX;
		const cy = e.clientY;

		timer = setTimeout(() => {
			timer = null;
			const pid = activePointerId;
			activePointerId = null;

			// Cancel the active pointer gesture so OrbitControls cleans up
			// its internal _pointers array before we show the context menu.
			if (pid !== null) {
				target.dispatchEvent(new PointerEvent('pointercancel', {
					bubbles: true,
					cancelable: true,
					clientX: cx,
					clientY: cy,
					pointerId: pid,
					pointerType: 'touch',
					isPrimary: true,
				}));
			}

			// Dispatch synthetic contextmenu on the actual touched element
			const evt = new MouseEvent('contextmenu', {
				bubbles: true,
				cancelable: true,
				clientX: cx,
				clientY: cy,
				screenX: e.screenX,
				screenY: e.screenY,
			});
			target.dispatchEvent(evt);
		}, duration);
	}

	function onPointerMove(e) {
		if (e.pointerId !== activePointerId) return;
		const dx = e.clientX - startX;
		const dy = e.clientY - startY;
		if (Math.sqrt(dx * dx + dy * dy) > threshold) {
			clear();
		}
	}

	function onPointerUp(e) {
		if (e.pointerId === activePointerId) {
			clear();
		}
	}

	function onPointerCancel(e) {
		if (e.pointerId === activePointerId) {
			clear();
		}
	}

	node.addEventListener('pointerdown', onPointerDown);
	window.addEventListener('pointermove', onPointerMove);
	window.addEventListener('pointerup', onPointerUp);
	window.addEventListener('pointercancel', onPointerCancel);

	return {
		destroy() {
			clear();
			node.removeEventListener('pointerdown', onPointerDown);
			window.removeEventListener('pointermove', onPointerMove);
			window.removeEventListener('pointerup', onPointerUp);
			window.removeEventListener('pointercancel', onPointerCancel);
		}
	};
}
