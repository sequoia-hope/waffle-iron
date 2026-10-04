/**
 * Document unit system for Waffle Iron.
 *
 * Internal coordinates are always in METERS (1 scene unit = 1 meter).
 * The display unit (mm, cm, m, in, ft) controls how values are shown
 * to the user and how user input is interpreted.
 *
 * Conversion happens at the UI boundary:
 *   displayToInternal(10, 'mm')  → 0.01   (10 mm = 0.01 m)
 *   internalToDisplay(0.01, 'mm') → 10    (0.01 m = 10 mm)
 *
 * ## Display settings are an object, not a default argument (M1)
 *
 * `specs/drawings_and_mbd.md` §9: "Precision and dual units become document
 * settings with per-annotation override. The formatter in `units.js` reads
 * them instead of its default argument, and gains fractional-inch output."
 *
 * So the fallbacks live in exactly one place — [`DEFAULT_DISPLAY`] — and
 * [`resolveDisplay`] is the one function that applies them. A formatter that
 * is handed no precision reads the document's; a caller that has one still
 * passes it, which is why every existing signature is unchanged.
 */

/** Unit definitions with conversion factors to/from meters. */
export const UNITS = {
	mm: { label: 'mm', toMeters: 0.001, fromMeters: 1000 },
	cm: { label: 'cm', toMeters: 0.01, fromMeters: 100 },
	m:  { label: 'm',  toMeters: 1,    fromMeters: 1 },
	in: { label: 'in', toMeters: 0.0254, fromMeters: 1 / 0.0254 },
	ft: { label: 'ft', toMeters: 0.3048, fromMeters: 1 / 0.3048 }
};

/** Ordered list of unit keys for cycling */
export const UNIT_ORDER = ['mm', 'cm', 'm', 'in', 'ft'];

/** Alias map: various spellings → canonical unit key */
const ALIASES = {
	mm: 'mm',
	millimeter: 'mm',
	millimeters: 'mm',
	millimetre: 'mm',
	millimetres: 'mm',
	cm: 'cm',
	centimeter: 'cm',
	centimeters: 'cm',
	centimetre: 'cm',
	centimetres: 'cm',
	m: 'm',
	meter: 'm',
	meters: 'm',
	metre: 'm',
	metres: 'm',
	in: 'in',
	inch: 'in',
	inches: 'in',
	'\u2033': 'in', // ″
	'"': 'in',
	ft: 'ft',
	foot: 'ft',
	feet: 'ft',
	'\u2032': 'ft', // ′
	"'": 'ft'
};

// ───────────────────────────────────────── Document display settings (M1)

/**
 * The denominators a fractional-inch dimension may be written at: the
 * halving sequence an imperial rule is divided into. Nothing outside this set
 * is a drafting fraction — a value "to the nearest 1/10 inch" is written as a
 * decimal — so [`formatFractionalInches`] refuses one rather than inventing
 * a reduction for it.
 */
export const INCH_DENOMINATORS = [2, 4, 8, 16, 32, 64];

/** The most decimals any formatter here will print. */
export const MAX_PLACES = 6;

/**
 * Every display setting's fallback, stated once.
 *
 * `unit` is the FALLBACK ONLY. The document's actual display unit lives in
 * the engine store (`getDocumentDisplayUnit`), which is its single writer;
 * callers that have it pass it in, and this value is what a formatter uses
 * when nobody said. Keeping it out of the mutable record below is what stops
 * this module becoming a second copy of the store's unit.
 *
 * @typedef {object} DisplaySettings
 * @property {string} unit            display unit key ('mm', 'in', …)
 * @property {number} precision       decimals on a displayed value
 * @property {number} inputPrecision  decimals when filling an input field
 * @property {string|null} dualUnit   the bracketed secondary unit, if any
 * @property {number|null} dualPrecision  decimals on the dual value; null ⇒ derived
 * @property {boolean} inchFraction   print inch values as whole + fraction
 * @property {number} inchDenominator the fraction's denominator
 * @property {boolean} fitBand        print an ISO 286 fit's resolved band beside its class
 */
/** @type {Readonly<DisplaySettings>} */
export const DEFAULT_DISPLAY = Object.freeze({
	unit: 'mm',
	precision: 2,
	inputPrecision: 4,
	dualUnit: null,
	dualPrecision: null,
	inchFraction: false,
	inchDenominator: 16,
	fitBand: false
});

/**
 * The document's settings as far as this module knows them — everything but
 * the unit, which the store owns (see [`DEFAULT_DISPLAY`]).
 * @type {DisplaySettings}
 */
let documentDisplay = { ...DEFAULT_DISPLAY };

/**
 * `places`, as a whole number of decimals in `0 .. max`, or `fallback`.
 *
 * `null` and `undefined` are ABSENCE and take the fallback — they are not a
 * precision of zero. `Number(null)` is `0`, and `0` is a legal precision, so
 * the obvious `Number.isFinite(Number(p))` reads "this document states no
 * precision" as "print this dimension to no decimal places": 25.40 mm becomes
 * `25`, silently, on every dimension in the document. It is the same
 * `Number(null) === 0` trap the mass panel hit one layer down, and the reason
 * the test is on the RAW value's type.
 *
 * It also makes the contract `mirrorSessionDocument` states true: that an
 * absent setting may be passed through as `null` and this module supplies the
 * fallback. That comment and this function disagreed, and the `??` defaults at
 * the mirror were all that stood between them.
 */
