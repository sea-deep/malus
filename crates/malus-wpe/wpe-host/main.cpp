/*
 * malus-wpe-host: Dedicated Malus-owned WPE WebKit host executable.
 *
 * Communicates with Malus over stdin/stdout via line-delimited JSON-RPC messages.
 * Operates headlessly by default, configuring WebKit sandbox mounts and persistent
 * cookie and website data storage.
 */

#include <glib.h>
#include <glib-unix.h>
#include <gio/gio.h>
#include <wpe/webkit.h>
#include <jsc/jsc.h>
#include <wpe/fdo.h>
#include <wpe/unstable/fdo-shm.h>
#include <WPEToolingBackends/WindowViewBackend.h>
#include <unistd.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>

static GMainLoop* s_main_loop = nullptr;
static gboolean s_headless_mode = FALSE;
static gboolean s_malus_ipc = TRUE;
static const char* s_cookies_file = nullptr;
static const char* s_data_dir = nullptr;
static const char* s_cache_dir = nullptr;
static char** s_sandbox_paths_ro = nullptr;
static char** s_sandbox_paths_rw = nullptr;
static const char** s_uri_args = nullptr;

static guint64 s_pending_load_id = 0;
static gboolean s_initial_load_pending = TRUE;

static const GOptionEntry s_command_line_entries[] = {
    { "headless", 'h', 0, G_OPTION_ARG_NONE, &s_headless_mode, "Run in headless mode", nullptr },
    { "malus-ipc", 0, 0, G_OPTION_ARG_NONE, &s_malus_ipc, "Run in Malus IPC mode (default)", nullptr },
    { "cookies-file", 'c', 0, G_OPTION_ARG_FILENAME, &s_cookies_file, "Persistent cookie database path", "FILE" },
    { "data-dir", 0, 0, G_OPTION_ARG_FILENAME, &s_data_dir, "Website data directory", "DIRECTORY" },
    { "cache-dir", 0, 0, G_OPTION_ARG_FILENAME, &s_cache_dir, "Website cache directory", "DIRECTORY" },
    { "sandbox-path-ro", 0, 0, G_OPTION_ARG_FILENAME_ARRAY, &s_sandbox_paths_ro, "Mount path read-only inside sandbox", "PATH" },
    { "sandbox-path-rw", 0, 0, G_OPTION_ARG_FILENAME_ARRAY, &s_sandbox_paths_rw, "Mount path read-write inside sandbox", "PATH" },
    { G_OPTION_REMAINING, 0, 0, G_OPTION_ARG_FILENAME_ARRAY, &s_uri_args, nullptr, "[URL]" },
    { nullptr, 0, 0, G_OPTION_ARG_NONE, nullptr, nullptr, nullptr }
};

static char* json_escape_string(const char* str) {
    if (!str) return g_strdup("");
    GString* s = g_string_new("");
    for (const char* p = str; *p; p++) {
        switch (*p) {
            case '"': g_string_append(s, "\\\""); break;
            case '\\': g_string_append(s, "\\\\"); break;
            case '\b': g_string_append(s, "\\b"); break;
            case '\f': g_string_append(s, "\\f"); break;
            case '\n': g_string_append(s, "\\n"); break;
            case '\r': g_string_append(s, "\\r"); break;
            case '\t': g_string_append(s, "\\t"); break;
            default:
                if ((unsigned char)*p < 0x20)
                    g_string_append_printf(s, "\\u%04x", (unsigned int)*p);
                else
                    g_string_append_c(s, *p);
        }
    }
    return g_string_free(s, FALSE);
}

static void add_sandbox_path_safe(WebKitWebContext* ctx, const char* path, gboolean read_only) {
    if (!path || !path[0])
        return;
    if (g_file_test(path, G_FILE_TEST_EXISTS)) {
        webkit_web_context_add_path_to_sandbox(ctx, path, read_only);
    }
}

