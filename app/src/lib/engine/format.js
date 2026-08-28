/**
 * .waffle format constants — the single JS-side source for envelope version
 * numbers. Must mirror `FORMAT_VERSION` / `MIN_READER_VERSION` in
 * `crates/file-format/src/save.rs` (the Rust writer). See
 * `docs/FILE_FORMAT.md` §3–4, §13.
 */

/** Format version this app writes. */
export const FORMAT_VERSION = 3;

/**
 * Oldest reader (by its FORMAT_VERSION) that can parse files this app writes.
 * Bump together with the Rust constant whenever a change lands that older
 * readers cannot parse (new enum variants included).
 */
export const MIN_READER_VERSION = 3;

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
