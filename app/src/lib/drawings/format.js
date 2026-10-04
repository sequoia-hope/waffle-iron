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
 * ## The dual unit's precision is its own (M1)
 *
 * D3 gave both units of a dual dimension the same number of decimals, which
 * is coarser in the larger unit: two places of millimetres is 0.01 mm, two
 * places of inches is 0.254 mm, so the bracketed value was 25× coarser than
 * the primary it restates. ASME Y14.5 §1.6.2 wants the conversion to preserve
 * the implied precision. So, in order:
 *
 * 1. the annotation's own `dual_precision`, when it has one;
 * 2. the document's `dualPrecision` setting, when it has one;
 * 3. **derived**: the fewest decimals of the dual unit whose resolution is no
 *    coarser than the primary's. `10^-q · metres(dual) ≤ 10^-p · metres(primary)`
 *    gives `q = ceil(p + log10(metres(dual) / metres(primary)))`, clamped to
 *    `0 .. 6`. From mm at 2 places that is 4 places of inches (0.0025 mm, so
 *    finer, never coarser); from inches at 3 places it is 2 places of mm.
 *
 * ## The tolerance band prints at its OWN resolution
 *
 * A ±0.021 band under a two-place dimension must not print as `±0.02`: that
 * is a different tolerance. So a band member prints at **the fewest decimals,
 * never below the dimension's own and never above six, that represent it
 * exactly** — 0.021 needs three, 0.10 is exact at two. Both members of a pair
 * share the finer of the two, because ISO 129-1 prints a deviation pair with
 * aligned decimals. A member that cannot be represented at six places (a
 * third of a millimetre) prints at six, rounded.
 *
 * A zero deviation prints as a bare `0` — no sign, no decimals — which is
 * ASME Y14.5 §2.3.2's rule for a unilateral tolerance (`25.00 +0.021 / 0`).
 *
 * ## Fractional inches apply to the NOMINAL only
 *
 * A fractional-inch document prints its sizes as `1-1/2`; its tolerances
 * still print as decimals. The coarsest drafting fraction, 1/64 inch, is
 * 0.4 mm — larger than any tolerance band a drawing carries, so a ±0.021 mm
 * band rounded to a fraction would print as `0` and a 0.2 mm position zone
 * would vanish. So the nominal and its bracketed dual may be fractions; a
 * deviation, a limit of size and a geometric tolerance's zone width are
 * always decimals.
 *
 * ## What this module NEVER does
 *
 * It never resolves a tolerance. `H7` became two numbers in
 * `waffle_types::annotation::tolerance::Tolerance::limits_of` and reaches
 * here as `deviations` / `limits` in the layout record; there is no ISO 286
 * table in JavaScript and there must not be one, or a drawing and its STEP
 * export could disagree about what the fit means. The only thing this module
 * decides on its own is a GLYPH: the ISO 1101 characteristic symbols, the
 * material-condition circles and the zone prefixes, which are presentation
 * and belong to the renderer.
 */

import {
	formatFractionalInches,
	internalToDisplay,
	MAX_PLACES,
	resolveDisplay,
	UNITS
} from '$lib/units.js';

/** Default decimal places when neither the annotation nor the document says. */
export const DEFAULT_PRECISION = 2;

/** What a non-finite number prints as, anywhere on a sheet. */
export const NO_VALUE = '—';

/** U+2212 MINUS SIGN — a drawing's minus, not a hyphen. */
const MINUS = '−';

/**
 * Format a measured value.
 *
 * The untoleranced single-line form, unchanged from D3 apart from the dual
 * unit's own precision. A toleranced dimension goes through
 * [`formatDimensionText`], which stacks where the standard stacks.
 *
 * @param {object} opts
 * @param {number} opts.value         measured — meters, or radians if angular
 * @param {any} opts.kind             the `DimensionKind` record
 * @param {string} [opts.unit]        display unit key ('mm', 'in', …); default 'mm'
 * @param {number|null} [opts.precision]  the annotation's override
 * @param {number} [opts.documentPrecision]
 * @param {string|null} [opts.dualUnit]
 * @param {number|null} [opts.dualPrecision] the annotation's override
 * @param {boolean} [opts.showUnit]
 * @param {Partial<import('$lib/units.js').DisplaySettings>} [opts.display]
 * @returns {string}
 */
export function formatDimension(opts) {
	return nominalText(opts).text;
}

