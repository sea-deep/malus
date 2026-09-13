# Building the Malus WPE WebKit Runtime

Malus uses a relocatable, customized build of WPE WebKit (`wpewebkit-2.52.6`) with EME (Encrypted Media Extensions), OpenCDM, and Bubblewrap sandboxing enabled.

The entire build process is fully automated and reproducible from a clean repository checkout.

## Quick Start

To build and assemble the complete WPE runtime:

```bash
./scripts/build-wpe-runtime.sh
```

The resulting relocatable runtime is placed at `runtime/wpe/`:
- `runtime/wpe/bin/malus-wpe-host`: Standalone WPE headless browser host
- `runtime/wpe/bin/WPEWebProcess`: Sandboxed WebKit web process
- `runtime/wpe/bin/WPENetworkProcess`: WebKit network process
- `runtime/wpe/lib/libWPEWebKit-2.0.so.1`: WebKit WPE shared library
- `runtime/wpe/lib/libwpe-1.0.so.1`: libwpe library
- `runtime/wpe/lib/libWPEBackend-fdo-1.0.so.1`: WPE FreeDesktop.org backend
- `runtime/wpe/lib/libocdm.so`: Malus OpenCDM Widevine shim

## Build Directory Layout

All build artifacts are generated under `build/wpe/` (ignored by git):
- `build/wpe/downloads/`: Pinned upstream source and helper packages with verified SHA256 hashes
- `build/wpe/sources/`: Extracted WebKit source tree with Malus patches applied
- `build/wpe/prefix/`: Build-time prefix with libwpe, WPEBackend-fdo, OpenCDM headers, and code-generator tools
- `build/wpe/webkit-build/`: CMake/Ninja build directory

## Build Characteristics & Rebuild Behavior

- **Initial Clean Build**:
  - WebKit compile takes ~45-60 minutes on an 8-core CPU (`-j4`).
  - Disk usage: ~800 MB (uses unified sources with `-g0 -O2` and `lld`).
- **Incremental Rebuild**:
  - Re-running `./scripts/build-wpe-runtime.sh` reuses the Ninja build tree and build prefix.
  - If no sources changed, the script validates and completes in under 2 seconds.
- **Clean Modes**:
  - `./scripts/build-wpe-runtime.sh --clean`: Removes sources, prefix, and build tree (preserves downloads).
  - `./scripts/build-wpe-runtime.sh --clean-webkit`: Cleans only the WebKit build directory.
  - `./scripts/build-wpe-runtime.sh --distclean`: Wipes the entire `build/wpe/` directory including cached downloads.

## System Dependencies

The build checks for required dependencies fail-fast before starting:
- **Compilers & Tools**: `clang`, `clang++`, `lld`, `cmake`, `ninja`, `pkg-config`, `tar`, `curl`, `python3`, `ruby`, `perl`, `bison`, `flex`.
- **Development Libraries**: `glib-2.0`, `gio-2.0`, `gmodule-2.0`, `gstreamer-1.0`, `gstreamer-base-1.0`, `gstreamer-app-1.0`, `gstreamer-video-1.0`, `gstreamer-audio-1.0`, `gstreamer-gl-1.0`, `libsoup-3.0`, `epoxy`, `wayland-client`, `wayland-egl`, `wayland-server`, `xkbcommon`, `atk`, `atk-bridge-2.0`, `libseccomp`, `libsystemd`, `libxml-2.0`, `libxslt`, `sqlite3`, `lcms2`, `harfbuzz`, `libgcrypt`, `libavif`, `libjxl`, `libwebp`, `libpng`, `libjpeg`, `zlib`, `freetype2`, `fontconfig`.

## Pinned Versions & Patches

- **Versions**: Defined in [`scripts/wpe-runtime/versions.env`](../scripts/wpe-runtime/versions.env)
  - `wpewebkit`: `2.52.6`
  - `libwpe`: `1.16.3-1`
  - `wpebackend-fdo`: `1.16.1-1`
- **Patches**: Stored in [`crates/malus-web-runtime/native/webkit/patches/`](../crates/malus-web-runtime/native/webkit/patches/)
  - `0001-glib-process-executable-path.patch`: Enables relative process discovery in Release mode.
  - `0002-coordinated-scrolling-guards.patch`: Fixes `-DENABLE_ASYNC_SCROLLING=OFF` compilation.
