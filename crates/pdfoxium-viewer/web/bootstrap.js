import init, { start, upload_file, repaint } from './pdfoxium_viewer.js';

const canvas = document.querySelector('#viewer');
const picker = document.querySelector('#open-pdf');
const error = document.querySelector('#error');

function fail(reason) {
  error.textContent = String(reason?.stack || reason);
  error.hidden = false;
  console.error(reason);
}

try {
  if (!globalThis.crossOriginIsolated || typeof SharedArrayBuffer === 'undefined') {
    throw new Error('This viewer requires SharedArrayBuffer. Serve it over HTTPS (or localhost) with Cross-Origin-Opener-Policy: same-origin and Cross-Origin-Embedder-Policy: require-corp.');
  }
  await init();
  const shimUrl = new URL('./pdfoxium_viewer.js', import.meta.url);
  const response = await fetch(shimUrl);
  if (!response.ok) throw new Error(`Cannot load worker runtime: HTTP ${response.status}`);
  // The dispatcher instantiates the same generated module in Blob workers.
  // Resolve wasm-bindgen snippet imports against the actual asset directory.
  const source = (await response.text()).replace(
    /(from\s*['"])(\.\/[^'"]+)(['"])/g,
    (_, prefix, path, suffix) => prefix + new URL(path, shimUrl).href + suffix,
  );
  await start(canvas, source);
  canvas.dataset.ready = "true";
  setInterval(repaint, 16);
  picker.addEventListener('change', async () => {
    const file = picker.files?.[0];
    if (!file) return;
    try { upload_file(picker.dataset.purpose || "open", file.name, new Uint8Array(await file.arrayBuffer())); }
    catch (reason) { fail(reason); }
  });
  // eframe handles canvas file drops and supplies in-memory bytes to the app.
  // Prevent the browser from navigating to a dropped file outside the canvas.
  window.addEventListener('dragover', event => event.preventDefault());
  window.addEventListener('drop', event => event.preventDefault());
} catch (reason) {
  fail(reason);
}
