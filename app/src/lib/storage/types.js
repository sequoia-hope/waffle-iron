/**
 * @typedef {Object} DocumentSummary
 * @property {string} id - Document storage ID (UUID in v4; 8-char base62 for legacy records)
 * @property {string} name - Human-readable document name
 * @property {number} created - Unix timestamp ms
 * @property {number} modified - Unix timestamp ms
 * @property {string|null} displayUnit - Display unit preference
 * @property {number} tabCount - Number of tabs
 * @property {string} provider - Provider ID ("local", "github", "git:...")
 * @property {object|null} [previewMesh] - Preview mesh for thumbnail
 * @property {import('./open-link.js').DocumentLink|null} [link] - Share-link provenance; non-null ⇒ read-only linked copy
 */

/**
 * @typedef {Object} StoredDocument
 * @property {string} id - Document storage ID
 * @property {string} json - Full serialized WaffleFile JSON
 * @property {number} created - Unix timestamp ms
 * @property {number} modified - Unix timestamp ms
 * @property {import('./open-link.js').DocumentLink|null} [link] - Share-link provenance (linked, read-only records only)
 * @property {boolean} [localFallback] - True when written as an offline local fallback for a remote provider
 */

/**
 * @typedef {Object} DocumentRecord
 * @property {StoredDocument} doc - Stored document record
 * @property {string} rev - Opaque revision identifier of the stored content
 */

/**
 * @typedef {Object} DocumentPutOptions
 * @property {string|null} base - Expected stored revision (`null` when creating a new record)
 */

/**
 * @typedef {{ rev: string } | { conflict: { theirs_rev: string | null } }} DocumentPutResult
 */

/**
 * @typedef {Object} DocumentChangeEvent
 * @property {'changed'|'deleted'} type
 * @property {string} id - Document storage ID
 * @property {string|null} rev - New stored revision (`null` when deleted)
 * @property {string} provider - Provider ID
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
 * @property {string} id - Provider identifier ("local", "github", "git:...")
 * @property {string} label - Display name ("This Browser", "GitHub", ...)
 * @property {boolean} canShare - Whether getShareUrl() is meaningful
 * @property {() => Promise<DocumentSummary[]>} list - List all documents, sorted by modified desc
 * @property {(id: string) => Promise<DocumentRecord|null>} get - Get a document and its revision by ID
 * @property {(doc: StoredDocument, options: DocumentPutOptions) => Promise<DocumentPutResult>} put - Create or update a document at `options.base`, refusing on revision mismatch
 * @property {(id: string) => Promise<void>} delete - Delete a document by ID
 * @property {(id: string) => Promise<string|null>} getShareUrl - Get a shareable URL for a document
 * @property {((fn: (event: DocumentChangeEvent) => void) => () => void)} [watch] - Subscribe to document changes (local or cross-tab); returns an unsubscribe function
 */