function clampPlaces(p, fallback, max = MAX_PLACES) {
	if (p == null) return fallback;
	const n = Number(p);
	if (!Number.isFinite(n)) return fallback;
	return Math.min(max, Math.max(0, Math.trunc(n)));
}

/**
 * The document's display settings with `overrides` applied — THE one place
 * the fallbacks are supplied.
 *
 * Unknown keys are kept, so a setting a later version adds survives a
 * round-trip through this function.
 *
 * @param {Partial<DisplaySettings>} [overrides]
 * @returns {DisplaySettings}
 */
export function resolveDisplay(overrides) {
	const merged = { ...DEFAULT_DISPLAY, ...documentDisplay, ...(overrides ?? {}) };
	merged.precision = clampPlaces(merged.precision, DEFAULT_DISPLAY.precision);
	merged.inputPrecision = clampPlaces(merged.inputPrecision, DEFAULT_DISPLAY.inputPrecision, 9);
	merged.dualPrecision =
		merged.dualPrecision == null ? null : clampPlaces(merged.dualPrecision, DEFAULT_DISPLAY.precision);
	// An out-of-set denominator is NOT corrected here: `formatFractionalInches`
	// refuses it and its caller falls back to a decimal and says so. Quietly
	// substituting 16 would print a dimension at a precision nobody asked for.
	return merged;
}

/** The document's display settings. */
export function getDisplay() {
	return resolveDisplay();
}

/**
 * Merge `patch` into the document's display settings.
 * @param {Partial<DisplaySettings>} patch
 * @returns {DisplaySettings}
 */
export function setDisplay(patch) {
	documentDisplay = resolveDisplay(patch);
	return documentDisplay;
}

/** Back to the defaults — a new document, and the tests' reset. */
export function resetDisplay() {
	documentDisplay = { ...DEFAULT_DISPLAY };
	return documentDisplay;
}

/**
 * An inch value as a whole number plus a vulgar fraction: `1-1/2`, `3/8`, `2`.
 *
 * The rules, all of them, because a drawing's reader has to be able to
 * predict them:
 *
 * 1. **Rounding** is to the nearest multiple of `1/denominator`, ties away
 *    from zero — the same direction `toFixed` takes, so a fractional and a
 *    decimal sheet round a half the same way.
 * 2. **Reduction** is by the common factor of two, which is the only factor a
 *    power-of-two denominator can have: `8/16` prints `1/2`, `6/16` prints
 *    `3/8`.
 * 3. **The carry** is done before the split, on the total count of
 *    sixteenths (or whatever the denominator is). `15.99/16` rounds to
 *    `16/16`, which is a whole inch, and prints `1` — never `1-16/16` or
 *    `0-16/16`.
 * 4. **A negative value** carries a leading U+2212 MINUS SIGN: `−1-1/2`. The
 *    sign and the whole-fraction separator are deliberately DIFFERENT
 *    characters — the separator is an ASCII hyphen — so the two are
 *    distinguishable in the markup even though they read alike. A value that
 *    rounds to zero prints `0` with no sign, the same rule `format.js`
 *    applies to a negative zero.
 * 5. **ASCII `1/2`, not U+2044 FRACTION SLASH and not `½`.** The PDF writer
 *    (`drawings/pdf.js`) encodes text as WinAnsi, which has neither U+2044
 *    nor the 1/16 vulgar fractions, so either would print as `?` in the
 *    exported file while looking right on screen. The sheet and the file must
 *    read the same.
 *
 * @param {number} inches
 * @param {number} [denominator] one of [`INCH_DENOMINATORS`]
 * @returns {string|null} null when the value is not finite or the denominator
 *   is not a drafting fraction — the caller then falls back to a decimal and
 *   reports it, rather than printing a number at a precision nobody chose.
 */
export function formatFractionalInches(inches, denominator = DEFAULT_DISPLAY.inchDenominator) {
	if (!Number.isFinite(inches)) return null;
	if (!INCH_DENOMINATORS.includes(denominator)) return null;
	const negative = inches < 0;
	// `Math.abs` first, so a tie goes AWAY from zero rather than toward +∞.
	const count = Math.round(Math.abs(inches) * denominator);
	const whole = Math.floor(count / denominator);
	let num = count - whole * denominator;
	let den = denominator;
	while (num !== 0 && num % 2 === 0) {
		num /= 2;
		den /= 2;
	}
	const sign = negative && count !== 0 ? '−' : '';
	if (num === 0) return `${sign}${whole}`;
	if (whole === 0) return `${sign}${num}/${den}`;
	return `${sign}${whole}-${num}/${den}`;
}

