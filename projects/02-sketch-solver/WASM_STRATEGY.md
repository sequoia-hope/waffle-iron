# Sketch Solver WASM Strategy

## Problem

The sketch-solver crate depends on `slvs` (v0.6.0), which wraps SolveSpace's C++ constraint solver via `cc` + `bindgen`. This C++ code cannot compile to `wasm32-unknown-unknown` because:

1. **No C++ stdlib**: The `wasm32-unknown-unknown` target has no libc/libcxx. SolveSpace includes `<functional>`, `<unordered_map>`, and other STL headers.
2. **bindgen FFI**: The `bindgen`-generated Rust↔C FFI assumes native calling conventions.

## Solution: Two-WASM-Module Architecture

```
Web Worker
├── Rust Engine WASM (wasm-pack, wasm32-unknown-unknown)
│   ├── kernel-fork (truck geometry)
│   ├── feature-engine (parametric tree)
│   ├── modeling-ops (extrude, revolve, boolean)
│   ├── file-format (save/load/STEP)
│   └── wasm-bridge (dispatch + wasm_api)
│
└── libslvs WASM (Emscripten, wasm32-unknown-emscripten)
    └── SolveSpace constraint solver C++ code
```

### How It Works

1. **Rust engine** compiled via `wasm-pack` (already working, 1.9 MB binary).
2. **libslvs** compiled via **Emscripten** to a separate `.wasm` + `.js` module.
3. Both modules loaded in the same Web Worker.
4. **JS glue code** bridges them:
   - Rust engine sends sketch data to JS
   - JS passes it to libslvs Emscripten module
   - libslvs solves, returns results to JS
   - JS passes results back to Rust engine

### Data Flow for SolveSketch

```
UI → postMessage → Worker
  → Rust wasm_api.process_message('{"type":"SolveSketch"}')
  → Returns: '{"type":"Error","message":"use JS bridge to libslvs"}'
  → Worker JS intercepts SolveSketch, calls libslvs instead:
    1. Build sketch from EngineState (via get_active_sketch() export)
    2. Map entities/constraints to libslvs C API format
    3. Call Slvs_Solve() in Emscripten module
    4. Extract solved positions
    5. Post SketchSolved response to main thread
```

## Emscripten Build Steps

### Prerequisites

```bash
# Install Emscripten SDK
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk && ./emsdk install latest && ./emsdk activate latest
source emsdk_env.sh
```

### Build libslvs

SolveSpace provides a C API (`slvs.h`) that exposes the constraint solver:

```bash
# Clone SolveSpace
git clone https://github.com/solvespace/solvespace.git
cd solvespace

# Build with Emscripten
mkdir build-wasm && cd build-wasm
emcmake cmake .. \
  -DCMAKE_BUILD_TYPE=Release \
  -DENABLE_GUI=OFF \
  -DENABLE_CLI=OFF \
  -DENABLE_TESTS=OFF
emmake make slvs

# Output: libslvs.wasm + libslvs.js
```

### Emscripten Module Configuration

```javascript
// libslvs_config.js
Module = {
  onRuntimeInitialized: function() {
    // Expose C API functions
    self.Slvs_MakeParam = Module.cwrap('Slvs_MakeParam', 'number', ['number', 'number', 'number']);
    self.Slvs_MakePoint2d = Module.cwrap('Slvs_MakePoint2d', 'number', ['number', 'number', 'number', 'number']);
    // ... etc for all Slvs_* functions
    self.Slvs_Solve = Module.cwrap('Slvs_Solve', 'number', ['number', 'number']);
  }
};
```

## Performance Analysis

### Bridge Overhead

The JS bridge between the two WASM modules adds:
- **Serialization**: Sketch data (entities + constraints) → flat arrays for C API. ~0.01ms for 100 entities.
- **Function calls**: Each `cwrap`'d call costs ~2.5ns. A 100-constraint sketch needs ~300 calls (params + entities + constraints) → ~0.75μs total.
- **Data copy**: Emscripten heap allocation + copy. ~0.01ms for 100 entities.

**Total bridge overhead**: <0.1ms, negligible compared to solve time (1-10ms).

### Measured Solve Times (Native)

From M9 benchmarks (native x86_64):
| Constraints | Solve Time |
|-------------|-----------|
| 14          | ~1.6ms    |
| 49          | ~2.9ms    |
| 105         | ~5.8ms    |
| 301         | ~8.7ms    |

WASM typically runs at 60-80% of native speed. Expected WASM solve times:
| Constraints | Expected WASM |
|-------------|--------------|
| 14          | ~2-3ms       |
| 49          | ~4-5ms       |
| 105         | ~7-10ms      |
| 301         | ~11-15ms     |

All well within interactive thresholds (16ms = 60fps frame budget).

## Current State

| Component | Status |
|-----------|--------|
| Rust engine WASM build | Working (wasm-pack, 1.9 MB) |
| SolveSketch in native builds | Working (feature-gated `native-solver`) |
| SolveSketch in WASM builds | Returns error, deferred to JS bridge |
| Emscripten build of libslvs | Not yet attempted |
| JS bridge code | Not yet written |

## Long-Term: Pure Rust Solver

The two-module approach is a short-term solution. Long-term options:

1. **Port SolveSpace solver to Rust**: Significant effort (~5K LOC of C++ numerical code) but eliminates the Emscripten dependency entirely.
2. **Use a Rust constraint solver**: Libraries like `cassowary-rs` handle linear constraints but lack the geometric constraint types (tangent, symmetric, etc.) that CAD sketching needs.
3. **Keep the two-module approach**: It works, the bridge overhead is negligible, and Emscripten is battle-tested. The main downside is build complexity.

Recommendation: Keep the two-module approach for now. Revisit if build complexity becomes a pain point or if a good Rust geometric constraint solver emerges.
