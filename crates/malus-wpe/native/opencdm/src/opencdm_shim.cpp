#include <opencdm/open_cdm.h>
#include <opencdm/open_cdm_adapter.h>
#include <content_decryption_module.h>
#include <gst/gst.h>
#include <gst/base/gstbytereader.h>
#include <dlfcn.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include <map>
#include <mutex>
#include <atomic>
#include <chrono>
#include <algorithm>

// --- CDM Buffer and DecryptedBlock Implementations ---

class CdmSimpleBuffer : public cdm::Buffer {
public:
    CdmSimpleBuffer(uint32_t capacity) : m_data(capacity) {}
    void Destroy() override { delete this; }
    uint32_t Capacity() const override { return m_data.size(); }
    uint8_t* Data() override { return m_data.data(); }
    void SetSize(uint32_t size) override { m_size = size; }
    uint32_t Size() const override { return m_size; }
private:
    std::vector<uint8_t> m_data;
    uint32_t m_size{0};
};

class CdmSimpleDecryptedBlock : public cdm::DecryptedBlock {
public:
    void SetDecryptedBuffer(cdm::Buffer* buffer) override { m_buffer = buffer; }
    cdm::Buffer* DecryptedBuffer() override { return m_buffer; }
    void SetTimestamp(int64_t timestamp) override { m_timestamp = timestamp; }
    int64_t Timestamp() const override { return m_timestamp; }
private:
    cdm::Buffer* m_buffer{nullptr};
    int64_t m_timestamp{0};
};

// --- Forward Declarations ---
struct OpenCDMSession;
struct OpenCDMSystem;

// Global CDM Dynamic Library Handle
static void* g_cdm_handle = nullptr;
static std::atomic<bool> g_cdm_initialized{false};

using InitFunc = void (*)();
using CreateCdmFunc = void* (*)(int, const char*, uint32_t, GetCdmHostFunc, void*);

static InitFunc g_init_module = nullptr;
static CreateCdmFunc g_create_cdm = nullptr;

static void ensure_cdm_loaded() {
    if (g_cdm_initialized.load()) return;

    const char* env_path = getenv("MALUS_WIDEVINE_PATH");
    if (!env_path || !*env_path) {
        fprintf(stderr, "[OpenCDM Shim] Widevine CDM disabled: MALUS_WIDEVINE_PATH is not set\n");
        g_cdm_initialized.store(true);
        return;
    }

    g_cdm_handle = dlopen(env_path, RTLD_NOW | RTLD_GLOBAL);
    if (!g_cdm_handle) {
        const char* err = dlerror();
        fprintf(stderr, "[OpenCDM Shim] Failed to dlopen MALUS_WIDEVINE_PATH (%s): %s\n", env_path, err ? err : "unknown error");
        g_cdm_initialized.store(true);
        return;
    }

    fprintf(stderr, "[OpenCDM Shim] Loaded Widevine CDM from MALUS_WIDEVINE_PATH: %s\n", env_path);

    g_init_module = (InitFunc)dlsym(g_cdm_handle, "InitializeCdmModule_4");
    g_create_cdm = (CreateCdmFunc)dlsym(g_cdm_handle, "CreateCdmInstance");

    if (g_init_module) {
        fprintf(stderr, "[OpenCDM Shim] Calling InitializeCdmModule_4...\n");
        g_init_module();
    }

    g_cdm_initialized.store(true);
}

// KeyStatus mapper
static KeyStatus convertKeyStatus(cdm::KeyStatus status) {
    switch (status) {
    case cdm::kUsable: return Usable;
    case cdm::kExpired: return Expired;
    case cdm::kReleased: return Released;
    case cdm::kOutputRestricted: return OutputRestricted;
    case cdm::kOutputDownscaled: return OutputDownscaled;
    case cdm::kStatusPending: return StatusPending;
    case cdm::kInternalError:
    default: return InternalError;
    }
}

// --- OpenCDMSession Definition ---

