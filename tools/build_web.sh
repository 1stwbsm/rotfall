#!/bin/sh
# Builds the browser version as one self-contained file: dist/rotfall-web.html
# Needs rustup and python3. The wasm-bindgen tool is installed once into target/tools.
set -e
cd "$(dirname "$0")/.."
TARGET=wasm32-unknown-unknown
rustup target add $TARGET >/dev/null 2>&1
VERSION=$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | tail -1 | cut -d'"' -f2)
BINDGEN=target/tools/bin/wasm-bindgen
if [ ! -x "$BINDGEN" ] || [ "$("$BINDGEN" --version | cut -d' ' -f2)" != "$VERSION" ]; then
    cargo install wasm-bindgen-cli --version "$VERSION" --root target/tools --locked --force
fi
# Every browser with WebGPU also has WebAssembly SIMD, and the math library benefits from it.
RUSTFLAGS="-C target-feature=+simd128" cargo build --release --target $TARGET "$@"
OUT=target/web
"$BINDGEN" --target no-modules --no-typescript --out-dir $OUT target/$TARGET/release/rotfall.wasm
mkdir -p dist
python3 -I - $OUT tools/web/template.html dist/rotfall-web.html <<'PY'
import base64, gzip, sys
out, template, dest = sys.argv[1:4]
page = open(template).read()
glue = open(out + "/rotfall.js").read()
wasm = open(out + "/rotfall_bg.wasm", "rb").read()
if "</script" in glue:
    sys.exit("the generated JavaScript would end its script tag early")
packed = base64.b64encode(gzip.compress(wasm, 9, mtime=0)).decode()
icon = base64.b64encode(open("assets/icon_80x80.png", "rb").read()).decode()
page = page.replace("/*ICON*/", icon).replace("/*GLUE*/", glue).replace("/*WASM*/", packed)
open(dest, "w").write(page)
print("%s: %.1f MB (WebAssembly %.1f MB before compression)" % (dest, len(page) / 1e6, len(wasm) / 1e6))
PY
