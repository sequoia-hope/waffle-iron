/**
 * Document and storage agent tools (specs/waffle_mcp_server.md §2.5 Documents
 * and storage, §3.4 S1–S4). Definitions only; implementations are in `../documents.js`.
 */

const documentInfoSchema = {
	type: 'object',
	properties: {
		document_id: { type: ['string', 'null'], description: 'The document\'s own identity (v4 document.id).' },
		storage_id: { type: ['string', 'null'], description: 'The storage record id (pass to document_open).' },
		name: { type: 'string' },
		storage_provider: {
			type: 'object',
			properties: { id: { type: 'string' }, label: { type: 'string' } },
			required: ['id', 'label']
		},
		tabs: {
			type: 'array',
			items: {
				type: 'object',
				properties: { id: { type: 'string' }, name: { type: 'string' }, kind: { type: 'string' } },
				required: ['id', 'name', 'kind']
			}
		},
		active_tab: { type: ['string', 'null'] },
		read_only: { type: 'boolean', description: 'Linked read-only copy: edits and saves are refused.' },
		sources: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					kind: { type: 'string' },
					available: { type: 'boolean' }
				}
			}
		},
		unsaved: { type: 'boolean', description: 'Changes from the last few seconds are still waiting for autosave.' }
	},
	required: ['document_id', 'storage_id', 'name', 'storage_provider', 'tabs', 'active_tab', 'read_only', 'sources', 'unsaved']
};

const discardUnsaved = {
	type: 'boolean',
	default: false,
	description:
		'Leave the current document even though its latest changes are not stored yet. The user is still asked ' +
		'to confirm in the page; declining returns UserDeclined.'
};

const providerId = {
	type: 'string',
	description: 'Storage provider id ("local" = this browser, or a connected git provider); default: the active one.'
};

export const documentInfoTool = {
	name: 'document_info',
	description:
		'The document open in the tab: identity, name, storage provider, tabs (id, name, kind), active tab, ' +
		'whether it is a linked read-only copy, its sources with availability, and whether changes are still ' +
		'waiting for autosave.',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: documentInfoSchema,
	annotations: { title: 'Document info', readOnlyHint: true }
};

export const storageListTool = {
	name: 'storage_list',
	description: 'The documents in a storage provider, newest first.',
	inputSchema: {
		type: 'object',
		properties: { provider: providerId },
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			provider: { type: 'string' },
			documents: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						id: { type: 'string' },
						name: { type: 'string' },
						created: { type: 'number', description: 'Unix ms.' },
						modified: { type: 'number', description: 'Unix ms.' },
						tab_count: { type: 'integer' },
						linked: { type: 'boolean' }
					},
					required: ['id', 'name']
				}
			}
		},
		required: ['provider', 'documents']
	},
	annotations: { title: 'List stored documents', readOnlyHint: true }
};

export const documentOpenTool = {
	name: 'document_open',
	description:
		'Open a stored document in the tab, as opening it from the Home screen does. Refused with UnsavedChanges ' +
		'while the current document has changes waiting for autosave (call document_save first).',
	inputSchema: {
		type: 'object',
		properties: {
			id: { type: 'string', description: 'Storage record id from storage_list.' },
			provider: providerId,
			discard_unsaved: discardUnsaved
		},
		required: ['id'],
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'Open document', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const documentNewTool = {
	name: 'document_new',
	description:
		'Create an empty document (one Part tab) in the active storage provider and open it. Refused with ' +
		'UnsavedChanges while the current document has changes waiting for autosave.',
	inputSchema: {
		type: 'object',
		properties: {
			name: { type: 'string', minLength: 1, default: 'Untitled' },
			discard_unsaved: discardUnsaved
		},
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'New document', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const documentImportTool = {
	name: 'document_import',
	description:
		'Load a .waffle file\'s text as a document, as opening a file from the Home screen\'s picker does: the ' +
		'document keeps its own identity (a record with that id in the active storage provider is replaced), is ' +
		'stored there, named after the file unless name is given, and opened. Refused with UnsavedChanges while ' +
		'the current document has changes waiting for autosave (call document_save first).',
	inputSchema: {
		type: 'object',
		properties: {
			file_name: { type: 'string', minLength: 1, description: 'The file\'s name, e.g. "Pinwheel.waffle"; the document is named after it without the extension.' },
			text: { type: 'string', minLength: 1, description: 'The .waffle file contents (JSON).' },
			name: { type: 'string', minLength: 1, description: 'Document name to use instead of the file name.' },
			discard_unsaved: discardUnsaved
		},
		required: ['file_name', 'text'],
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'Import document file', readOnlyHint: false, destructiveHint: true, openWorldHint: false }
};

export const documentSaveTool = {
	name: 'document_save',
	description:
		'Save the open document to its storage provider now, exactly as the Save button does. A provider ' +
		'failure (git authentication, a conflict) is returned verbatim as SaveFailed.',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: {
		type: 'object',
		properties: {
			provider: { type: 'string' },
			id: { type: 'string' },
			saved_at: { type: 'string', description: 'RFC 3339.' }
		},
		required: ['provider', 'id', 'saved_at']
	},
	annotations: { title: 'Save document', readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true }
};

export const tabSwitchTool = {
	name: 'tab_switch',
	description:
		'Make another tab of the document active. On a Part tab the feature tools work (sketch_create, feature_add, …); ' +
		'on an Assembly tab the assembly tools do (assembly_get, instance_add, connector_add, mate_add, …). Switching ' +
		'to an Assembly tab evaluates it (parts built, mates solved).',
	inputSchema: {
		type: 'object',
		properties: { tab_id: { type: 'string', description: 'Tab id from document_info.tabs.' } },
		required: ['tab_id'],
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'Switch tab', readOnlyHint: false, destructiveHint: false, idempotentHint: true }
};

const tabId = { type: 'string', description: 'Tab id from document_info.tabs.' };

export const tabAddTool = {
	name: 'tab_add',
	description:
		'Add a tab to the open document, as the tab bar\'s + buttons do: an empty Part (default) or Assembly, named ' +
		'"Part N" / "Assembly N" unless name is given. activate (default true) makes it the active tab: the feature ' +
		'tools work on a Part tab, the assembly tools on an Assembly tab. Returns the new tab_id with the document info.',
	inputSchema: {
		type: 'object',
		properties: {
			kind: { type: 'string', enum: ['Part', 'Assembly'], default: 'Part' },
			name: { type: 'string', minLength: 1 },
			activate: { type: 'boolean', default: true }
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: { tab_id: { type: 'string' }, ...documentInfoSchema.properties },
		required: ['tab_id', ...documentInfoSchema.required]
	},
	annotations: { title: 'Add tab', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const tabMoveTool = {
	name: 'tab_move',
	description:
		'Move a tab to a new position in the tab bar (0 = first), as dragging it does. An index past the end moves it ' +
		'last. The order is saved with the document.',
	inputSchema: {
		type: 'object',
		properties: { tab_id: tabId, index: { type: 'integer', minimum: 0 } },
		required: ['tab_id', 'index'],
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'Move tab', readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false }
};

export const tabRenameTool = {
	name: 'tab_rename',
	description: 'Rename a tab of the open document, as double-clicking its name does.',
	inputSchema: {
		type: 'object',
		properties: { tab_id: tabId, name: { type: 'string', minLength: 1 } },
		required: ['tab_id', 'name'],
		additionalProperties: false
	},
	outputSchema: documentInfoSchema,
	annotations: { title: 'Rename tab', readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false }
};
