/**
 * Cloudflare Worker — GitHub OAuth Device Flow proxy.
 *
 * Forwards two GitHub OAuth endpoints that lack browser CORS headers:
 *   POST /login/device/code      → https://github.com/login/device/code
 *   POST /login/oauth/access_token → https://github.com/login/oauth/access_token
 *
 * Deploy: `npx wrangler deploy` from this directory.
 */

const ALLOWED_ORIGINS = [
	'https://sequoia-hope.github.io',
	'http://localhost:5173',
	'http://localhost:8083'
];

const GITHUB_ROUTES = {
	'/login/device/code': 'https://github.com/login/device/code',
	'/login/oauth/access_token': 'https://github.com/login/oauth/access_token'
};

function corsHeaders(origin) {
	return {
		'Access-Control-Allow-Origin': origin,
		'Access-Control-Allow-Methods': 'POST, OPTIONS',
		'Access-Control-Allow-Headers': 'Content-Type, Accept'
	};
}

export default {
	async fetch(request, env) {
		const origin = request.headers.get('Origin') || '';
		const allowed = ALLOWED_ORIGINS.includes(origin) ? origin : ALLOWED_ORIGINS[0];

		// CORS preflight
		if (request.method === 'OPTIONS') {
			return new Response(null, { status: 204, headers: corsHeaders(allowed) });
		}

		if (request.method !== 'POST') {
			return new Response('Method not allowed', { status: 405, headers: corsHeaders(allowed) });
		}

		const url = new URL(request.url);
		const target = GITHUB_ROUTES[url.pathname];
		if (!target) {
			return new Response('Not found', { status: 404, headers: corsHeaders(allowed) });
		}

		const body = await request.text();

		const ghResponse = await fetch(target, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				Accept: 'application/json'
			},
			body
		});

		const responseBody = await ghResponse.text();

		return new Response(responseBody, {
			status: ghResponse.status,
			headers: {
				'Content-Type': 'application/json',
				...corsHeaders(allowed)
			}
		});
	}
};
