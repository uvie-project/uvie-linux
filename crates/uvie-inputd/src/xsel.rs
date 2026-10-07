//! X11 CLIPBOARD selection ownership with fetch acknowledgment.
//!
//! arboard serves selection requests on a hidden internal thread — we can
//! never observe *when* the target app actually fetched our pasted text,
//! which is the root cause of the clipboard-paste race (a fetch landing
//! after the next `set_text` serves the wrong string). Owning the
//! selection ourselves through xcb exposes `SelectionRequest` events, so
//! `paste` can wait until a fetch has actually been served.
//!
//! X11 only — under XWayland the X selection doesn't reach native Wayland
//! apps, so Wayland sessions keep the arboard path.

use std::time::{Duration, Instant};

/// A hidden X window that owns CLIPBOARD and serves conversion requests.
pub struct XSel {
    conn: xcb::Connection,
    win: xcb::x::Window,
    atom_clipboard: xcb::x::Atom,
    atom_targets: xcb::x::Atom,
    atom_utf8: xcb::x::Atom,
    atom_text: xcb::x::Atom,
    atom_string: xcb::x::Atom,
    content: Vec<u8>,
    /// At least one conversion has been served since the last `set_text`.
    fetched: bool,
}

impl XSel {
    /// Connect when running a real X11 session; `None` elsewhere.
    pub fn connect() -> Option<Self> {
        let is_x11 = match std::env::var("XDG_SESSION_TYPE") {
            Ok(v) => v == "x11",
            Err(_) => {
                std::env::var_os("DISPLAY").is_some()
                    && std::env::var_os("WAYLAND_DISPLAY").is_none()
            }
        };
        if !is_x11 {
            return None;
        }
        let (conn, screen_idx) = xcb::Connection::connect(None).ok()?;
        let root = conn.get_setup().roots().nth(screen_idx as usize)?.root();
        let win: xcb::x::Window = conn.generate_id();
        conn.check_request(conn.send_request_checked(&xcb::x::CreateWindow {
            depth: 0,
            wid: win,
            parent: root,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            border_width: 0,
            class: xcb::x::WindowClass::InputOnly,
            visual: 0,
            value_list: &[],
        }))
        .ok()?;
        let atom_clipboard = intern(&conn, b"CLIPBOARD")?;
        let atom_targets = intern(&conn, b"TARGETS")?;
        let atom_utf8 = intern(&conn, b"UTF8_STRING")?;
        let atom_text = intern(&conn, b"TEXT")?;
        let atom_string = intern(&conn, b"STRING")?;
        Some(Self {
            conn,
            win,
            atom_clipboard,
            atom_targets,
            atom_utf8,
            atom_text,
            atom_string,
            content: Vec::new(),
            fetched: false,
        })
    }

    /// Take ownership of CLIPBOARD advertising `text`.
    pub fn set_text(&mut self, text: &str) {
        self.content = text.as_bytes().to_vec();
        self.fetched = false;
        let _ = self.conn.send_request(&xcb::x::SetSelectionOwner {
            owner: self.win,
            selection: self.atom_clipboard,
            time: xcb::x::CURRENT_TIME,
        });
        let _ = self.conn.flush();
    }

    /// Serve pending selection requests for up to `budget`, returning true
    /// as soon as one UTF8 conversion has been served (i.e. someone
    /// fetched our text).
    pub fn wait_fetched(&mut self, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            self.serve_queued();
            if self.fetched {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5).min(left));
        }
    }

    /// Forget that a fetch was served — used between the pre-Ctrl+V
    /// drain (clipboard-manager requests) and the post-Ctrl+V wait so
    /// only the app's own fetch counts.
    pub fn reset_fetched(&mut self) {
        self.fetched = false;
    }

    /// Drain whatever requests are queued right now (housekeeping — lets
    /// a later manual paste of our text keep working).
    pub fn pump(&mut self) {
        self.serve_queued();
        let _ = self.conn.flush();
    }

    fn serve_queued(&mut self) {
        loop {
            let ev = match self.conn.poll_for_event() {
                Ok(Some(ev)) => ev,
                _ => break,
            };
            let xcb::Event::X(xcb::x::Event::SelectionRequest(req)) = ev else {
                continue;
            };
            let target = req.target();
            let property = if req.property() == xcb::x::ATOM_NONE {
                target // obsolete clients may send NONE — answer on target
            } else {
                req.property()
            };
            let served = if target == self.atom_targets {
                let atoms = [self.atom_utf8, self.atom_text, self.atom_string];
                self.conn.send_request_checked(&xcb::x::ChangeProperty {
                    mode: xcb::x::PropMode::Replace,
                    window: req.requestor(),
                    property,
                    r#type: xcb::x::ATOM_ATOM,
                    data: &atoms,
                });
                true
            } else if target == self.atom_utf8
                || target == self.atom_text
                || target == self.atom_string
            {
                self.conn.send_request_checked(&xcb::x::ChangeProperty {
                    mode: xcb::x::PropMode::Replace,
                    window: req.requestor(),
                    property,
                    r#type: target,
                    data: &self.content,
                });
                self.fetched = true;
                true
            } else {
                false
            };
            let notify = xcb::x::SelectionNotifyEvent::new(
                req.time(),
                req.requestor(),
                req.selection(),
                target,
                if served { property } else { xcb::x::ATOM_NONE },
            );
            self.conn.send_request(&xcb::x::SendEvent {
                propagate: false,
                destination: xcb::x::SendEventDest::Window(req.requestor()),
                event_mask: xcb::x::EventMask::NO_EVENT,
                event: &notify,
            });
        }
        let _ = self.conn.flush();
    }
}

fn intern(conn: &xcb::Connection, name: &[u8]) -> Option<xcb::x::Atom> {
    conn.wait_for_reply(conn.send_request(&xcb::x::InternAtom {
        only_if_exists: false,
        name,
    }))
    .ok()
    .map(|r| r.atom())
}
