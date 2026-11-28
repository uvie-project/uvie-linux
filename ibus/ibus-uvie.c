/*
 * ibus-uvie — UVie Vietnamese input engine for IBus.
 *
 * Thin shell around libuvie (the Rust engine, via uvie.h). The engine
 * keeps a `preedit` GString that mirrors exactly what the engine has
 * rendered since the last commit: every feed()/backspace() returns a
 * (backspaces, suffix) diff which we apply to that buffer, so real text
 * only reaches the application on word boundaries or focus changes.
 *
 * Options are shared with the daemon: ~/.config/uvie/settings.json
 * (written by uvie-ui).
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include <ibus.h>
#include <string.h>
#include <stdlib.h>
#include <ctype.h>

#include "uvie.h"
#include "uvie-config.h"
#include "x11-focus.h"

#define IBUS_TYPE_UVIE_ENGINE (ibus_uvie_engine_get_type())

/* ibus 1.5 headers don't declare an autoptr for IBusEngine, which
 * G_DECLARE_FINAL_TYPE's parent chain needs. */
G_DEFINE_AUTOPTR_CLEANUP_FUNC(IBusEngine, g_object_unref)

G_DECLARE_FINAL_TYPE(IBusUvieEngine, ibus_uvie_engine, IBUS, UVIE_ENGINE,
                     IBusEngine)

struct _IBusUvieEngine {
    IBusEngine parent;

    UvieDispatcher *uvie;
    /* Everything the dispatcher rendered since the last commit — a plan's
     * backspaces never reach further back than this buffer. */
    GString *preedit;
    UvieSettings cfg;
    UvieX11Focus focus;
};

G_DEFINE_TYPE(IBusUvieEngine, ibus_uvie_engine, IBUS_TYPE_ENGINE)

/* IBus creates one engine object per input context. Keep the process-wide
 * language and best-effort per-app memory outside those objects so the
 * per_app_language setting can choose either behavior. Values are the
 * uvie_dispatch language encoding plus one because zero is a valid value. */
static int uvie_global_language;
static GHashTable *uvie_app_languages;

static GHashTable *uvie_language_memory(void) {
    if (!uvie_app_languages) {
        uvie_app_languages =
            g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL);
    }
    return uvie_app_languages;
}

static void ibus_uvie_apply_language_policy(IBusUvieEngine *uvie,
                                            const char *app) {
    if (uvie->cfg.per_app_language) {
        if (app && *app) {
            gpointer remembered =
                g_hash_table_lookup(uvie_language_memory(), app);
            if (remembered) {
                uvie_dispatch_set_language(uvie->uvie,
                                           GPOINTER_TO_INT(remembered) - 1);
            }
        }
    } else {
        uvie_dispatch_set_language(uvie->uvie, uvie_global_language);
    }
}

static void ibus_uvie_note_language(IBusUvieEngine *uvie, const char *app) {
    int language = uvie_dispatch_get_language(uvie->uvie);
    uvie_global_language = language;
    if (uvie->cfg.per_app_language && app && *app) {
        g_hash_table_replace(uvie_language_memory(), g_strdup(app),
                             GINT_TO_POINTER(language + 1));
    }
}

static void ibus_uvie_engine_update_preedit(IBusUvieEngine *uvie);
static void ibus_uvie_engine_commit_pending(IBusUvieEngine *uvie);

/* ------------------------------------------------------------------ */
/* Engine class                                                        */
/* ------------------------------------------------------------------ */

static void ibus_uvie_engine_init(IBusUvieEngine *uvie) {
    uvie->uvie = uvie_dispatch_new();
    uvie->preedit = g_string_new("");
    uvie_config_load(&uvie->cfg);
    uvie_config_apply_dispatch(uvie->uvie, &uvie->cfg);
}

static void ibus_uvie_engine_destroy(IBusUvieEngine *uvie) {
    if (uvie->uvie) {
        uvie_dispatch_free(uvie->uvie);
        uvie->uvie = NULL;
    }
    if (uvie->preedit) {
        g_string_free(uvie->preedit, TRUE);
        uvie->preedit = NULL;
    }
    uvie_x11_focus_close(&uvie->focus);
    IBUS_OBJECT_CLASS(ibus_uvie_engine_parent_class)->destroy(
        (IBusObject *)uvie);
}

