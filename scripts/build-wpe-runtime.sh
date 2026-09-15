#!/usr/bin/env bash
#
# build-wpe-runtime.sh: Unified, reproducible build and assembly for Malus WPE runtime.
#
# Reconstructs the complete relocatable WPE WebKit stack from clean checkout:
#   1. Verifies host toolchain and library dependencies fail-fast
#   2. Fetches pinned upstream releases & helper packages with SHA256 integrity checks
#   3. Prepares local build prefix (libwpe, WPEBackend-fdo, OpenCDM headers, ruby shim)
#   4. Applies minimal Malus patches to WebKit source
#   5. Configures WebKit with exact proven features
#   6. Builds only required targets (WPEWebProcess, WPENetworkProcess, WPEToolingBackends)
#   7. Compiles malus-wpe-host and libocdm.so
#   8. Assembles relocatable runtime/wpe/ with relative $ORIGIN RPATHs
#   9. Validates all output binaries and dynamic sections
#
# Usage:
#   ./scripts/build-wpe-runtime.sh [OPTIONS]
#
# Options:
#   -j, --jobs <N>         Number of parallel build jobs (default: 4)
#   -o, --output <DIR>     Output directory for assembled runtime (default: runtime/wpe)
#   --clean                Clean build tree (preserves downloaded archives in downloads/)
#   --clean-webkit         Clean only the WebKit build directory
#   --distclean            Clean everything including downloaded archives
#   --help                 Display this help message
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Pinned versions manifest
VERSIONS_FILE="${REPO_ROOT}/scripts/wpe-runtime/versions.env"
if [[ ! -f "${VERSIONS_FILE}" ]]; then
    echo "ERROR: Versions manifest missing at ${VERSIONS_FILE}" >&2
    exit 1
fi
# shellcheck source=scripts/wpe-runtime/versions.env
source "${VERSIONS_FILE}"

# Patch directory
PATCH_DIR="${REPO_ROOT}/crates/malus-wpe/native/webkit/patches"

# Build root locations (under build/wpe, gitignored)
BUILD_ROOT="${REPO_ROOT}/build/wpe"
DOWNLOAD_DIR="${BUILD_ROOT}/downloads"
SOURCES_DIR="${BUILD_ROOT}/sources"
PREFIX_DIR="${BUILD_ROOT}/prefix"
WEBKIT_BUILD_DIR="${BUILD_ROOT}/webkit-build"

# Default configuration
JOBS=4
OUTPUT_DIR="${REPO_ROOT}/runtime/wpe"
DO_CLEAN=0
DO_CLEAN_WEBKIT=0
DO_DISTCLEAN=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        -j|--jobs)
            JOBS="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --clean)
            DO_CLEAN=1
            shift
            ;;
        --clean-webkit)
            DO_CLEAN_WEBKIT=1
            shift
            ;;
        --distclean)
            DO_DISTCLEAN=1
            shift
            ;;
        --help|-h)
            sed -n '2,/^set -euo/p' "$0" | sed 's/^#//' | sed '$d'
            exit 0
            ;;
        *)
            # Backward compatibility: positional argument for output dir
            OUTPUT_DIR="$1"
            shift
            ;;
    esac
done

if [[ "${DO_DISTCLEAN}" -eq 1 ]]; then
    echo "Cleaning all build artifacts and downloads at ${BUILD_ROOT}..."
    rm -rf "${BUILD_ROOT}"
    echo "Distclean complete."
    exit 0
fi

if [[ "${DO_CLEAN}" -eq 1 ]]; then
    echo "Cleaning build artifacts at ${BUILD_ROOT} (preserving downloads)..."
    rm -rf "${SOURCES_DIR}" "${PREFIX_DIR}" "${WEBKIT_BUILD_DIR}"
    echo "Clean complete."
    exit 0
fi

if [[ "${DO_CLEAN_WEBKIT}" -eq 1 ]]; then
    echo "Cleaning WebKit build directory at ${WEBKIT_BUILD_DIR}..."
    rm -rf "${WEBKIT_BUILD_DIR}"
    echo "WebKit build clean complete."
    exit 0
fi

