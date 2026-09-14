/**
 * The page tool registry: every tool the agent link exposes is defined once here
 * (specs/waffle_mcp_server.md §2.4). `app/scripts/gen-agent-manifest.mjs` emits the
 * relay's bundled manifest from this list. Plain data — no store imports.
 */
import { modelSummaryTool } from './model_summary.js';

/** Tool definitions, in manifest order. */
export const TOOLS = [modelSummaryTool];

export const TOOL_NAMES = new Set(TOOLS.map((t) => t.name));
