/**
 * Formatting a measured value as dimension text
 * (`specs/drawings_and_mbd.md` §7: `precision`, `dual_unit`).
 *
 * The number itself is never computed here — it arrives already measured from
 * `waffle_types::annotation::measure`, in meters (radians for an angular
 * dimension). This module only decides how it READS: the unit conversion, the
 * decimal places, the `R` / `⌀` prefix, the degree sign, and the bracketed
 * dual unit.
 *
 * ## Why the unit suffix is omitted by default
 *
 * ISO 129-1 §4.2 and ASME Y14.5 both have the drawing declare its unit once,
 * in the title block, and print dimensions as bare numbers. So a linear
 * dimension formats as `40.00`, not `40.00 mm` — `showUnit` exists for the
 * places that want it (a dual unit, a note, a tooltip) and defaults off.
 *
 * Angular dimensions are the exception the standards make: degrees always
 * carry their `°`, because there is no title-block declaration for them.
 */

import { internalToDisplay, UNITS } from '$lib/units.js';

/** Default decimal places when neither the annotation nor the document says. */
export const DEFAULT_PRECISION = 2;

/**
 * Format a measured value.
 *
 * @param {object} opts
 * @param {number} opts.value         measured — meters, or radians if angular
 * @param {any} opts.kind             the `DimensionKind` record
 * @param {string} [opts.unit]        display unit key ('mm', 'in', …); default 'mm'
 * @param {number|null} [opts.precision]  the annotation's override
 * @param {number} [opts.documentPrecision]
 * @param {string|null} [opts.dualUnit]
 * @param {boolean} [opts.showUnit]
 * @returns {string}
 */
export function formatDimension({
	value,
	kind,
	unit = 'mm',
	precision = null,
	documentPrecision = DEFAULT_PRECISION,
	dualUnit = null,
	showUnit = false
}) {
	const places = clampPrecision(precision ?? documentPrecision);
	// A non-finite value must never reach a sheet as a number. `measure`
	// refuses one in Rust, so arriving here means the record was built by
	// something that did not — say it, rather than printing "NaN" where a
	// machinist reads a size.
	if (!Number.isFinite(value)) return '—';

	if (kind?.type === 'Angle') {
		return `${fixed(radToDeg(value), places)}°`;
	}

	const prefix = kind?.type === 'Radius' ? 'R' : kind?.type === 'Diameter' ? '⌀' : '';
	const primary = fixed(internalToDisplay(value, unit), places);
	const suffix = showUnit ? ` ${label(unit)}` : '';
	let text = `${prefix}${primary}${suffix}`;
	if (dualUnit && dualUnit !== unit) {
		text += ` [${fixed(internalToDisplay(value, dualUnit), places)} ${label(dualUnit)}]`;
	}
	return text;
}

/** @param {number} rad */
export function radToDeg(rad) {
	return (rad * 180) / Math.PI;
}

/**
 * `toFixed`, with negative zero normalized away.
 *
 * `(-0).toFixed(2)` is `"-0.00"`, and a dimension that prints `-0.00` because
 * a coordinate happened to be a negative zero is a bug a reader cannot
 * diagnose. Rounding to the stated places first means the sign is dropped
 * only when the ROUNDED value is zero.
 */
export function fixed(x, places) {
	const s = x.toFixed(places);
	return Number(s) === 0 ? (0).toFixed(places) : s;
}

function clampPrecision(p) {
	const n = Number(p);
	if (!Number.isFinite(n)) return DEFAULT_PRECISION;
	return Math.min(6, Math.max(0, Math.trunc(n)));
}

function label(unit) {
	return UNITS[unit]?.label ?? unit;
}
