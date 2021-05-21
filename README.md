`wasm-bindgen-rayon` is an adapter for enabling [Rayon](https://github.com/rayon-rs/rayon)-based concurrency on the Web with WebAssembly (via [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen), Web Workers and SharedArrayBuffer support).

### Story.ai adaptation

This is the [Story.ai](https://github.com/storyai) adaptation of the `wasm-bindgen-rayon` crate.
This code is not published to crates.io, and we do not have any intention of this code being contributed
to [the existing `wasm-bindgen-rayon` repo](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon).

This fork and the descriptions of how `wasm-bindgen-rayon` works are based off of the files from [May 2021, v1.0.3, @9d14d78](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/tree/9d14d78124d528b71ba326314845cd2bc84a08bb)

With that on the table, here's what you'll find different about our fork:

 * We've added User Timing measurements to make it easier to observe the sequence of events taken to load the thread workers. (use DevTools Performance panel)
 * We've adapted workerHelpers.js to TypeScript (`workerHelpers.js` can be regenerated with a `npx tsc` call)
 * We've made it possible to create the rayon thread web workers manually
  * As of May 2021, Chromium does not support reporting User Timings originating from nested dedicated workers (web workers created by web workers)
  * In our product,
    * Our main WASM module must be managed from within a dedicated worker to not block input on the main thread
    * `wasm-bindgen-rayon` is set up so `workerHelpers.js` is used to [create `WebWorker`s directly from a call to `startWorkers`](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/blob/9d14d78124d528b71ba326314845cd2bc84a08bb/src/workerHelpers.js#L79-L100)
    * `startWorkers` is [called from `lib.rs` (the wasm context)](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon/blob/9d14d78124d528b71ba326314845cd2bc84a08bb/src/lib.rs#L118-L122)
    * So, since our wasm context is in a web worker, this call to `startWorkers` will result in this sort of nested dedicated workers which seem to have limitations in chromium
    * We've opened [a bug for chromium to get attention on this issue](https://bugs.chromium.org/p/chromium/issues/detail?id=1211924)

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
<!-- param::isNotitle::true:: -->

- [Usage](#usage)
  - [Caveats](#caveats)
  - [Setting up](#setting-up)
  - [Using Rayon](#using-rayon)
  - [Building Rust code](#building-rust-code)
    - [Using config files](#using-config-files)
    - [Using command-line params](#using-command-line-params)
  - [Feature detection](#feature-detection)
  - [Usage with various bundlers](#usage-with-various-bundlers)
    - [Usage with Webpack](#usage-with-webpack)
    - [Usage with Rollup](#usage-with-rollup)
    - [Usage with Parcel](#usage-with-parcel)
    - [Usage without bundlers](#usage-without-bundlers)
- [License](#license)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# Usage

WebAssembly thread support is not yet a first-class citizen in Rust, so there are a few things to keep in mind when using this crate. Bear with me :)

For a quick demo, check out https://rreverser.com/wasm-bindgen-rayon-demo/.

## Caveats

Before we get started, check out caveats listed in the [wasm-bindgen threading docs](https://rustwasm.github.io/wasm-bindgen/examples/raytrace.html). While this library specifically targets Rayon and automatically provides the necessary shims for you, some of the caveats still apply.

Most notably, even when you're using multithreading, the main thread still **can't be blocked** while waiting for the Rayon pool to get ready or for all the background operations to finish.

You must instantiate the main JS+Wasm in a dedicated `Worker` to avoid blocking the main thread - that is, don't mix UI and Rayon code together. Instead, use a library like [Comlink](https://github.com/GoogleChromeLabs/comlink) or a custom glue code to expose required wasm-bindgen methods to the main thread, and do the UI work from there.

Note: Chrome currently does allow blocking on the main thread, but it's a [bug](https://bugs.chromium.org/p/chromium/issues/detail?id=1190951) that's going to be fixed soon.

## Setting up

First of all, in order to use `SharedArrayBuffer` on the Web, you need to enable [cross-origin isolation policies](https://web.dev/coop-coep/). Check out the linked article for details.

First of all, add this crate as a dependency to your `Cargo.toml` (in addition to `wasm-bindgen` and `rayon` themselves):

```toml
[dependencies]
wasm-bindgen = "0.2"
rayon = "1.5"
wasm-bindgen-rayon = "1.0"
```

Then, reexport the `init_thread_pool` function:

```rust
pub use wasm_bindgen_rayon::init_thread_pool;

// ...
```

This will expose an async `initThreadPool` function in the final generated JavaScript for your library.

You'll need to invoke it right after instantiating your module on the main thread in order to prepare the threadpool before calling into actual library functions:

```js
import init, { initThreadPool /* ... */ } from './pkg/index.js';

// Regular wasm-bindgen initialization.
await init();

// Thread pool initialization with the given number of threads
// (pass `navigator.hardwareConcurrency` if you want to use all cores).
await initThreadPool(navigator.hardwareConcurrency);

// ...now you can invoke any exported functions as you normally would
```

## Using Rayon

Use [Rayon](https://github.com/rayon-rs/rayon) iterators as you normally would, e.g.

```rust
#[wasm_bindgen]
pub fn sum(numbers: &[i32]) -> i32 {
    numbers.par_iter().sum()
}
```

will accept an `Int32Array` from JavaScript side and calculate the sum of its values using all available threads.

## Building Rust code

First limitation to note is that you'll have to use `wasm-bindgen`/`wasm-pack`'s `web` target (`--target web`).

<details>
<summary><i>Why?</i></summary>

This is because the Wasm code needs to take its own object (the `WebAssembly.Module`) and share it with other threads when spawning them. This object is only accessible from the `--target web` and `--target no-modules` outputs, but we further restrict it to only `--target web` as we also use [JS snippets feature](https://rustwasm.github.io/wasm-bindgen/reference/js-snippets.html).

</details>

The other issue is that the Rust standard library for the WebAssembly target is built without threads support to ensure maximum portability.

Since we do want standard APIs like [`Mutex`, `Arc` and so on to work](https://doc.rust-lang.org/std/sync/), you'll need to use the nightly compiler toolchain and pass some flags to rebuild the standard library in addition to your own code.

In order to reduce risk of breakages, it's strongly recommended to use a fixed nightly version. For example, the latest stable Rust at the moment of writing is version 1.50, which corresponds to `nightly-2021-02-11`, which was tested and works with this crate.

### Using config files

The easiest way to configure those flags is:

1. Put a string `nightly-2021-02-11` in a `rust-toolchain` file in your project directory. This tells Rustup to use nightly toolchain by default for your project.
2. Put the following in a `.cargo/config` file in your project directory:

   ```toml
   [target.wasm32-unknown-unknown]
   rustflags = ["-C", "target-feature=+atomics,+bulk-memory"]

   [unstable]
   build-std = ["panic_abort", "std"]
   ```

   This tells Cargo to rebuild the standard library with support for Wasm atomics.

Then, run [`wasm-pack`](https://rustwasm.github.io/wasm-pack/book/) as you normally would with `--target web`:

```sh
$ wasm-pack build --target web [...normal wasm-pack params...]
```

### Using command-line params

If you prefer not to configure those parameters by default, you can pass them as part of the build command itself.

In that case, the whole command looks like this:

```sh
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory' \
	rustup run nightly-2021-02-11 \
	wasm-pack build --target web [...] \
	-- -Z build-std=panic_abort,std
```

It looks a bit scary, but it takes care of everything - choosing the nightly toolchain, enabling the required features as well as telling Cargo to rebuild the standard library. You only need to copy it once and hopefully forget about it :)

## Feature detection

[Not all browsers](https://webassembly.org/roadmap/) support WebAssembly threads yet, so you'll likely want to make two builds - one with threads support and one without - and use feature detection to choose the right one on the JavaScript side.

You can use [`wasm-feature-detect`](https://github.com/GoogleChromeLabs/wasm-feature-detect) library for this purpose. The code will look roughly like this:

```js
import { threads } from 'wasm-feature-detect';

let wasmPkg;

if (await threads()) {
  wasmPkg = await import('./pkg-with-threads/index.js');
  await wasmPkg.default();
  await wasmPkg.initThreadPool(navigator.hardwareConcurrency);
} else {
  wasmPkg = await import('./pkg-without-threads/index.js');
  await wasmPkg.default();
}

wasmPkg.nowCallAnyExportedFuncs();
```

## Usage with various bundlers

WebAssembly threads use Web Workers under the hood for instantiating other threads with the same WebAssembly module & memory.

wasm-bindgen-rayon provides the required JS code for those Workers internally, and uses a syntax that is recognised across various bundlers.

### Usage with Webpack

If you're using Webpack v5 (version >= 5.25.1), you don't need to do anything special, as it already supports [bundling Workers](https://webpack.js.org/guides/web-workers/) out of the box.

### Usage with Rollup

For Rollup, you'll need [`@surma/rollup-plugin-off-main-thread`](https://github.com/surma/rollup-plugin-off-main-thread) plugin (version >= 2.1.0) which brings the same functionality and was tested with this crate.

### Usage with Parcel

_[Coming soon...]_ Parcel v2 also recognises the used syntax, but it's still in development and there are some minor issues to fix before it can be used with this crate.

### Usage without bundlers

The default JS glue was designed in a way that works great with bundlers and code-splitting, but, sadly, not yet in browsers due to different treatment of import paths (see [`WICG/import-maps#244`](https://github.com/WICG/import-maps/issues/244) which might help unify those in the future).

If you want to build this library for usage without bundlers, enable the `no-bundler` feature for `wasm-bindgen-rayon` in your `Cargo.toml`:

```toml
wasm-bindgen-rayon = { version = "1.0", features = ["no-bundler"] }
```

Note that, in addition to the earlier mentioned restrictions, this will work only in browsers with [support for Module Workers](https://caniuse.com/mdn-api_worker_worker_ecmascript_modules) (when using bundlers, those are bundled to regular Workers automatically).

# License

This crate is licensed under the Apache-2.0 license.