static gboolean decide_permission_request(WebKitWebView*, WebKitPermissionRequest* request, gpointer) {
    g_printerr("[malus-wpe-host] Accepting %s request\n", G_OBJECT_TYPE_NAME(request));
    webkit_permission_request_allow(request);
    return TRUE;
}

struct HeadlessFdoBackend {
    struct wpe_view_backend_exportable_fdo* exportable { nullptr };
};

static const struct wpe_view_backend_exportable_fdo_client s_fdo_client = {
    nullptr,
    nullptr,
    [](void* data, struct wpe_fdo_shm_exported_buffer* buffer) {
        auto* backend = static_cast<HeadlessFdoBackend*>(data);
        if (backend && backend->exportable) {
            wpe_view_backend_exportable_fdo_dispatch_release_shm_exported_buffer(backend->exportable, buffer);
            wpe_view_backend_exportable_fdo_dispatch_frame_complete(backend->exportable);
        }
    },
    nullptr,
    nullptr
};

static WebKitWebViewBackend* create_headless_view_backend(uint32_t width, uint32_t height) {
    wpe_fdo_initialize_shm();
    auto* backend = new HeadlessFdoBackend();
    backend->exportable = wpe_view_backend_exportable_fdo_create(&s_fdo_client, backend, width, height);
    struct wpe_view_backend* wpe_backend = wpe_view_backend_exportable_fdo_get_view_backend(backend->exportable);
    return webkit_web_view_backend_new(wpe_backend, [](gpointer data) {
        auto* b = static_cast<HeadlessFdoBackend*>(data);
        if (b) {
            if (b->exportable)
                wpe_view_backend_exportable_fdo_destroy(b->exportable);
            delete b;
        }
    }, backend);
}

static WebKitWebViewBackend* create_view_backend(bool headless, uint32_t width, uint32_t height) {
    if (headless) {
        return create_headless_view_backend(width, height);
    }
    auto windowBackend = std::make_unique<WPEToolingBackends::WindowViewBackend>(width, height);
    struct wpe_view_backend* wpe_backend = windowBackend->backend();
    return webkit_web_view_backend_new(wpe_backend, [](gpointer data) {
        delete static_cast<WPEToolingBackends::WindowViewBackend*>(data);
    }, windowBackend.release());
}

static gboolean on_unix_signal(gpointer) {
    g_printerr("[malus-wpe-host] Caught termination signal, quitting main loop\n");
    if (s_main_loop && g_main_loop_is_running(s_main_loop)) {
        g_main_loop_quit(s_main_loop);
    }
    return G_SOURCE_REMOVE;
}

