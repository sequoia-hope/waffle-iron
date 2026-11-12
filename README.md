# Waffle Iron

An open-source parametric CAD system that runs in the browser, built by Sequoia Alexander and [Claude](https://claude.ai) to find out whether coding agents can build solid, dependable tools for everyday use. MIT licensed.

**[Try it in your browser](https://sequoia-hope.github.io/waffle-iron/)**

## What it does

- **Sketch** on a plane or a face with lines, arcs and circles, and constrain them with a geometric constraint solver.
- **Model** with extrude, revolve, pipe, boolean combine, datum planes, and linear, circular and mirror patterns, in a feature tree you can edit, reorder, suppress and roll back.
- **Assemble** bodies with mate connectors.
- **Draw**: projected and section views with hidden-line removal, dimensions and tolerances.
- **Exchange**: `.waffle` documents (a JSON recipe, see `docs/FILE_FORMAT.md`), STEP import and export, STL export, KiCad board import.
- **Drive it from an agent**: an MCP server (`relay/`) lets an AI agent author, measure and inspect a document in a paired browser tab.

Fillet, chamfer and shell are not available.

## The geometry kernel

The kernel is written from scratch in Rust and implements a published algorithm: *Boolean Operation for CAD Models Using a Hybrid Representation* (Yang et al., ACM TOG 2025). Each solid carries an exact B-Rep and a triangle mesh tied to it. A boolean runs an exact mesh boolean (Cherchi et al. 2020, 2022) to get the topology, then moves the result back onto the exact surfaces and their intersection curves. Faces stay analytic (planes, cylinders, cones, spheres, tori) from input to output.

The kernel is not at 1.0. `docs/kernel_1_0.md` defines what 1.0 means, as four measured gates (correct, faithful to the paper, capable, bounded), and `docs/yang_faithfulness_roadmap.md` is the work order. A boolean the kernel cannot do returns a typed error; it does not return a wrong solid.

## Stack

| Layer | Choice |
|---|---|
| Geometry kernel | Rust: `kernel-v2` (B-Rep), `yang-rs` (hybrid boolean), `cherchi-rs` (exact mesh boolean), `ssi-rs` (surface intersections) |
| Sketch solver | Rust, Levenberg–Marquardt |
| Parametric engine | Rust: `feature-engine`, `modeling-ops`, `file-format` |
| Browser bridge | WebAssembly (`wasm-bridge`) in a Web Worker |
| UI | [Svelte](https://svelte.dev/) / SvelteKit |
| 3D rendering | [three.js](https://threejs.org/) via [Threlte](https://threlte.xyz/) |
| Agent link | Python MCP relay (`relay/`) |

## Working on it

- [DEVELOPMENT.md](DEVELOPMENT.md): setting up a machine and running the app.
- [ARCHITECTURE.md](ARCHITECTURE.md) and [INTERFACES.md](INTERFACES.md): system design and the type contracts between crates.
- [docs/TESTING.md](docs/TESTING.md): the test tiers (`./scripts/test.sh`).
- [CONTRIBUTING.md](CONTRIBUTING.md): rules for changes.
- [HISTORY.md](HISTORY.md): where the project came from.

## License

MIT. See [LICENSE](LICENSE).
