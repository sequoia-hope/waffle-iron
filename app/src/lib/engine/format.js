/**
 * .waffle format constants — the JS-side mirror of `FORMAT_VERSION` /
 * `MIN_READER_VERSION` in `crates/file-format/src/save.rs`. Used only by the
 * READ paths that run without the engine (home page, file picker refusal) and
 * by the home page's empty-document template; every real save goes through
 * the Rust writer (`SaveDocument`). See `docs/FILE_FORMAT.md` §3–4, §13 and
 * `specs/waffle_v4_document_model.md`.
 */

/** Format version this app writes. */
export const FORMAT_VERSION = 12;

/**
 * Oldest reader (by its FORMAT_VERSION) that can parse files this app writes.
 * Bump together with the Rust constant whenever a change lands that older
 * readers cannot parse (new enum variants included).
 */
export const MIN_READER_VERSION = 12;

/**
 * True if a parsed document declares it needs a newer reader than this build.
 * Files without the field (all pre-2026-08-28 files) impose no requirement.
 * @param {any} parsed - JSON.parse'd .waffle document
 * @returns {boolean}
 */
export function fileTooNew(parsed) {
	const required = Math.max(parsed?.version ?? 0, parsed?.min_reader_version ?? 0);
	return required > FORMAT_VERSION;
}

/**
 * Edit a stored `.waffle`'s DOCUMENT METADATA without round-tripping the rest
 * of the file through JavaScript.
 *
 * ## Why this exists rather than `JSON.parse` → mutate → `JSON.stringify`
 *
 * A `.waffle` carries persistent entity ids (`Selector::Pid`, and anything
 * else `waffle_types::pid_str` governs). Since format v10 those are written as
 * decimal strings, which a JSON round trip preserves exactly — but a stored
 * file may be ANY earlier version, where they are JSON numbers, and a JSON
 * number in JavaScript is an `f64`: every id above `2^53` comes back as a
 * *different entity*. `JSON.parse(text)` then `JSON.stringify` is therefore a
 * lossy operation on a document, and two call sites were doing it (the home
 * page's rename and the examples panel's open). The damage is silent and
 * permanent: the record is written back with ids that name nothing, so every
 * drawing dimension and every entity name in it stops resolving.
 *
 * The fix is to parse only what is being changed. The `document` object holds
 * the name, id and timestamps — strings and a UUID, nothing a round trip can
 * hurt — so it is parsed, mutated, re-serialized and spliced back into the
 * ORIGINAL text. Everything outside it, `tabs` included, stays byte-identical.
 *
 * Falls back to the legacy `project` member for a pre-v3 file.
 *
 * @param {string} text - the stored `.waffle` text
 * @param {(meta: Record<string, any>) => void} mutate - applied to the parsed
 *   metadata object (which must stay free of large integers)
 * @returns {string} the edited text
 * @throws {Error} if the file has neither a `document` nor a `project` object —
 *   loudly, because the alternative is the lossy path this replaces
 */
export function editDocumentMeta(text, mutate) {
	const span = objectMemberSpan(text, 'document') ?? objectMemberSpan(text, 'project');
	if (!span) {
		throw new Error('not a .waffle document: no `document` or `project` object');
	}
	const meta = JSON.parse(text.slice(span.start, span.end));
	mutate(meta);
	return text.slice(0, span.start) + JSON.stringify(meta) + text.slice(span.end);
}

/**
 * The `[start, end)` character span of the VALUE of top-level member `key`,
 * when that value is an object. `null` if there is no such member.
 *
 * A small JSON scanner rather than a regex: a `"document"` substring can
 * appear inside a string value (a body named "document", a source path), and a
 * regex that matched one would splice over the wrong bytes. This tracks string
 * literals and escapes, and only considers members at depth 1.
 *
 * @param {string} text
 * @param {string} key
 * @returns {{start: number, end: number} | null}
 */
function objectMemberSpan(text, key) {
	let i = text.indexOf('{');
	if (i < 0) return null;
	i += 1;
	let depth = 0;
	while (i < text.length) {
		const c = text[i];
		if (c === '"') {
			const name = scanString(text, i);
			if (!name) return null;
			if (depth === 0) {
				// A member name at the envelope's own depth — is it ours?
				let j = name.end;
				while (j < text.length && /\s/.test(text[j])) j += 1;
				if (text[j] === ':') {
					j += 1;
					while (j < text.length && /\s/.test(text[j])) j += 1;
					if (name.value === key) {
						return text[j] === '{' ? { start: j, end: skipValue(text, j) } : null;
					}
					// Not ours: step over its value so a nested `"document"`
					// key cannot be mistaken for the envelope's.
					i = skipValue(text, j);
					continue;
				}
			}
			i = name.end;
			continue;
		}
		if (c === '{' || c === '[') depth += 1;
		else if (c === '}' || c === ']') {
			if (depth === 0) return null; // end of the envelope
			depth -= 1;
		}
		i += 1;
	}
	return null;
}

/**
 * Scan the JSON string literal starting at `text[at] === '"'`.
 * @param {string} text
 * @param {number} at
 * @returns {{value: string, end: number} | null} `end` is one past the closing quote
 */
function scanString(text, at) {
	let i = at + 1;
	while (i < text.length) {
		if (text[i] === '\\') {
			i += 2;
			continue;
		}
		if (text[i] === '"') {
			return { value: JSON.parse(text.slice(at, i + 1)), end: i + 1 };
		}
		i += 1;
	}
	return null;
}

/**
 * One past the end of the JSON value starting at `text[at]`.
 * @param {string} text
 * @param {number} at
 * @returns {number}
 */
function skipValue(text, at) {
	const c = text[at];
	if (c === '"') return scanString(text, at)?.end ?? text.length;
	if (c !== '{' && c !== '[') {
		// A number, `true`, `false` or `null`: ends at the next structural
		// character.
		let i = at;
		while (i < text.length && !',}]'.includes(text[i])) i += 1;
		return i;
	}
	let i = at + 1;
	let depth = 1;
	while (i < text.length && depth > 0) {
		const ch = text[i];
		if (ch === '"') {
			i = scanString(text, i)?.end ?? text.length;
			continue;
		}
		if (ch === '{' || ch === '[') depth += 1;
		else if (ch === '}' || ch === ']') depth -= 1;
		i += 1;
	}
	return i;
}
