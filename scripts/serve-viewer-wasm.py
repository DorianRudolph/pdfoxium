#!/usr/bin/env python3
"""Serve the built viewer with the isolation headers required by WASM threads."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class Handler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--directory", type=Path, default=Path(__file__).resolve().parents[1] / "target/viewer-wasm")
    args = parser.parse_args()
    if not (args.directory / "index.html").is_file():
        parser.error("viewer build is missing; run scripts/build-viewer-wasm.sh first")
    with ThreadingHTTPServer((args.bind, args.port), partial(Handler, directory=str(args.directory))) as server:
        print(f"Viewer: http://{args.bind}:{server.server_port}/", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
