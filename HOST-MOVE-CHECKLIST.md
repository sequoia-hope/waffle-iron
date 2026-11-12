# Moving the Waffle Iron Claude session from the container to the host

**Done 2026-11-12.** The containers are stopped and removed (their volumes are kept);
agents run from `launch.sh` / `opencode.sh` in host tmux, and `serve.sh` runs the dev
server on the registry's port. Docker files (`Dockerfile`, `docker-compose*.yml`,
`shell.sh`, `run.sh`, `start.sh`, `stop.sh`, `claude-remote/`) are no longer used.

Written 2026-11-06 from inside the container (`waffle-iron-claude`). Uncommitted. Every fact
below was checked on that date; the "verify" lines say how to re-check.

## Why the container blocks messaging

Two things, not one:

1. **Liveness.** `~/.claude` is already shared (bind mount of the host's `/home/sequoia/.claude`), so
   this session can read every host session's entry in `~/.claude/sessions/`. Each entry carries
   its pid and a `pidDomain`. The container has its own PID namespace, so it can't see those
   pids alive and drops them. `ListAgents` here shows only `workspace-8e`, the other session
   in this container.
2. **Delivery.** Each session's messaging socket lives in `/tmp/cc-socks/` (this one is
   `/tmp/cc-socks/20.sock`). The container's `/tmp` is its own.

`pid: host` alone would fix (1) and not (2). Running on the host fixes both.

## Before the move

- [ ] **Nothing in flight.** As of 2f787b95 every change is landed and pushed. The sequoia
      checkouts are back on `main` and `o5-rigid-fix`.
