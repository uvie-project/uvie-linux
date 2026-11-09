/*
 * uvie-config.h — tiny shared-settings reader for the C/C++ engines.
 *
 * Reads ~/.config/uvie/settings.json (the file uvie-ui writes and
 * uvie-inputd reads) with dumb substring scans — pretty-printed
 * serde_json, so `"key": value` always appears on its own line.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#ifndef UVIE_CONFIG_H_
#define UVIE_CONFIG_H_

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#include "uvie.h"

#define UVIE_MAX_EXCLUDED 32

typedef struct UvieSettings {
    int enabled;           /* master on/off (daemon toggle) */
    int input_method;      /* 0 telex, 1 vni, 2 simple-telex */
    int quick_start;
    int quick_telex;
    int modern_orthography;
    int relaxed_coda;
    int english_override;  /* default on, matching Settings::default */
    int auto_capitalize;
    int macro_enabled;
    /* Vi/En toggle chord: evdev key code + required modifiers. */
    int hotkey_key;
    int hotkey_ctrl;
    int hotkey_alt;
    int hotkey_shift;
    /* App ids the IME ignores entirely (fcitx5 honors via program()). */
    char excluded[UVIE_MAX_EXCLUDED][64];
    int excluded_count;
} UvieSettings;

static const char *uvie_config_path(char *buf, size_t len,
                                    const char *filename) {
    const char *xdg = getenv("XDG_CONFIG_HOME");
    const char *home = getenv("HOME");
    if (xdg && *xdg) {
        snprintf(buf, len, "%s/uvie/%s", xdg, filename);
    } else if (home && *home) {
        snprintf(buf, len, "%s/.config/uvie/%s", home, filename);
    } else {
        buf[0] = '\0';
    }
    return buf;
}

static const char *uvie_settings_path(char *buf, size_t len) {
    return uvie_config_path(buf, len, "settings.json");
}

static const char *uvie_macros_path(char *buf, size_t len) {
    return uvie_config_path(buf, len, "macros.json");
}

static int uvie_json_bool(const char *text, const char *key, int fallback) {
    char needle[64];
    snprintf(needle, sizeof(needle), "\"%s\"", key);
    const char *p = strstr(text, needle);
    if (!p)
        return fallback;
    p += strlen(needle);
    while (*p == ' ' || *p == ':' || *p == '\t')
        p++;
    if (strncmp(p, "true", 4) == 0)
        return 1;
    if (strncmp(p, "false", 5) == 0)
        return 0;
    return fallback;
}

static int uvie_json_int(const char *text, const char *key, int fallback) {
    char needle[64];
    snprintf(needle, sizeof(needle), "\"%s\"", key);
    const char *p = strstr(text, needle);
    if (!p)
        return fallback;
    p += strlen(needle);
    while (*p == ' ' || *p == ':' || *p == '\t')
        p++;
    char *end = NULL;
    long v = strtol(p, &end, 10);
    return end && end != p ? (int)v : fallback;
}

static int uvie_json_method(const char *text) {
    const char *p = strstr(text, "\"input_method\"");
    if (!p)
        return 0;
    p += strlen("\"input_method\"");
    /* bound the search to the value's own line */
    char window[80];
    snprintf(window, sizeof(window), "%.79s", p);
    if (strstr(window, "\"vni\""))
        return 1;
    if (strstr(window, "\"simple-telex\"") || strstr(window, "\"simple_telex\""))
        return 2;
    return 0;
}

/* Parse the "hotkey" nested object { "key": N, "ctrl": B, "alt": B,
 * "shift": B }. Anything missing keeps its current cfg value. */
static void uvie_json_hotkey(const char *text, UvieSettings *cfg) {
    const char *h = strstr(text, "\"hotkey\"");
    if (!h)
        return;
    const char *open = strchr(h, '{');
    if (!open)
        return;
    const char *close = strchr(open, '}');
    if (!close)
        return;
    char window[512];
    size_t n = (size_t)(close - open) < sizeof(window) - 1
                   ? (size_t)(close - open)
                   : sizeof(window) - 1;
    memcpy(window, open, n);
    window[n] = '\0';
    cfg->hotkey_key = uvie_json_int(window, "key", cfg->hotkey_key);
    cfg->hotkey_ctrl = uvie_json_bool(window, "ctrl", cfg->hotkey_ctrl);
    cfg->hotkey_alt = uvie_json_bool(window, "alt", cfg->hotkey_alt);
    cfg->hotkey_shift = uvie_json_bool(window, "shift", cfg->hotkey_shift);
}