struct OpenCDMSession : public cdm::Host_10 {
    std::atomic<int> refCount{1};
    std::string keySystem;
    LicenseType licenseType{Temporary};
    std::string sessionId;
    OpenCDMSessionCallbacks callbacks{};
    void* userData{nullptr};
    cdm::ContentDecryptionModule_10* cdm{nullptr};
    OpenCDMSystem* system{nullptr};

    std::mutex lock;
    std::map<std::vector<uint8_t>, KeyStatus> keys;
    std::vector<uint8_t> defaultKeyId;
    std::vector<uint8_t> lastChallenge;

    OpenCDMSession(const std::string& ks, LicenseType lt, OpenCDMSessionCallbacks* cb, void* ud)
        : keySystem(ks), licenseType(lt), userData(ud) {
        if (cb) callbacks = *cb;
    }

    ~OpenCDMSession() {
        fprintf(stderr, "[OpenCDM Shim] Session %s destructed\n", sessionId.c_str());
        if (cdm) {
            cdm->Destroy();
            cdm = nullptr;
        }
    }

    // --- cdm::Host_10 Implementation ---

    cdm::Buffer* Allocate(uint32_t capacity) override {
        return new CdmSimpleBuffer(capacity);
    }

    void SetTimer(int64_t delay_ms, void* context) override {
        // Widevine timer
    }

    cdm::Time GetCurrentWallTime() override {
        auto now = std::chrono::system_clock::now().time_since_epoch();
        return std::chrono::duration_cast<std::chrono::milliseconds>(now).count() / 1000.0;
    }