/**
 * Convert a display-unit value to internal (meters).
 * @param {number} displayValue - value in display units
 * @param {string} displayUnit - the unit key (e.g. 'mm')
 * @returns {number} value in meters
 */
export function displayToInternal(displayValue, displayUnit) {
	const u = UNITS[displayUnit];
	if (!u) return displayValue;
	return displayValue * u.toMeters;
}

/**
 * Convert an internal (meters) value to display units.
 * @param {number} internalValue - value in meters
 * @param {string} displayUnit - the unit key (e.g. 'mm')
 * @returns {number} value in display units
 */
export function internalToDisplay(internalValue, displayUnit) {
	const u = UNITS[displayUnit];
	if (!u) return internalValue;
	return internalValue * u.fromMeters;
}

/**
 * Format an internal (meters) value with its unit label.
 * Converts to display units first, then appends the label.
 * e.g. formatWithUnit(0.01, 'mm') → "10.00 mm"
 *
 * `precision` is the caller's override; omitted, the DOCUMENT's precision
 * applies (M1 — it used to be a `= 2` on this signature, which no document
 * setting could reach). An inch value prints as a fraction when the document
 * asks for one.
 *
 * @param {number} internalValue - value in meters
 * @param {string} displayUnit
 * @param {number} [precision] - decimals; default: the document's
 * @returns {string}
 */
export function formatWithUnit(internalValue, displayUnit, precision) {
	const settings = resolveDisplay();
	const displayVal = internalToDisplay(internalValue, displayUnit);
	const label = UNITS[displayUnit]?.label ?? displayUnit;
	if (displayUnit === 'in' && settings.inchFraction) {
		const frac = formatFractionalInches(displayVal, settings.inchDenominator);
		if (frac !== null) return `${frac} ${label}`;
	}
	const places = precision ?? settings.precision;
	return `${displayVal.toFixed(places)} ${label}`;
}

/**
 * Format an internal (meters) value for input fields (no unit suffix).
 * Converts to display units first.
 *
 * Always a DECIMAL, even when the document displays fractional inches:
 * what this fills is an editable field, and `parseAndConvert` reads a number
 * with an optional unit suffix — it cannot read `1-1/2` back. A field the
 * user cannot re-submit unchanged is worse than one that disagrees
 * cosmetically with the drawing.
 *
 * @param {number} internalValue - value in meters
 * @param {string} displayUnit
 * @param {number} [precision] - decimals; default: the document's input precision
 * @returns {string}
 */
export function formatForInput(internalValue, displayUnit, precision) {
	const displayVal = internalToDisplay(internalValue, displayUnit);
	const places = precision ?? resolveDisplay().inputPrecision;
	return parseFloat(displayVal.toFixed(places)).toString();
}

/**
 * Parse a string that may contain a numeric value with an optional unit suffix.
 * Examples: "25.4", "1 inch", "1in", "2.5 ft", "10mm"
 * @param {string} input
 * @returns {{ value: number, unit: string | null }}
 */
export function parseValueWithUnit(input) {
	const trimmed = input.trim();
	if (!trimmed) return { value: NaN, unit: null };

	// Try to match number + optional whitespace + optional unit suffix
	const match = trimmed.match(/^([+-]?\d*\.?\d+(?:[eE][+-]?\d+)?)\s*(.*)$/);
	if (!match) return { value: NaN, unit: null };

	const value = parseFloat(match[1]);
	const suffix = match[2].trim().toLowerCase();

	if (!suffix) return { value, unit: null };

	const unitKey = ALIASES[suffix];
	if (unitKey) return { value, unit: unitKey };

	// Unknown suffix — return value with null unit (caller decides)
	return { value, unit: null };
}

/**
 * True when the WHOLE input is a plain measurement: a number with an optional
 * known unit suffix ("25", "10mm", "1.5 in"). Anything else — "width / 2",
 * "2*45", "10mm + 1" — is not, and should be treated as an expression.
 * (parseAndConvert alone can't tell: it reads the leading number and ignores
 * an unknown tail, so "2*45" would silently parse as 2.)
 * @param {string} input
 * @returns {boolean}
 */
export function isPlainMeasurement(input) {
	const match = input.trim().match(/^([+-]?\d*\.?\d+(?:[eE][+-]?\d+)?)\s*(.*)$/);
	if (!match) return false;
	const suffix = match[2].trim().toLowerCase();
	return !suffix || ALIASES[suffix] != null;
}

/**
 * Parse user input and convert to internal units (meters).
 * If the input has a unit suffix, that unit is used for conversion.
 * If no suffix, the value is assumed to be in displayUnit.
 * @param {string} input
 * @param {string} displayUnit - the document's display unit
 * @returns {number} value in meters, or NaN if unparseable
 */
export function parseAndConvert(input, displayUnit) {
	const { value, unit } = parseValueWithUnit(input);
	if (isNaN(value)) return NaN;

	// Use the explicit unit if provided, otherwise assume displayUnit
	const effectiveUnit = unit || displayUnit;
	return displayToInternal(value, effectiveUnit);
}
