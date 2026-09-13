# DRM and OpenCDM Interface Headers Provenance

This directory contains external C/C++ interface headers required to compile Malus's `libocdm` shim for Widevine CDM hosting and GStreamer buffer decryption.

## 1. OpenCDM Headers (`opencdm/`)
- **Files**:
  - `open_cdm.h`
  - `open_cdm_adapter.h`
- **Origin**: OpenCDM project (Fraunhofer FOKUS, Metrological, TATA ELXSI).
- **License**: Apache License, Version 2.0.
- **Copyright**:
  - Copyright 2016-2017 TATA ELXSI
  - Copyright 2016-2017 Metrological
  - Copyright 2020 Metrological
- **Purpose**: Defines the OpenCDM C interface expected by WebKit's GStreamer WebKitMediaCommonEncryptionDecrypt (cdm-decrypt) element.

## 2. Chromium Content Decryption Module Headers (`cdm/`)
- **Files**:
  - `content_decryption_module.h`
  - `content_decryption_module_export.h`
- **Origin**: Chromium Open Source Project (`src/media/cdm/api`).
- **License**: BSD-style license (see original headers).
- **Copyright**:
  - Copyright 2012 The Chromium Authors
  - Copyright 2017 The Chromium Authors
- **Purpose**: Defines the C++ ABI (`cdm::ContentDecryptionModule_10`, `cdm::Host_10`) exported by Google's proprietary Widevine CDM library (`libwidevinecdm.so`).

> [!IMPORTANT]
> Neither `libwidevinecdm.so` nor any proprietary binaries are stored in this repository. Widevine CDM is discovered dynamically on the user's system at runtime.
