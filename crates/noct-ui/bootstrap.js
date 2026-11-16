/* global window, document, URL, dispatchEvent, CustomEvent */
// Starts the WebAssembly application. Loaded by index.html as an external classic script, so the shell's
// Content-Security-Policy can keep `script-src 'self' 'wasm-unsafe-eval'` without `'unsafe-inline'`.
// Trunk's default bootstrap is an inline module with the same steps: import the generated JavaScript,
// initialize the WebAssembly module, publish the bindings, and announce the start.
(async () => {
  // Read the script element before the first await: document.currentScript is only set synchronously.
  const script = document.currentScript;
  const directory = new URL('.', script.src);
  const bindings = await import(new URL(script.dataset.js, directory).href);
  const wasm = await bindings.default({ module_or_path: new URL(script.dataset.wasm, directory).href });
  window.wasmBindings = bindings;
  dispatchEvent(new CustomEvent('TrunkApplicationStarted', { detail: { wasm } }));
})();
