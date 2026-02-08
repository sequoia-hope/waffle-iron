# Waffle Iron

**The KiCad of mechanical CAD.** Waffle Iron is an open-source parametric CAD system designed to replace Onshape for daily mechanical design work. GPL-3.0 licensed, community-driven, built for the workflow engineers actually use: sketch on plane, constrain, extrude, fillet, pattern, assemble.

## Status

**Documentation / Planning Phase.** Architecture, interfaces, and agent workflow are being defined. No production code yet.

## Stack

| Layer | Choice | License |
|-------|--------|---------|
| Geometry kernel | Fork of [truck](https://github.com/ricosjp/truck) | Apache-2.0 |
| 2D constraint solver | [slvs](https://crates.io/crates/slvs) (SolveSpace libslvs) | GPL-3.0 |
| 3D rendering | [three.js](https://threejs.org/) via [Threlte](https://threlte.xyz/) | MIT |
| UI framework | [Svelte](https://svelte.dev/) / SvelteKit | MIT |
| WASM bridge | wasm-bindgen + Web Worker | — |
| Desktop wrapper | [Tauri](https://tauri.app/) (deferred) | MIT/Apache-2.0 |

## Vision

Replace proprietary parametric CAD with an open-source alternative that is good enough for daily professional use. The same way KiCad replaced Eagle for PCB design, Waffle Iron aims to replace Onshape for mechanical CAD.

Target workflow: sketch on plane → constrain sketch → extrude/revolve → fillet/chamfer → pattern → assembly.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for system design, and [INTERFACES.md](INTERFACES.md) for cross-project type contracts.

## License

GPL-3.0 — see [LICENSE](LICENSE) for details.
