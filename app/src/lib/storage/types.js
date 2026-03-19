/**
 * @typedef {Object} DocumentSummary
 * @property {string} id - Short base62 document ID (8 chars)
 * @property {string} name - Human-readable document name
 * @property {number} created - Unix timestamp ms
 * @property {number} modified - Unix timestamp ms
 * @property {string|null} displayUnit - Display unit preference
 * @property {number} tabCount - Number of tabs
 * @property {string} provider - Provider ID ("local", "github")
 * @property {object|null} [previewMesh] - Preview mesh for thumbnail
 */

/**
 * @typedef {Object} StoredDocument
 * @property {string} id - Short base62 document ID
 * @property {string} json - Full serialized WaffleFile v3 JSON
 * @property {number} created - Unix timestamp ms
 * @property {number} modified - Unix timestamp ms
 */

/**
 * Generate a short document ID (8-char base62).
 * Uses crypto.getRandomValues for uniqueness.
 * @returns {string}
 */
export function generateDocId() {
	const chars = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
	const bytes = new Uint8Array(8);
	crypto.getRandomValues(bytes);
	let result = '';
	for (let i = 0; i < 8; i++) {
		result += chars[bytes[i] % 62];
	}
	return result;
}

/**
 * @typedef {Object} DocumentStore
 * @property {string} id - Provider identifier ("local", "github")
 * @property {string} label - Display name ("This Browser", "GitHub")
 * @property {boolean} canShare - Whether getShareUrl() is meaningful
 * @property {() => Promise<DocumentSummary[]>} list - List all documents, sorted by modified desc
 * @property {(id: string) => Promise<StoredDocument|null>} get - Get a document by ID
 * @property {(doc: StoredDocument) => Promise<void>} put - Create or update a document
 * @property {(id: string) => Promise<void>} delete - Delete a document by ID
 * @property {(id: string) => Promise<string|null>} getShareUrl - Get a shareable URL for a document
 */
