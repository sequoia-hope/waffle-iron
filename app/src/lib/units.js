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
 * @param {number} internalValue - value in meters
 * @param {string} displayUnit
 * @param {number} [precision=2]
 * @returns {string}
 */
export function formatWithUnit(internalValue, displayUnit, precision = 2) {
	const displayVal = internalToDisplay(internalValue, displayUnit);
	return `${displayVal.toFixed(precision)} ${UNITS[displayUnit]?.label ?? displayUnit}`;
}

/**
 * Format an internal (meters) value for input fields (no unit suffix).
 * Converts to display units first.
 * @param {number} internalValue - value in meters
 * @param {string} displayUnit
 * @param {number} [precision=4]
 * @returns {string}
 */
export function formatForInput(internalValue, displayUnit, precision = 4) {
	const displayVal = internalToDisplay(internalValue, displayUnit);
	return parseFloat(displayVal.toFixed(precision)).toString();
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