static void ibus_uvie_engine_class_init(IBusUvieEngineClass *klass);

static void ibus_uvie_engine_reset_state(IBusUvieEngine *uvie) {
    uvie_dispatch_reset(uvie->uvie);
    g_string_set_size(uvie->preedit, 0);
}

static void ibus_uvie_engine_reset(IBusEngine *engine) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    ibus_uvie_engine_commit_pending(uvie);
    ibus_engine_hide_preedit_text(engine);
}

static void ibus_uvie_engine_focus_out(IBusEngine *engine) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    ibus_uvie_engine_commit_pending(uvie);
    ibus_engine_hide_preedit_text(engine);
}

static void ibus_uvie_engine_focus_in(IBusEngine *engine) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    /* The next key belongs to the newly-focused app; do not let the 200ms
     * X11 focus cache apply the previous app's exclusion/language state. */
    uvie->focus.last_query_ms = 0;
}

static void ibus_uvie_engine_enable(IBusEngine *engine) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    /* Re-read settings each activation — the settings window may have
     * written new values while the engine was off. */
    uvie_config_load(&uvie->cfg);
    uvie_config_apply_dispatch(uvie->uvie, &uvie->cfg);
}

/* Apply one plan action onto the preedit buffer. */
static void preedit_action(IBusUvieEngine *uvie, const UvieAction *a) {
    GString *p = uvie->preedit;
    if (a->type == 0) {          /* Backspace n */
        int n = a->n;
        while (n-- > 0 && p->len > 0) {
            /* erase one UTF-8 char */
            gchar *prev = g_utf8_prev_char(p->str + p->len);
            g_string_truncate(p, prev - p->str);
        }
    } else if (a->type == 2 && a->text[0]) {   /* Text */
        g_string_append(p, a->text);
    }
    /* ForwardDelete (type 1) never arrives: post-commit editing is off on
     * the preedit-model engines (see uvie-config.h). */
}

static void commit_all(IBusUvieEngine *uvie) {
    if (uvie->preedit->len == 0) {
        return;
    }
    IBusText *text = ibus_text_new_from_string(uvie->preedit->str);
    ibus_engine_commit_text((IBusEngine *)uvie, text);
    g_string_set_size(uvie->preedit, 0);
}

static void ibus_uvie_engine_commit_pending(IBusUvieEngine *uvie) {
    commit_all(uvie);
    ibus_uvie_engine_reset_state(uvie);
    ibus_uvie_engine_update_preedit(uvie);
}

static void ibus_uvie_engine_update_preedit(IBusUvieEngine *uvie) {
    IBusEngine *engine = (IBusEngine *)uvie;
    if (uvie->preedit->len == 0) {
        ibus_engine_hide_preedit_text(engine);
        return;
    }
    IBusText *text = ibus_text_new_from_string(uvie->preedit->str);
    /* Underline the composing span. ibus_text_append_attribute keeps the
     * attr-list ownership inside `text`; setting a floating attr list then
     * unreffing it leaves `text` with a dangling pointer and crashes IBus. */
    ibus_text_append_attribute(text, IBUS_ATTR_TYPE_UNDERLINE,
                               IBUS_ATTR_UNDERLINE_SINGLE, 0, -1);
    ibus_engine_update_preedit_text_with_mode(
        engine, text, g_utf8_strlen(uvie->preedit->str, -1), TRUE,
        IBUS_ENGINE_PREEDIT_COMMIT);
}

/* Classify a keysym into the dispatcher's KeyKind + resolved codepoint.
 * Keysyms ≤ 0xFDFF are the unicode value for printable keys; the
 * 0xFE00-0xFFFF range holds dead keys and function keysyms, which fall
 * through to Other (pass-through). */
