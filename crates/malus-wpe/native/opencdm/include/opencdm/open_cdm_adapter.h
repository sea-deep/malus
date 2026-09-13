 /*
 * If not stated otherwise in this file or this component's LICENSE file the
 * following copyright and licenses apply:
 *
 * Copyright 2020 Metrological
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
 
#ifndef __OPEN_CDM_ADAPTER_H
#define __OPEN_CDM_ADAPTER_H

#include "open_cdm.h"

struct _GstBuffer;
typedef struct _GstBuffer GstBuffer;

struct _GstCaps;
typedef struct _GstCaps GstCaps;

#ifdef __cplusplus
extern "C" {
#endif

/**
 * \brief Creates private data required for decryption.
 *
 * This method allows for creation of private data that is associated with the session. The private data itself is
 * opaque to OpenCDMSession and it lives until the lifetime of the \ref OpenCDMSession.
 *
 * \param session \ref OpenCDMSession instance.
 * \param pvtData Output parameter that will contain the Opaque private data created
 *
 * \return Zero on success, non-zero on error.
 */
EXTERNAL uint32_t opencdm_construct_session_private(struct OpenCDMSession* session, void* &pvtData);

/**
 * \brief Destroys private data associated with the Session.
 *
 * This method destroys the private data that is associated with the session.
 *
 * \param session \ref OpenCDMSession instance.
 * \param pvtData Input parameter pointer to private data.
 *
 * \return Zero on success, non-zero on error.
 */
EXTERNAL uint32_t opencdm_destruct_session_private(struct OpenCDMSession* session, void* &pvtData);

/**
 * \brief Performs decryption based on adapter implementation.
 *
 * This method accepts encrypted data and will typically decrypt it out-of-process (for security reasons). The actual data copying is performed
 * using a memory-mapped file (for performance reasons). If the DRM system allows access to decrypted data (i.e. decrypting is not
 * performed in a TEE), the decryption is performed in-place.
 * \param session \ref OpenCDMSession instance.
 * \param buffer Gstreamer buffer containing encrypted data and related meta data. If applicable, decrypted data will be stored here after this call returns.
 * \param subSample Gstreamer buffer containing subsamples size which has been parsed from protection meta data.
 * \param subSampleCount count of subsamples
 * \param IV Gstreamer buffer containing initial vector (IV) used during decryption.
 * \param keyID Gstreamer buffer containing keyID to use for decryption
 *
 * This method handles the Subsample mapping by consolidating all the encrypted data into one buffer before decrypting. This means the Subsample mappings are
 * not passed on to the DRM implementation side.
 *
 * For CBCS support, EncryptionScheme and EncryptionPattern information can be added as part of the ProtectionMeta in the given format below
 *      "cipher-mode"         G_TYPE_STRING   (One of the Four Character Code (FOURCC) Protection schemes as defined in https://www.iso.org/obp/ui/#iso:std:iso-iec:23001:-7:ed-3:v1:en)
 *      "crypt_byte_block"    G_TYPE_UINT     (Present only if cipher-mode is "cbcs")
 *      "skip_byte_block"     G_TYPE_UINT     (Present only cipher-mode is "cbcs")

 * \return Zero on success, non-zero on error.
 */
    EXTERNAL OpenCDMError opencdm_gstreamer_session_decrypt(struct OpenCDMSession* session, GstBuffer* buffer, GstBuffer* subSample, const uint32_t subSampleCount,
                                                   GstBuffer* IV, GstBuffer* keyID, uint32_t initWithLast15);

/**
 * \brief Performs decryption based on adapter implementation.
 *
 * This version 3 method accepts encrypted data and will typically decrypt it out-of-process (for security reasons). The actual data copying is performed
 * using a memory-mapped file (for performance reasons). If the DRM system allows access to decrypted data (i.e. decrypting is not
 * performed in a TEE), the decryption is performed in-place. 
 * This version assumes all data required is attached as metadata to the buffer. Specification for this data is as follows:
 *
 * Typically, the caller would parse the protection information for a video/audio frame from its input data and use this information to populate the
 * GstStructure info field, which is then encapsulated in a GstProtectionMeta object and attached to the corresponding GstBuffer using the
 * gst_buffer_add_protection_meta function.
 *
 * gst_structure [application/x-cenc]
 *      "iv"                  GST_TYPE_BUFFER
 *      "kid"                 GST_TYPE_BUFFER
 *      "subsample_count"     G_TYPE_UINT
 *      "subsamples"          GST_TYPE_BUFFER
 *      "cipher-mode"         G_TYPE_STRING   (One of the Four Character Code (FOURCC) Protection schemes as defined in https://www.iso.org/obp/ui/#iso:std:iso-iec:23001:-7:ed-3:v1:en)
 *      "crypt_byte_block"    G_TYPE_UINT     (Present only if cipher-mode is "cbcs")
 *      "skip_byte_block"     G_TYPE_UINT     (Present only cipher-mode is "cbcs")
 *
 * This method passes on the subsample mapping to the DRM implementation and assumes that the DRM implementaion will handle the decryption based on subsample mapping.
 *
 * \param session \ref OpenCDMSession instance.
 * \param buffer Gstreamer buffer containing encrypted data and related meta data. If applicable, decrypted data will be stored here after this call returns.
 * \return Zero on success, non-zero on error.
 */
    
    EXTERNAL OpenCDMError opencdm_gstreamer_session_decrypt_buffer(struct OpenCDMSession* session, GstBuffer* buffer, GstCaps* caps);


#ifdef __cplusplus
}
#endif

#endif // __OPEN_CDM_ADAPTER_H