    void OnInitialized(bool success) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnInitialized(success=%d)\n", success);
    }

    void OnResolveKeyStatusPromise(uint32_t promise_id, cdm::KeyStatus key_status) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnResolveKeyStatusPromise(id=%u, status=%d)\n", promise_id, (int)key_status);
    }

    void OnResolveNewSessionPromise(uint32_t promise_id, const char* s_id, uint32_t s_id_size) override {
        sessionId.assign(s_id, s_id_size);
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnResolveNewSessionPromise(id=%u, session_id=%s)\n",
                promise_id, sessionId.c_str());
    }

    void OnResolvePromise(uint32_t promise_id) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnResolvePromise(id=%u)\n", promise_id);
    }

    void OnRejectPromise(uint32_t promise_id, cdm::Exception exception, uint32_t system_code,
                         const char* error_message, uint32_t error_message_size) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnRejectPromise(id=%u, exc=%d, code=%u, msg=%.*s)\n",
                promise_id, (int)exception, system_code, (int)error_message_size, error_message);
        if (callbacks.error_message_callback) {
            std::string msg(error_message, error_message_size);
            callbacks.error_message_callback(this, userData, msg.c_str());
        }
    }

    void OnSessionMessage(const char* s_id, uint32_t s_id_size, cdm::MessageType message_type,
                          const char* message, uint32_t message_size) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnSessionMessage(session=%.*s, type=%d, len=%u)\n",
                (int)s_id_size, s_id, (int)message_type, message_size);

        lastChallenge.assign((const uint8_t*)message, (const uint8_t*)message + message_size);

        // WebKit CDMThunder expects "{type}:Type:" prefix, e.g. "0:Type:" (7 bytes)
        // 0 = license-request, 1 = license-renewal, 2 = license-release, 3 = individualization-request
        char prefix[16];
        snprintf(prefix, sizeof(prefix), "%d:Type:", (int)message_type);
        size_t prefixLen = strlen(prefix);

        std::vector<uint8_t> payload(prefixLen + message_size);
        memcpy(payload.data(), prefix, prefixLen);
        memcpy(payload.data() + prefixLen, message, message_size);

        if (callbacks.process_challenge_callback) {
            fprintf(stderr, "[OpenCDM Shim] Delivering challenge of %zu bytes (with prefix '%s') to WebKit...\n",
                    payload.size(), prefix);
            callbacks.process_challenge_callback(this, userData, "", payload.data(), payload.size());
        }
    }

    void OnSessionKeysChange(const char* s_id, uint32_t s_id_size, bool has_additional_usable_key,
                             const cdm::KeyInformation* keys_info, uint32_t keys_info_count) override {
        std::lock_guard<std::mutex> lk(lock);
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnSessionKeysChange(session=%.*s, add_usable=%d, count=%u)\n",
                (int)s_id_size, s_id, has_additional_usable_key, keys_info_count);

        for (uint32_t i = 0; i < keys_info_count; ++i) {
            std::vector<uint8_t> kid(keys_info[i].key_id, keys_info[i].key_id + keys_info[i].key_id_size);
            KeyStatus ks = convertKeyStatus(keys_info[i].status);
            keys[kid] = ks;
            if (ks == Usable && defaultKeyId.empty()) {
                defaultKeyId = kid;
            }

            fprintf(stderr, "  [OpenCDM Shim] Key ID: ");
            for (auto b : kid) fprintf(stderr, "%02x", b);
            fprintf(stderr, " -> Status: %d (%s)\n", (int)ks, ks == Usable ? "USABLE" : "OTHER");

            // Compatibility contract with WebKit WPE (CDMProxyThunder):
            // The OpenCDM `key_update_callback` signature carries only (keyId, length) without
            // key status. In WebKit, receiving `key_update_callback` adds the key ID to `m_keyStore`,
            // which causes `CDMProxy::isKeyAvailableUnlocked(keyId)` to return true.
            // If non-usable keys (Released, Expired, etc.) are forwarded, WebKit assumes the key
            // is available, skips waiting for a replacement license, and permanently aborts decryption.
            // Therefore, Malus must only publish Usable keys through `key_update_callback`.
            if (ks == Usable) {
                if (callbacks.key_update_callback) {
                    callbacks.key_update_callback(this, userData, kid.data(), kid.size());
                }
            }
        }

        if (has_additional_usable_key && callbacks.keys_updated_callback) {
            callbacks.keys_updated_callback(this, userData);
        }
    }

    void OnExpirationChange(const char* s_id, uint32_t s_id_size, cdm::Time new_expiry_time) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnExpirationChange(expiry=%f)\n", new_expiry_time);
    }

    void OnSessionClosed(const char* s_id, uint32_t s_id_size) override {
        fprintf(stderr, "[OpenCDM Shim] Host_10: OnSessionClosed(session=%.*s)\n", (int)s_id_size, s_id);
    }

    void SendPlatformChallenge(const char*, uint32_t, const char*, uint32_t) override {}
    void EnableOutputProtection(uint32_t) override {}
    void QueryOutputProtectionStatus() override {}
    void OnDeferredInitializationDone(cdm::StreamType, cdm::Status) override {}
    cdm::FileIO* CreateFileIO(cdm::FileIOClient*) override { return nullptr; }
    void RequestStorageId(uint32_t) override {}
};

static void* GetCdmHostCallback(int host_interface_version, void* user_data) {
    if (host_interface_version == cdm::Host_10::kVersion) {
        return static_cast<cdm::Host_10*>(static_cast<OpenCDMSession*>(user_data));
    }
    return nullptr;
}

// --- OpenCDMSystem Definition ---

struct OpenCDMSystem {
    std::string keySystem;
    std::mutex lock;
    std::vector<OpenCDMSession*> sessions;
    std::vector<uint8_t> serverCertificate;
};

// --- Exported OpenCDM APIs ---

