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
};

G_DEFINE_TYPE(IBusUvieEngine, ibus_uvie_engine, IBUS_TYPE_ENGINE)

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
    IBusAttrList *attrs = ibus_attr_list_new();
    /* underline the composing span */
    ibus_attr_list_append(attrs,
                          ibus_attr_underline_new(IBUS_ATTR_UNDERLINE_SINGLE,
                                                  0, uvie->preedit->len));
    ibus_text_set_attributes(text, attrs);
    g_object_unref(attrs);
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
    case IBUS_KEY_KP_Delete:
        return UVIE_KIND_BACKSPACE;
    case IBUS_KEY_Return:
    case IBUS_KEY_KP_Enter:
        return UVIE_KIND_ENTER;
    case IBUS_KEY_Tab:
    case IBUS_KEY_KP_Tab:
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

/* The Vi/En toggle chord from settings.json. IBus reports the hardware
 * keycode (evdev+8 on X11); accept both encodings so the chord matches
 * regardless of session type. Exact modifiers only — an extra Super held
 * means the chord belongs to the WM. */
static gboolean is_toggle_chord(const UvieSettings *cfg, guint keycode,
                                guint modifiers) {
    if (cfg->hotkey_key <= 0)
        return FALSE;
    if (keycode != (guint)cfg->hotkey_key &&
        keycode != (guint)cfg->hotkey_key + 8)
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

    /* Master toggle off: finalize any pending text and pass through. */
    if (!uvie->cfg.enabled) {
        if (uvie->preedit->len > 0) {
            ibus_uvie_engine_commit_pending(uvie);
        }
        return FALSE;
    }

    /* Vi/En toggle chord — swallowed, like the daemon's hotkey. */
    if (is_toggle_chord(&uvie->cfg, keycode, modifiers)) {
        ibus_uvie_engine_commit_pending(uvie);
        uvie_dispatch_toggle_language(uvie->uvie);
        return TRUE;
    }

    /* Hard modifiers: never compose through them. Commit pending text,
     * let the accelerator reach the app. */
    if (modifiers & (IBUS_CONTROL_MASK | IBUS_MOD1_MASK | IBUS_SUPER_MASK |
                     IBUS_HYPER_MASK | IBUS_META_MASK)) {
        if (uvie->preedit->len > 0) {
            ibus_uvie_engine_commit_pending(uvie);
        }
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
    int n = 16;
    int consumed =
        uvie_dispatch_feed(uvie->uvie, kind, ch, mods_flags(modifiers), 0,
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
