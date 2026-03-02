/**
 * Get edge range data for a specific feature by index.
 *
 * Returns a JSON array of edge ranges enriched with GeomRef data.
 * Each entry contains a `geom_ref` (persistent geometry reference) plus
 * `start_index` and `end_index` into the edge vertices array (in vertex count,
 * not float count).
 * @param {number} feature_index
 * @returns {string}
 */
export function get_edge_data(feature_index) {
    let deferred1_0;
    let deferred1_1;
    try {
        let ret;
        __wbg_termination_guard();
        try {
            ret = wasm.get_edge_data(feature_index);;
        } catch(e) {
            __wbg_handle_catch(e);
        }
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        __wbg_termination_guard();
        try {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

/**
 * Get edge vertex positions as a Float32Array view into WASM memory.
 *
 * Returns the edge polyline vertices for a feature as a zero-copy typed array.
 * The array contains [x0, y0, z0, x1, y1, z1, ...] where consecutive pairs
 * of vertices form line segments for rendering with THREE.LineSegments.
 * @param {number} feature_index
 * @returns {Float32Array}
 */
export function get_edge_vertices(feature_index) {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_edge_vertices(feature_index);;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Get face data for a specific feature by index.
 *
 * Returns a JSON array of face ranges enriched with GeomRef data.
 * Each entry contains a `geom_ref` (persistent geometry reference) plus
 * `start_index` and `end_index` into the mesh indices array.
 *
 * For faces with role assignments from provenance, a Role-based selector is used.
 * For faces without roles, a Signature-based selector with a centroid fallback is used.
 * @param {number} feature_index
 * @returns {string}
 */
export function get_face_data(feature_index) {
    let deferred1_0;
    let deferred1_1;
    try {
        let ret;
        __wbg_termination_guard();
        try {
            ret = wasm.get_face_data(feature_index);;
        } catch(e) {
            __wbg_handle_catch(e);
        }
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        __wbg_termination_guard();
        try {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

/**
 * Get the current feature tree as JSON.
 *
 * Useful for the UI to query state without sending a full command.
 * Wrapped in catch_unwind to prevent panics from crashing the WASM module
 * if engine state is corrupted after a failed boolean cascade.
 * @returns {string}
 */
export function get_feature_tree() {
    let deferred1_0;
    let deferred1_1;
    try {
        let ret;
        __wbg_termination_guard();
        try {
            ret = wasm.get_feature_tree();;
        } catch(e) {
            __wbg_handle_catch(e);
        }
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        __wbg_termination_guard();
        try {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

/**
 * Get the number of features with mesh data.
 * @returns {number}
 */
export function get_mesh_count() {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_mesh_count();;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret >>> 0;
}

/**
 * Get mesh triangle indices as a Uint32Array view into WASM memory.
 *
 * Returns [i0, i1, i2, i3, i4, i5, ...] where each triple is a triangle.
 * @param {number} feature_index
 * @returns {Uint32Array}
 */
export function get_mesh_indices(feature_index) {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_mesh_indices(feature_index);;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Get mesh data for a specific feature by index.
 *
 * Returns a JSON object with vertices, normals, and indices arrays.
 * For high-performance rendering, the web worker should use the
 * `get_mesh_vertices`, `get_mesh_normals`, and `get_mesh_indices`
 * functions instead, which return typed arrays directly.
 * Wrapped in catch_unwind to prevent panics from crashing the WASM module.
 * @param {number} feature_index
 * @returns {string}
 */
export function get_mesh_json(feature_index) {
    let deferred1_0;
    let deferred1_1;
    try {
        let ret;
        __wbg_termination_guard();
        try {
            ret = wasm.get_mesh_json(feature_index);;
        } catch(e) {
            __wbg_handle_catch(e);
        }
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        __wbg_termination_guard();
        try {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

/**
 * Get mesh vertex normals as a Float32Array view into WASM memory.
 *
 * Returns [nx0, ny0, nz0, nx1, ny1, nz1, ...].
 * @param {number} feature_index
 * @returns {Float32Array}
 */
export function get_mesh_normals(feature_index) {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_mesh_normals(feature_index);;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Get mesh vertex positions as a Float32Array view into WASM memory.
 *
 * Returns the vertices of the latest (last) feature's mesh as a zero-copy
 * typed array view. The array contains [x0, y0, z0, x1, y1, z1, ...].
 *
 * IMPORTANT: The returned view is invalidated by any WASM memory growth.
 * Copy or transfer the data immediately after calling this function.
 * @param {number} feature_index
 * @returns {Float32Array}
 */
export function get_mesh_vertices(feature_index) {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_mesh_vertices(feature_index);;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Get which feature indices should be rendered.
 *
 * Returns indices of features that have mesh data and are NOT consumed
 * by a later boolean operation. When a boolean union succeeds, the target
 * feature is consumed (its geometry is merged into the result feature).
 * When union fails, both features are renderable (multi-body mode).
 * @returns {Uint32Array}
 */
export function get_renderable_feature_indices() {
    let ret;
    __wbg_termination_guard();
    try {
        ret = wasm.get_renderable_feature_indices();;
    } catch(e) {
        __wbg_handle_catch(e);
    }
    return ret;
}

/**
 * Initialize the WASM engine. Must be called once before any other function.
 *
 * Sets up panic hooks for better error messages and creates the engine state.
 */
export function init() {
    __wbg_termination_guard();
    try {
        wasm.init();
    } catch(e) {
        __wbg_handle_catch(e);
    }
}

/**
 * Process a JSON message from the UI and return a JSON response.
 *
 * This is the main entry point for the web worker's message handler.
 * The input should be a JSON-serialized `UiToEngine` message.
 * Returns a JSON-serialized `EngineToUi` response.
 * @param {string} json_input
 * @returns {string}
 */
export function process_message(json_input) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(json_input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        let ret;
        __wbg_termination_guard();
        try {
            ret = wasm.process_message(ptr0, len0);;
        } catch(e) {
            __wbg_handle_catch(e);
        }
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        __wbg_termination_guard();
        try {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        } catch(e) {
            __wbg_handle_catch(e);
        }
    }
}

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_panic_error_1bf6d8b40c6eefa1: function(arg0) {
            const ret = new PanicError(arg0);
            return ret;
        },
        __wbg___wbindgen_rethrow_5d3a9250cec92549: function(arg0) {
            throw new WebAssembly.Exception(__wbindgen_wrapped_jstag, [arg0]);
        },
        __wbg___wbindgen_throw_6ddd609b62940d55: function(arg0, arg1) {
            throw new WebAssembly.Exception(__wbindgen_wrapped_jstag, [new Error(getStringFromWasm0(arg0, arg1))]);
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                __wbg_termination_guard();
                try {
                    wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
                } catch(e) {
                    __wbg_handle_catch(e);
                }
            }
        },
        __wbg_getRandomValues_ef8a9e8b447216e2: function(arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        },
        __wbg_getTime_1dad7b5386ddd2d9: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_length_27280eca2d70010e: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_log_524eedafa26daa59: function(arg0) {
            console.log(arg0);
        },
        __wbg_new_0_1dcafdf5e786e876: function() {
            const ret = new Date();
            return ret;
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_new_with_length_3437fa6f550bd3d8: function(arg0) {
            const ret = new Uint32Array(arg0 >>> 0);
            return ret;
        },
        __wbg_new_with_length_81c1c31d4432cb9f: function(arg0) {
            const ret = new Float32Array(arg0 >>> 0);
            return ret;
        },
        __wbg_now_16f0c993d5dd6c27: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_set_1be21701d704e71d: function(arg0, arg1, arg2) {
            arg0.set(getArrayU32FromWasm0(arg1, arg2));
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(F32)) -> NamedExternref("Float32Array")`.
            const ret = getArrayF32FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U32)) -> NamedExternref("Uint32Array")`.
            const ret = getArrayU32FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
        __wbindgen_jstag: WebAssembly.JSTag,
        __wbindgen_wrapped_jstag: __wbindgen_wrapped_jstag,
    };
    return {
        __proto__: null,
        "./wasm_bridge_bg.js": import0,
    };
}

const __wbindgen_wrapped_jstag = new WebAssembly.Tag({ parameters: ['externref'] });


let __wbg_terminated_addr;


function __wbg_termination_guard() {
    __wbg_terminated_addr ??= wasm.__instance_terminated.value / 4;
    if (getInt32ArrayMemory0()[__wbg_terminated_addr]) {
        throw new Error('Module terminated');
    }
}


function __wbg_handle_catch(e) {
    if (e instanceof WebAssembly.Exception && e.is(__wbindgen_wrapped_jstag)) {
        throw e.getArg(__wbindgen_wrapped_jstag, 0);
    }
    getInt32ArrayMemory0()[__wbg_terminated_addr] = 1;
    throw e;
}

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

let cachedInt32ArrayMemory0 = null;
function getInt32ArrayMemory0() {
    if (cachedInt32ArrayMemory0 === null || cachedInt32ArrayMemory0.byteLength === 0) {
        cachedInt32ArrayMemory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachedInt32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint32ArrayMemory0 = null;
function getUint32ArrayMemory0() {
    if (cachedUint32ArrayMemory0 === null || cachedUint32ArrayMemory0.byteLength === 0) {
        cachedUint32ArrayMemory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32ArrayMemory0;
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

class PanicError extends Error {}
Object.defineProperty(PanicError.prototype, 'name', {
    value: PanicError.name,
});

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedInt32ArrayMemory0 = null;
    cachedUint32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('wasm_bridge_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
