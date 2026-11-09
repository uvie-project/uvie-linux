/*
 * uvie.h — C API for the uvie Vietnamese input engine.
 *
 * Hand-written mirror of uvie-rs `src/ffi.rs`. Link against libuvie_ffi
 * (built by `cargo build --release -p uvie-ffi`) or the shipped static lib.
 *
 * All feed/backspace/commit functions use the diff API: they return a
 * backspace count and write the new output suffix into a caller-provided
 * buffer (always NUL-terminated on success).
 */

#ifndef UVIE_H
#define UVIE_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque engine handle. */
typedef struct UvieEngine UvieEngine;

/* InputMethod values for uvie_engine_set_input_method. */
enum UvieInputMethod {
    UVIE_INPUT_TELEX = 0,
    UVIE_INPUT_VNI = 1,
    UVIE_INPUT_SIMPLE_TELEX = 2,
};

/* Lifecycle */
UvieEngine *uvie_engine_new(void);
void uvie_engine_free(UvieEngine *engine);

/* Options */
void uvie_engine_set_input_method(UvieEngine *engine, int method);
int uvie_engine_get_input_method(const UvieEngine *engine);
void uvie_engine_set_quick_start(UvieEngine *engine, int enabled);
void uvie_engine_set_quick_telex(UvieEngine *engine, int enabled);
void uvie_engine_set_modern_orthography(UvieEngine *engine, int enabled);
void uvie_engine_set_relaxed_coda(UvieEngine *engine, int enabled);
void uvie_engine_set_english_override(UvieEngine *engine, int enabled);

/* Typing — each returns the number of backspaces the caller must send and
 * writes the new output suffix into out_buf (UTF-8). */
size_t uvie_engine_feed(UvieEngine *engine, char ch, char *out_buf, size_t out_len);
size_t uvie_engine_backspace(UvieEngine *engine, char *out_buf, size_t out_len);
size_t uvie_engine_commit(UvieEngine *engine, char *out_buf, size_t out_len);

/* LabanKey-style post-commit edit: caret_back is the distance back to the
 * end of the newest committed word. Returns backspaces + 1 when handled
 * (out_fwd_del receives forward-delete count), 0 when no match. */
size_t uvie_engine_edit_at(UvieEngine *engine, size_t caret_back, char ch,
                           char *out_buf, size_t out_len, size_t *out_fwd_del);

/* Reset all engine state. */
void uvie_engine_reset(UvieEngine *engine);

/* ------------------------------------------------------------------ */
/* uvie_dispatch_* — the shared host dispatcher (uvie-host) over the     */
/* same ABI. Frontends feed classified keys and get a consume/pass       */
/* decision plus an action plan; this gives them the daemon's behavior   */
/* (macros, auto-capitalize, escape-restore, caret/sentence state).      */
/* ------------------------------------------------------------------ */

/* Opaque dispatcher handle. */
typedef struct UvieDispatcher UvieDispatcher;

/* KeyKind codes for uvie_dispatch_feed. */
enum UvieKeyKind {
    UVIE_KIND_CHAR = 0,
    UVIE_KIND_BACKSPACE = 1,
    UVIE_KIND_ENTER = 2,
    UVIE_KIND_TAB = 3,
    UVIE_KIND_ESCAPE = 4,
    UVIE_KIND_LEFT = 5,
    UVIE_KIND_RIGHT = 6,
    UVIE_KIND_NAVIGATION = 7,
    UVIE_KIND_FORWARD_DELETE = 8,
    UVIE_KIND_BREAK = 9,
    UVIE_KIND_OTHER = 10,
};

/* `mods` bits for uvie_dispatch_feed. */
#define UVIE_MOD_CTRL (1u << 0)
#define UVIE_MOD_ALT (1u << 1)
#define UVIE_MOD_SHIFT (1u << 2)
#define UVIE_MOD_WIN (1u << 3)

/* `ctx` bits for uvie_dispatch_feed. */
#define UVIE_CTX_APP_EXCLUDED (1u << 0)
#define UVIE_CTX_NON_LATIN (1u << 1)

/* One plan action: type 0 = Backspace(n), 1 = ForwardDelete(n),
 * 2 = Text (NUL-terminated UTF-8). Apply in order. */
typedef struct UvieAction {
    int type;
    int n;
    char text[512];
} UvieAction;

/* Lifecycle */
UvieDispatcher *uvie_dispatch_new(void);
void uvie_dispatch_free(UvieDispatcher *d);

/* Engine options — same fields/defaults as the uvie_engine_set_* chain. */
void uvie_dispatch_apply_options(UvieDispatcher *d, int input_method,
                                 int quick_start, int quick_telex,
                                 int modern_orthography, int relaxed_coda,
                                 int english_override);

/* Language: 0 = Vietnamese, 1 = English. */
void uvie_dispatch_set_language(UvieDispatcher *d, int english);
int uvie_dispatch_get_language(const UvieDispatcher *d);
void uvie_dispatch_toggle_language(UvieDispatcher *d);

void uvie_dispatch_set_auto_capitalize(UvieDispatcher *d, int on);
void uvie_dispatch_set_macro_enabled(UvieDispatcher *d, int on);
/* Post-commit (LabanKey) editing of already-committed words. Keep OFF in
 * preedit-model frontends — they cannot rewrite text committed to the
 * app. uvie-inputd leaves it on. */
void uvie_dispatch_set_edit_committed(UvieDispatcher *d, int on);
/* Replace the macro table from `path` (macros.json); returns entry
 * count, 0 on missing/corrupt file. */
int uvie_dispatch_load_macros(UvieDispatcher *d, const char *path);

void uvie_dispatch_reset(UvieDispatcher *d);
void uvie_dispatch_on_app_switch(UvieDispatcher *d, const char *app);
void uvie_dispatch_on_pointer_down(UvieDispatcher *d);
int uvie_dispatch_is_composing(const UvieDispatcher *d);

/* Feed one key press. Returns 1 when the key is consumed (the frontend
 * swallows it), 0 when it should pass through to the app. `out` receives
 * up to `*n_out` actions; `*n_out` is updated to the action count. The
 * plan must be applied BEFORE a passed-through key reaches the app. */
int uvie_dispatch_feed(UvieDispatcher *d, int kind, unsigned int ch,
                       unsigned int mods, unsigned int ctx,
                       UvieAction *out, int *n_out);

/* Introspection — all return a byte count written (excluding terminator). */
int uvie_engine_is_composing(const UvieEngine *engine);
size_t uvie_engine_committed_text(const UvieEngine *engine, char *out_buf, size_t out_len);
size_t uvie_engine_current_output(const UvieEngine *engine, char *out_buf, size_t out_len);
size_t uvie_engine_raw_chars(const UvieEngine *engine, char *out_buf, size_t out_len);

#ifdef __cplusplus
}
#endif

#endif /* UVIE_H */
