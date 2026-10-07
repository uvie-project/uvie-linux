//! StatusNotifierItem tray (ksni) — the KDE/GNOME-with-AppIndicator
//! counterpart of uvie-mac's menu-bar item and uvie-win's tray icon.
//!
//! Menu actions travel to the daemon loop over the shared `Msg` channel;
//! display state lives in `TrayShared`, written by the daemon.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use uvie_core::dispatcher::InputLanguage;
use uvie_core::InputMethod;

/// Commands the tray menu can send to the daemon.
pub enum TrayCmd {
    ToggleEnabled,
    ToggleLanguage,
    SetMethod(InputMethod),
    OpenSettings,
    Quit,
}

/// Snapshot of what the tray should display.
#[derive(Clone)]
pub struct TrayShared {
    pub enabled: bool,
    pub language: InputLanguage,
    pub method: InputMethod,
}

impl Default for TrayShared {
    fn default() -> Self {
        Self {
            enabled: true,
            language: InputLanguage::Vietnamese,
            method: InputMethod::Telex,
        }
    }
}

/// Tray icon: the UVie logo (org avatar) decoded at startup and
/// downscaled to 32x32 ARGB. Falls back to the drawn "V" icon when the
/// decode fails.
fn icon_pixmap() -> ksni::Icon {
    logo_pixmap().unwrap_or_else(drawn_pixmap)
}

fn logo_pixmap() -> Option<ksni::Icon> {
    let img = image::load_from_memory_with_format(
        include_bytes!("../../../assets/logo.png"),
        image::ImageFormat::Png,
    )
    .ok()?;
    let small = img
        .resize_exact(32, 32, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut data = vec![0u8; 32 * 32 * 4];
    for (i, px) in small.pixels().enumerate() {
        // ksni wants big-endian ARGB32: A R G B per pixel.
        data[i * 4] = px[3];
        data[i * 4 + 1] = px[0];
        data[i * 4 + 2] = px[1];
        data[i * 4 + 3] = px[2];
    }
    Some(ksni::Icon {
        width: 32,
        height: 32,
        data,
    })
}

/// 32x32 ARGB icon: blue rounded square with a white "V".
fn drawn_pixmap() -> ksni::Icon {
    let (w, h) = (32i32, 32i32);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            // Rounded rect fill.
            let inset = (x >= 3 && x < w - 3 && y >= 3 && y < h - 3)
                || ((x - 3).abs() + (y - 3).abs() < 4)
                || ((x - (w - 4)).abs() + (y - 3).abs() < 4)
                || ((x - 3).abs() + (y - (h - 4)).abs() < 4)
                || ((x - (w - 4)).abs() + (y - (h - 4)).abs() < 4);
            if inset {
                // A R G B (big-endian ARGB32)
                data[i] = 255;
                data[i + 1] = 0x25;
                data[i + 2] = 0x63;
                data[i + 3] = 0xeb;
            }
            // White "V": two diagonals meeting at bottom center.
            let on_v = {
                let fx = x as f32;
                let fy = y as f32;
                let left_arm = (fx - (7.0 + fy * 0.31)).abs() < 1.2 && (9.0..=24.0).contains(&fy);
                let right_arm = (fx - (25.0 - fy * 0.31)).abs() < 1.2 && (9.0..=24.0).contains(&fy);
                left_arm || right_arm
            };
            if on_v {
                data[i] = 255;
                data[i + 1] = 255;
                data[i + 2] = 255;
                data[i + 3] = 255;
            }
        }
    }
    ksni::Icon {
        width: w,
        height: h,
        data,
    }
}

struct UvieTray {
    tx: Sender<crate::daemon::Msg>,
    shared: Arc<Mutex<TrayShared>>,
}

impl ksni::Tray for UvieTray {
    fn id(&self) -> String {
        "uvie".into()
    }

    fn title(&self) -> String {
        "UVie".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon_pixmap()]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let shared = self.shared.lock().map(|s| s.clone()).unwrap_or_default();
        let vi_label = if shared.enabled {
            "Gõ tiếng Việt"
        } else {
            "Gõ tiếng Việt (tắt)"
        };
        vec![
            StandardItem {
                label: format!(
                    "UVie — {}",
                    match shared.language {
                        InputLanguage::Vietnamese => "Tiếng Việt",
                        InputLanguage::English => "English",
                    }
                ),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: vi_label.into(),
                checked: shared.enabled,
                activate: Box::new(|this: &mut Self| {
                    let _ = this
                        .tx
                        .send(crate::daemon::Msg::Tray(TrayCmd::ToggleEnabled));
                }),
                ..Default::default()
            }
            .into(),
            RadioGroup {
                selected: match shared.language {
                    InputLanguage::Vietnamese => 0,
                    InputLanguage::English => 1,
                },
                select: Box::new(|this: &mut Self, _current| {
                    let _ = this
                        .tx
                        .send(crate::daemon::Msg::Tray(TrayCmd::ToggleLanguage));
                }),
                options: vec![
                    RadioItem {
                        label: "Tiếng Việt".into(),
                        ..Default::default()
                    },
                    RadioItem {
                        label: "English".into(),
                        ..Default::default()
                    },
                ],
            }
            .into(),
            SubMenu {
                label: "Kiểu gõ".into(),
                submenu: vec![RadioGroup {
                    selected: match shared.method {
                        InputMethod::Telex => 0,
                        InputMethod::Vni => 1,
                        InputMethod::SimpleTelex => 2,
                    },
                    select: Box::new(|this: &mut Self, i| {
                        let m = match i {
                            1 => InputMethod::Vni,
                            2 => InputMethod::SimpleTelex,
                            _ => InputMethod::Telex,
                        };
                        let _ = this
                            .tx
                            .send(crate::daemon::Msg::Tray(TrayCmd::SetMethod(m)));
                    }),
                    options: vec![
                        RadioItem {
                            label: "Telex".into(),
                            ..Default::default()
                        },
                        RadioItem {
                            label: "VNI".into(),
                            ..Default::default()
                        },
                        RadioItem {
                            label: "Simple Telex".into(),
                            ..Default::default()
                        },
                    ],
                }
                .into()],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Cài đặt…".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this
                        .tx
                        .send(crate::daemon::Msg::Tray(TrayCmd::OpenSettings));
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Thoát".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(crate::daemon::Msg::Tray(TrayCmd::Quit));
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the SNI service on a dedicated thread (async-io executor). Safe to call when no
/// StatusNotifierWatcher exists — the daemon still works; the tray just
/// doesn't appear (e.g. stock GNOME without the AppIndicator extension).
pub fn spawn_tray(tx: Sender<crate::daemon::Msg>, shared: Arc<Mutex<TrayShared>>) {
    std::thread::Builder::new()
        .name("uvie-tray".into())
        .spawn(move || {
            futures_lite::future::block_on(async move {
                let tray = UvieTray { tx, shared };
                match ksni::TrayMethods::spawn(tray).await {
                    Ok(handle) => {
                        let _keep = handle;
                        std::future::pending::<()>().await;
                    }
                    Err(e) => eprintln!("[uvie-inputd] tray unavailable: {e}"),
                }
            });
        })
        .ok();
}
