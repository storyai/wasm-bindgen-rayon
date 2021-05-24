declare interface WasmBindgenRayonPoolBuilder {
  ptr: number
  receiver(): number
}
declare type WasmBindgenRayonWorkerInitMessage = {
  type: "wasm_bindgen_worker_init"
  module: WebAssembly.Module
  memory: WebAssembly.Memory
  receiver: number
}
declare type WasmBindgenRayonWorkerResponseReadyMessage = {
  type: "wasm_bindgen_worker_ready"
}
declare type WasmBindgenRayonWorkerResponsePanicMessage = {
  type: "wasm_bindgen_worker_panic"
  message: string
}
