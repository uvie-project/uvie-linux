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

    UvieEngine *uvie;
    /* Everything the engine rendered since the last commit — the diff's
     * backspaces never reach further back than this buffer. */
    GString *preedit;
    /* How much of engine committed_text() we already forwarded. */
    gsize flushed;
    UvieSettings cfg;
};

G_DEFINE_TYPE(IBusUvieEngine, ibus_uvie_engine, IBUS_TYPE_ENGINE)

static void ibus_uvie_engine_update_preedit(IBusUvieEngine *uvie);
static void ibus_uvie_engine_commit_pending(IBusUvieEngine *uvie);

/* ------------------------------------------------------------------ */
/* Engine class                                                        */
/* ------------------------------------------------------------------ */

static void ibus_uvie_engine_init(IBusUvieEngine *uvie) {
    uvie->uvie = uvie_engine_new();
    uvie->preedit = g_string_new("");
    uvie->flushed = 0;
    uvie_config_load(&uvie->cfg);
    uvie_config_apply(uvie->uvie, &uvie->cfg);
}

static void ibus_uvie_engine_destroy(IBusUvieEngine *uvie) {
    if (uvie->uvie) {
        uvie_engine_free(uvie->uvie);
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
    uvie_engine_reset(uvie->uvie);
    g_string_set_size(uvie->preedit, 0);
    uvie->flushed = 0;
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
    uvie_config_apply(uvie->uvie, &uvie->cfg);
}

/* Apply (backspaces, suffix) onto the preedit buffer. */
static void preedit_apply(IBusUvieEngine *uvie, gsize backspaces,
                          const char *suffix) {
    GString *p = uvie->preedit;
    while (backspaces-- > 0 && p->len > 0) {
        /* erase one UTF-8 char */
        gchar *prev = g_utf8_prev_char(p->str + p->len);
        g_string_truncate(p, prev - p->str);
    }
    if (suffix && *suffix) {
        g_string_append(p, suffix);
    }
}

/* Flush newly-committed engine output (V-C-V auto-committed syllables):
 * forward the delta to the app so the preedit stays only the live part. */
static void flush_committed(IBusUvieEngine *uvie) {
    char buf[512];
    gsize total = uvie_engine_committed_text(uvie->uvie, buf, sizeof(buf));
    if (total <= uvie->flushed) {
        return;
    }
    /* the delta is buf[flushed..total] */
    gchar *delta = g_strndup(buf + uvie->flushed, total - uvie->flushed);
    IBusText *text = ibus_text_new_from_string(delta);
    ibus_engine_commit_text((IBusEngine *)uvie, text);
    g_free(delta);
    /* drop the same byte count off the front of the preedit */
    g_string_erase(uvie->preedit, 0, total - uvie->flushed);
    uvie->flushed = total;
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
    flush_committed(uvie);
    commit_all(uvie);
    ibus_uvie_engine_reset_state(uvie);
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

static gboolean is_break_keysym(guint keyval) {
    switch (keyval) {
    case IBUS_KEY_Return:
    case IBUS_KEY_KP_Enter:
    case IBUS_KEY_Tab:
    case IBUS_KEY_Escape:
        return TRUE;
    default:
        return FALSE;
    }
}

static gboolean is_nav_keysym(guint keyval) {
    return (keyval >= IBUS_KEY_Home && keyval <= IBUS_KEY_Begin) ||
           keyval == IBUS_KEY_Left || keyval == IBUS_KEY_Right ||
           keyval == IBUS_KEY_Up || keyval == IBUS_KEY_Down ||
           keyval == IBUS_KEY_Page_Up || keyval == IBUS_KEY_Page_Down ||
           keyval == IBUS_KEY_Insert || keyval == IBUS_KEY_Delete;
}

static gboolean ibus_uvie_engine_process_key_event(IBusEngine *engine,
                                                   guint keyval,
                                                   guint keycode,
                                                   guint modifiers) {
    IBusUvieEngine *uvie = (IBusUvieEngine *)engine;
    char out[512];

    if (modifiers & IBUS_RELEASE_MASK) {
        return FALSE;
    }

    /* Hard modifiers: never compose through them. Commit pending text,
     * let the accelerator reach the app. */
    if (modifiers & (IBUS_CONTROL_MASK | IBUS_MOD1_MASK | IBUS_SUPER_MASK |
                     IBUS_HYPER_MASK | IBUS_META_MASK)) {
        if (uvie->preedit->len > 0 || uvie->flushed > 0) {
            ibus_uvie_engine_commit_pending(uvie);
        }
        return FALSE;
    }

    /* Backspace walks the engine's own history; a real delete only when
     * there is nothing to walk. */
    if (keyval == IBUS_KEY_BackSpace) {
        if (uvie->preedit->len == 0) {
            return FALSE;
        }
        gsize bs = uvie_engine_backspace(uvie->uvie, out, sizeof(out));
        preedit_apply(uvie, bs, out);
        ibus_uvie_engine_update_preedit(uvie);
        return TRUE;
    }

    /* Break/navigation keys: commit pending text, pass the key through. */
    if (is_break_keysym(keyval) || is_nav_keysym(keyval)) {
        ibus_uvie_engine_commit_pending(uvie);
        return FALSE;
    }

    /* Space commits the word and still goes through (user sees the space).
     * Macro expansion is a daemon feature; the IBus path keeps it simple. */
    if (keyval == IBUS_KEY_space || keyval == IBUS_KEY_KP_Space) {
        gsize bs = uvie_engine_commit(uvie->uvie, out, sizeof(out));
        preedit_apply(uvie, bs, out);
        ibus_uvie_engine_commit_pending(uvie);
        return FALSE;
    }

    /* Printable ASCII → the engine. */
    if (keyval >= 32 && keyval <= 126) {
        gsize bs = uvie_engine_feed(uvie->uvie, (char)keyval, out, sizeof(out));
        preedit_apply(uvie, bs, out);
        flush_committed(uvie);
        ibus_uvie_engine_update_preedit(uvie);
        return TRUE;
    }

    /* Anything else (function keys, IME keys, non-ASCII): commit and pass. */
    if (uvie->preedit->len > 0) {
        ibus_uvie_engine_commit_pending(uvie);
    }
    return FALSE;
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