/* Collect the quoted strings inside `"key": [ "a", "b" ]` into cfg. */
static void uvie_json_excluded(const char *text, UvieSettings *cfg) {
    const char *p = strstr(text, "\"excluded_apps\"");
    if (!p)
        return;
    const char *open = strchr(p, '[');
    if (!open)
        return;
    const char *close = strchr(open, ']');
    if (!close)
        return;
    const char *q = open;
    while (cfg->excluded_count < UVIE_MAX_EXCLUDED) {
        const char *s = strchr(q, '"');
        if (!s || s >= close)
            break;
        const char *e = strchr(s + 1, '"');
        if (!e || e >= close)
            break;
        size_t n = (size_t)(e - s - 1);
        if (n > 63)
            n = 63;
        memcpy(cfg->excluded[cfg->excluded_count], s + 1, n);
        cfg->excluded[cfg->excluded_count][n] = '\0';
        cfg->excluded_count++;
        q = e + 1;
    }
}

/* Case-insensitive membership check, mirroring Settings::is_excluded. */
static int uvie_cfg_app_excluded(const UvieSettings *cfg, const char *app) {
    if (!app || !*app)
        return 0;
    for (int i = 0; i < cfg->excluded_count; i++) {
        if (strcasecmp(cfg->excluded[i], app) == 0)
            return 1;
    }
    return 0;
}

static void uvie_config_load(UvieSettings *cfg) {
    char path[1024];
    cfg->enabled = 1;
    cfg->input_method = 0;
    cfg->quick_start = 0;
    cfg->quick_telex = 0;
    cfg->modern_orthography = 0;
    cfg->relaxed_coda = 0;
    cfg->english_override = 1;
    cfg->auto_capitalize = 0;
    cfg->macro_enabled = 0;
    /* Ctrl+Shift+Z — same default as Settings::hotkey (KEY_Z = 44). */
    cfg->hotkey_key = 44;
    cfg->hotkey_ctrl = 1;
    cfg->hotkey_alt = 0;
    cfg->hotkey_shift = 1;
    cfg->excluded_count = 0;

    uvie_settings_path(path, sizeof(path));
    if (!*path)
        return;
    FILE *f = fopen(path, "rb");
    if (!f)
        return;
    char *text = (char *)malloc(1 << 16);
    if (!text) {
        fclose(f);
        return;
    }
    size_t n = fread(text, 1, (1 << 16) - 1, f);
    fclose(f);
    text[n] = '\0';

    cfg->enabled = uvie_json_bool(text, "enabled", 1);
    cfg->input_method = uvie_json_method(text);
    cfg->quick_start = uvie_json_bool(text, "quick_start", 0);
    cfg->quick_telex = uvie_json_bool(text, "quick_telex", 0);
    cfg->modern_orthography = uvie_json_bool(text, "modern_orthography", 0);
    cfg->relaxed_coda = uvie_json_bool(text, "relaxed_coda", 0);
    cfg->english_override = uvie_json_bool(text, "english_override", 1);
    cfg->auto_capitalize = uvie_json_bool(text, "auto_capitalize", 0);
    cfg->macro_enabled = uvie_json_bool(text, "macro_enabled", 0);
    uvie_json_hotkey(text, cfg);
    uvie_json_excluded(text, cfg);
    free(text);
}

/* Apply settings to the dispatcher and (re)load the macro table. */
static void uvie_config_apply_dispatch(UvieDispatcher *d,
                                       const UvieSettings *cfg) {
    char path[1024];
    uvie_dispatch_apply_options(d, cfg->input_method, cfg->quick_start,
                                cfg->quick_telex, cfg->modern_orthography,
                                cfg->relaxed_coda, cfg->english_override);
    uvie_dispatch_set_auto_capitalize(d, cfg->auto_capitalize);
    uvie_dispatch_set_macro_enabled(d, cfg->macro_enabled);
    /* These engines mirror output in a preedit buffer and commit on word
     * boundaries — they cannot rewrite text already committed to the
     * app, so LabanKey post-commit editing stays off (daemon keeps it). */
    uvie_dispatch_set_edit_committed(d, 0);
    uvie_macros_path(path, sizeof(path));
    uvie_dispatch_load_macros(d, path);
}

#endif /* UVIE_CONFIG_H_ */