echo "=================================================="
echo "Malus WPE Runtime Builder"
echo "  Source Root:   ${REPO_ROOT}"
echo "  Build Root:    ${BUILD_ROOT}"
echo "  Target Output: ${OUTPUT_DIR}"
echo "  Parallel Jobs: ${JOBS}"
echo "=================================================="

# ================================================================
# STEP 1: FAST-FAIL DEPENDENCY VERIFICATION
# ================================================================
echo "[1/8] Verifying host toolchain and library dependencies..."

REQUIRED_TOOLS=(
    clang
    clang++
    lld
    cmake
    ninja
    pkg-config
    tar
    curl
    python3
    ruby
    perl
    bison
    flex
)

MISSING_TOOLS=()
for tool in "${REQUIRED_TOOLS[@]}"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        MISSING_TOOLS+=("$tool")
    fi
done

REQUIRED_LIBS=(
    glib-2.0
    gio-2.0
    gmodule-2.0
    gstreamer-1.0
    gstreamer-base-1.0
    gstreamer-app-1.0
    gstreamer-video-1.0
    gstreamer-audio-1.0
    gstreamer-gl-1.0
    libsoup-3.0
    epoxy
    wayland-client
    wayland-egl
    wayland-server
    xkbcommon
    atk
    atk-bridge-2.0
    libseccomp
    libsystemd
    libxml-2.0
    libxslt
    sqlite3
    lcms2
    harfbuzz
    libgcrypt
    libavif
    libjxl
    libwebp
    libpng
    libjpeg
    zlib
    freetype2
    fontconfig
)

MISSING_LIBS=()
for lib in "${REQUIRED_LIBS[@]}"; do
    if ! pkg-config --exists "$lib" 2>/dev/null; then
        MISSING_LIBS+=("$lib")
    fi
done