static int classify_keysym(guint keyval, unsigned int *ch) {
    *ch = 0;
    switch (keyval) {
    case IBUS_KEY_BackSpace:
        return UVIE_KIND_BACKSPACE;
    case IBUS_KEY_Return:
    case IBUS_KEY_KP_Enter:
        return UVIE_KIND_ENTER;
    case IBUS_KEY_Tab:
    case IBUS_KEY_KP_Tab:
    case IBUS_KEY_ISO_Left_Tab:
        return UVIE_KIND_TAB;
    case IBUS_KEY_Escape:
        return UVIE_KIND_ESCAPE;
    case IBUS_KEY_Left:
    case IBUS_KEY_KP_Left:
        return UVIE_KIND_LEFT;
    case IBUS_KEY_Right:
    case IBUS_KEY_KP_Right:
        return UVIE_KIND_RIGHT;
    case IBUS_KEY_Delete:
    case IBUS_KEY_KP_Delete:
        return UVIE_KIND_FORWARD_DELETE;
    }
    if ((keyval >= IBUS_KEY_Home && keyval <= IBUS_KEY_Begin) ||
        keyval == IBUS_KEY_Up || keyval == IBUS_KEY_Down ||
        keyval == IBUS_KEY_Page_Up || keyval == IBUS_KEY_Page_Down ||
        keyval == IBUS_KEY_Insert ||
        (keyval >= IBUS_KEY_KP_Home && keyval <= IBUS_KEY_KP_Begin) ||
        keyval == IBUS_KEY_KP_Up || keyval == IBUS_KEY_KP_Down ||
        keyval == IBUS_KEY_KP_Page_Up || keyval == IBUS_KEY_KP_Page_Down ||
        keyval == IBUS_KEY_KP_Insert) {
        return UVIE_KIND_NAVIGATION;
    }
    /* Numpad: digits feed the engine like their ASCII twins (VNI uses
     * them); the rest of the numpad operators map to their ASCII
     * equivalents so they still commit the word. */
    if (keyval >= IBUS_KEY_KP_0 && keyval <= IBUS_KEY_KP_9) {
        *ch = '0' + (keyval - IBUS_KEY_KP_0);
        return UVIE_KIND_CHAR;
    }
    switch (keyval) {
    case IBUS_KEY_KP_Space:
        *ch = ' ';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Add:
        *ch = '+';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Subtract:
        *ch = '-';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Multiply:
        *ch = '*';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Divide:
        *ch = '/';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Decimal:
        *ch = '.';
        return UVIE_KIND_CHAR;
    case IBUS_KEY_KP_Equal:
        *ch = '=';
        return UVIE_KIND_CHAR;
    }
    if (keyval >= 0x20 && keyval <= 0xFDFF) {
        *ch = keyval;
        return UVIE_KIND_CHAR;
    }
    return UVIE_KIND_OTHER;
}

static unsigned int mods_flags(guint modifiers) {
    unsigned int m = 0;
    if (modifiers & IBUS_CONTROL_MASK)
        m |= UVIE_MOD_CTRL;
    if (modifiers & IBUS_MOD1_MASK)
        m |= UVIE_MOD_ALT;
    if (modifiers & IBUS_SHIFT_MASK)
        m |= UVIE_MOD_SHIFT;
    if (modifiers & (IBUS_SUPER_MASK | IBUS_HYPER_MASK | IBUS_META_MASK))
        m |= UVIE_MOD_WIN;
    return m;
}

/* The Vi/En toggle chord from settings.json. The keycode delivered to
 * IBus engines is the evdev hardware code (for example Z is 44), the
 * same encoding used by uvie-inputd and stored in settings.json.
 * Exact modifiers only — an extra Super held means the chord belongs
 * to the WM. */
static gboolean is_toggle_chord(const UvieSettings *cfg, guint keycode,
                                guint modifiers) {
    if (cfg->hotkey_key <= 0)
        return FALSE;
    if (keycode != (guint)cfg->hotkey_key)
        return FALSE;
    if (!!(modifiers & IBUS_CONTROL_MASK) != !!cfg->hotkey_ctrl)
        return FALSE;
    if (!!(modifiers & IBUS_MOD1_MASK) != !!cfg->hotkey_alt)
        return FALSE;
    if (!!(modifiers & IBUS_SHIFT_MASK) != !!cfg->hotkey_shift)
        return FALSE;
    return !(modifiers & (IBUS_SUPER_MASK | IBUS_HYPER_MASK));
}

