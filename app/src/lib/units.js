/**
 * Document unit system for Waffle Iron.
 *
 * Internal storage is always meters. This module provides conversion
 * between meters and display units (mm, cm, m, in, ft).
 */

export const UNITS = {
	mm: { label: 'mm', toMeters: 0.001, fromMeters: 1000 },
	cm: { label: 'cm', toMeters: 0.01, fromMeters: 100 },
	m: { label: 'm', toMeters: 1, fromMeters: 1 },
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
 * Convert a value in display units to meters (internal).
 * @param {number} displayValue
 * @param {string} displayUnit - unit key (mm, cm, m, in, ft)
 * @returns {number}
 */
export function displayToInternal(displayValue, displayUnit) {
	const unit = UNITS[displayUnit];
	if (!unit) return displayValue;
	return displayValue * unit.toMeters;
}

/**
 * Convert a value in meters (internal) to display units.
 * @param {number} internalValue
 * @param {string} displayUnit
 * @returns {number}
 */
export function internalToDisplay(internalValue, displayUnit) {
	const unit = UNITS[displayUnit];
	if (!unit) return internalValue;
	return internalValue * unit.fromMeters;
}

/**
 * Format an internal (meters) value for display labels.
 * e.g. formatWithUnit(0.0254, 'mm') → "25.40 mm"
 * @param {number} internalValue
 * @param {string} displayUnit
 * @param {number} [precision=2]
 * @returns {string}
 */
export function formatWithUnit(internalValue, displayUnit, precision = 2) {
	const display = internalToDisplay(internalValue, displayUnit);
	return `${display.toFixed(precision)} ${UNITS[displayUnit]?.label ?? displayUnit}`;
}

/**
 * Format an internal value for input fields (no unit suffix).
 * @param {number} internalValue
 * @param {string} displayUnit
 * @param {number} [precision=4]
 * @returns {string}
 */
export function formatForInput(internalValue, displayUnit, precision = 4) {
	const display = internalToDisplay(internalValue, displayUnit);
	// Remove trailing zeros after decimal point for cleaner display
	return parseFloat(display.toFixed(precision)).toString();
}

/**
 * Parse user input and convert to meters.
 * If the input has a unit suffix, convert from that unit.
 * If no suffix, assume the value is in displayUnit.
 * @param {string} input
 * @param {string} displayUnit - fallback unit if no suffix
 * @returns {number} value in meters, or NaN if unparseable
 */
export function parseAndConvert(input, displayUnit) {
	const { value, unit } = parseValueWithUnit(input);
	if (isNaN(value)) return NaN;

	const effectiveUnit = unit || displayUnit;
	return displayToInternal(value, effectiveUnit);
}
