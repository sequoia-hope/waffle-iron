# Waffle Iron

**An experimental open-source parametric CAD system** created by Sequoia Alexander and [Claude](https://claude.ai) to push the limits of coding agent capabilities, and see if we can use them to build solid, dependable tools for everyday use. GPL-3.0 licensed.

**[Try it in your browser](https://sequoia-hope.github.io/waffle-iron/)**

## Status

**Early experimental.** Core sketch-constrain-extrude-revolve workflow is functional but rough. Boolean operations are fragile. Fillet, chamfer, and shell are deferred. There is no file save/load, no assemblies, and no desktop app yet. Expect bugs.

## Stack

| Layer | Choice | License |
|-------|--------|---------|
| Geometry kernel | Clean-sheet B-Rep kernel | GPL-3.0 |
| 2D constraint solver | [slvs](https://crates.io/crates/slvs) (SolveSpace libslvs) | GPL-3.0 |
| 3D rendering | [three.js](https://threejs.org/) via [Threlte](https://threlte.xyz/) | MIT |
| UI framework | [Svelte](https://svelte.dev/) / SvelteKit | MIT |
| WASM bridge | wasm-bindgen + Web Worker | — |
| Desktop wrapper | [Tauri](https://tauri.app/) (deferred) | MIT/Apache-2.0 |

## Vision

A dream of a world where open-source parametric CAD is good enough for daily professional use. We're not there yet, but every sketch, extrude, and boolean gets us a little closer.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for system design, and [INTERFACES.md](INTERFACES.md) for cross-project type contracts.

## License

GPL-3.0 — see [LICENSE](LICENSE) for details.