/**
 * The nominal value as text, plus the pieces a toleranced form needs again.
 *
 * @returns {{ text: string, prefix: string, places: number, dual: string,
 *             known: boolean, settings: any }}
 */
function nominalText({
	value,
	kind,
	unit = undefined,
	precision = null,
	documentPrecision = undefined,
	dualUnit = undefined,
	dualPrecision = null,
	showUnit = false,
	display = undefined
}) {
	const settings = resolveDisplay({
		...(display ?? {}),
		...(unit !== undefined ? { unit } : {}),
		...(documentPrecision !== undefined ? { precision: documentPrecision } : {}),
		...(dualUnit !== undefined ? { dualUnit } : {})
	});
	const u = settings.unit;
	const places = clampPrecision(precision ?? settings.precision);
	const none = { text: NO_VALUE, prefix: '', places, dual: '', known: false, settings };

	// A non-finite value must never reach a sheet as a number. `measure`
	// refuses one in Rust, so arriving here means the record was built by
	// something that did not — say it, rather than printing "NaN" where a
	// machinist reads a size.
	if (!Number.isFinite(value)) return none;

	if (kind?.type === 'Angle') {
		return {
			text: `${fixed(radToDeg(value), places)}°`,
			prefix: '',
			places,
			dual: '',
			known: true,
			settings
		};
	}

	// An unknown unit key converts by a factor of ONE in `units.js`, which
	// means a 40 mm feature prints as "0.04" under a label the reader will
	// take at face value. A length with no known unit has no legible value at
	// all, so it is withheld the same way a non-finite one is.
	if (!isKnownUnit(u)) return none;

	const prefix = kind?.type === 'Radius' ? 'R' : kind?.type === 'Diameter' ? '⌀' : '';
	const primary = lengthText(value, u, places, settings);
	const suffix = showUnit ? ` ${label(u)}` : '';
	// A dual unit is dropped rather than withheld: the primary value is still
	// correct and legible, and the omission is reported in the render's
	// `warnings`.
	const d = settings.dualUnit;
	let dual = '';
	if (d && d !== u && isKnownUnit(d)) {
		const dp = dualPlaces({
			unit: u,
			dualUnit: d,
			places,
			annotation: dualPrecision,
			document: settings.dualPrecision
		});
		dual = ` [${lengthText(value, d, dp, settings)} ${label(d)}]`;
	}
	return {
		text: `${prefix}${primary}${suffix}${dual}`,
		prefix,
		places,
		dual,
		known: true,
		settings
	};
}

/**
 * A LENGTH in meters as display text at `places` decimals — fractional when
 * the unit is inches and the document asks for fractions.
 */
function lengthText(meters, unit, places, settings) {
	const v = internalToDisplay(meters, unit);
	if (unit === 'in' && settings?.inchFraction) {
		const frac = formatFractionalInches(v, settings.inchDenominator);
		if (frac !== null) return frac;
	}
	return fixed(v, places);
}

/**
 * The dual unit's decimal places: the annotation's, else the document's, else
 * derived so the bracketed value is no coarser than the primary (see the
 * module docs).
 */
export function dualPlaces({ unit, dualUnit, places, annotation = null, document = null }) {
	if (annotation != null && Number.isFinite(Number(annotation))) return clampPrecision(annotation);
	if (document != null && Number.isFinite(Number(document))) return clampPrecision(document);
	const mPrimary = UNITS[unit]?.toMeters;
	const mDual = UNITS[dualUnit]?.toMeters;
	if (!(mPrimary > 0) || !(mDual > 0)) return clampPrecision(places);
	// `- 1e-9` so an exact power of ten (mm ↔ m) does not round up by a float
	// hair and print a decimal the primary does not imply.
	const q = Math.ceil(places + Math.log10(mDual / mPrimary) - 1e-9);
	return clampPrecision(q);
}

/**
 * The fewest decimals, at least `min` and at most [`MAX_PLACES`], at which
 * `x` prints exactly — the band-precision rule from the module docs.
 *
 * "Exactly" is to within a relative 1e-9, because the numbers arrive as
 * doubles that went through a unit conversion: 0.021 mm is 2.1e-5 m times
 * 1000, which is 0.021 to within 1e-18 but not bit-identical to the literal.
 */
export function exactPlaces(x, min) {
	const start = clampPrecision(min);
	if (!Number.isFinite(x)) return start;
	for (let p = start; p <= MAX_PLACES; p++) {
		const s = x.toFixed(p);
		if (Math.abs(Number(s) - x) <= 1e-9 * Math.max(1, Math.abs(x))) return p;
	}
	return MAX_PLACES;
}