if [[ ${#MISSING_TOOLS[@]} -gt 0 || ${#MISSING_LIBS[@]} -gt 0 ]]; then
    echo "ERROR: Missing required system dependencies for building WPE WebKit:" >&2
    if [[ ${#MISSING_TOOLS[@]} -gt 0 ]]; then
        echo "  Missing tools:   ${MISSING_TOOLS[*]}" >&2
    fi
    if [[ ${#MISSING_LIBS[@]} -gt 0 ]]; then
        echo "  Missing libraries (pkg-config): ${MISSING_LIBS[*]}" >&2
    fi
    exit 1
fi
echo "  All host tools and development libraries present."

install_if_different() {
    local src="$1"
    local dst="$2"
    if [[ ! -f "${dst}" ]] || ! cmp -s "${src}" "${dst}"; then
        mkdir -p "$(dirname "${dst}")"
        cp -f "${src}" "${dst}"
    fi
}

write_if_different() {
    local dst="$1"
    local tmp="${dst}.tmp.$$"
    mkdir -p "$(dirname "${dst}")"
    cat > "${tmp}"
    if [[ ! -f "${dst}" ]] || ! cmp -s "${tmp}" "${dst}"; then
        mv -f "${tmp}" "${dst}"
    else
        rm -f "${tmp}"
    fi
}

needs_rebuild() {
    local target="$1"
    shift
    if [[ ! -f "${target}" ]]; then
        return 0
    fi
    for src in "$@"; do
        if [[ "${src}" -nt "${target}" ]]; then
            return 0
        fi
    done
    return 1
}

# ================================================================
# STEP 2: DOWNLOAD & VERIFY PINNED UPSTREAM ARCHIVES
# ================================================================
echo "[2/8] Fetching pinned upstream archives..."
mkdir -p "${DOWNLOAD_DIR}"

fetch_and_verify() {
    local name="$1"
    local url="$2"
    local expected_sha256="$3"
    local target_file="${DOWNLOAD_DIR}/$(basename "${url}")"

    if [[ -f "${target_file}" ]]; then
        local actual_sha256
        actual_sha256=$(sha256sum "${target_file}" | awk '{print $1}')
        if [[ "${actual_sha256}" == "${expected_sha256}" ]]; then
            return 0
        fi
        echo "  Checksum mismatch for cached ${name}; re-downloading..."
        rm -f "${target_file}"
    fi

    echo "  Downloading ${name} from ${url}..."
    curl -fSL --retry 3 "${url}" -o "${target_file}.tmp"
    local downloaded_sha256
    downloaded_sha256=$(sha256sum "${target_file}.tmp" | awk '{print $1}')
    if [[ "${downloaded_sha256}" != "${expected_sha256}" ]]; then
        echo "ERROR: SHA256 checksum verification failed for ${name}!" >&2
        echo "  Expected: ${expected_sha256}" >&2
        echo "  Actual:   ${downloaded_sha256}" >&2
        rm -f "${target_file}.tmp"
        exit 1
    fi
    mv "${target_file}.tmp" "${target_file}"
    echo "  Verified ${name} (${expected_sha256:0:16}...)"
}

fetch_and_verify "wpewebkit-${WEBKIT_VERSION}" "${WEBKIT_URL}" "${WEBKIT_SHA256}"
fetch_and_verify "libwpe-${LIBWPE_VERSION}" "${LIBWPE_URL}" "${LIBWPE_SHA256}"
fetch_and_verify "wpebackend-fdo-${WPEBACKEND_FDO_VERSION}" "${WPEBACKEND_FDO_URL}" "${WPEBACKEND_FDO_SHA256}"
fetch_and_verify "gperf-${GPERF_VERSION}" "${GPERF_URL}" "${GPERF_SHA256}"
fetch_and_verify "unifdef-${UNIFDEF_VERSION}" "${UNIFDEF_URL}" "${UNIFDEF_SHA256}"
fetch_and_verify "ruby-erb-${RUBY_ERB_VERSION}" "${RUBY_ERB_URL}" "${RUBY_ERB_SHA256}"
fetch_and_verify "ruby-getoptlong-${RUBY_GETOPTLONG_VERSION}" "${RUBY_GETOPTLONG_URL}" "${RUBY_GETOPTLONG_SHA256}"
fetch_and_verify "ruby-base64-${RUBY_BASE64_VERSION}" "${RUBY_BASE64_URL}" "${RUBY_BASE64_SHA256}"
fetch_and_verify "ruby-mutex_m-${RUBY_MUTEX_M_VERSION}" "${RUBY_MUTEX_M_URL}" "${RUBY_MUTEX_M_SHA256}"

# ================================================================
# STEP 3: PREPARE LOCAL BUILD PREFIX
# ================================================================
echo "[3/8] Preparing local build prefix..."
mkdir -p "${PREFIX_DIR}/bin" "${PREFIX_DIR}/lib/pkgconfig" "${PREFIX_DIR}/usr/lib/pkgconfig" "${PREFIX_DIR}/include/opencdm"

PREFIX_STAMP="${PREFIX_DIR}/.prefix_initialized"
if [[ ! -f "${PREFIX_STAMP}" ]]; then
    # Unpack prebuilt helpers and libraries into prefix
    echo "  Unpacking helper packages into ${PREFIX_DIR}..."
    for pkg in libwpe wpebackend-fdo gperf unifdef ruby-erb ruby-getoptlong ruby-base64 ruby-mutex_m; do
        PKG_FILE=$(find "${DOWNLOAD_DIR}" -name "${pkg}-*.pkg.tar.zst")
        tar --zstd -xf "${PKG_FILE}" -C "${PREFIX_DIR}/"
    done

    # Symlink tools into prefix/bin
    ln -sf "${PREFIX_DIR}/usr/bin/gperf" "${PREFIX_DIR}/bin/gperf"
    ln -sf "${PREFIX_DIR}/usr/bin/unifdef" "${PREFIX_DIR}/bin/unifdef"

    # Adjust pkgconfig prefixes
    sed -i "s|^prefix=/usr|prefix=${PREFIX_DIR}/usr|" "${PREFIX_DIR}"/usr/lib/pkgconfig/*.pc

    # Create ruby wrapper with complete gem library paths for Ruby 3.4
    cat << EOF > "${PREFIX_DIR}/bin/ruby"
#!/bin/sh
export RUBYLIB="${PREFIX_DIR}/usr/lib/ruby/3.4.0:${PREFIX_DIR}/usr/lib/ruby/gems/3.4.0/gems/getoptlong-0.2.1/lib:${PREFIX_DIR}/usr/lib/ruby/gems/3.4.0/gems/base64-0.3.0/lib:${PREFIX_DIR}/usr/lib/ruby/gems/3.4.0/gems/mutex_m-0.3.0/lib:\${RUBYLIB:-}"
exec /usr/sbin/ruby "\$@"
EOF
    chmod +x "${PREFIX_DIR}/bin/ruby"

    touch "${PREFIX_STAMP}"
fi

# Install Malus OpenCDM headers into prefix (only if different to preserve timestamps)
for h in "${REPO_ROOT}/crates/malus-wpe/native/opencdm/include/opencdm/"*.h; do
    install_if_different "$h" "${PREFIX_DIR}/include/opencdm/$(basename "$h")"
done

# Build OpenCDM shim inside prefix only if source or headers are newer
OPENCDM_SRCS=(
    "${REPO_ROOT}/crates/malus-wpe/native/opencdm/src/opencdm_shim.cpp"
    "${REPO_ROOT}/crates/malus-wpe/native/opencdm/include/opencdm/"*.h
)
if needs_rebuild "${PREFIX_DIR}/lib/libocdm.so" "${OPENCDM_SRCS[@]}"; then
    echo "  Compiling libocdm.so inside build prefix..."
    g++ -O2 -fPIC -shared -std=c++14 \
        -I"${REPO_ROOT}/crates/malus-wpe/native/opencdm/include" \
        -I"${REPO_ROOT}/crates/malus-wpe/native/opencdm/include/cdm" \
        $(pkg-config --cflags gstreamer-1.0 gstreamer-base-1.0) \
        "${REPO_ROOT}/crates/malus-wpe/native/opencdm/src/opencdm_shim.cpp" \
        $(pkg-config --libs gstreamer-1.0 gstreamer-base-1.0) \
        -ldl \
        -o "${PREFIX_DIR}/lib/libocdm.so"
else
    echo "  Prefix libocdm.so is up to date."
fi

# Create thunder.pc so WebKit finds OpenCDM (only if content changed)
cat << EOF | write_if_different "${PREFIX_DIR}/usr/lib/pkgconfig/thunder.pc"
prefix=${PREFIX_DIR}
includedir=\${prefix}/include/opencdm
libdir=\${prefix}/lib

Name: thunder
Description: Minimal Thunder OpenCDM Shim
Version: 1.0.0
Cflags: -I\${includedir}
Libs: -L\${libdir} -locdm
EOF
install_if_different "${PREFIX_DIR}/usr/lib/pkgconfig/thunder.pc" "${PREFIX_DIR}/lib/pkgconfig/thunder.pc"


# ================================================================
# STEP 4: EXTRACT WEBKIT & APPLY MALUS PATCHES
# ================================================================
echo "[4/8] Extracting WebKit and applying patches..."
mkdir -p "${SOURCES_DIR}"
WEBKIT_SRC_DIR="${SOURCES_DIR}/wpewebkit-${WEBKIT_VERSION}"

if [[ ! -d "${WEBKIT_SRC_DIR}" ]]; then
    echo "  Extracting wpewebkit-${WEBKIT_VERSION}.tar.xz..."
    tar -xf "${DOWNLOAD_DIR}/wpewebkit-${WEBKIT_VERSION}.tar.xz" -C "${SOURCES_DIR}/"
fi

PATCH_STAMP="${WEBKIT_SRC_DIR}/.malus_patches_applied"
if [[ ! -f "${PATCH_STAMP}" ]]; then
    echo "  Applying Malus patches to WebKit source..."
    for patch in "${PATCH_DIR}"/*.patch; do
        if [[ -f "${patch}" ]]; then
            echo "    Applying $(basename "${patch}")..."
            patch -d "${WEBKIT_SRC_DIR}" -p1 < "${patch}"
        fi
    done
    touch "${PATCH_STAMP}"
else
    echo "  Patches already applied."
fi

# ================================================================
# STEP 5: CONFIGURE WEBKIT WITH CMAKE
# ================================================================
echo "[5/8] Configuring WebKit..."
mkdir -p "${WEBKIT_BUILD_DIR}"

export PATH="${PREFIX_DIR}/bin:${PATH}"
export PKG_CONFIG_PATH="${PREFIX_DIR}/usr/lib/pkgconfig:${PREFIX_DIR}/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
export CC=clang
export CXX=clang++

if [[ ! -f "${WEBKIT_BUILD_DIR}/build.ninja" ]]; then
    echo "  Running CMake configure for WPE WebKit..."
    cmake -S "${WEBKIT_SRC_DIR}" -B "${WEBKIT_BUILD_DIR}" -G Ninja \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_PREFIX_PATH="${PREFIX_DIR};${PREFIX_DIR}/usr" \
      -DGPERF_EXECUTABLE="${PREFIX_DIR}/bin/gperf" \
      -DUNIFDEF_EXECUTABLE="${PREFIX_DIR}/bin/unifdef" \
      -DRuby_EXECUTABLE="${PREFIX_DIR}/bin/ruby" \
      -DTHUNDER_INCLUDE_DIR="${PREFIX_DIR}/include/opencdm" \
      -DTHUNDER_LIBRARY="${PREFIX_DIR}/lib/libocdm.so" \
      -DPORT=WPE \
      -DENABLE_ENCRYPTED_MEDIA=ON \
      -DENABLE_THUNDER=ON \
      -DENABLE_MINIBROWSER=OFF \
      -DENABLE_WEB_AUDIO=ON \
      -DENABLE_VIDEO=ON \
      -DENABLE_DOCUMENTATION=OFF \
      -DENABLE_INTROSPECTION=OFF \
      -DENABLE_API_TESTS=OFF \
      -DENABLE_LAYOUT_TESTS=OFF \
      -DENABLE_GEOLOCATION=OFF \
      -DENABLE_GAMEPAD=OFF \
      -DENABLE_SPEECH_SYNTHESIS=OFF \
      -DENABLE_WEB_RTC=OFF \
      -DENABLE_WEBXR=OFF \
      -DENABLE_PDFJS=OFF \
      -DENABLE_NOTIFICATIONS=OFF \
      -DENABLE_DEVICE_ORIENTATION=OFF \
      -DENABLE_MEDIA_RECORDER=OFF \
      -DENABLE_MEDIA_STREAM=OFF \
      -DENABLE_MATHML=OFF \
      -DENABLE_FTPDIR=OFF \
      -DENABLE_WEB_AUTHN=OFF \
      -DENABLE_WEB_CODECS=OFF \
      -DENABLE_SERVICE_CONTROLS=OFF \
      -DENABLE_POINTER_LOCK=OFF \
      -DENABLE_ASYNC_SCROLLING=OFF \
      -DENABLE_DARK_MODE_CSS=OFF \
      -DENABLE_DRAG_SUPPORT=OFF \
      -DUSE_LIBBACKTRACE=OFF \
      -DUSE_FLITE=OFF \
      -DCMAKE_EXE_LINKER_FLAGS="-fuse-ld=lld" \
      -DCMAKE_SHARED_LINKER_FLAGS="-fuse-ld=lld" \
      -DCMAKE_CXX_FLAGS="-g0 -O2" \
      -DCMAKE_C_FLAGS="-g0 -O2"
else
    echo "  CMake configuration already exists; reusing Ninja build tree."
fi

# ================================================================
# STEP 6: COMPILE REQUIRED WEBKIT TARGETS
# ================================================================
echo "[6/8] Building required WebKit targets (WPEWebProcess, WPENetworkProcess, WPEToolingBackends, WPEToolingBackends_CopyHeaders)..."
ninja -C "${WEBKIT_BUILD_DIR}" -j"${JOBS}" WPEWebProcess WPENetworkProcess WPEToolingBackends WPEToolingBackends_CopyHeaders
# Touch CMake custom target dummy outputs so Ninja knows they completed and doesn't re-run them
touch -c "${WEBKIT_BUILD_DIR}/Source/ThirdParty/ANGLE/CMakeFiles/ANGLE-webgl-headers" \
         "${WEBKIT_BUILD_DIR}/Source/WebCore/CMakeFiles/WebCoreBindings" \
         "${WEBKIT_BUILD_DIR}/Source/WebKit/CMakeFiles/webkitwpe-forwarding-headers" 2>/dev/null || true

# ================================================================
# STEP 7: ASSEMBLE RELOCATABLE RUNTIME
# ================================================================
echo "[7/8] Assembling relocatable runtime in ${OUTPUT_DIR}..."
mkdir -p "${OUTPUT_DIR}/bin" "${OUTPUT_DIR}/lib"

patch_rpath_if_needed() {
    local target="$1"
    local old_rpath="$2"
    local new_rpath="$3"

    local current_rpath
    current_rpath=$(readelf -d "${target}" 2>/dev/null | grep -E "RPATH|RUNPATH" || true)
    if [[ "${current_rpath}" == *"${new_rpath}"* ]]; then
        return 0
    fi
    echo "  Patching RPATH of $(basename "${target}") to ${new_rpath}..."
    local rpath_script="${BUILD_ROOT}/rpath_patch.cmake"
    cat << CMAKE_EOF > "${rpath_script}"
file(RPATH_CHANGE FILE "${target}" OLD_RPATH "${old_rpath}" NEW_RPATH "${new_rpath}")
CMAKE_EOF
    cmake -P "${rpath_script}"
    rm -f "${rpath_script}"
}

# 1. Copy WebKit libraries and create canonical symlinks
if needs_rebuild "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1.9.10" "${WEBKIT_BUILD_DIR}/lib/libWPEWebKit-2.0.so.1.9.10"; then
    echo "  Copying libWPEWebKit-2.0.so.1.9.10 to runtime..."
    cp -f "${WEBKIT_BUILD_DIR}/lib/libWPEWebKit-2.0.so.1.9.10" "${OUTPUT_DIR}/lib/"
fi
ln -sf libWPEWebKit-2.0.so.1.9.10 "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1"
ln -sf libWPEWebKit-2.0.so.1 "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so"

install_if_different "${PREFIX_DIR}/usr/lib/libwpe-1.0.so.1.9.6" "${OUTPUT_DIR}/lib/libwpe-1.0.so.1.9.6"
ln -sf libwpe-1.0.so.1.9.6 "${OUTPUT_DIR}/lib/libwpe-1.0.so.1"
ln -sf libwpe-1.0.so.1 "${OUTPUT_DIR}/lib/libwpe-1.0.so"

install_if_different "${PREFIX_DIR}/usr/lib/libWPEBackend-fdo-1.0.so.1.10.2" "${OUTPUT_DIR}/lib/libWPEBackend-fdo-1.0.so.1.10.2"
ln -sf libWPEBackend-fdo-1.0.so.1.10.2 "${OUTPUT_DIR}/lib/libWPEBackend-fdo-1.0.so.1"
ln -sf libWPEBackend-fdo-1.0.so.1 "${OUTPUT_DIR}/lib/libWPEBackend-fdo-1.0.so"
ln -sf libWPEBackend-fdo-1.0.so.1 "${OUTPUT_DIR}/lib/libWPEBackend-default.so"

# 2. OpenCDM shim for runtime (identical to prefix shim)
install_if_different "${PREFIX_DIR}/lib/libocdm.so" "${OUTPUT_DIR}/lib/libocdm.so"

# 3. Copy WPE helper processes
if needs_rebuild "${OUTPUT_DIR}/bin/WPEWebProcess" "${WEBKIT_BUILD_DIR}/bin/WPEWebProcess"; then
    echo "  Copying WPEWebProcess to runtime..."
    cp -f "${WEBKIT_BUILD_DIR}/bin/WPEWebProcess" "${OUTPUT_DIR}/bin/WPEWebProcess"
    chmod +x "${OUTPUT_DIR}/bin/WPEWebProcess"
fi

if needs_rebuild "${OUTPUT_DIR}/bin/WPENetworkProcess" "${WEBKIT_BUILD_DIR}/bin/WPENetworkProcess"; then
    echo "  Copying WPENetworkProcess to runtime..."
    cp -f "${WEBKIT_BUILD_DIR}/bin/WPENetworkProcess" "${OUTPUT_DIR}/bin/WPENetworkProcess"
    chmod +x "${OUTPUT_DIR}/bin/WPENetworkProcess"
fi

# 4. Patch RPATHs of copied binaries to $ORIGIN relative paths
patch_rpath_if_needed "${OUTPUT_DIR}/bin/WPEWebProcess" "${WEBKIT_BUILD_DIR}/lib:${PREFIX_DIR}/usr/lib:" '$ORIGIN/../lib'
patch_rpath_if_needed "${OUTPUT_DIR}/bin/WPENetworkProcess" "${WEBKIT_BUILD_DIR}/lib:${PREFIX_DIR}/usr/lib:" '$ORIGIN/../lib'
patch_rpath_if_needed "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1.9.10" "${PREFIX_DIR}/usr/lib:${PREFIX_DIR}/lib:" '$ORIGIN'

# 5. Build malus-wpe-host
HOST_SRCS=(
    "${REPO_ROOT}/crates/malus-wpe/wpe-host/main.cpp"
    "${WEBKIT_BUILD_DIR}/lib/libWPEToolingBackends.a"
    "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1.9.10"
)
if needs_rebuild "${OUTPUT_DIR}/bin/malus-wpe-host" "${HOST_SRCS[@]}"; then
    echo "  Compiling malus-wpe-host..."
    g++ -O2 -std=c++20 \
        -I"${WEBKIT_BUILD_DIR}/DerivedSources/ForwardingHeaders/wpe" \
        -I"${WEBKIT_BUILD_DIR}/DerivedSources/WebKit" \
        -I"${WEBKIT_BUILD_DIR}/DerivedSources/ForwardingHeaders/wpe-jsc" \
        -I"${WEBKIT_BUILD_DIR}/JavaScriptCoreGLib/Headers" \
        -I"${WEBKIT_BUILD_DIR}/JavaScriptCoreGLib/DerivedSources" \
        -I"${WEBKIT_BUILD_DIR}/WPEToolingBackends/Headers" \
        -I"${PREFIX_DIR}/usr/include/wpe-1.0" \
        -I"${PREFIX_DIR}/usr/include/wpe-fdo-1.0" \
        $(pkg-config --cflags glib-2.0 gio-2.0 libsoup-3.0) \
        "${REPO_ROOT}/crates/malus-wpe/wpe-host/main.cpp" \
        "${WEBKIT_BUILD_DIR}/lib/libWPEToolingBackends.a" \
        -L"${OUTPUT_DIR}/lib" -lWPEWebKit-2.0 -lwpe-1.0 -lWPEBackend-fdo-1.0 \
        $(pkg-config --libs glib-2.0 gio-2.0 libsoup-3.0 atk-bridge-2.0 atk) -lepoxy -lwayland-client -lwayland-egl -lxkbcommon \
        -Wl,-rpath,'$ORIGIN/../lib',--disable-new-dtags \
        -o "${OUTPUT_DIR}/bin/malus-wpe-host"
    chmod +x "${OUTPUT_DIR}/bin/malus-wpe-host"
else
    echo "  malus-wpe-host is up to date."
fi


# ================================================================
# STEP 8: ASSEMBLY VALIDATION
# ================================================================
echo "[8/8] Validating assembled runtime..."
REQUIRED_FILES=(
    "${OUTPUT_DIR}/bin/malus-wpe-host"
    "${OUTPUT_DIR}/bin/WPEWebProcess"
    "${OUTPUT_DIR}/bin/WPENetworkProcess"
    "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1"
    "${OUTPUT_DIR}/lib/libwpe-1.0.so.1"
    "${OUTPUT_DIR}/lib/libWPEBackend-fdo-1.0.so.1"
    "${OUTPUT_DIR}/lib/libocdm.so"
)

for f in "${REQUIRED_FILES[@]}"; do
    if [[ ! -e "$f" ]]; then
        echo "ERROR: Required runtime component missing: $f" >&2
        exit 1
    fi
    if [[ ! -s "$f" ]]; then
        echo "ERROR: Runtime component is empty: $f" >&2
        exit 1
    fi
done

# Ensure ZERO build directory or lab paths exist in binary dynamic sections
for elf in "${OUTPUT_DIR}/bin/malus-wpe-host" "${OUTPUT_DIR}/bin/WPEWebProcess" "${OUTPUT_DIR}/bin/WPENetworkProcess" "${OUTPUT_DIR}/lib/libWPEWebKit-2.0.so.1.9.10"; do
    if readelf -d "$elf" | grep -E "PATH.*(malus-wpe-lab|/build/wpe)" > /dev/null; then
        echo "ERROR: Runtime ELF $elf still contains build/lab paths in dynamic section!" >&2
        readelf -d "$elf" | grep PATH >&2
        exit 1
    fi
done

echo "=================================================="
echo "Malus WPE Runtime successfully built and assembled!"
echo "Layout:"
ls -lh "${OUTPUT_DIR}/bin" "${OUTPUT_DIR}/lib"
echo "=================================================="
