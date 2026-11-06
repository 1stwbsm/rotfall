#!/bin/sh
# Cross-compiles the Windows build from Linux and packages it as dist/rotfall-windows-x64.zip.
# Needs: rustup target x86_64-pc-windows-gnu, zig, python3.
set -e
cd "$(dirname "$0")/.."
ROOT=$PWD
WRAP=$ROOT/target/zig-wrap
mkdir -p "$WRAP"
# rustc passes GCC-flavoured flags that zig's linker driver rejects, so translate them.
cat > "$WRAP/cc" <<'SH'
#!/bin/sh
args=""
for a in "$@"; do
  case "$a" in
    # zig ships its own C runtime and unwinder under different names.
    -lgcc_eh|-lgcc_s) args="$args -lunwind" ;;
    -lgcc|-lmsvcrt|-lmingwex|-lmingw32|--target=*) ;;
    -l:libpthread.a) ;;
    # Use the copy made below, which leaves ___chkstk_ms to zig's runtime.
    */libcompiler_builtins-*.rlib) args="$args '$(dirname "$0")/libcompiler_builtins-patched.rlib'" ;;
    # Link mode switches stop zig from finding the plain .a import libraries in the crates.
    -Wl,-Bdynamic|-Wl,-Bstatic) ;;
    -Wl,--disable-auto-image-base|-Wl,--enable-auto-image-base|-Wl,--dynamicbase|-Wl,--nxcompat|-Wl,--large-address-aware|-Wl,--high-entropy-va) ;;
    *) args="$args '$(printf %s "$a" | sed "s/'/'\\\\''/g")'" ;;
  esac
done
eval exec zig cc -target x86_64-windows-gnu "$args"
SH
# Rust's runtime and zig's both define ___chkstk_ms, and zig offers no way to tolerate that. Both are
# the same stack probe, so a private copy of Rust's library gets its definition renamed out of the
# way. The new name has the same length, so nothing in the file moves. The toolchain is not touched.
python3 -I - "$(rustc --print sysroot)" "$WRAP/libcompiler_builtins-patched.rlib" <<'PY'
import glob, sys
sysroot, out = sys.argv[1], sys.argv[2]
libs = glob.glob(sysroot + "/lib/rustlib/x86_64-pc-windows-gnu/lib/libcompiler_builtins-*.rlib")
if len(libs) != 1:
    sys.exit("expected one compiler_builtins library, found %d" % len(libs))
data = open(libs[0], "rb").read()
if b"___chkstk_ms" not in data:
    sys.exit("___chkstk_ms not found: this workaround may no longer be needed")
open(out, "wb").write(data.replace(b"___chkstk_ms", b"___chkstk_rs"))
PY
printf '#!/bin/sh\nexec zig dlltool "$@"\n' > "$WRAP/dlltool"
printf '#!/bin/sh\nexec zig ar "$@"\n' > "$WRAP/ar"
chmod +x "$WRAP"/*
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$WRAP/cc"
export RUSTFLAGS="-C dlltool=$WRAP/dlltool -C link-self-contained=no"
cargo build --release --target x86_64-pc-windows-gnu "$@"

DIST=dist/rotfall-windows-x64
rm -rf "$DIST" "$DIST.zip"
mkdir -p "$DIST"
cp target/x86_64-pc-windows-gnu/release/rotfall.exe "$DIST/rotfall.exe"
cp LICENSE tools/windows/README.txt tools/windows/start-server.bat "$DIST/"
python3 -I - "$DIST" <<'PY'
import os, sys, zipfile
root = sys.argv[1]
with zipfile.ZipFile(root + ".zip", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for name in sorted(os.listdir(root)):
        z.write(os.path.join(root, name), os.path.join(os.path.basename(root), name))
PY
ls -la "$DIST.zip"
