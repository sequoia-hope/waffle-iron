/**
 * The SVG dimension renderer (`specs/drawings_and_mbd.md` §7, increment D3).
 *
 * Asserts on the SVG **DOM** — element counts, text content, coordinate
 * finiteness, theme tokens — not on pixels. A screenshot baseline for a
 * drawing rots on every font or antialiasing change and tells you nothing
 * about whether the number is right; the DOM says exactly that.
 *
 * ## Where the number comes from, and what each suite owns
 *
 * The measurement itself is Rust's and is pinned there:
 * `waffle_types::annotation::measure` for the rules, and
 * `crates/test-harness/tests/d3_annotation_measure.rs` for a real box
 * projected top-down measuring its authored 40 mm. This spec takes the
 * already-measured `ViewLayout` the engine will emit (D4a wires it) and pins
 * the half Rust cannot reach: that the renderer PRINTS that value at the
 * stated precision, and that the geometry it draws agrees with the text.
 *
 * That last check is what keeps the two halves honest: the dimension line's
 * drawn length in paper mm must equal the printed number times the view
 * scale. A renderer that took its text from one place and its arrows from
 * another would pass a text-only assertion and fail this one.
 */
import { test, expect } from './helpers/waffle-test.js';

/** A 40 × 25 mm plate's top view, as `ViewLayout` JSON (meters, v up). */
const PLATE_W = 0.04;
const PLATE_D = 0.025;

function line(start, end) {
	return { type: 'Line', start, end };
}

function edge(curve, visibility = 'Visible') {
	return { geometry: curve, visibility, kind: 'Edge' };
}

/** The plate outline: four edges, plus one hidden edge to exercise the dash. */
function plateLayout(annotations) {
	return {
		curves: [
			edge(line([0, 0], [PLATE_W, 0])),
			edge(line([PLATE_W, 0], [PLATE_W, PLATE_D])),
			edge(line([PLATE_W, PLATE_D], [0, PLATE_D])),
			edge(line([0, PLATE_D], [0, 0])),
			edge(line([0.01, 0], [0.01, PLATE_D]), 'Hidden')
		],
		bbox: [
			[0, 0],
			[PLATE_W, PLATE_D]
		],
		annotations
	};
}

/** The two vertical walls as anchors, for a horizontal width dimension. */
const WIDTH_DIMENSION = {
	type: 'Dimension',
	kind: { type: 'HDistance' },
	anchors: [
		{ type: 'Curve', curve: line([0, 0], [0, PLATE_D]) },
		{ type: 'Curve', curve: line([PLATE_W, 0], [PLATE_W, PLATE_D]) }
	],
	value: PLATE_W,
	precision: 2,
	placement: { dx: 0, dy: 0 }
};

const HOLE_R = 0.005;
const HOLE_CIRCLE = {
	type: 'Circle',
	center: [0.02, 0.0125],
	radius: HOLE_R,
	start_angle: 0,
	end_angle: Math.PI * 2
};

/** Call the renderer in the page and get back its result. */
async function render(page, input) {
	return page.evaluate((i) => window.__waffle.renderDrawingSvg(i), input);
}

/**
 * Render and parse, returning the root `<svg>` element handle plus the raw
 * string. The SVG is parsed with DOMParser in the page rather than injected
 * into the live document, so the spec cannot accidentally depend on the
 * app's own layout.
 */
async function renderAndQuery(page, input, query) {
	return page.evaluate(
		({ i, q }) => {
			const out = window.__waffle.renderDrawingSvg(i);
			const doc = new DOMParser().parseFromString(out.svg, 'image/svg+xml');
			const err = doc.querySelector('parsererror');
			if (err) throw new Error(`the SVG does not parse: ${err.textContent}`);
			const root = doc.documentElement;
			const sel = (s) => Array.from(root.querySelectorAll(s));
			return {
				warnings: out.warnings,
				widthMm: out.widthMm,
				heightMm: out.heightMm,
				svg: out.svg,
				viewBox: root.getAttribute('viewBox'),
				width: root.getAttribute('width'),
				counts: Object.fromEntries(
					Object.entries(q.counts ?? {}).map(([k, s]) => [k, sel(s).length])
				),
				texts: sel(q.texts ?? 'text').map((t) => t.textContent),
				attrs: Object.fromEntries(
					Object.entries(q.attrs ?? {}).map(([k, [s, a]]) => [
						k,
						sel(s).map((e) => e.getAttribute(a))
					])
				),
				// Every attribute in the whole document as `name=value`, for
				// the NaN tripwire and the colour-literal check below.
				allNumbers: sel('*').flatMap((e) =>
					Array.from(e.attributes).map((a) => `${a.name}=${a.value}`)
				)
			};
		},
		{ i: input, q: query ?? {} }
	);
}

