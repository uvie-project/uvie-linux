/*
 * x11-focus.h — focused-application identity for the IBus engine.
 *
 * IBus does not expose the client's application name to engines. On X11
 * the focused top-level window's WM_CLASS is the same app key uvie-inputd
 * uses; on Wayland the compositor deliberately hides it, so this helper
 * returns no identity.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#ifndef UVIE_X11_FOCUS_H_
#define UVIE_X11_FOCUS_H_

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <xcb/xcb.h>

typedef struct UvieX11Focus {
    xcb_connection_t *conn;
    xcb_window_t root;
    xcb_atom_t active_atom;
    xcb_atom_t wm_class_atom;
    char app[64];
    int has_app;
    int connect_attempted;
    long long last_query_ms;
} UvieX11Focus;

static long long uvie_x11_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000LL + ts.tv_nsec / 1000000LL;
}

static void uvie_x11_focus_close(UvieX11Focus *f) {
    if (f->conn) {
        xcb_disconnect(f->conn);
        f->conn = NULL;
    }
    f->has_app = 0;
    f->app[0] = '\0';
    f->last_query_ms = 0;
}

static xcb_atom_t uvie_x11_intern(xcb_connection_t *conn, const char *name) {
    xcb_intern_atom_cookie_t cookie =
        xcb_intern_atom(conn, 1, (uint16_t)strlen(name), name);
    xcb_intern_atom_reply_t *reply =
        xcb_intern_atom_reply(conn, cookie, NULL);
    xcb_atom_t atom = XCB_ATOM_NONE;
    if (reply) {
        atom = reply->atom;
        free(reply);
    }
    return atom;
}

static int uvie_x11_focus_connect(UvieX11Focus *f) {
    if (f->conn)
        return 1;
    if (f->connect_attempted)
        return 0;
    f->connect_attempted = 1;

    /* XWayland exposes a DISPLAY on Wayland too, but its active window is
     * not necessarily the native client that sent this key. Do not risk
     * applying another app's exclusion state there. */
    const char *session_type = getenv("XDG_SESSION_TYPE");
    if (!getenv("DISPLAY") ||
        (session_type && strcmp(session_type, "wayland") == 0)) {
        return 0;
    }

    int preferred_screen = 0;
    f->conn = xcb_connect(NULL, &preferred_screen);
    if (!f->conn || xcb_connection_has_error(f->conn)) {
        uvie_x11_focus_close(f);
        return 0;
    }
    const xcb_setup_t *setup = xcb_get_setup(f->conn);
    xcb_screen_iterator_t screens = xcb_setup_roots_iterator(setup);
    for (int i = 0; screens.rem && i < preferred_screen; i++) {
        xcb_screen_next(&screens);
    }
    if (!screens.rem) {
        uvie_x11_focus_close(f);
        return 0;
    }
    f->root = screens.data->root;
    f->active_atom = uvie_x11_intern(f->conn, "_NET_ACTIVE_WINDOW");
    f->wm_class_atom = uvie_x11_intern(f->conn, "WM_CLASS");
    if (f->active_atom == XCB_ATOM_NONE ||
        f->wm_class_atom == XCB_ATOM_NONE) {
        uvie_x11_focus_close(f);
        return 0;
    }
    return 1;
}

static void uvie_x11_copy_app(char *dst, size_t dst_len, const char *src,
                              size_t src_len) {
    size_t i = 0;
    if (dst_len == 0)
        return;
    for (; i < src_len && src[i] && i + 1 < dst_len; i++) {
        dst[i] = (char)tolower((unsigned char)src[i]);
    }
    dst[i] = '\0';
}

static int uvie_x11_query_active_app(UvieX11Focus *f) {
    xcb_get_property_cookie_t active_cookie = xcb_get_property(
        f->conn, 0, f->root, f->active_atom, XCB_ATOM_WINDOW, 0, 1);
    xcb_get_property_reply_t *active_reply =
        xcb_get_property_reply(f->conn, active_cookie, NULL);
    if (!active_reply) {
        if (xcb_connection_has_error(f->conn))
            uvie_x11_focus_close(f);
        return 0;
    }
    xcb_window_t win = XCB_NONE;
    if (xcb_get_property_value_length(active_reply) > 0) {
        win = *(xcb_window_t *)xcb_get_property_value(active_reply);
    }
    free(active_reply);
    if (win == XCB_NONE)
        return 0;

    xcb_get_property_cookie_t class_cookie = xcb_get_property(
        f->conn, 0, win, f->wm_class_atom, XCB_GET_PROPERTY_TYPE_ANY, 0, 1024);
    xcb_get_property_reply_t *class_reply =
        xcb_get_property_reply(f->conn, class_cookie, NULL);
    if (!class_reply) {
        if (xcb_connection_has_error(f->conn))
            uvie_x11_focus_close(f);
        return 0;
    }

    int len = xcb_get_property_value_length(class_reply);
    const unsigned char *value =
        (const unsigned char *)xcb_get_property_value(class_reply);
    if (len <= 0 || !value) {
        free(class_reply);
        return 0;
    }

    /* WM_CLASS is instance\0class\0; prefer the class half, falling back
     * to the instance when the class is absent. */
    const unsigned char *nul = memchr(value, '\0', (size_t)len);
    const unsigned char *class_part =
        (nul && nul + 1 < value + len) ? nul + 1 : value;
    size_t class_len = (size_t)((value + len) - class_part);
    uvie_x11_copy_app(f->app, sizeof(f->app), (const char *)class_part,
                      class_len);
    free(class_reply);
    f->has_app = f->app[0] != '\0';
    return f->has_app;
}

/* Return the cached lowercase WM_CLASS app key, refreshing at most every
 * 200ms (the same polling cadence as uvie-inputd's X11 focus watcher). */
static const char *uvie_x11_active_app(UvieX11Focus *f) {
    if (!uvie_x11_focus_connect(f))
        return NULL;
    long long now = uvie_x11_now_ms();
    if (f->last_query_ms && now - f->last_query_ms < 200) {
        return f->has_app ? f->app : NULL;
    }
    f->last_query_ms = now;
    if (!uvie_x11_query_active_app(f))
        return NULL;
    return f->app;
}

#endif /* UVIE_X11_FOCUS_H_ */