/**
 * A dimension's printed text, tolerance and all (M1).
 *
 * Returns LINES, because two of the five tolerance forms are stacked on a
 * drawing and a single string cannot say where the break goes. `boxed` asks
 * the renderer for the basic-dimension rectangle. Nothing here draws; the
 * arrangement of the lines is `layout.js`'s.
 *
 * ## The arrangement, form by form (ISO 129-1, ASME Y14.5 §2.3)
 *
 * | form | lines |
 * |---|---|
 * | none | `25.00` |
 * | `Symmetric` | `25.00 ±0.10` |
 * | `Deviations` | `25.00 +0.021 / −0.005` |
 * | `Limits` | `25.021` over `25.000` — the two SIZES, no nominal |
 * | `Fit` | `⌀25.00 H7/g6` |
 * | `Basic` | `25.00`, boxed |
 *
 * `Symmetric`, `Deviations` and `Fit` are inline: they are short, and a
 * drawing that stacks every band uses twice the vertical room for no gain in
 * clarity. `Limits` is stacked because it has no nominal to be inline WITH —
 * a limit dimension prints the two sizes and nothing else, and printing them
 * as `25.021 / 25.000` invites reading the second as a deviation.
 *
 * A dual unit is appended to each line that carries a size, which for the
 * stacked `Limits` form means both: each line is a size in its own right.
 *
 * @param {any} a an `AnnotationLayout::Dimension` record
 * @param {Partial<import('$lib/units.js').DisplaySettings>} [display]
 * @returns {{ lines: string[], boxed: boolean, warnings: string[] }}
 */
export function formatDimensionText(a, display) {
	const warnings = [];
	const nominal = nominalText({
		value: a?.value,
		kind: a?.kind,
		precision: a?.precision ?? null,
		dualUnit: a?.dual_unit ?? undefined,
		dualPrecision: a?.dual_precision ?? null,
		display
	});
	const tol = a?.tolerance ?? null;
	if (!tol || !nominal.known) return { lines: [nominal.text], boxed: false, warnings };

	const form = tol.display?.type ?? null;
	const angular = a?.kind?.type === 'Angle';
	const settings = nominal.settings;
	// Every band number is in the dimension's own model unit — metres, or
	// RADIANS on an angular dimension — exactly like `value`, so it takes the
	// same conversion.
	// `Number.isFinite` on the RAW value, before the conversion: `null` and
	// `""` multiply to a perfectly finite 0, so a record missing a deviation
	// would otherwise print a tolerance of zero — the most dangerous possible
	// reading of an absent one.
	const toDisplay = (x) =>
		typeof x === 'number' && Number.isFinite(x)
			? angular
				? radToDeg(x)
				: internalToDisplay(x, settings.unit)
			: NaN;
	const band = (xs, min) => {
		const vals = xs.map(toDisplay);
		if (!vals.every(Number.isFinite)) return null;
		// One shared precision for the pair: the finer of what each needs, and
		// never fewer decimals than the dimension itself.
		const places = Math.max(...vals.map((v) => exactPlaces(v, min)));
		return { vals, places };
	};

	switch (form) {
		case 'Symmetric': {
			const b = band([a.tolerance.deviations?.[0] ?? NaN], nominal.places);
			if (!b) {
				warnings.push('a symmetric tolerance carries no finite deviation');
				return { lines: [`${nominal.text} ${NO_VALUE}`], boxed: false, warnings };
			}
			return {
				lines: [`${nominal.text} ±${fixed(Math.abs(b.vals[0]), b.places)}`],
				boxed: false,
				warnings
			};
		}
		case 'Deviations': {
			const text = deviationPair(a.tolerance.deviations, nominal.places, toDisplay, warnings);
			return { lines: [`${nominal.text} ${text}`], boxed: false, warnings };
		}
		case 'Limits': {
			const b = band(a.tolerance.limits ?? [NaN, NaN], nominal.places);
			if (!b) {
				warnings.push('a limits tolerance carries no finite pair of sizes');
				return { lines: [nominal.text], boxed: false, warnings };
			}
			// Upper first: it is the line that reads above the other once
			// `layout.js` stacks them.
			return {
				lines: b.vals.map((v) => `${nominal.prefix}${fixed(v, b.places)}${nominal.dual}`),
				boxed: false,
				warnings
			};
		}
		case 'Fit': {
			const classes = [tol.display.hole, tol.display.shaft].filter(
				(c) => typeof c === 'string' && c.length > 0
			);
			if (classes.length === 0) {
				// Rust refuses a fit with neither class (`FitWithNoClass`), so a
				// record carrying one was not built by `ToleranceLayout::resolve`.
				warnings.push('a fit tolerance names neither a hole class nor a shaft class');
				return { lines: [nominal.text], boxed: false, warnings };
			}
			let text = `${nominal.text} ${classes.join('/')}`;
			// The resolved band beside the class, for a drawing that prints both
			// (ISO 286 allows either). The numbers are the ENGINE's — there is no
			// ISO 286 table here.
			if (settings.fitBand && a.tolerance.deviations) {
				text += ` (${deviationPair(a.tolerance.deviations, nominal.places, toDisplay, warnings)})`;
			}
			return { lines: [text], boxed: false, warnings };
		}
		case 'Basic':
			return { lines: [nominal.text], boxed: true, warnings };
		default:
			// A tolerance form this build does not know. The value still prints —
			// it is measured and correct — but the omission is named, because a
			// dimension silently missing its tolerance looks finished.
			warnings.push(`a tolerance of display type ${form ?? '?'} was not printed`);
			return { lines: [nominal.text], boxed: false, warnings };
	}
}

