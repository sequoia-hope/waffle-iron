/**
 * Client API for test case CRUD operations.
 * Communicates with the Vite dev server plugin at /api/test-cases.
 */

const BASE = '/api/test-cases';

/**
 * Fetch all test cases (manifest).
 * @returns {Promise<{ cases: Array<object> }>}
 */
export async function fetchTestCases() {
	const res = await fetch(BASE);
	if (!res.ok) throw new Error(`Failed to fetch test cases: ${res.status}`);
	return res.json();
}

/**
 * Fetch a single test case's .waffle data.
 * @param {string} id
 * @returns {Promise<string>} Raw JSON string
 */
export async function fetchTestCase(id) {
	const res = await fetch(`${BASE}/${id}`);
	if (!res.ok) throw new Error(`Failed to fetch test case ${id}: ${res.status}`);
	return res.text();
}

/**
 * Create a new test case.
 * @param {{ name: string, description?: string, expectedOutcome?: string, tags?: string[], waffleData: string }} data
 * @returns {Promise<object>} Created manifest entry
 */
export async function createTestCase(data) {
	const res = await fetch(BASE, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(data)
	});
	if (!res.ok) {
		const err = await res.json().catch(() => ({}));
		throw new Error(err.error || `Failed to create test case: ${res.status}`);
	}
	return res.json();
}

/**
 * Delete a test case.
 * @param {string} id
 * @returns {Promise<void>}
 */
export async function deleteTestCase(id) {
	const res = await fetch(`${BASE}/${id}`, { method: 'DELETE' });
	if (!res.ok) throw new Error(`Failed to delete test case ${id}: ${res.status}`);
}

/**
 * Update a test case's metadata (name, description, expectedOutcome, tags).
 * @param {string} id
 * @param {object} updates
 * @returns {Promise<object>} Updated manifest entry
 */
export async function updateTestCase(id, updates) {
	const res = await fetch(`${BASE}/${id}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(updates)
	});
	if (!res.ok) throw new Error(`Failed to update test case ${id}: ${res.status}`);
	return res.json();
}
