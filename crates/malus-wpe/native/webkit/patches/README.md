# WebKit Upstream Patches for Malus WPE Runtime

This directory contains minimal, deterministic patches applied to upstream WebKit sources
during the Malus WPE runtime build.

Upstream release: **`wpewebkit-2.52.6`** (`https://wpewebkit.org/releases/wpewebkit-2.52.6.tar.xz`)

## Patches

### `0001-glib-process-executable-path.patch`
- **Classification**: `REQUIRED FOR RUNTIME`
- **Component**: `Source/WebKit/Shared/glib/ProcessExecutablePathGLib.cpp`
- **Rationale**: In upstream WebKit, `findWebKitProcess` checks the `WEBKIT_EXEC_PATH` environment variable and `FileSystem::currentExecutablePath()` (the directory containing the running executable) only when `ENABLE(DEVELOPER_MODE)` is turned ON. In Release builds, it strictly falls back to hardcoded `PKGLIBEXECDIR`. This patch enables relative discovery and `WEBKIT_EXEC_PATH` in Release mode, allowing `malus-wpe-host` to find `WPEWebProcess` and `WPENetworkProcess` co-located in `runtime/wpe/bin/` as a relocatable bundle.
- **Upstream Status**: Temporary until upstream WPE adds an explicit runtime API or public configuration for custom helper process paths in non-developer builds.

### `0002-coordinated-scrolling-guards.patch`
- **Classification**: `REQUIRED FOR BUILD`
- **Component**: `Source/WebCore/page/scrolling/coordinated/`
- **Rationale**: When WebKit is configured with `-DENABLE_ASYNC_SCROLLING=OFF`, coordinated scrollbar files are still unconditionally included by `USE(COORDINATED_GRAPHICS_ASYNC_SCROLLBAR)`, causing compilation errors because the underlying scrolling node classes are guarded by `ENABLE(ASYNC_SCROLLING)`. This patch adds `&& ENABLE(ASYNC_SCROLLING)` guards to these 5 files.
- **Upstream Status**: Bugfix for `-DENABLE_ASYNC_SCROLLING=OFF` configuration.

### `0003-tooling-backends-build.patch`
- **Classification**: `REQUIRED FOR BUILD`
- **Component**: `Tools/PlatformWPE.cmake`
- **Rationale**: In upstream WebKit, `Tools/wpe/backends` (which builds `libWPEToolingBackends.a` containing `HeadlessViewBackend`) is only built when `ENABLE_MINIBROWSER` or test suites are ON. Malus builds `malus-wpe-host` which uses `HeadlessViewBackend`, but Malus disables MiniBrowser and test suites. This patch allows `Tools/wpe/backends` to build whenever `ENABLE_WPE_LEGACY_API` is enabled, without building MiniBrowser.
- **Upstream Status**: Build configuration fix for headless host tooling.