static gboolean ibus_uvie_engine_process_key_event(IBusEngine *engine,
                                                   guint keyval,
                                                   guint keycode,
                                                   guint modifiers) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    UvieAction actions[16];

    if (modifiers & IBUS_RELEASE_MASK) {
        return FALSE;
    }

    /* uvie-ui writes the same settings file as the daemon. Unlike the
     * daemon there is no timer here, so poll mtimes on the key path. */
    if (uvie_config_reload_if_changed(&uvie->cfg)) {
        uvie_config_apply_dispatch(uvie->uvie, &uvie->cfg);
        ibus_uvie_engine_commit_pending(uvie);
    }

    const char *app = uvie_x11_active_app(&uvie->focus);
    ibus_uvie_apply_language_policy(uvie, app);

    /* Master toggle off: finalize pending text and pass through. The
     * commit is unconditional — it also resets dispatcher state that the
     * preedit can't show (raw keys buffered with no output yet). */
    if (!uvie->cfg.enabled) {
        ibus_uvie_engine_commit_pending(uvie);
        return FALSE;
    }

    /* Vi/En toggle chord — swallowed, like the daemon's hotkey. */
    if (is_toggle_chord(&uvie->cfg, keycode, modifiers)) {
        ibus_uvie_engine_commit_pending(uvie);
        uvie_dispatch_toggle_language(uvie->uvie);
        ibus_uvie_note_language(uvie, app);
        return TRUE;
    }

    /* Hard modifiers: never compose through them. Commit pending text,
     * let the accelerator reach the app. */
    if (modifiers & (IBUS_CONTROL_MASK | IBUS_MOD1_MASK | IBUS_SUPER_MASK |
                     IBUS_HYPER_MASK | IBUS_META_MASK)) {
        ibus_uvie_engine_commit_pending(uvie);
        return FALSE;
    }

    /* Backspace with nothing mirrored: the app deletes committed text
     * natively — drop the engine's view of it, like the old path. */
    if (keyval == IBUS_KEY_BackSpace && uvie->preedit->len == 0 &&
        !uvie_dispatch_is_composing(uvie->uvie)) {
        return FALSE;
    }

    unsigned int ch = 0;
    int kind = classify_keysym(keyval, &ch);
    unsigned int ctx = 0;
    if (uvie_cfg_app_excluded(&uvie->cfg, app)) {
        ctx |= UVIE_CTX_APP_EXCLUDED;
    }
    int n = 16;
    int consumed =
        uvie_dispatch_feed(uvie->uvie, kind, ch, mods_flags(modifiers), ctx,
                           actions, &n);
    for (int i = 0; i < n; i++) {
        preedit_action(uvie, &actions[i]);
    }
    /* A finished word (dispatcher no longer composing) is final output —
     * commit it to the app whether the key was consumed or passed. */
    if (!uvie_dispatch_is_composing(uvie->uvie)) {
        commit_all(uvie);
    }
    ibus_uvie_engine_update_preedit(uvie);
    return consumed ? TRUE : FALSE;
}

static void ibus_uvie_engine_class_init(IBusUvieEngineClass *klass) {
    IBusObjectClass *object_class = IBUS_OBJECT_CLASS(klass);
    IBusEngineClass *engine_class = IBUS_ENGINE_CLASS(klass);

    object_class->destroy = (IBusObjectDestroyFunc)ibus_uvie_engine_destroy;
    engine_class->process_key_event = ibus_uvie_engine_process_key_event;
    engine_class->reset = ibus_uvie_engine_reset;
    engine_class->focus_out = ibus_uvie_engine_focus_out;
    engine_class->focus_in = ibus_uvie_engine_focus_in;
    engine_class->enable = ibus_uvie_engine_enable;
}

/* ------------------------------------------------------------------ */
/* Process entry: spawned by ibus-daemon via the component XML.        */
/* ------------------------------------------------------------------ */

static void ibus_disconnected_cb(IBusBus *bus, gpointer user_data) {
    (void)bus;
    (void)user_data;
    ibus_quit();
}

int main(int argc, char **argv) {
    ibus_init();

    IBusBus *bus = ibus_bus_new();
    g_signal_connect(bus, "disconnected", G_CALLBACK(ibus_disconnected_cb),
                     NULL);

    IBusFactory *factory = ibus_factory_new(ibus_bus_get_connection(bus));
    ibus_factory_add_engine(factory, "uvie", IBUS_TYPE_UVIE_ENGINE);

    ibus_bus_request_name(bus, "org.freedesktop.IBus.Uvie", 0);

    ibus_main();
    return 0;
}
