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

#include "uvie.h"

typedef struct UvieSettings {
    int input_method;      /* 0 telex, 1 vni, 2 simple-telex */
    int quick_start;
    int quick_telex;
    int modern_orthography;
    int relaxed_coda;
    int english_override;  /* default on, matching Settings::default */
} UvieSettings;

static const char *uvie_settings_path(char *buf, size_t len) {
    const char *xdg = getenv("XDG_CONFIG_HOME");
    const char *home = getenv("HOME");
    if (xdg && *xdg) {
        snprintf(buf, len, "%s/uvie/settings.json", xdg);
    } else if (home && *home) {
        snprintf(buf, len, "%s/.config/uvie/settings.json", home);
    } else {
        buf[0] = '\0';
    }
    return buf;
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

static void uvie_config_load(UvieSettings *cfg) {
    char path[1024];
    cfg->input_method = 0;
    cfg->quick_start = 0;
    cfg->quick_telex = 0;
    cfg->modern_orthography = 0;
    cfg->relaxed_coda = 0;
    cfg->english_override = 1;

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

    cfg->input_method = uvie_json_method(text);
    cfg->quick_start = uvie_json_bool(text, "quick_start", 0);
    cfg->quick_telex = uvie_json_bool(text, "quick_telex", 0);
    cfg->modern_orthography = uvie_json_bool(text, "modern_orthography", 0);
    cfg->relaxed_coda = uvie_json_bool(text, "relaxed_coda", 0);
    cfg->english_override = uvie_json_bool(text, "english_override", 1);
    free(text);
}

static void uvie_config_apply(UvieEngine *engine, const UvieSettings *cfg) {
    uvie_engine_set_input_method(engine, cfg->input_method);
    uvie_engine_set_quick_start(engine, cfg->quick_start);
    uvie_engine_set_quick_telex(engine, cfg->quick_telex);
    uvie_engine_set_modern_orthography(engine, cfg->modern_orthography);
    uvie_engine_set_relaxed_coda(engine, cfg->relaxed_coda);
    uvie_engine_set_english_override(engine, cfg->english_override);
}

#endif /* UVIE_CONFIG_H_ */