- [ ] **Worktrees.** `~/wt/` exists only inside the container (it's not a mount), so it vanishes
      on the host. That is safe:
      - Every branch's commits are in the shared `.git`, including the `agy/*` branches not on
        origin (`agy/pair-matrix`, `agy/suite-app`, `agy/board-shape`, …).
      - The only uncommitted files are in `f2-step1`: a stale WASM bundle, which is disposable.

## On the host

### 1. Toolchain (the Dockerfile's list, against what the host has)

- [ ] **Rust:** nothing to do. `rust-toolchain.toml` pins `1.97.1` (with clippy, rustfmt and
      wasm32), and rustup installs it on the first `cargo` in the repo. The host default is
      1.95. Verify: `cd ~/Software/waffle-iron && cargo --version` prints 1.97.1.
- [x] **wasm-pack:** (0.13.1 installed 2026-11-12) the host has 0.14.0. The container built every committed bundle with
      0.13.1. To match: `cargo install wasm-pack --version 0.13.1 --locked`. Or keep 0.14,
      and check that the first `./scripts/build-wasm.sh` bundle still passes the GUI fast tier.
- [ ] **Node:** host 22 with npm 9.2, container 18, CI 20. Node 20 (e.g. via nvm) matches CI.
      Node 22 will probably work, but nothing has tested it.
- [x] **Playwright 1.61.0 browsers:** (installed 2026-11-12; WebKit still wants
      `sudo apt-get install libmanette-0.2-0`) the host has chromium and webkit but **no firefox**.
      `cd app && npx playwright install chromium firefox webkit`. If a browser fails to start,
      `sudo npx playwright install-deps`.
- [ ] **C/C++:** g++ 15, cmake 4.2.3, pkg-config and libclang (18, 21) are present.
      `indirect-predicates-sidecar-rs` needs `cc` and `bindgen`, which use libclang, so this
      should be enough. The `clang` binary is missing; `sudo apt install clang` if that crate's
      build complains.
- [ ] **gh:** present (2.46). Verify `gh auth status`. The pre-push hook reads CI status with it.
- [ ] `claude`, `agy` and `proj` are in `~/.local/bin`. A non-login ssh shell doesn't see them.

### 2. Cherchi sidecar (dev-only parity oracle; nothing about it needs Docker)

- [x] Copied the container's build from the Docker volume to `~/cherchi2022` (2026-11-12).
      Originally: build it on the host:
      `CHERCHI2022_ROOT=$HOME/cherchi2022 ./scripts/build_sidecars.sh`.
      Or copy the container's build: `sudo cp -a /var/lib/docker/volumes/waffle-iron_cherchi2022/_data ~/cherchi2022 && sudo chown -R sequoia: ~/cherchi2022`.
- [x] **No env vars needed any more** (dca3466c). The crates now default to
      `$CHERCHI2022_ROOT`, else `$HOME/cherchi2022`, which is where `build_sidecars.sh`
      builds. `CHERCHI2022_BIN` and `INDIRECT_PREDICATES_SRC` still override.
      Verify: `cargo build -p indirect-predicates-sidecar-rs` warns "linked real shim against …",
      not "built no-op stub".

### 3. Memory and history (all in the shared `~/.claude`)

- [x] **Memory merged** (2026-11-06). Memory is keyed by path: a host session in
      `~/Software/waffle-iron` reads `~/.claude/projects/-home-sequoia-Software-waffle-iron/memory/`.
      All ~600 container memories were copied there and its `MEMORY.md` is now the container's
      index. The host's old March-2026 index is kept as `MEMORY.host-old.md`, and
      `truck-api.md` stays, linked from the index's last section. The container copy under
      `-home-claude-workspace/` is untouched. Memories written in the container after the merge
      won't carry over, so make it the last thing before the move, or re-run (re-run 2026-11-12)
      `cp -pu ~/.claude/projects/-home-claude-workspace/memory/*.md ~/.claude/projects/-home-sequoia-Software-waffle-iron/memory/`.
- [ ] **Old transcripts** stay under `-home-claude-workspace/`. A host `claude --resume` in the
      repo won't list them, but they remain readable as files.

### 4. Build cache

- [ ] `target/` was built at `/home/claude/workspace`, so the first host build recompiles
      everything. That's expected; no `cargo clean` is needed. The container's cargo registry is a
      Docker volume, so the host downloads crates into its own `~/.cargo` once.
- [x] `git worktree prune`, to drop the `~/wt` entries that now point at nothing.

### 5. The container, the dev server and ports

- [x] **The container starts Claude sessions on every boot** (`claude-remote/entrypoint.sh`: tmux
      `claude` and `default`, plus `agy` and `opencode`). Two Claude sessions on one tree
      will collide, so stop the in-container ones:
      `docker exec waffle-iron-claude tmux kill-session -t default` (and `-t claude`). Or stop
      the container once the host side works.
- [x] **The dev server runs in the container** (now `serve.sh` via `proj up waffle-iron`, autostart) (`npm run dev`, published as 8083 → 5173, ttyd on
      8082). Two options:
      - Keep the container running only for that. It sees host edits through the bind mount.
      - Or stop the container and serve from the host the registered way
        (`proj up waffle-iron`; the registry has `web_port = 8083`, `web_start = "./start.sh"`).
        8083 is bound by the container until it stops.
- [x] **The `proj` registry entry still launches the agent in the container** (now `./launch.sh`)
      (`command = "./shell.sh"`, `container_dir = "/home/claude/workspace"` in
      `~/scripts/launcher.toml`). Point it at the host per `~/scripts/README.md`, then run
      `proj doctor`.

### 6. Cross-machine plumbing (should just work, so verify)

- [ ] `~/.ssh` is the same keys, so `ssh sequoia@192.168.86.22 true` works from the host.
- [ ] `~/.claude/bin/reserve --status`. On the host it finds the server via `proj url reserve`.

## First run on the host (in this order)

1. Start the session in `/home/sequoia/Software/waffle-iron` with the usual auto-mode flags.
2. Ask it to run `ListAgents`. The boardroom and PCB sessions should now appear. Send one a
   test message.
3. `cargo test -q -p yang-rs --lib`: the toolchain pin installs and the suite passes
   (1167 passed, 3 ignored on 2f787b95).
4. (2026-11-12: the source fingerprint matched, but the `.wasm` grew 92 bytes: Rust's panic
   locations embed `~/.cargo/registry` paths, `/home/claude` before and `/home/sequoia` now. Same
   code; the first host rebuild after a Rust change will carry that diff.) `./scripts/build-wasm.sh` must leave `app/static/pkg/` unchanged: the source fingerprint
   matches, and the bytes should too with wasm-pack 0.13.1.
5. `./scripts/test.sh rewrite` (booked through `reserve`): green and records a stamp. This proves
   the sidecar env vars.