test.describe('D3 SVG dimension renderer', () => {
	test('a linear dimension draws two extension lines, a dimension line and two arrowheads', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(page, { layout: plateLayout([WIDTH_DIMENSION]) }, {
			counts: {
				svg: 'g.wi-annotations',
				extension: 'line.wi-dim-extension',
				dimension: 'line.wi-dim-dimension',
				arrow: 'polygon.wi-dim-arrow',
				value: 'text.wi-dim-value',
				curves: 'g.wi-curves path.wi-curve'
			},
			texts: 'text.wi-dim-value'
		});

		expect(r.warnings).toEqual([]);
		// §7's render list for a linear dimension, item by item.
		expect(r.counts.extension).toBe(2);
		expect(r.counts.dimension).toBe(1);
		expect(r.counts.arrow).toBe(2);
		expect(r.counts.value).toBe(1);
		// The five projected curves of the fixture, all drawn.
		expect(r.counts.curves).toBe(5);
	});

	test('the value text is the measured number at the stated precision', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// 0.04 m = 40 mm. Two decimals as the annotation asks.
		const two = await renderAndQuery(page, { layout: plateLayout([WIDTH_DIMENSION]) }, {
			texts: 'text.wi-dim-value'
		});
		expect(two.texts).toEqual(['40.00']);

		// The annotation's own precision wins over the document's.
		const three = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3 }]),
				documentPrecision: 1
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(three.texts).toEqual(['40.000']);

		// With no annotation precision, the document's applies.
		const doc = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: null }]),
				documentPrecision: 0
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(doc.texts).toEqual(['40']);

		// And the display unit converts: 40 mm is 1.575 in.
		const inches = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3 }]), unit: 'in' },
			{ texts: 'text.wi-dim-value' }
		);
		expect(inches.texts).toEqual(['1.575']);
	});

	test('a dual unit is bracketed after the primary value', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 2, dual_unit: 'in' }]),
				unit: 'mm'
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['40.00 [1.57 in]']);
	});

	test('the dimension line that is drawn is as long as the number that is printed', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// The cross-check between the two halves of the renderer. At 1:1 the
		// drawn dimension line must span 40 paper mm, because the printed
		// value says 40.00 mm — a renderer whose text and geometry came from
		// different places would pass the text assertion and fail this.
		for (const scale of [1, 0.5, 2]) {
			const r = await renderAndQuery(
				page,
				{ layout: plateLayout([WIDTH_DIMENSION]), scale },
				{
					texts: 'text.wi-dim-value',
					attrs: {
						x1: ['line.wi-dim-dimension', 'x1'],
						x2: ['line.wi-dim-dimension', 'x2'],
						y1: ['line.wi-dim-dimension', 'y1'],
						y2: ['line.wi-dim-dimension', 'y2']
					}
				}
			);
			const printedMm = Number(r.texts[0]);
			const drawn = Math.hypot(
				Number(r.attrs.x2[0]) - Number(r.attrs.x1[0]),
				Number(r.attrs.y2[0]) - Number(r.attrs.y1[0])
			);
			expect(printedMm).toBeCloseTo(40, 6);
			expect(drawn).toBeCloseTo(printedMm * scale, 3);
		}
	});

	test('a radial dimension draws a leader with one arrowhead and an R prefix', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [edge(HOLE_CIRCLE)],
					bbox: [
						[0.015, 0.0075],
						[0.025, 0.0175]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Radius' },
							anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
							value: HOLE_R,
							precision: 2,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{
				counts: {
					arrow: 'polygon.wi-dim-arrow',
					leader: 'line.wi-dim-leader',
					dimension: 'line.wi-dim-dimension'
				},
				texts: 'text.wi-dim-value'
			}
		);
		expect(r.warnings).toEqual([]);
		expect(r.counts.arrow).toBe(1);
		expect(r.counts.leader).toBe(1);
		expect(r.counts.dimension).toBe(1);
		expect(r.texts).toEqual(['R5.00']);
	});

	test('a diameter dimension spans the circle and prints the diameter symbol', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [edge(HOLE_CIRCLE)],
					bbox: [
						[0.015, 0.0075],
						[0.025, 0.0175]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Diameter' },
							anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
							value: 2 * HOLE_R,
							precision: 1,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{ counts: { arrow: 'polygon.wi-dim-arrow' }, texts: 'text.wi-dim-value' }
		);
		// Two heads: a diameter is dimensioned rim to rim.
		expect(r.counts.arrow).toBe(2);
		expect(r.texts).toEqual(['⌀10.0']);
	});

	test('an angular dimension draws an arc and prints degrees', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [],
					bbox: [
						[0, 0],
						[0.02, 0.02]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Angle' },
							anchors: [
								{ type: 'Curve', curve: line([0, 0], [0.02, 0]) },
								{ type: 'Curve', curve: line([0, 0], [0, 0.02]) }
							],
							// 90° in radians, as `measure` returns it.
							value: Math.PI / 2,
							precision: 1,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{
				counts: { arc: 'path.wi-dim-dimension', arrow: 'polygon.wi-dim-arrow' },
				texts: 'text.wi-dim-value'
			}
		);
		expect(r.warnings).toEqual([]);
		expect(r.counts.arc).toBe(1);
		expect(r.counts.arrow).toBe(2);
		expect(r.texts).toEqual(['90.0°']);
	});

	test('a note with a leader draws a two-segment leader and its text', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Note',
						text: 'DEBURR ALL EDGES',
						leader: { type: 'Point', at: [0.02, 0.0125] },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: { leader: 'line.wi-dim-leader', dot: 'circle.wi-dim-leader' },
				texts: 'text.wi-dim-note'
			}
		);
		expect(r.counts.leader).toBe(2);
		// A leader onto a face terminates in a dot, not an arrow (ISO 128-22).
		expect(r.counts.dot).toBe(1);
		expect(r.texts).toEqual(['DEBURR ALL EDGES']);
	});

	test('a centre mark and a centre line draw dash-dotted strokes', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{ type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R },
					{ type: 'CentreLine', from: [0, 0.0125], to: [PLATE_W, 0.0125] }
				])
			},
			{
				counts: { centre: 'line.wi-dim-centre' },
				attrs: { dash: ['line.wi-dim-centre', 'stroke-dasharray'] }
			}
		);
		// Two arms of the cross plus the centre line.
		expect(r.counts.centre).toBe(3);
		for (const d of r.attrs.dash) expect(d).toBe('12 2 2 2');
	});

	test('a datum draws a filled triangle, a box and its letter', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Datum',
						label: 'A',
						anchor: { type: 'Curve', curve: line([0, 0], [PLATE_W, 0]) },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: { tri: 'polygon.wi-dim-arrow', box: 'rect.wi-dim-datum' },
				texts: 'text.wi-dim-datum'
			}
		);
		expect(r.counts.tri).toBe(1);
		expect(r.counts.box).toBe(1);
		expect(r.texts).toEqual(['A']);
	});

	test('a hidden curve is dashed and a visible one is not', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([]) },
			{
				counts: {
					visible: 'path.wi-curve-visible',
					hidden: 'path.wi-curve-hidden'
				},
				attrs: {
					hiddenDash: ['path.wi-curve-hidden', 'stroke-dasharray'],
					visibleDash: ['path.wi-curve-visible', 'stroke-dasharray'],
					hiddenWidth: ['path.wi-curve-hidden', 'stroke-width'],
					visibleWidth: ['path.wi-curve-visible', 'stroke-width']
				}
			}
		);
		expect(r.counts.visible).toBe(4);
		expect(r.counts.hidden).toBe(1);
		expect(r.attrs.hiddenDash).toEqual(['4 2']);
		// A visible outline carries no dash attribute at all.
		expect(r.attrs.visibleDash).toEqual([null, null, null, null]);
		// ISO 128-20 width group 0.5: outlines wide, hidden detail narrower.
		expect(r.attrs.hiddenWidth).toEqual(['0.35']);
		expect(r.attrs.visibleWidth).toEqual(['0.5', '0.5', '0.5', '0.5']);
	});

	test('no coordinate anywhere in the output is NaN or infinite', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// Every annotation kind at once, so one pass covers every code path
		// that can emit a coordinate.
		const r = await renderAndQuery(page, {
			layout: plateLayout([
				WIDTH_DIMENSION,
				{ ...WIDTH_DIMENSION, kind: { type: 'Distance' } },
				{ ...WIDTH_DIMENSION, kind: { type: 'VDistance' } },
				{
					type: 'Dimension',
					kind: { type: 'Ordinate', axis: { type: 'U' } },
					anchors: [{ type: 'Point', at: [0.02, 0.0125] }],
					value: 0.02,
					precision: 2,
					placement: { dx: 0, dy: 0 }
				},
				{
					type: 'Dimension',
					kind: { type: 'Radius' },
					anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
					value: HOLE_R,
					precision: 2,
					placement: { dx: 0, dy: 0 }
				},
				{ type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R },
				{ type: 'CentreLine', from: [0, 0.0125], to: [PLATE_W, 0.0125] },
				{
					type: 'Note',
					text: 'N',
					leader: { type: 'Point', at: [0.02, 0.0125] },
					placement: { dx: 0, dy: 0 }
				},
				{
					type: 'Datum',
					label: 'B',
					anchor: { type: 'Point', at: [0, 0] },
					placement: { dx: 0, dy: 0 }
				}
			])
		});
		expect(r.warnings).toEqual([]);
		const bad = r.allNumbers.filter((s) => /\b(NaN|Infinity|-Infinity|undefined)\b/.test(s));
		expect(bad).toEqual([]);
		expect(Number.isFinite(r.widthMm)).toBe(true);
		expect(Number.isFinite(r.heightMm)).toBe(true);
		expect(r.viewBox.split(' ').every((v) => Number.isFinite(Number(v)))).toBe(true);
	});

	test('a non-finite measured value prints a dash rather than NaN', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// `measure` refuses a non-finite value in Rust, so a record carrying
		// one was built by something that did not. The renderer must say so
		// visibly, not print "NaN" where a machinist reads a size.
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, value: null }]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['—']);
		const bad = r.allNumbers.filter((s) => /\bNaN\b/.test(s));
		expect(bad).toEqual([]);
	});

	test('every paint is a CSS variable, so the sheet follows the theme', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(page, {
			layout: plateLayout([WIDTH_DIMENSION, { type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R }])
		});
		// No hex or rgb() literal may appear in a stroke or fill.
		const literals = r.allNumbers.filter(
			(s) => /^(stroke|fill)=/.test(s) && !/var\(--drawing-/.test(s) && !/=none$/.test(s)
		);
		expect(literals).toEqual([]);
		expect(r.svg).toContain('var(--drawing-ink)');
		expect(r.svg).toContain('var(--drawing-paper)');
		expect(r.svg).toContain('var(--drawing-annotation)');
	});

	test('the renderer is deterministic — the same input gives the same bytes', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// What makes a V3-style byte oracle possible, and what `export_svg`
		// (D4a) will rely on to write the same markup the sheet shows.
		const input = {
			layout: plateLayout([
				WIDTH_DIMENSION,
				{
					type: 'Dimension',
					kind: { type: 'Radius' },
					anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
					value: HOLE_R,
					precision: 2,
					placement: { dx: 0.001, dy: -0.002 }
				}
			])
		};
		const a = await render(page, input);
		const b = await render(page, input);
		expect(b.svg).toBe(a.svg);
		expect(a.svg.length).toBeGreaterThan(500);
		// And a changed input changes the bytes — otherwise the check above
		// would pass on a renderer that ignored its argument.
		const c = await render(page, { ...input, scale: 0.5 });
		expect(c.svg).not.toBe(a.svg);
	});

	test('the architectural-tick option replaces arrowheads with ticks', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([WIDTH_DIMENSION]), style: { architecturalTicks: true } },
			{ counts: { arrow: 'polygon.wi-dim-arrow', tick: 'line.wi-dim-tick' } }
		);
		expect(r.counts.arrow).toBe(0);
		expect(r.counts.tick).toBe(2);
	});

	test('an unknown annotation kind draws nothing and says so', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// Forward compatibility: a record from a newer build. A placeholder
		// glyph on a manufacturing drawing is worse than a visible absence,
		// so nothing is drawn — but the omission is reported, never silent.
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{ type: 'FeatureControlFrame', anchor: { type: 'Point', at: [0, 0] } }
				])
			},
			{ counts: { ann: 'g.wi-annotations > *' } }
		);
		expect(r.counts.ann).toBe(0);
		expect(r.warnings.length).toBe(1);
		expect(r.warnings[0]).toContain('FeatureControlFrame');
	});

	test('an empty layout still renders a valid, finite sheet', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: { curves: [], annotations: [] } },
			{ counts: { paper: 'rect.wi-paper', curves: 'path.wi-curve' } }
		);
		expect(r.counts.paper).toBe(1);
		expect(r.counts.curves).toBe(0);
		expect(r.widthMm).toBeGreaterThan(0);
		expect(r.heightMm).toBeGreaterThan(0);
	});
});