extern "C" {

EXTERNAL OpenCDMError opencdm_is_type_supported(const char keySystem[], const char mimeType[]) {
    ensure_cdm_loaded();
    if (!keySystem) return ERROR_INVALID_ARG;

    if (strcmp(keySystem, "com.widevine.alpha") == 0 || strcmp(keySystem, "org.w3.clearkey") == 0) {
        return ERROR_NONE;
    }
    return ERROR_KEYSYSTEM_NOT_SUPPORTED;
}

EXTERNAL struct OpenCDMSystem* opencdm_create_system(const char keySystem[]) {
    ensure_cdm_loaded();
    fprintf(stderr, "[OpenCDM Shim] opencdm_create_system('%s')\n", keySystem ? keySystem : "null");
    auto* sys = new OpenCDMSystem();
    sys->keySystem = keySystem ? keySystem : "";
    return sys;
}

EXTERNAL OpenCDMError opencdm_create_system_extended(const char keySystem[], struct OpenCDMSystem** system) {
    if (!system) return ERROR_INVALID_ARG;
    *system = opencdm_create_system(keySystem);
    return ERROR_NONE;
}

EXTERNAL OpenCDMError opencdm_destruct_system(struct OpenCDMSystem* system) {
    fprintf(stderr, "[OpenCDM Shim] opencdm_destruct_system\n");
    if (system) {
        delete system;
    }
    return ERROR_NONE;
}

EXTERNAL OpenCDMBool opencdm_system_supports_server_certificate(struct OpenCDMSystem* system) {
    return OPENCDM_BOOL_TRUE;
}

EXTERNAL OpenCDMError opencdm_system_set_server_certificate(
    struct OpenCDMSystem* system,
    const uint8_t serverCertificate[],
    const uint16_t serverCertificateLength) {
    fprintf(stderr, "[OpenCDM Shim] opencdm_system_set_server_certificate len=%u\n", serverCertificateLength);
    if (system && serverCertificate && serverCertificateLength > 0) {
        std::lock_guard<std::mutex> lk(system->lock);
        system->serverCertificate.assign(serverCertificate, serverCertificate + serverCertificateLength);
        for (auto* s : system->sessions) {
            if (s && s->cdm) {
                fprintf(stderr, "[OpenCDM Shim] Setting server certificate on active session %s\n", s->sessionId.c_str());
                s->cdm->SetServerCertificate(100, serverCertificate, serverCertificateLength);
            }
        }
    }
    return ERROR_NONE;
}

EXTERNAL OpenCDMError opencdm_construct_session(
    struct OpenCDMSystem* system,
    const LicenseType licenseType,
    const char initDataType[],
    const uint8_t initData[],
    const uint16_t initDataLength,
    const uint8_t CDMData[],
    const uint16_t CDMDataLength,
    OpenCDMSessionCallbacks* callbacks,
    void* userData,
    struct OpenCDMSession** session) {

    ensure_cdm_loaded();
    fprintf(stderr, "[OpenCDM Shim] opencdm_construct_session: type='%s', len=%u\n",
            initDataType ? initDataType : "null", initDataLength);

    if (!system || !session) return ERROR_INVALID_ARG;

    auto* s = new OpenCDMSession(system->keySystem, licenseType, callbacks, userData);
    s->system = system;

    if (g_create_cdm) {
        const char* ks = "com.widevine.alpha";
        void* cdm_ptr = g_create_cdm(cdm::ContentDecryptionModule_10::kVersion, ks, strlen(ks), GetCdmHostCallback, s);
        if (!cdm_ptr) {
            fprintf(stderr, "[OpenCDM Shim] ERROR: CreateCdmInstance returned NULL!\n");
            delete s;
            return ERROR_UNKNOWN;
        }
        s->cdm = static_cast<cdm::ContentDecryptionModule_10*>(cdm_ptr);
        s->cdm->Initialize(true, false, false);

        if (!system->serverCertificate.empty()) {
            fprintf(stderr, "[OpenCDM Shim] Setting server certificate (%zu bytes) on new session CDM...\n",
                    system->serverCertificate.size());
            s->cdm->SetServerCertificate(100, system->serverCertificate.data(), system->serverCertificate.size());
        }

        cdm::InitDataType cdm_init_type = cdm::kCenc;
        if (initDataType) {
            if (strcmp(initDataType, "cenc") == 0) cdm_init_type = cdm::kCenc;
            else if (strcmp(initDataType, "keyids") == 0) cdm_init_type = cdm::kKeyIds;
            else if (strcmp(initDataType, "webm") == 0) cdm_init_type = cdm::kWebM;
        }

        fprintf(stderr, "[OpenCDM Shim] Calling cdm->CreateSessionAndGenerateRequest...\n");
        s->cdm->CreateSessionAndGenerateRequest(1, cdm::kTemporary, cdm_init_type, initData, initDataLength);
    } else {
        fprintf(stderr, "[OpenCDM Shim] WARNING: g_create_cdm is NULL\n");
    }

    {
        std::lock_guard<std::mutex> lk(system->lock);
        system->sessions.push_back(s);
    }

    *session = s;
    return ERROR_NONE;
}

EXTERNAL OpenCDMError opencdm_destruct_session(struct OpenCDMSession* session) {
    if (!session) return ERROR_NONE;
    if (session->system) {
        std::lock_guard<std::mutex> lk(session->system->lock);
        int rc = --session->refCount;
        fprintf(stderr, "[OpenCDM Shim] opencdm_destruct_session (remaining refCount=%d)\n", rc);
        if (rc <= 0) {
            auto& v = session->system->sessions;
            v.erase(std::remove(v.begin(), v.end(), session), v.end());
            delete session;
        }
    } else {
        int rc = --session->refCount;
        fprintf(stderr, "[OpenCDM Shim] opencdm_destruct_session (remaining refCount=%d)\n", rc);
        if (rc <= 0) {
            delete session;
        }
    }
    return ERROR_NONE;
}

EXTERNAL const char* opencdm_session_id(const struct OpenCDMSession* session) {
    if (!session) return "";
    return session->sessionId.c_str();
}

EXTERNAL KeyStatus opencdm_session_status(const struct OpenCDMSession* session,
    const uint8_t keyId[], const uint8_t length) {
    if (!session || !keyId || length == 0) return StatusPending;

    auto* s = const_cast<OpenCDMSession*>(session);
    std::lock_guard<std::mutex> lk(s->lock);
    std::vector<uint8_t> kid(keyId, keyId + length);
    auto it = s->keys.find(kid);
    if (it != s->keys.end()) {
        return it->second;
    }
    return StatusPending;
}

EXTERNAL OpenCDMError opencdm_session_update(struct OpenCDMSession* session,
    const uint8_t keyMessage[], const uint16_t keyLength) {
    if (!session || !keyMessage || keyLength == 0) return ERROR_INVALID_ARG;

    fprintf(stderr, "[OpenCDM Shim] opencdm_session_update: received %u bytes license payload for session %s (first 16 bytes: ",
            keyLength, session->sessionId.c_str());
    for (uint16_t i = 0; i < std::min<uint16_t>(16, keyLength); ++i) {
        fprintf(stderr, "%02x ", keyMessage[i]);
    }
    fprintf(stderr, ")\n");

    if (session->cdm) {
        session->cdm->UpdateSession(2, session->sessionId.c_str(), session->sessionId.size(),
                                   keyMessage, keyLength);
        return ERROR_NONE;
    }
    return ERROR_INVALID_SESSION;
}

EXTERNAL OpenCDMError opencdm_session_close(struct OpenCDMSession* session) {
    if (!session) return ERROR_NONE;
    fprintf(stderr, "[OpenCDM Shim] opencdm_session_close: session %s\n", session->sessionId.c_str());
    if (session->cdm) {
        session->cdm->CloseSession(3, session->sessionId.c_str(), session->sessionId.size());
    }
    return ERROR_NONE;
}

EXTERNAL OpenCDMError opencdm_session_remove(struct OpenCDMSession* session) {
    if (!session) return ERROR_NONE;
    fprintf(stderr, "[OpenCDM Shim] opencdm_session_remove: session %s\n", session->sessionId.c_str());
    if (session->cdm) {
        session->cdm->RemoveSession(4, session->sessionId.c_str(), session->sessionId.size());
    }
    return ERROR_NONE;
}

EXTERNAL OpenCDMError opencdm_session_load(struct OpenCDMSession* session) {
    return ERROR_NONE;
}

EXTERNAL struct OpenCDMSession* opencdm_get_system_session(struct OpenCDMSystem* system,
    const uint8_t keyId[], const uint8_t length, const uint32_t waitTime) {

    if (!system) return nullptr;
    std::lock_guard<std::mutex> lk(system->lock);

    std::vector<uint8_t> kid(keyId, keyId + length);
    for (auto* s : system->sessions) {
        std::lock_guard<std::mutex> slk(s->lock);
        if (s->keys.find(kid) != s->keys.end() || s->defaultKeyId == kid || length == 0) {
            s->refCount++;
            fprintf(stderr, "[OpenCDM Shim] opencdm_get_system_session: Found session %s for key\n", s->sessionId.c_str());
            return s;
        }
    }

    if (!system->sessions.empty()) {
        auto* s = system->sessions.front();
        s->refCount++;
        fprintf(stderr, "[OpenCDM Shim] opencdm_get_system_session: Returning fallback session %s\n", s->sessionId.c_str());
        return s;
    }

    fprintf(stderr, "[OpenCDM Shim] opencdm_get_system_session: No session found\n");
    return nullptr;
}

// --- Core GStreamer Decryption Implementation ---

static OpenCDMError do_decrypt_sample(
    OpenCDMSession* session,
    GstBuffer* buffer,
    GstBuffer* subSampleBuffer,
    const uint32_t subSampleCount,
    GstBuffer* IV,
    GstBuffer* keyID,
    cdm::EncryptionScheme scheme = cdm::EncryptionScheme::kCenc,
    uint32_t crypt_byte_block = 0,
    uint32_t skip_byte_block = 0) {

    if (!session || !session->cdm) {
        return ERROR_INVALID_SESSION;
    }

    GstMapInfo dataMap;
    if (!gst_buffer_map(buffer, &dataMap, GST_MAP_READWRITE)) {
        fprintf(stderr, "[OpenCDM Shim] Decrypt: Failed to map data buffer\n");
        return ERROR_INVALID_DECRYPT_BUFFER;
    }

    GstMapInfo ivMap;
    if (!gst_buffer_map(IV, &ivMap, GST_MAP_READ)) {
        gst_buffer_unmap(buffer, &dataMap);
        fprintf(stderr, "[OpenCDM Shim] Decrypt: Failed to map IV buffer\n");
        return ERROR_INVALID_DECRYPT_BUFFER;
    }

    GstMapInfo keyIDMap;
    uint8_t* mappedKeyID = nullptr;
    uint32_t mappedKeyIDSize = 0;
    if (keyID && gst_buffer_map(keyID, &keyIDMap, GST_MAP_READ)) {
        mappedKeyID = keyIDMap.data;
        mappedKeyIDSize = keyIDMap.size;
    }

    // Parse subsamples if present
    std::vector<cdm::SubsampleEntry> subsamples;
    GstMapInfo subSampleMap;
    if (subSampleBuffer && subSampleCount > 0 && gst_buffer_map(subSampleBuffer, &subSampleMap, GST_MAP_READ)) {
        GstByteReader reader;
        gst_byte_reader_init(&reader, subSampleMap.data, subSampleMap.size);
        for (uint32_t i = 0; i < subSampleCount; ++i) {
            uint16_t clear_bytes = 0;
            uint32_t cipher_bytes = 0;
            if (gst_byte_reader_get_uint16_be(&reader, &clear_bytes) &&
                gst_byte_reader_get_uint32_be(&reader, &cipher_bytes)) {
                subsamples.push_back({clear_bytes, cipher_bytes});
            }
        }
        gst_buffer_unmap(subSampleBuffer, &subSampleMap);
    }

    // Determine Key ID
    const uint8_t* actualKeyId = mappedKeyID;
    uint32_t actualKeyIdSize = mappedKeyIDSize;
    if (!actualKeyId || actualKeyIdSize == 0) {
        std::lock_guard<std::mutex> lk(session->lock);
        if (!session->defaultKeyId.empty()) {
            actualKeyId = session->defaultKeyId.data();
            actualKeyIdSize = session->defaultKeyId.size();
        }
    }

    cdm::InputBuffer_2 input_buf = {};
    input_buf.data = dataMap.data;
    input_buf.data_size = dataMap.size;
    input_buf.encryption_scheme = scheme;
    input_buf.key_id = actualKeyId;
    input_buf.key_id_size = actualKeyIdSize;
    input_buf.iv = ivMap.data;
    input_buf.iv_size = ivMap.size;
    cdm::SubsampleEntry whole_sample = { 0, (uint32_t)dataMap.size };
    if (!subsamples.empty()) {
        input_buf.subsamples = subsamples.data();
        input_buf.num_subsamples = subsamples.size();
    } else {
        input_buf.subsamples = &whole_sample;
        input_buf.num_subsamples = 1;
    }
    input_buf.pattern.crypt_byte_block = crypt_byte_block;
    input_buf.pattern.skip_byte_block = skip_byte_block;

    CdmSimpleDecryptedBlock dec_block;
    cdm::Status status = session->cdm->Decrypt(input_buf, &dec_block);

    static uint64_t s_sampleCount = 0;
    s_sampleCount++;

    if (status == cdm::kSuccess) {
        cdm::Buffer* dec_buffer = dec_block.DecryptedBuffer();
        if (dec_buffer && dec_buffer->Data()) {
            if (dec_buffer->Size() == dataMap.size || subsamples.empty()) {
                uint32_t copy_size = std::min<uint32_t>(dec_buffer->Size(), dataMap.size);
                memcpy(dataMap.data, dec_buffer->Data(), copy_size);
            } else {
                // Widevine returned only decrypted cipher bytes; reconstruct back into buffer over clear positions
                uint8_t* dec_ptr = dec_buffer->Data();
                uint8_t* out_ptr = dataMap.data;
                size_t rem_dec = dec_buffer->Size();
                for (const auto& sub : subsamples) {
                    out_ptr += sub.clear_bytes;
                    uint32_t to_copy = std::min<uint32_t>(sub.cipher_bytes, rem_dec);
                    memcpy(out_ptr, dec_ptr, to_copy);
                    out_ptr += to_copy;
                    dec_ptr += to_copy;
                    rem_dec -= to_copy;
                    if (rem_dec == 0) break;
                }
            }
            dec_buffer->Destroy();
        }
        if (s_sampleCount % 50 == 1) {
            fprintf(stderr, "[OpenCDM Shim] DECRYPT SUCCESS: sample #%lu (size=%u, subsamples=%zu, scheme=%s)\n",
                    s_sampleCount, (uint32_t)dataMap.size, subsamples.size(),
                    scheme == cdm::EncryptionScheme::kCbcs ? "cbcs" : "cenc");
        }
    } else {
        fprintf(stderr, "[OpenCDM Shim] DECRYPT FAILED: sample #%lu returned status %d (size=%u, scheme=%s, iv_len=%u)\n",
                s_sampleCount, (int)status, (uint32_t)dataMap.size,
                scheme == cdm::EncryptionScheme::kCbcs ? "cbcs" : "cenc",
                (uint32_t)ivMap.size);
    }

    if (keyID && mappedKeyID) {
        gst_buffer_unmap(keyID, &keyIDMap);
    }
    gst_buffer_unmap(IV, &ivMap);
    gst_buffer_unmap(buffer, &dataMap);

    if (status == cdm::kSuccess) return ERROR_NONE;
    if (status == cdm::kNoKey) return ERROR_INVALID_ACCESSOR;
    return ERROR_UNKNOWN;
}

EXTERNAL OpenCDMError opencdm_gstreamer_session_decrypt(
    struct OpenCDMSession* session,
    GstBuffer* buffer,
    GstBuffer* subSample,
    const uint32_t subSampleCount,
    GstBuffer* IV,
    GstBuffer* keyID,
    uint32_t initWithLast15) {

    return do_decrypt_sample(session, buffer, subSample, subSampleCount, IV, keyID);
}

EXTERNAL OpenCDMError opencdm_gstreamer_session_decrypt_buffer(
    struct OpenCDMSession* session,
    GstBuffer* buffer,
    GstCaps* caps) {

    if (!session || !buffer) return ERROR_INVALID_SESSION;

    GstProtectionMeta* protectionMeta = (GstProtectionMeta*)gst_buffer_get_protection_meta(buffer);
    if (!protectionMeta) {
        fprintf(stderr, "[OpenCDM Shim] decrypt_buffer: Buffer has no protection meta\n");
        return ERROR_NONE;
    }

    GstBuffer* ivBuffer = nullptr;
    const GValue* val = gst_structure_get_value(protectionMeta->info, "iv");
    if (!val) val = gst_structure_get_value(protectionMeta->info, "constant_iv");
    if (val) ivBuffer = gst_value_get_buffer(val);

    GstBuffer* keyIDBuffer = nullptr;
    val = gst_structure_get_value(protectionMeta->info, "kid");
    if (val) keyIDBuffer = gst_value_get_buffer(val);

    unsigned subSampleCount = 0;
    gst_structure_get_uint(protectionMeta->info, "subsample_count", &subSampleCount);

    GstBuffer* subSamplesBuffer = nullptr;
    if (subSampleCount > 0) {
        val = gst_structure_get_value(protectionMeta->info, "subsamples");
        if (val) subSamplesBuffer = gst_value_get_buffer(val);
    }

    const char* cipher_mode = nullptr;
    if (gst_structure_has_field(protectionMeta->info, "cipher-mode")) {
        cipher_mode = gst_structure_get_string(protectionMeta->info, "cipher-mode");
    } else if (gst_structure_has_field(protectionMeta->info, "cipher_mode")) {
        cipher_mode = gst_structure_get_string(protectionMeta->info, "cipher_mode");
    }

    uint32_t crypt_byte_block = 0;
    uint32_t skip_byte_block = 0;
    if (!gst_structure_get_uint(protectionMeta->info, "crypt_byte_block", &crypt_byte_block)) {
        gst_structure_get_uint(protectionMeta->info, "crypt-byte-block", &crypt_byte_block);
    }
    if (!gst_structure_get_uint(protectionMeta->info, "skip_byte_block", &skip_byte_block)) {
        gst_structure_get_uint(protectionMeta->info, "skip-byte-block", &skip_byte_block);
    }

    if (caps) {
        GstStructure* caps_s = gst_caps_get_structure(caps, 0);
        if (caps_s) {
            if (!cipher_mode) {
                if (gst_structure_has_field(caps_s, "cipher-mode")) {
                    cipher_mode = gst_structure_get_string(caps_s, "cipher-mode");
                } else if (gst_structure_has_field(caps_s, "cipher_mode")) {
                    cipher_mode = gst_structure_get_string(caps_s, "cipher_mode");
                }
            }
            if (crypt_byte_block == 0 && skip_byte_block == 0) {
                if (!gst_structure_get_uint(caps_s, "crypt_byte_block", &crypt_byte_block)) {
                    gst_structure_get_uint(caps_s, "crypt-byte-block", &crypt_byte_block);
                }
                if (!gst_structure_get_uint(caps_s, "skip_byte_block", &skip_byte_block)) {
                    gst_structure_get_uint(caps_s, "skip-byte-block", &skip_byte_block);
                }
            }
        }
    }

    cdm::EncryptionScheme scheme = cdm::EncryptionScheme::kCenc;
    if (cipher_mode) {
        if (strcasecmp(cipher_mode, "cbcs") == 0 || strcasecmp(cipher_mode, "cbc1") == 0) {
            scheme = cdm::EncryptionScheme::kCbcs;
        } else if (strcasecmp(cipher_mode, "cenc") == 0 || strcasecmp(cipher_mode, "cens") == 0) {
            scheme = cdm::EncryptionScheme::kCenc;
        }
    }

    return do_decrypt_sample(session, buffer, subSamplesBuffer, subSampleCount, ivBuffer, keyIDBuffer,
                             scheme, crypt_byte_block, skip_byte_block);
}

} // extern "C"