/**
 * `+0.021 / −0.005`, or `+0.021 / 0` when one deviation is zero.
 *
 * ASME Y14.5 §2.3.2: a zero deviation on a unilateral tolerance is written as
 * a single `0` with no sign and no decimals, so the reader sees at a glance
 * which side the feature may not vary on.
 */
function deviationPair(deviations, min, toDisplay, warnings) {
	const vals = (deviations ?? [NaN, NaN]).map(toDisplay);
	if (!vals.every(Number.isFinite)) {
		warnings.push('a deviation tolerance carries no finite pair of deviations');
		return NO_VALUE;
	}
	// A zero contributes no precision requirement — it prints as a bare `0`.
	const nonZero = vals.filter((v) => v !== 0);
	const places = nonZero.length ? Math.max(...nonZero.map((v) => exactPlaces(v, min))) : min;
	const one = (v) => {
		const s = fixed(Math.abs(v), places);
		if (Number(s) === 0) return '0';
		return `${v < 0 ? MINUS : '+'}${s}`;
	};
	return `${one(vals[0])} / ${one(vals[1])}`;
}

/**
 * The ISO 1101 symbols, as GLYPHS. The mirror of
 * `waffle_types::annotation::tolerance::Characteristic::symbol`, which is the
 * one thing in M1 that is presentation rather than value: no number is
 * duplicated here.
 */
export const CHARACTERISTIC_SYMBOLS = {
	Flatness: '⏥',
	Straightness: '⏤',
	Circularity: '○',
	Cylindricity: '⌭',
	Perpendicularity: '⟂',
	Parallelism: '∥',
	Angularity: '∠',
	Position: '⌖',
	Concentricity: '◎',
	Symmetry: '⌯',
	Profile: '⌓',
	Runout: '↗'
};

/** The zone prefix a tolerance value carries (ISO 1101). */
export const ZONE_PREFIXES = { Diametral: '⌀', Width: '', Spherical: 'S⌀' };

/**
 * A material-condition glyph. `Rfs` is the DEFAULT condition and has no
 * symbol in any standard, so it prints nothing — a frame that drew one would
 * be drawing a glyph nobody defines.
 */
export const MODIFIER_SYMBOLS = { Mmc: 'Ⓜ', Lmc: 'Ⓛ', Rfs: '' };

/**
 * A `GeometricTolerance` as the compartments of a feature control frame
 * (ISO 1101 §6): the characteristic symbol, then the zone + value +
 * modifier, then one compartment per datum with its own modifier.
 *
 * The value is a LENGTH in metres whatever the characteristic — including
 * angularity, whose zone is two parallel planes a distance apart — so it
 * converts like any other length and prints at the band precision rule
 * (never coarser than the document's).
 *
 * @param {any} gt a `GeometricTolerance` record
 * @param {Partial<import('$lib/units.js').DisplaySettings>} [display]
 * @returns {{ cells: string[], warnings: string[] }}
 */
