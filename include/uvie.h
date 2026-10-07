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

/* Introspection — all return a byte count written (excluding terminator). */
int uvie_engine_is_composing(const UvieEngine *engine);
size_t uvie_engine_committed_text(const UvieEngine *engine, char *out_buf, size_t out_len);
size_t uvie_engine_current_output(const UvieEngine *engine, char *out_buf, size_t out_len);
size_t uvie_engine_raw_chars(const UvieEngine *engine, char *out_buf, size_t out_len);

#ifdef __cplusplus
}
#endif

#endif /* UVIE_H */
