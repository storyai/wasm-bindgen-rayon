/// <reference path="../types.d.ts"/>
//#region utils
/** Random module ID to distinguish measurements */
let usedAs = new Array();
const moduleID = Math.random().toString(36).slice(2, 6);
function waitForMsgType(target, type) {
    return new Promise((resolve) => {
        target.addEventListener("message", function onMsg({ data }) {
            if (data == null || data.type !== type)
                return;
            target.removeEventListener("message", onMsg);
            resolve(data);
        });
    });
}
function postResponseMessage(message) {
    // @ts-ignore do not need to specify target origin
    postMessage(message);
}
let lastMarkID = 0;
// @internal
export function measure(name) {
    const markName = `wbgr-${moduleID}-${++lastMarkID}`;
    performance.mark(markName);
    return () => {
        performance.measure(`|${name}| (wbg-rayon) [${usedAs.join("; ")} #${moduleID}]`, markName);
    };
}
// @internal
export function exitMeasure(exitFn) {
    exitFn();
}
//#endregion utils
//#region rayonThreadWorker
// Relevant when the workerHelpers.js file is loaded as a Web Worker,
// otherwise, it does not have any effect when workerHelpers.js file is imported in other places.
// Note: we use `wasm_bindgen_worker_`-prefixed message types to make sure
// we can handle bundling into other files, which might happen to have their
// own `postMessage`/`onmessage` communication channels.
//
// If we didn't take that into the account, we could send much simpler signals
// like just `0` or whatever, but the code would be less resilient.
/** This will only be exited if this workerHelpers.js was loaded as a web worker */
const exitWorkerInit = measure(`worker received "wasm_bindgen_worker_init" message`);
waitForMsgType(self, "wasm_bindgen_worker_init").then(async (data) => {
    usedAs.push("thread");
    exitWorkerInit();
    const exitReady = measure(`worker thread ready; will post "wasm_bindgen_worker_ready" and then start the worker (which blocks the thread)`);
    // # Note 1
    // Our JS should have been generated in
    // `[out-dir]/snippets/wasm-bindgen-rayon-[hash]/workerHelpers.js`,
    // resolve the main module via `../../..`.
    //
    // This might need updating if the generated structure changes on wasm-bindgen
    // side ever in the future, but works well with bundlers today. The whole
    // point of this crate, after all, is to abstract away unstable features
    // and temporary bugs so that you don't need to deal with them in your code.
    //
    // # Note 2
    // This could be a regular import, but then some bundlers complain about
    // circular deps.
    //
    // Dynamic import could be cheap if this file was inlined into the parent,
    // which would require us just using `../../..` in `new Worker` below,
    // but that doesn't work because wasm-pack unconditionally adds
    // "sideEffects":false (see below).
    //
    // OTOH, even though it can't be inlined, it should be still reasonably
    // cheap since the requested file is already in cache (it was loaded by
    // the main thread).
    // @ts-ignore import will be there at `../../../[wasmName].js`
    const pkg = await import("../../..");
    await pkg.default(data.module, data.memory);
    exitReady();
    postResponseMessage({ type: "wasm_bindgen_worker_ready" });
    // this call blocks here
    try {
        pkg.wbg_rayon_start_worker(data.receiver);
    }
    catch (err) {
        // surface panics occuring in the thread
        // this is contingent on calling `wasm_bindgen::throw_str("panic");` from the WASM side
        postResponseMessage({ type: "wasm_bindgen_worker_panic", message: err.toString() });
    }
});
//#endregion rayonThreadWorker
//#region createWorkerInitMessage
// Relevant when the workerHelpers.js file is imported via lib.rs
// and used to call createWorkerInitMessage via wasmbindgen.
// @internal
export async function createWorkerInitMessage(module, memory, builder) {
    usedAs.push("wasm-helper");
    const exit = measure("wasm called createWorkerInitMessage");
    const value = {
        builder,
        // The module url of this file is the one that we need to load in order to
        // @ts-ignore import.meta should be available because this is loaded as a module
        workerScriptHref: import.meta.url,
        message: {
            type: "wasm_bindgen_worker_init",
            module,
            memory,
            receiver: builder.receiver(),
        },
    };
    exit();
    return value;
}
//#endregion createWorkerInitMessage
