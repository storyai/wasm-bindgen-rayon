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
