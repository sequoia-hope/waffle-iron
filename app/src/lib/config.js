// Pointer/drag thresholds (pixels)
export const DRAG_THRESHOLD_PX = 5;
export const RIGHT_DRAG_THRESHOLD = 5;

// Snap thresholds (pixels / degrees)
export const COINCIDENT_SNAP_PX = 8;
export const ON_ENTITY_SNAP_PX = 5;
export const HV_ANGLE_DEG = 3;

// Viewport geometry
export const SIDE_FACE_GROUP_THRESHOLD = 8;

// Toast auto-dismiss durations (ms)
export const TOAST_DISMISS_MS = {
	error: 6000,
	warning: 4000,
	info: 3000,
	success: 2500,
};

// Gear defaults
export const DEFAULT_GEAR_MODULE_DISPLAY = {
	mm: '1', cm: '0.1', m: '0.001', in: '0.04', ft: '0.003',
};
export const GEAR_PREVIEW_MODULE_M = 0.001;
export const DEFAULT_GEAR_TOOTH_COUNT = 20;
export const DEFAULT_GEAR_PRESSURE_ANGLE = 20;
