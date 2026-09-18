/**
 * Drive the REAL `waffle-mcp-relay` (relay/) as an MCP client over stdio from a
 * Playwright spec (specs/waffle_mcp_server.md §5 harness b).
 */
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(here, '../../../..');

/** `uv` from $UV, else PATH, else the official installer's ~/.local/bin. */
function uvCommand() {
	if (process.env.UV) return process.env.UV;
	const local = path.join(os.homedir(), '.local', 'bin', 'uv');
	const onPath = (process.env.PATH ?? '')
		.split(path.delimiter)
		.some((dir) => dir && existsSync(path.join(dir, 'uv')));
	return onPath || !existsSync(local) ? 'uv' : local;
}

/**
 * A port for the relay under test: $AGENT_RELAY_PORT when the test runner
 * provides one (not $PORT, which the hub may already use for the dev server),
 * else an OS-assigned free port. This is
 * test-fixture allocation passed to the relay EXPLICITLY via --port; the relay
 * itself has no default and never picks one.
 * @returns {Promise<number>}
 */
export async function relayTestPort() {
	if (process.env.AGENT_RELAY_PORT) return Number(process.env.AGENT_RELAY_PORT);
	return new Promise((resolve, reject) => {
		const srv = net.createServer();
		srv.on('error', reject);
		srv.listen(0, '127.0.0.1', () => {
			const { port } = /** @type {net.AddressInfo} */ (srv.address());
			srv.close(() => resolve(port));
		});
	});
}

/**
 * Pair `page` with `relay` the way a user does: `waffle_connect`, open the
 * pairing link, click Allow, land in the editor with the agent bar, engine ready.
 * @param {import('@playwright/test').Page} page
 * @param {McpRelay} relay
 * @param {string} agentName - the clientInfo name the relay was initialized with
 */
export async function pairAgent(page, relay, agentName) {
	const connect = await relay.callTool('waffle_connect');
	if (connect.isError) throw new Error(`waffle_connect failed: ${JSON.stringify(connect)}`);
	await page.goto(connect.structuredContent.pairing_url);
	await page.getByTestId('agent-consent-allow').click();
	await page.waitForURL((url) => url.pathname === '/', { timeout: 30000 });
	await page.getByTestId('agent-bar-label').filter({ hasText: agentName }).waitFor({ timeout: 30000 });
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
}

export class McpRelay {
	/**
	 * @param {{ port: number, appUrl: string, allowOrigin: string, extraArgs?: string[] }} opts
	 */
	constructor({ port, appUrl, allowOrigin, extraArgs = [] }) {
		this._args = ['--app-url', appUrl, '--allow-origin', allowOrigin, ...extraArgs];
		/** @type {string[]} */
		this.stdoutLines = [];
		/** @type {object[]} */
		this.notifications = [];
		this._nextId = 0;
		/** @type {Map<number, (msg: any) => void>} */
		this._pending = new Map();
		this._buffer = '';
		this._spawn(port);
	}

	/** @param {number} port */
	_spawn(port) {
		this.port = port;
		this.stderr = '';
		const env = { ...process.env };
		delete env.PORT;
		this.proc = spawn(
			uvCommand(),
			['run', '--project', path.join(REPO_ROOT, 'relay'), 'waffle-mcp-relay', '--port', String(port), ...this._args],
			{ cwd: REPO_ROOT, env, stdio: ['pipe', 'pipe', 'pipe'] }
		);
		this.exited = new Promise((resolve) => this.proc.on('exit', (code) => resolve(code)));
		this.proc.stderr.on('data', (chunk) => {
			this.stderr += chunk.toString();
		});
		this.proc.stdout.on('data', (chunk) => this._onStdout(chunk.toString()));
	}

	_onStdout(text) {
		this._buffer += text;
		let nl;
		while ((nl = this._buffer.indexOf('\n')) >= 0) {
			const line = this._buffer.slice(0, nl);
			this._buffer = this._buffer.slice(nl + 1);
			if (!line.trim()) continue;
			this.stdoutLines.push(line);
			const msg = JSON.parse(line); // I13: a non-JSON stdout line throws and fails the test
			const resolve = this._pending.get(msg.id);
			if (resolve && ('result' in msg || 'error' in msg)) {
				this._pending.delete(msg.id);
				resolve(msg);
			} else {
				this.notifications.push(msg);
			}
		}
	}

	/**
	 * Resolves once the relay logs that its WebSocket is listening.
	 *
	 * `relayTestPort()` releases the OS-assigned port before the relay binds it,
	 * so a parallel worker can take it in between (CI run 35291254557: `[Errno
	 * 98] address already in use`). An address-in-use exit on an allocated (not
	 * $AGENT_RELAY_PORT-pinned) port respawns the relay on a fresh one; the
	 * `port` field is final only after this resolves.
	 */
	async waitListening(timeoutMs = 60000, maxBindRetries = 5) {
		const deadline = Date.now() + timeoutMs;
		let bindRetries = 0;
		while (!this.stderr.includes('listening on')) {
			if (this.proc.exitCode !== null) {
				const pinned = Boolean(process.env.AGENT_RELAY_PORT);
				if (!pinned && this.stderr.includes('address already in use') && bindRetries < maxBindRetries) {
					bindRetries += 1;
					this._spawn(await relayTestPort());
					continue;
				}
				throw new Error(`relay exited early:\n${this.stderr}`);
			}
			if (Date.now() > deadline) throw new Error(`relay did not start:\n${this.stderr}`);
			await new Promise((r) => setTimeout(r, 50));
		}
	}

	/** @param {string} method @param {object} [params] */
	request(method, params, timeoutMs = 30000) {
		const id = ++this._nextId;
		const msg = { jsonrpc: '2.0', id, method, ...(params !== undefined ? { params } : {}) };
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this._pending.delete(id);
				reject(new Error(`MCP ${method} timed out; relay stderr:\n${this.stderr}`));
			}, timeoutMs);
			this._pending.set(id, (response) => {
				clearTimeout(timer);
				resolve(response);
			});
			this.proc.stdin.write(`${JSON.stringify(msg)}\n`);
		});
	}

	/**
	 * Start a request without waiting: its JSON-RPC id (for `cancel`) and the
	 * response promise. A cancelled request never gets a response, so the
	 * promise rejects at `timeoutMs`; callers that cancel should `.catch()` it.
	 * @param {string} method @param {object} [params]
	 */
	start(method, params, timeoutMs = 30000) {
		const id = this._nextId + 1;
		return { id, response: this.request(method, params, timeoutMs) };
	}

	/** MCP `notifications/cancelled` for an in-flight request. @param {number} requestId */
	cancel(requestId, reason = 'cancelled by test') {
		const msg = { jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId, reason } };
		this.proc.stdin.write(`${JSON.stringify(msg)}\n`);
	}

	async initialize(clientName) {
		const response = await this.request('initialize', {
			protocolVersion: '2025-11-25',
			capabilities: {},
			clientInfo: { name: clientName, version: '0' }
		});
		this.proc.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
		return response;
	}

	/** @returns {Promise<any>} the CallToolResult */
	async callTool(name, args = {}, timeoutMs = 30000) {
		const response = await this.request('tools/call', { name, arguments: args }, timeoutMs);
		if (!response.result) throw new Error(`tools/call ${name} failed: ${JSON.stringify(response)}`);
		return response.result;
	}

	async close() {
		this.proc.stdin.end();
		const timer = setTimeout(() => this.proc.kill('SIGTERM'), 10000);
		const code = await this.exited;
		clearTimeout(timer);
		return code;
	}
}