export function formatGeometricTolerance(gt, display) {
	const warnings = [];
	const settings = resolveDisplay(display);
	const characteristic = gt?.characteristic?.type ?? null;
	const symbol = CHARACTERISTIC_SYMBOLS[characteristic];
	if (!symbol) {
		// No compartments at all, rather than a frame with an empty first one.
		// A feature control frame says "this characteristic, within this zone";
		// without the characteristic there is no control to draw, and an
		// unlabelled box on a manufacturing drawing is worse than a visible
		// absence (the same rule the unknown-annotation arm follows). Named,
		// never substituted: a frame drawn with a different symbol is a
		// different control than the one authored.
		warnings.push(`a geometric tolerance characteristic ${characteristic ?? '?'} has no symbol`);
		return { cells: [], warnings };
	}

	const zone = gt?.zone?.type ?? 'Width';
	const prefix = ZONE_PREFIXES[zone];
	if (prefix === undefined) warnings.push(`a tolerance zone shape ${zone} is not known`);

	// The raw value, not `Number(…)`: `null` would coerce to a finite 0 and
	// print a zone of zero width, which is a tolerance nothing can satisfy.
	const raw = gt?.value?.magnitude;
	const magnitude = typeof raw === 'number' ? raw : NaN;
	const dimension = gt?.value?.dimension;
	let valueText = NO_VALUE;
	if (dimension !== undefined && dimension !== 'Length') {
		// Rust's `GeometricTolerance::validate` refuses a non-length zone, so a
		// record carrying one did not come through it.
		warnings.push(`a geometric tolerance zone is a length, not ${dimension}`);
	} else if (!Number.isFinite(magnitude)) {
		warnings.push('a geometric tolerance zone width is not finite');
	} else if (!isKnownUnit(settings.unit)) {
		warnings.push(`unknown display unit "${settings.unit}" — the zone width is withheld`);
	} else {
		const v = internalToDisplay(magnitude, settings.unit);
		valueText = fixed(v, exactPlaces(v, settings.precision));
	}

	const modifier = modifierGlyph(gt?.modifier?.type, warnings);
	const cells = [symbol, `${prefix ?? ''}${valueText}${modifier}`];
	for (const datum of gt?.datums ?? []) {
		const label = String(datum?.label ?? '').trim();
		if (!label) warnings.push('a datum reference in a feature control frame has no label');
		cells.push(`${label}${modifierGlyph(datum?.modifier?.type, warnings)}`);
	}
	return { cells, warnings };
}

function modifierGlyph(type, warnings) {
	if (type == null) return '';
	const glyph = MODIFIER_SYMBOLS[type];
	if (glyph === undefined) {
		warnings.push(`a material condition ${type} is not known`);
		return '';
	}
	return glyph;
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
 * `toFixed`, with negative zero normalized away and the sign set in a true
 * MINUS.
 *
 * `(-0).toFixed(2)` is `"-0.00"`, and a dimension that prints `-0.00` because
 * a coordinate happened to be a negative zero is a bug a reader cannot
 * diagnose. Rounding to the stated places first means the sign is dropped
 * only when the ROUNDED value is zero.
 *
 * `toFixed`'s own sign is an ASCII hyphen; every negative number on a sheet
 * carries U+2212 instead, so the one that appears in a deviation, in a
 * fractional inch and in a negative ordinate is the same character. The
 * hyphen keeps one job on a drawing — joining a whole inch to its fraction in
 * `1-1/2` — and a reader (or a test) can tell the two apart.
 */
export function fixed(x, places) {
	const s = x.toFixed(places);
	if (Number(s) === 0) return (0).toFixed(places);
	return s.startsWith('-') ? `${MINUS}${s.slice(1)}` : s;
}

/**
 * `p` as a whole number of decimals in `0 .. 6`, or [`DEFAULT_PRECISION`].
 *
 * `null` and `undefined` are ABSENCE, not a precision of zero — the same
 * `Number(null) === 0` trap as `units.js`'s `clampPlaces` and the band
 * guards above. Zero IS a legal precision, so an absence that coerces to it
 * is indistinguishable from an author who asked for whole millimetres.
 */
function clampPrecision(p) {
	if (p == null) return DEFAULT_PRECISION;
	const n = Number(p);
	if (!Number.isFinite(n)) return DEFAULT_PRECISION;
	return Math.min(6, Math.max(0, Math.trunc(n)));
}

function label(unit) {
	return UNITS[unit]?.label ?? unit;
}
