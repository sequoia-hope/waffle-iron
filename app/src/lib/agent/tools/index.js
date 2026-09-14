/**
 * The page tool registry: every tool the agent link exposes is defined once here
 * (specs/waffle_mcp_server.md §2.4). `app/scripts/gen-agent-manifest.mjs` emits the
 * relay's bundled manifest from this list. Plain data — no store imports.
 */
import {
	bodyRenameTool,
	featureAddTool,
	featureDeleteTool,
	featureEditTool,
	featureRenameTool,
	featureReorderTool,
	featureSuppressTool,
	parametersSetTool,
	redoTool,
	rollbackSetTool,
	sketchCreateTool,
	undoTool
} from './authoring.js';
import {
	documentInfoTool,
	documentNewTool,
	documentOpenTool,
	documentSaveTool,
	storageListTool,
	tabAddTool,
	tabMoveTool,
	tabRenameTool,
	tabSwitchTool
} from './documents.js';
import {
	bodyMeasureTool,
	expressionEvaluateTool,
	faceListTool,
	featureGetTool,
	selectionGetTool,
	sketchRegionsTool
} from './inspection.js';
import { modelSummaryTool } from './model_summary.js';
import { viewportCaptureTool, viewportViewTool } from './viewport.js';

/** Tool definitions, in manifest order. */
export const TOOLS = [
	documentInfoTool,
	storageListTool,
	documentOpenTool,
	documentNewTool,
	documentSaveTool,
	tabSwitchTool,
	tabAddTool,
	tabMoveTool,
	tabRenameTool,
	modelSummaryTool,
	featureGetTool,
	selectionGetTool,
	bodyMeasureTool,
	faceListTool,
	sketchRegionsTool,
	expressionEvaluateTool,
	viewportViewTool,
	viewportCaptureTool,
	sketchCreateTool,
	featureAddTool,
	featureEditTool,
	featureDeleteTool,
	featureSuppressTool,
	featureReorderTool,
	featureRenameTool,
	bodyRenameTool,
	rollbackSetTool,
	parametersSetTool,
	undoTool,
	redoTool
];

export const TOOL_NAMES = new Set(TOOLS.map((t) => t.name));
