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

export class McpRelay {
	/**
	 * @param {{ port: number, appUrl: string, allowOrigin: string, extraArgs?: string[] }} opts
	 */
	constructor({ port, appUrl, allowOrigin, extraArgs = [] }) {
		this.port = port;
		this.stderr = '';
		/** @type {string[]} */
		this.stdoutLines = [];
		/** @type {object[]} */
		this.notifications = [];
		this._nextId = 0;
		/** @type {Map<number, (msg: any) => void>} */
		this._pending = new Map();
		this._buffer = '';
		const env = { ...process.env };
		delete env.PORT;
		this.proc = spawn(
			uvCommand(),
			[
				'run', '--project', path.join(REPO_ROOT, 'relay'), 'waffle-mcp-relay',
				'--port', String(port), '--app-url', appUrl, '--allow-origin', allowOrigin,
				...extraArgs
			],
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

	/** Resolves once the relay logs that its WebSocket is listening. */
	async waitListening(timeoutMs = 60000) {
		const deadline = Date.now() + timeoutMs;
		while (!this.stderr.includes('listening on')) {
			if (this.proc.exitCode !== null) throw new Error(`relay exited early:\n${this.stderr}`);
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
	async callTool(name, args = {}) {
		const response = await this.request('tools/call', { name, arguments: args });
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
