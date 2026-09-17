/**
 * The model snapshot an agent call is measured against
 * (specs/waffle_mcp_server.md §2.5, §3.3).
 *
 * This module used to hold every agent command. They now run in the engine
 * (`crates/wasm-bridge/src/tools/`): the twelve authoring tools moved at S3 C4
 * and their bodies were deleted at C4b, and `sketch_create` — the last one,
 * which needed `buildFinishProfiles` ported to Rust first — moved at C5.
 *
 * What is left is the page's own view of the document, which stays a page
 * concern: the executor snapshots before a call so it can tell whether a
 * CANCELLED step changed anything (A18), and which features started failing
 * because of it (A2/A3) — both are diffs against what this page already
 * showed, not something the engine can answer.
 */
import { getBodies, getFeatureErrors, getFeatureTree } from '$lib/engine/store.svelte.js';
import { takeSnapshot } from './delta.js';

/** The document as this page currently shows it. */
export function snapshotNow() {
	return takeSnapshot({ featureTree: getFeatureTree(), featureErrors: getFeatureErrors(), bodies: getBodies() });
}
