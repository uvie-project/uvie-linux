//! Focused-window identity for per-app memory and the exclusion list.
//!
//! X11: `_NET_ACTIVE_WINDOW` + `WM_CLASS` — works everywhere on X11,
//! including XWayland clients, for any session that exports DISPLAY.
//! Wayland: the compositor deliberately hides this — returns None and the
//! daemon runs in global mode (same limitation documented for Phase 1).

use std::time::{Duration, Instant};

use xcb::Xid as _;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

pub enum Focus {
    X11(X11Focus),
    /// No identity source — Wayland session (or X11 unavailable).
    None,
}

pub(crate) struct X11Focus {
    conn: xcb::Connection,
    root: xcb::x::Window,
    atom_active: xcb::x::Atom,
    atom_wm_class: xcb::x::Atom,
    last_poll: Option<Instant>,
    cached: Option<String>,
}

impl Focus {
    /// Connect lazily to the X server when DISPLAY is set.
    pub fn connect() -> Self {
        let Ok((conn, screen)) = xcb::Connection::connect(None) else {
            return Focus::None;
        };
        let Some(root) = conn
            .get_setup()
            .roots()
            .nth(screen as usize)
            .map(|s| s.root())
        else {
            return Focus::None;
        };
        let Some(atom_active) = intern(&conn, b"_NET_ACTIVE_WINDOW") else {
            return Focus::None;
        };
        let Some(atom_wm_class) = intern(&conn, b"WM_CLASS") else {
            return Focus::None;
        };
        Focus::X11(X11Focus {
            conn,
            root,
            atom_active,
            atom_wm_class,
            last_poll: None,
            cached: None,
        })
    }

    /// Identifier of the focused app (WM_CLASS res part / process-ish name).
    /// Polls at most every `POLL_INTERVAL` and caches between calls.
    pub fn active_app(&mut self) -> Option<&str> {
        match self {
            Focus::None => None,
            Focus::X11(f) => {
                if f.last_poll
                    .map(|t| t.elapsed() < POLL_INTERVAL)
                    .unwrap_or(false)
                {
                    return f.cached.as_deref();
                }
                f.last_poll = Some(Instant::now());
                f.cached = f.query_active();
                f.cached.as_deref()
            }
        }
    }
}

fn intern(conn: &xcb::Connection, name: &[u8]) -> Option<xcb::x::Atom> {
    let cookie = conn.send_request(&xcb::x::InternAtom {
        only_if_exists: true,
        name,
    });
    conn.wait_for_reply(cookie)
        .ok()
        .map(|r| r.atom())
        .filter(|a| *a != xcb::x::ATOM_NONE)
}

fn get_property_string(
    conn: &xcb::Connection,
    window: xcb::x::Window,
    atom: xcb::x::Atom,
) -> Option<Vec<u8>> {
    let cookie = conn.send_request(&xcb::x::GetProperty {
        delete: false,
        window,
        property: atom,
        r#type: xcb::x::ATOM_ANY,
        long_offset: 0,
        long_length: 1024,
    });
    let reply = conn.wait_for_reply(cookie).ok()?;
    let value = reply.value();
    if value.is_empty() {
        return None;
    }
    Some(value.to_vec())
}

impl X11Focus {
    fn query_active(&mut self) -> Option<String> {
        // _NET_ACTIVE_WINDOW holds the active top-level window.
        let cookie = self.conn.send_request(&xcb::x::GetProperty {
            delete: false,
            window: self.root,
            property: self.atom_active,
            r#type: xcb::x::ATOM_WINDOW,
            long_offset: 0,
            long_length: 1,
        });
        let reply = self.conn.wait_for_reply(cookie).ok()?;
        let win = reply.value::<xcb::x::Window>().first().copied()?;
        if win.resource_id() == 0 {
            return None;
        }
        // WM_CLASS = "instance\0class\0" — use the class part.
        let raw = get_property_string(&self.conn, win, self.atom_wm_class)?;
        let mut parts = raw.split(|b| *b == 0).filter(|p| !p.is_empty());
        let class = parts.nth(1).or_else(|| parts.next())?;
        String::from_utf8(class.to_vec())
            .ok()
            .map(|s| s.to_ascii_lowercase())
    }
}
