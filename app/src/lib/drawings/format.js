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
 *
 * ## The rounding rule, stated
 *
 * ISO 129-1 expects a drawing to round by a stated rule rather than by
 * whatever the renderer happens to do. The rule here is
 * `Number.prototype.toFixed`: **round half away from zero, applied to the
 * exact binary value of the double, after the unit conversion**. A
 * representable half goes up rather than to even — 40.125 mm prints "40.13",
 * and 2.5 mm at zero places prints "3", not "2".
 *
 * The "after the unit conversion" is the part that surprises. As a literal,
 * `1.005` stores a hair below the half, so `(1.005).toFixed(2)` is `"1.00"`;
 * but a 0.001005 m measurement times 1000 lands a hair above it and prints
 * `"1.01"`. The number being rounded is the converted double, not the decimal
 * anyone wrote. That is not a defect to patch around — a decimal re-rounding
 * would print digits the value does not have — and the difference only ever
 * appears one place past what the drawing claims to control. Pinned in
 * `app/tests/gui/drawing-dimension-svg.spec.js`.
 *
 * Both units of a dual dimension use the SAME number of decimals, which is
 * the simple reading of §7's single `precision`. Note that this is coarser in
 * the secondary unit when it is the larger one: two places of mm is 0.01 mm,
 * two places of inches is 0.254 mm. ASME Y14.5 §1.6.2 wants the converted
 * value to preserve the implied precision, so a separate dual precision
 * belongs with M1's document settings.
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

	// An unknown unit key converts by a factor of ONE in `units.js`, which
	// means a 40 mm feature prints as "0.04" under a label the reader will
	// take at face value. A length with no known unit has no legible value at
	// all, so it is withheld the same way a non-finite one is.
	if (!isKnownUnit(unit)) return '—';

	const prefix = kind?.type === 'Radius' ? 'R' : kind?.type === 'Diameter' ? '⌀' : '';
	const primary = fixed(internalToDisplay(value, unit), places);
	const suffix = showUnit ? ` ${label(unit)}` : '';
	let text = `${prefix}${primary}${suffix}`;
	// A dual unit is dropped rather than withheld: the primary value is still
	// correct and legible, and the omission is reported in the render's
	// `warnings`.
	if (dualUnit && dualUnit !== unit && isKnownUnit(dualUnit)) {
		text += ` [${fixed(internalToDisplay(value, dualUnit), places)} ${label(dualUnit)}]`;
	}
	return text;
}

/** Whether `units.js` knows this unit key, i.e. can actually convert it. */
export function isKnownUnit(unit) {
	return typeof unit === 'string' && Object.hasOwn(UNITS, unit);
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
