import fs from 'fs';
import path from 'path';
import zlib from 'zlib';

export default function testCaseApiPlugin() {
	return {
		name: 'test-case-api',
		configureServer(server) {
			const CASES_DIR = path.resolve(server.config.root, 'tests/cases');
			const MANIFEST_PATH = path.join(CASES_DIR, 'manifest.json');
			const ASSAY_DIR = path.resolve(server.config.root, 'tests/cases/assay');
			const SKETCH_DIR = path.resolve(server.config.root, 'tests/cases/sketch');

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
						// Stored gzipped, like the shipped examples: a `.waffle`
						// is mostly indentation and nothing compresses it in
						// transit (`docs/notes/eiffel/FEATURE_NOTES.md` §6).
						// `mtime: 0` keeps two saves of the same document
						// byte-identical.
						const filename = `${id}.waffle.gz`;
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
						fs.writeFileSync(
							path.join(EXAMPLES_DIR, filename),
							zlib.gzipSync(Buffer.from(waffleData, 'utf-8'), { level: 9, mtime: 0 })
						);
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

			// Sketch cases (S4 of `specs/agent_mechanical_design.md` §10.4):
			// READ-ONLY, like the assay cases beside them and deliberately NOT
			// the generic `/api/test-cases` slot the spec originally named.
			//
			// That slot is a CRUD API the Tests browser panel owns, and
			// `app/tests/gui/test-case-browser.spec.js` exercises its DELETE —
			// which wiped all thirteen committed `.waffle` files and emptied
			// the manifest the first time the corpus was served from there
			// (measured 2026-10-08). A committed fixture cannot live behind a
			// mutable endpoint a test clears as part of its own setup.
			server.middlewares.use('/api/sketch-cases', async (req, res) => {
				res.setHeader('Content-Type', 'application/json');
				try {
					if (req.method !== 'GET') {
						res.statusCode = 405;
						res.end(JSON.stringify({ error: 'The sketch corpus is read-only' }));
						return;
					}
					const url = new URL(req.url, 'http://localhost');
					const parts = url.pathname.split('/').filter(Boolean);
					const id = parts[0] || null;
					const subResource = parts[1] || null;

					// The listing is derived from what is on disk, so the
					// corpus generator does not have to write a manifest too.
					if (!id) {
						if (!fs.existsSync(SKETCH_DIR)) {
							res.end(JSON.stringify({ count: 0, cases: [] }));
							return;
						}
						const cases = fs
							.readdirSync(SKETCH_DIR)
							.filter((f) => f.endsWith('.meta.json'))
							.map((f) => f.replace(/\.meta\.json$/, ''))
							.sort()
							.map((caseId) => {
								const meta = JSON.parse(
									fs.readFileSync(path.join(SKETCH_DIR, `${caseId}.meta.json`), 'utf-8')
								);
								return {
									id: caseId,
									description: meta.description,
									exercises: meta.exercises ?? [],
									status: meta.expectations?.status,
									dof: meta.expectations?.dof
								};
							});
						res.end(JSON.stringify({ count: cases.length, cases }));
						return;
					}

					// A case id must be a bare S-number: no path separators, so
					// no traversal out of the corpus directory.
					if (!/^[A-Za-z0-9_-]+$/.test(id)) {
						res.statusCode = 400;
						res.end(JSON.stringify({ error: 'Bad case id' }));
						return;
					}
					const file =
						subResource === 'meta'
							? path.join(SKETCH_DIR, `${id}.meta.json`)
							: path.join(SKETCH_DIR, `${id}.waffle`);
					if (subResource && subResource !== 'meta') {
						res.statusCode = 404;
						res.end(JSON.stringify({ error: 'Not found' }));
						return;
					}
					if (!fs.existsSync(file)) {
						res.statusCode = 404;
						res.end(JSON.stringify({ error: 'Case not found' }));
						return;
					}
					res.end(fs.readFileSync(file, 'utf-8'));
				} catch (err) {
					res.statusCode = 500;
					res.end(JSON.stringify({ error: err.message }));
				}
			});
		}
	};
}
