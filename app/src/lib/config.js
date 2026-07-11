// Pointer/drag thresholds (pixels)
export const DRAG_THRESHOLD_PX = 5;
export const RIGHT_DRAG_THRESHOLD = 5;

// Click-vs-drag disambiguation for drawing tools. A press that moved past
// DRAG_THRESHOLD_PX finalizes a click-drag on release ONLY IF it also either
// moved past DRAG_COMMIT_PX (unambiguously a drag) OR was held at least
// DRAG_MIN_DURATION_MS. A small, quick twitch is therefore a click-in-place, so
// a fast click that jitters a few pixels no longer drops a tiny segment.
export const DRAG_MIN_DURATION_MS = 200;
export const DRAG_COMMIT_PX = 16;

// Snap thresholds (pixels / degrees)
export const COINCIDENT_SNAP_PX = 8;
export const ON_ENTITY_SNAP_PX = 5;
export const HV_ANGLE_DEG = 3;
// Point-alignment inference (screen-px calibrated; see specs/snap_inference_and_priority.md)
export const INFERENCE_ALIGN_PX = 6; // half-band around an armed source's axis
export const INFERENCE_SOURCES_MAX = 3; // LRU size of armed inference sources
export const CANDIDATE_DEDUP_PX = 4; // preview-candidate dedup radius (was 0.001 sketch units)

// Viewport geometry
export const SIDE_FACE_GROUP_THRESHOLD = 8;

// Toast auto-dismiss durations (ms)
export const TOAST_DISMISS_MS = {
	error: 6000,
	warning: 4000,
	info: 3000,
	success: 2500,
};

// Toast rate limiting: an identical (level, message) toast is suppressed while
// one is visible and for this window after it was last shown, so a repeating
// problem toasts at most once per window instead of once per firing.
export const TOAST_REPEAT_SUPPRESS_MS = 5000;
// When a burst pushes the visible stack past this cap, the older toasts are
// auto-cleared and only the newest is kept.
export const TOAST_STACK_MAX = 4;

// Long-press (mobile context menu)
export const LONG_PRESS_DURATION_MS = 500;
export const LONG_PRESS_THRESHOLD_PX = 10;

// Gear defaults
export const DEFAULT_GEAR_MODULE_DISPLAY = {
	mm: '1', cm: '0.1', m: '0.001', in: '0.04', ft: '0.003',
};
export const GEAR_PREVIEW_MODULE_M = 0.001;
export const DEFAULT_GEAR_TOOTH_COUNT = 20;
export const DEFAULT_GEAR_PRESSURE_ANGLE = 20;
