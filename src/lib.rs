#![cfg_attr(
    feature = "nightly",
    feature(external_doc),
    doc(include = "../README.md")
)]
#![cfg_attr(
    not(feature = "nightly"),
    doc = "Check out documentation in [README.md](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon)."
)]

// Note: `atomics` is whitelisted in `target_feature` detection, but `bulk-memory` isn't,
// so we can check only presence of the former. This should be enough to catch most common
// mistake (forgetting to pass `RUSTFLAGS` altogether).
#[cfg(not(target_feature = "atomics"))]
compile_error!("Did you forget to enable `atomics` and `bulk-memory` features as outlined in wasm-bindgen-rayon README?");

/**
 * Copyright 2021 Google Inc. All Rights Reserved.
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
use spmc::{channel, Receiver, Sender};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;

#[cfg(feature = "no-bundler")]
use js_sys::JsString;

// Naming is a workaround for https://github.com/rustwasm/wasm-bindgen/issues/2429
// and https://github.com/rustwasm/wasm-bindgen/issues/1762.
#[allow(non_camel_case_types)]
#[wasm_bindgen]
#[doc(hidden)]
pub struct wbg_rayon_PoolBuilder {
    num_threads: usize,
    sender: Sender<rayon::ThreadBuilder>,
    receiver: Receiver<rayon::ThreadBuilder>,
}

#[wasm_bindgen(module = "/src/workerHelpers.js")]
extern "C" {
    /// Identity function to create a JavaScript value with module, memory, builder attrs
    #[wasm_bindgen(js_name = createWorkerInitMessage)]
    fn js_create_worker_init_message(
        module: JsValue,
        memory: JsValue,
        builder: wbg_rayon_PoolBuilder,
    ) -> JsValue;
}

mod span {
    //! Basic [Drop] oriented measurements using the imported js measure functions.
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "/src/workerHelpers.js")]
    extern "C" {
        /// Measure function to make it easier to understand the order which things are loading
        /// Returns an exit function which should be passed to [exit_measure] to finish
        #[wasm_bindgen(js_name = measure)]
        fn js_measure(name: &str) -> JsValue;
        /// Exits a measurement by calling the function returned from [measure]
        #[wasm_bindgen(js_name = exitMeasure)]
        fn js_exit_measure(exit_fn: JsValue);
    }
    pub(super) struct MeasureSpan(Option<wasm_bindgen::JsValue>);
    impl Drop for MeasureSpan {
        fn drop(&mut self) {
            if let Some(exit_fn) = self.0.take() {
                js_exit_measure(exit_fn);
            }
        }
    }
    pub(super) fn measure(name: &str) -> MeasureSpan {
        MeasureSpan(Some(js_measure(name)))
    }
}

#[wasm_bindgen]
impl wbg_rayon_PoolBuilder {
    fn new(num_threads: usize) -> Self {
        let _exit = span::measure("PoolBuilder::new");
        let (sender, receiver) = channel();
        Self {
            num_threads,
            sender,
            receiver,
        }
    }

    #[cfg(feature = "no-bundler")]
    #[wasm_bindgen(js_name = mainJS)]
    pub fn main_js(&self) -> JsString {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = ["import", "meta"], js_name = url)]
            static URL: JsString;
        }

        URL.clone()
    }

    #[wasm_bindgen(js_name = numThreads)]
    pub fn num_threads(&self) -> usize {
        self.num_threads
    }

    pub fn receiver(&self) -> *const Receiver<rayon::ThreadBuilder> {
        &self.receiver
    }

    // This should be called by the JS side once all the Workers are spawned.
    // Important: it must take `self` by reference, otherwise
    // `start_worker_thread` will try to receive a message on a moved value.
    pub fn build(&mut self) {
        let _exit = span::measure("PoolBuilder::build");
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            // We could use postMessage here instead of Rust channels,
            // but currently we can't due to a Chrome bug that will cause
            // the main thread to lock up before it even sends the message:
            // https://bugs.chromium.org/p/chromium/issues/detail?id=1075645
            .spawn_handler(move |thread| {
                // Note: `send` will return an error if there are no receivers.
                // We can use it because all the threads are spawned and ready to accept
                // messages by the time we call `build()` to instantiate spawn handler.
                self.sender.send(thread).unwrap_throw();
                Ok(())
            })
            .build_global()
            .unwrap_throw();
    }
}

#[wasm_bindgen(js_name = buildThreadPool)]
#[doc(hidden)]
pub fn build_thread_pool(builder: &mut wbg_rayon_PoolBuilder) {
    let _exit = span::measure("build_thread_pool");
    builder.build();
}

#[wasm_bindgen(js_name = manualThreadWorkerInitMessage)]
#[doc(hidden)]
pub fn manual_thread_worker_init_message(num_threads: usize) -> JsValue {
    let _exit = span::measure("manual_thread_worker_init_message");
    js_create_worker_init_message(
        wasm_bindgen::module(),
        wasm_bindgen::memory(),
        wbg_rayon_PoolBuilder::new(num_threads),
    )
}

#[wasm_bindgen]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[doc(hidden)]
/// Executes the main loop for the received thread. This will not return until the thread pool is dropped.
pub fn wbg_rayon_start_worker(receiver: *const Receiver<rayon::ThreadBuilder>)
where
    // Statically assert that it's safe to accept `Receiver` from another thread.
    Receiver<rayon::ThreadBuilder>: Sync,
{
    let exit =
        span::measure("wbg_rayon_start_worker() received thread builder and will start blocking");
    // This is safe, because we know it came from a reference to PoolBuilder,
    // allocated on the heap by wasm-bindgen and dropped only once all the
    // threads are running.
    //
    // The only way to violate safety is if someone externally calls
    // `exports.wbg_rayon_start_worker(garbageValue)`, but then no Rust tools
    // would prevent us from issues anyway.
    let receiver = unsafe { &*receiver };
    // Wait for a task (`ThreadBuilder`) on the channel, and, once received,
    // start executing it.
    let thread_builder = receiver.recv().unwrap_throw();
    drop(exit);
    // On practice this will start running Rayon's internal event loop.
    thread_builder.run()
}