int main(int argc, char* argv[]) {
    GError* error = nullptr;
    GOptionContext* context = g_option_context_new("- Malus WPE Host");
    g_option_context_add_main_entries(context, s_command_line_entries, nullptr);
    if (!g_option_context_parse(context, &argc, &argv, &error)) {
        g_printerr("Failed to parse options: %s\n", error->message);
        g_error_free(error);
        g_option_context_free(context);
        return 1;
    }
    g_option_context_free(context);

    s_main_loop = g_main_loop_new(nullptr, FALSE);

    // Register clean signal handlers
    g_unix_signal_add(SIGTERM, on_unix_signal, nullptr);
    g_unix_signal_add(SIGINT, on_unix_signal, nullptr);

    // Initialize WebKitNetworkSession
    WebKitNetworkSession* networkSession = nullptr;
    if (s_data_dir || s_cache_dir) {
        networkSession = webkit_network_session_new(s_data_dir, s_cache_dir);
    } else {
        networkSession = webkit_network_session_get_default();
    }

    if (s_cookies_file) {
        WebKitCookieManager* cookieManager = webkit_network_session_get_cookie_manager(networkSession);
        WebKitCookiePersistentStorage storageType = g_str_has_suffix(s_cookies_file, ".txt")
            ? WEBKIT_COOKIE_PERSISTENT_STORAGE_TEXT
            : WEBKIT_COOKIE_PERSISTENT_STORAGE_SQLITE;
        webkit_cookie_manager_set_persistent_storage(cookieManager, s_cookies_file, storageType);
        webkit_cookie_manager_set_accept_policy(cookieManager, WEBKIT_COOKIE_POLICY_ACCEPT_ALWAYS);
    }

    // Initialize WebKitWebContext
    WebKitWebContext* webContext = WEBKIT_WEB_CONTEXT(g_object_new(WEBKIT_TYPE_WEB_CONTEXT, nullptr));

    // Configure sandbox paths
    const char* execPath = g_getenv("WEBKIT_EXEC_PATH");
    if (execPath)
        add_sandbox_path_safe(webContext, execPath, TRUE);

    g_autofree char* selfExe = g_file_read_link("/proc/self/exe", nullptr);
    if (selfExe) {
        g_autofree char* selfDir = g_path_get_dirname(selfExe);
        add_sandbox_path_safe(webContext, selfDir, TRUE);
        g_autofree char* libDir = g_build_filename(selfDir, "..", "lib", nullptr);
        add_sandbox_path_safe(webContext, libDir, TRUE);
    }

    if (s_data_dir)
        add_sandbox_path_safe(webContext, s_data_dir, FALSE);
    if (s_cache_dir)
        add_sandbox_path_safe(webContext, s_cache_dir, FALSE);

    if (s_sandbox_paths_ro) {
        for (gsize i = 0; s_sandbox_paths_ro[i]; i++)
            add_sandbox_path_safe(webContext, s_sandbox_paths_ro[i], TRUE);
    }
    if (s_sandbox_paths_rw) {
        for (gsize i = 0; s_sandbox_paths_rw[i]; i++)
            add_sandbox_path_safe(webContext, s_sandbox_paths_rw[i], FALSE);
    }

    // WebKit settings
    WebKitSettings* settings = webkit_settings_new_with_settings(
        "enable-developer-extras", TRUE,
        "enable-webgl", TRUE,
        "enable-media-stream", TRUE,
        "enable-webrtc", TRUE,
        "enable-encrypted-media", TRUE,
        "enable-write-console-messages-to-stdout", TRUE,
        "media-playback-requires-user-gesture", FALSE,
        "media-playback-allows-inline", TRUE,
        nullptr
    );

    WebKitWebsitePolicies* defaultPolicies = webkit_website_policies_new_with_policies(
        "autoplay", WEBKIT_AUTOPLAY_ALLOW,
        nullptr
    );

    WebKitUserContentManager* ucm = webkit_user_content_manager_new();
    webkit_user_content_manager_register_script_message_handler(ucm, "malus", nullptr);

    WebKitWebViewBackend* viewBackend = create_view_backend(s_headless_mode, 1280, 720);

    WebKitWebView* webView = WEBKIT_WEB_VIEW(g_object_new(
        WEBKIT_TYPE_WEB_VIEW,
        "backend", viewBackend,
        "web-context", webContext,
        "network-session", networkSession,
        "settings", settings,
        "user-content-manager", ucm,
        "website-policies", defaultPolicies,
        nullptr
    ));

    g_object_unref(settings);
    g_object_unref(defaultPolicies);

    g_signal_connect(webView, "permission-request", G_CALLBACK(decide_permission_request), nullptr);

    g_signal_connect(webView, "load-failed", G_CALLBACK(+[](WebKitWebView*, WebKitLoadEvent, const gchar* uri, GError* err, gpointer) -> gboolean {
        g_printerr("[malus-wpe-host] Load failed uri=%s err=%s\n", uri ? uri : "(null)", err ? err->message : "unknown");
        return FALSE;
    }), nullptr);

    g_signal_connect(webView, "load-changed", G_CALLBACK(+[](WebKitWebView* view, WebKitLoadEvent loadEvent, gpointer) {
        if (loadEvent == WEBKIT_LOAD_FINISHED) {
            if (s_initial_load_pending) {
                s_initial_load_pending = FALSE;
                g_print("{\"event\":\"ready\",\"pid\":%d}\n", getpid());
                fflush(stdout);
            } else if (s_pending_load_id > 0) {
                const char* uri = webkit_web_view_get_uri(view);
                g_print("{\"id\":%" G_GUINT64_FORMAT ",\"result\":{\"url\":\"%s\"}}\n", s_pending_load_id, uri ? uri : "");
                fflush(stdout);
                s_pending_load_id = 0;
            }
        }
    }), nullptr);

    g_signal_connect(ucm, "script-message-received::malus", G_CALLBACK(+[](WebKitUserContentManager*, JSCValue* value, gpointer) {
        char* str = jsc_value_to_string(value);
        if (str && str[0] == '{') {
            g_print("%s\n", str);
        } else {
            char* esc = json_escape_string(str);
            g_print("{\"event\":\"log\",\"payload\":\"%s\"}\n", esc);
            g_free(esc);
        }
        fflush(stdout);
        g_free(str);
    }), nullptr);

    // Watch stdin for JSON IPC commands
    GIOChannel* stdinChannel = g_io_channel_unix_new(fileno(stdin));
    g_io_channel_set_flags(stdinChannel, G_IO_FLAG_NONBLOCK, nullptr);
    g_io_add_watch(stdinChannel, (GIOCondition)(G_IO_IN | G_IO_HUP | G_IO_ERR), +[](GIOChannel* channel, GIOCondition condition, gpointer data) -> gboolean {
        if (condition & (G_IO_HUP | G_IO_ERR)) {
            g_printerr("[malus-wpe-host] Stdin hangup/error, terminating main loop\n");
            g_main_loop_quit(s_main_loop);
            return FALSE;
        }

        auto* view = WEBKIT_WEB_VIEW(data);
        char* line = nullptr;
        gsize length = 0;
        GIOStatus status;
        while ((status = g_io_channel_read_line(channel, &line, &length, nullptr, nullptr)) == G_IO_STATUS_NORMAL) {
            if (line) {
                g_strstrip(line);
                if (strlen(line) > 0 && line[0] == '{') {
                    JSCContext* ctx = jsc_context_new();
                    JSCValue* root = jsc_value_new_from_json(ctx, line);
                    JSCValue* idVal = jsc_value_object_get_property(root, "id");
                    guint64 reqId = jsc_value_is_number(idVal) ? (guint64)jsc_value_to_double(idVal) : 0;
                    JSCValue* methodVal = jsc_value_object_get_property(root, "method");
                    char* method = jsc_value_to_string(methodVal);
                    JSCValue* paramsVal = jsc_value_object_get_property(root, "params");

                    if (g_strcmp0(method, "load_document") == 0) {
                        JSCValue* urlVal = jsc_value_object_get_property(paramsVal, "url");
                        JSCValue* htmlVal = jsc_value_object_get_property(paramsVal, "html");
                        char* urlStr = jsc_value_to_string(urlVal);
                        char* htmlStr = jsc_value_to_string(htmlVal);
                        s_pending_load_id = reqId;
                        webkit_web_view_load_alternate_html(view, htmlStr, urlStr, urlStr);
                        g_free(urlStr);
                        g_free(htmlStr);
                    } else if (g_strcmp0(method, "navigate") == 0) {
                        JSCValue* urlVal = jsc_value_object_get_property(paramsVal, "url");
                        char* urlStr = jsc_value_to_string(urlVal);
                        s_pending_load_id = reqId;
                        webkit_web_view_load_uri(view, urlStr);
                        g_free(urlStr);
                    } else if (g_strcmp0(method, "reload") == 0) {
                        s_pending_load_id = reqId;
                        webkit_web_view_reload(view);
                    } else if (g_strcmp0(method, "evaluate") == 0) {
                        JSCValue* exprVal = jsc_value_object_get_property(paramsVal, "expression");
                        char* exprStr = jsc_value_to_string(exprVal);
                        char* evalScript = g_strdup_printf(
                            "Promise.resolve().then(() => (%s)).then(res => {"
                            " window.webkit.messageHandlers.malus.postMessage(JSON.stringify({id: %" G_GUINT64_FORMAT ", result: (res === undefined ? null : res)}));"
                            "}).catch(err => {"
                            " window.webkit.messageHandlers.malus.postMessage(JSON.stringify({id: %" G_GUINT64_FORMAT ", error: (err && err.message) ? err.message : String(err)}));"
                            "});",
                            exprStr ? exprStr : "null",
                            reqId
                        );
                        webkit_web_view_evaluate_javascript(view, evalScript, -1, nullptr, nullptr, nullptr, nullptr, nullptr);
                        g_free(exprStr);
                        g_free(evalScript);
                    } else if (g_strcmp0(method, "call_function") == 0) {
                        JSCValue* fnVal = jsc_value_object_get_property(paramsVal, "function");
                        JSCValue* argsVal = jsc_value_object_get_property(paramsVal, "arguments");
                        char* fnStr = jsc_value_to_string(fnVal);
                        char* argsStr = jsc_value_to_json(argsVal, 0);
                        char* callScript = g_strdup_printf(
                            "Promise.resolve().then(() => {"
                            " const __f = (%s); return __f.apply(null, %s);"
                            "}).then(res => {"
                            " window.webkit.messageHandlers.malus.postMessage(JSON.stringify({id: %" G_GUINT64_FORMAT ", result: (res === undefined ? null : res)}));"
                            "}).catch(err => {"
                            " window.webkit.messageHandlers.malus.postMessage(JSON.stringify({id: %" G_GUINT64_FORMAT ", error: (err && err.message) ? err.message : String(err)}));"
                            "});",
                            fnStr ? fnStr : "null",
                            argsStr ? argsStr : "[]",
                            reqId
                        );
                        webkit_web_view_evaluate_javascript(view, callScript, -1, nullptr, nullptr, nullptr, nullptr, nullptr);
                        g_free(fnStr);
                        g_free(argsStr);
                        g_free(callScript);
                    } else if (g_strcmp0(method, "register_event_sink") == 0) {
                        JSCValue* nameVal = jsc_value_object_get_property(paramsVal, "name");
                        char* nameStr = jsc_value_to_string(nameVal);
                        char* bridgeScript = g_strdup_printf(
                            "window['%s'] = function(payload) { window.webkit.messageHandlers.malus.postMessage(JSON.stringify({event: '%s', payload: payload})); };",
                            nameStr ? nameStr : "",
                            nameStr ? nameStr : ""
                        );
                        webkit_web_view_evaluate_javascript(view, bridgeScript, -1, nullptr, nullptr, nullptr, nullptr, nullptr);
                        g_print("{\"id\":%" G_GUINT64_FORMAT ",\"result\":true}\n", reqId);
                        fflush(stdout);
                        g_free(nameStr);
                        g_free(bridgeScript);
                    } else if (g_strcmp0(method, "shutdown") == 0) {
                        g_print("{\"id\":%" G_GUINT64_FORMAT ",\"result\":true}\n", reqId);
                        fflush(stdout);
                        g_main_loop_quit(s_main_loop);
                    }

                    g_free(method);
                    g_object_unref(root);
                    g_object_unref(ctx);
                }
                g_free(line);
            }
        }

        if (status == G_IO_STATUS_EOF) {
            g_printerr("[malus-wpe-host] Stdin reached EOF, quitting main loop\n");
            g_main_loop_quit(s_main_loop);
            return FALSE;
        }

        return TRUE;
    }, webView);

    // Initial navigation
    const char* initialUrl = (s_uri_args && s_uri_args[0]) ? s_uri_args[0] : "about:blank";
    webkit_web_view_load_uri(webView, initialUrl);

    // Run the main loop
    g_main_loop_run(s_main_loop);

    // Orderly teardown
    g_io_channel_unref(stdinChannel);
    g_main_loop_unref(s_main_loop);
    g_object_unref(webView);
    g_object_unref(webContext);

    return 0;
}
