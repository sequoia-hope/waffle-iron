import fs from 'fs';
import path from 'path';

export default function testCaseApiPlugin() {
	return {
		name: 'test-case-api',
		configureServer(server) {
			const CASES_DIR = path.resolve(server.config.root, 'tests/cases');
			const MANIFEST_PATH = path.join(CASES_DIR, 'manifest.json');
			const ASSAY_DIR = path.resolve(server.config.root, 'tests/cases/assay');

			// Ensure directory + manifest exist
			if (!fs.existsSync(CASES_DIR)) {
				fs.mkdirSync(CASES_DIR, { recursive: true });
			}
			if (!fs.existsSync(MANIFEST_PATH)) {
				fs.writeFileSync(MANIFEST_PATH, JSON.stringify({ cases: [] }, null, 2));
			}

			function readManifest() {
				return JSON.parse(fs.readFileSync(MANIFEST_PATH, 'utf-8'));
			}

			function writeManifest(manifest) {
				fs.writeFileSync(MANIFEST_PATH, JSON.stringify(manifest, null, 2));
			}

			function slugify(name) {
				return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
			}

			function uniqueSlug(name, manifest) {
				let slug = slugify(name);
				if (!slug) slug = 'test-case';
				const existing = new Set(manifest.cases.map(c => c.id));
				if (!existing.has(slug)) return slug;
				let i = 2;
				while (existing.has(`${slug}-${i}`)) i++;
				return `${slug}-${i}`;
			}

			function parseBody(req) {
				return new Promise((resolve, reject) => {
					let body = '';
					req.on('data', chunk => body += chunk);
					req.on('end', () => {
						try { resolve(JSON.parse(body)); }
						catch (e) { reject(e); }
					});
				});
			}

			server.middlewares.use('/api/test-cases', async (req, res, next) => {
				// Set JSON content type for all responses
				res.setHeader('Content-Type', 'application/json');

				try {
					const url = new URL(req.url, 'http://localhost');
					const pathParts = url.pathname.split('/').filter(Boolean);
					const id = pathParts[0] || null;

					if (req.method === 'GET' && !id) {
						// GET /api/test-cases — list all
						const manifest = readManifest();
						res.end(JSON.stringify(manifest));
						return;
					}

					if (req.method === 'GET' && id) {
						// GET /api/test-cases/:id — get one .waffle file
						const manifest = readManifest();
						const entry = manifest.cases.find(c => c.id === id);
						if (!entry) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'Not found' }));
							return;
						}
						const filePath = path.join(CASES_DIR, entry.filename);
						if (!fs.existsSync(filePath)) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'File not found' }));
							return;
						}
						const data = fs.readFileSync(filePath, 'utf-8');
						res.end(data);
						return;
					}

					if (req.method === 'POST' && !id) {
						// POST /api/test-cases — create new test case
						const body = await parseBody(req);
						const { name, description, expectedOutcome, tags, waffleData } = body;
						if (!name || !waffleData) {
							res.statusCode = 400;
							res.end(JSON.stringify({ error: 'name and waffleData are required' }));
							return;
						}
						const manifest = readManifest();
						const slug = uniqueSlug(name, manifest);
						const filename = `${slug}.waffle`;
						const entry = {
							id: slug,
							name,
							filename,
							description: description || '',
							expectedOutcome: expectedOutcome || 'should_pass',
							tags: tags || [],
							created: new Date().toISOString()
						};
						fs.writeFileSync(path.join(CASES_DIR, filename), waffleData);
						manifest.cases.push(entry);
						writeManifest(manifest);
						res.statusCode = 201;
						res.end(JSON.stringify(entry));
						return;
					}

					if (req.method === 'DELETE' && id) {
						// DELETE /api/test-cases/:id
						const manifest = readManifest();
						const idx = manifest.cases.findIndex(c => c.id === id);
						if (idx === -1) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'Not found' }));
							return;
						}
						const entry = manifest.cases[idx];
						const filePath = path.join(CASES_DIR, entry.filename);
						if (fs.existsSync(filePath)) fs.unlinkSync(filePath);
						manifest.cases.splice(idx, 1);
						writeManifest(manifest);
						res.end(JSON.stringify({ ok: true }));
						return;
					}

					if (req.method === 'PATCH' && id) {
						// PATCH /api/test-cases/:id — update metadata
						const body = await parseBody(req);
						const manifest = readManifest();
						const entry = manifest.cases.find(c => c.id === id);
						if (!entry) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'Not found' }));
							return;
						}
						if (body.name !== undefined) entry.name = body.name;
						if (body.description !== undefined) entry.description = body.description;
						if (body.expectedOutcome !== undefined) entry.expectedOutcome = body.expectedOutcome;
						if (body.tags !== undefined) entry.tags = body.tags;
						writeManifest(manifest);
						res.end(JSON.stringify(entry));
						return;
					}

					// Unknown method/path combo
					res.statusCode = 405;
					res.end(JSON.stringify({ error: 'Method not allowed' }));

				} catch (err) {
					res.statusCode = 500;
					res.end(JSON.stringify({ error: err.message }));
				}
			});

			// Official examples (static/examples): the panel reads the manifest
			// as static files; this endpoint is the development-only WRITE side
			// ("Save current as example"), so a document built in the page can
			// become a shipped example without leaving the browser.
			const EXAMPLES_DIR = path.resolve(server.config.root, 'static/examples');
			const EXAMPLES_MANIFEST = path.join(EXAMPLES_DIR, 'manifest.json');
			function readExamples() {
				if (!fs.existsSync(EXAMPLES_MANIFEST)) return { examples: [] };
				const manifest = JSON.parse(fs.readFileSync(EXAMPLES_MANIFEST, 'utf-8'));
				if (!Array.isArray(manifest.examples)) manifest.examples = [];
				return manifest;
			}
			server.middlewares.use('/api/examples', async (req, res) => {
				res.setHeader('Content-Type', 'application/json');
				try {
					if (req.method === 'OPTIONS') {
						res.end(JSON.stringify({ writable: true }));
						return;
					}
					if (req.method === 'GET') {
						res.end(JSON.stringify(readExamples()));
						return;
					}
					if (req.method === 'POST') {
						const body = await parseBody(req);
						const { name, description, waffleData } = body;
						if (!name || !waffleData) {
							res.statusCode = 400;
							res.end(JSON.stringify({ error: 'name and waffleData are required' }));
							return;
						}
						let parsed;
						try {
							parsed = JSON.parse(waffleData);
						} catch {
							res.statusCode = 400;
							res.end(JSON.stringify({ error: 'waffleData is not a .waffle document' }));
							return;
						}
						const manifest = readExamples();
						let id = slugify(name) || 'example';
						const taken = new Set(manifest.examples.map(e => e.id));
						for (let i = 2; taken.has(id); i++) id = `${slugify(name) || 'example'}-${i}`;
						const filename = `${id}.waffle`;
						const entry = {
							id,
							name,
							filename,
							description: description || '',
							tabs: Array.isArray(parsed.tabs) ? parsed.tabs.map(t => t?.name).filter(Boolean) : [],
							built: new Date().toISOString().slice(0, 10),
							built_with: 'saved from the page (Examples panel)'
						};
						fs.mkdirSync(EXAMPLES_DIR, { recursive: true });
						fs.writeFileSync(path.join(EXAMPLES_DIR, filename), waffleData);
						manifest.examples.push(entry);
						fs.writeFileSync(EXAMPLES_MANIFEST, JSON.stringify(manifest, null, 2) + '\n');
						res.statusCode = 201;
						res.end(JSON.stringify(entry));
						return;
					}
					res.statusCode = 405;
					res.end(JSON.stringify({ error: 'Method not allowed' }));
				} catch (err) {
					res.statusCode = 500;
					res.end(JSON.stringify({ error: err.message }));
				}
			});

			// Assay cases API
			server.middlewares.use('/api/assay-cases', async (req, res, next) => {
				res.setHeader('Content-Type', 'application/json');
				try {
					const url = new URL(req.url, 'http://localhost');
					const pathParts = url.pathname.split('/').filter(Boolean);
					const id = pathParts[0] || null;
					const subResource = pathParts[1] || null;

					if (req.method === 'GET' && !id) {
						const manifestPath = path.join(ASSAY_DIR, 'manifest.json');
						if (!fs.existsSync(manifestPath)) {
							res.end(JSON.stringify({ master_seed: 0, count: 0, generator_version: 0, cases: [] }));
							return;
						}
						res.end(fs.readFileSync(manifestPath, 'utf-8'));
						return;
					}

					if (req.method === 'GET' && id === 'results' && !subResource) {
						const resultsPath = path.join(ASSAY_DIR, 'results.json');
						if (!fs.existsSync(resultsPath)) {
							res.end(JSON.stringify({ total: 0, passed: 0, failed: 0, errored: 0, results: [] }));
							return;
						}
						res.end(fs.readFileSync(resultsPath, 'utf-8'));
						return;
					}

					if (req.method === 'GET' && id && subResource === 'meta') {
						const metaPath = path.join(ASSAY_DIR, `${id}.meta.json`);
						if (!fs.existsSync(metaPath)) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'Meta not found' }));
							return;
						}
						res.end(fs.readFileSync(metaPath, 'utf-8'));
						return;
					}

					if (req.method === 'GET' && id) {
						const wafflePath = path.join(ASSAY_DIR, `${id}.waffle`);
						if (!fs.existsSync(wafflePath)) {
							res.statusCode = 404;
							res.end(JSON.stringify({ error: 'Case not found' }));
							return;
						}
						res.end(fs.readFileSync(wafflePath, 'utf-8'));
						return;
					}

					res.statusCode = 405;
					res.end(JSON.stringify({ error: 'Method not allowed' }));
				} catch (err) {
					res.statusCode = 500;
					res.end(JSON.stringify({ error: err.message }));
				}
			});
		}
	};
}
