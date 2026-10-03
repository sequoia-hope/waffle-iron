/**
 * Entity-name tools (N1, specs/agent_mechanical_design.md §5.2). Definitions
 * only; all three run in the engine (`crates/wasm-bridge/src/tools/names.rs`),
 * routed by `../executor.js`.
 *
 * A name is an alias over a persistent reference, not a second identity
 * system: it stores the entity's persistent id, so it is exactly as durable as
 * that id and says so when it is not.
 */
import { commandOutputSchema } from './common.js';

const edit = (title, extra = {}) => ({ title, readOnlyHint: false, destructiveHint: false, ...extra });
const readOnly = (title) => ({ title, readOnlyHint: true });

const entityTarget = {
	type: 'object',
	description:
		'What to name: {"type":"entity","geom_ref":…} (a face/edge/vertex GeomRef from face_list), ' +
		'{"type":"body","body_id":…} (from model_summary.bodies), or {"type":"name","name":…} ' +
		'(something already named, to re-label it).',
	properties: {
		type: { type: 'string', enum: ['entity', 'body', 'name'] },
		geom_ref: { type: 'object' },
		body_id: { type: 'string' },
		name: { type: 'string' }
	},
	required: ['type']
};

export const entityNameTool = {
	name: 'entity_name',
	description:
		'Give a face, edge, vertex or body a name you can use afterwards wherever a GeomRef or body id ' +
		'is taken (one undo step). A name is one or two dot-separated identifiers: "top_face", or ' +
		'"plate.top_face" where "plate" is the display name of the body the entity is in. Naming a BODY ' +
		'sets its display name, which is what that first segment matches. Names are unique across ' +
		'entities and bodies alike: a taken one is NameTaken, a malformed one InvalidName. The name is ' +
		'stored against the entity\'s persistent id, so it follows that entity across rebuilds and ' +
		'reports it when the id is lost rather than quietly binding to something else.',
	inputSchema: {
		type: 'object',
		properties: {
			target: entityTarget,
			name: { type: 'string', minLength: 1, description: 'The name to assign.' }
		},
		required: ['target', 'name'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({
		name: { type: 'string' },
		kind: { type: 'object', description: 'TopoKind of the named entity; {"type":"Solid"} for a body.' },
		geom_ref: { type: 'object', description: 'The reference stored under the name (a Pid selector).' },
		body_id: { type: 'string' }
	}),
	annotations: edit('Name entity')
};

export const entityUnnameTool = {
	name: 'entity_unname',
	description:
		'Remove an entity name (one undo step). The geometry is untouched. An unknown name is ' +
		'NameNotFound. A body\'s display name is not an entity name: clear that with body_rename and an ' +
		'empty new_name.',
	inputSchema: {
		type: 'object',
		properties: { name: { type: 'string', minLength: 1 } },
		required: ['name'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({ name: { type: 'string' } }),
	annotations: edit('Unname entity')
};

export const namesListTool = {
	name: 'names_list',
	description:
		'Every name in the open Part, in name order: the entity names and the body display names they ' +
		'are scoped by. resolves says whether the name still points at geometry — a name whose entity ' +
		'was deleted stays listed and resolves false, so the hole is visible. resolved_by is "pid" when ' +
		'the persistent id answered, "query" when it was lost and the reference the name was authored ' +
		'with answered instead (the name may then describe different geometry — re-name it), and absent ' +
		'for a body name. body is the owning body\'s CURRENT display name, so a dotted name whose first ' +
		'segment has drifted is visible too. body_id limits the answer to one body.',
	inputSchema: {
		type: 'object',
		properties: { body_id: { type: 'string', description: 'Only this body\'s names.' } },
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			names: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						name: { type: 'string' },
						kind: { type: 'object' },
						geom_ref: { type: 'object' },
						body_id: { type: 'string' },
						body: { type: 'string' },
						resolves: { type: 'boolean' },
						resolved_by: { type: 'string', enum: ['pid', 'selector', 'query'] },
						warnings: { type: 'array', items: { type: 'string' } },
						created: { type: 'object' }
					},
					required: ['name', 'kind', 'resolves']
				}
			}
		},
		required: ['names']
	},
	annotations: readOnly('List names')
};
