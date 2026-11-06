//! 系统托盘：关闭窗口时隐藏到这里，菜单可显示窗口 / 暂停 / 退出。

use std::cell::RefCell;

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config::SharedConfig;

thread_local! {
    /// TrayIcon 内部是 Rc，非 Send，只能在创建它的（主）线程持有。
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

const WINDOW_TITLE: &str = "Polish Tools";

/// 创建托盘图标与菜单。失败时只记录日志，不影响主程序。
pub fn setup(ctx: egui::Context, cfg: SharedConfig) {
    let menu = Menu::new();
    let show_item = MenuItem::new("显示主窗口", true, None);
    let pause_item = MenuItem::new("暂停 / 恢复", true, None);
    let sep = PredefinedMenuItem::separator();
    let quit_item = MenuItem::new("退出 Polish Tools", true, None);

    if let Err(e) = menu.append_items(&[&show_item, &pause_item, &sep, &quit_item]) {
        crate::log_line(&format!("托盘菜单创建失败: {e}"));
        return;
    }

    let icon = match make_icon() {
        Ok(i) => i,
        Err(e) => {
            crate::log_line(&format!("托盘图标生成失败: {e}"));
            return;
        }
    };

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Polish Tools · 连按三次空格润色")
        .with_icon(icon)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            crate::log_line(&format!("托盘创建失败: {e}"));
            return;
        }
    };

    let _ = TRAY.try_with(|slot| {
        *slot.borrow_mut() = Some(tray);
    });

    let show_id = show_item.id().clone();
    let pause_id = pause_item.id().clone();
    let quit_id = quit_item.id().clone();
    let menu_cfg = cfg.clone();
    let menu_ctx = ctx.clone();

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            show_window(&menu_ctx);
        } else if event.id == pause_id {
            toggle_pause(&menu_cfg);
        } else if event.id == quit_id {
            quit_now();
        }
    }));

    let tray_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(event, TrayIconEvent::DoubleClick { .. }) {
            show_window(&tray_ctx);
        }
    }));

    crate::log_line("系统托盘已创建");
}

/// 从托盘恢复主窗口
pub fn show_window(ctx: &egui::Context) {
    if let Some(hwnd) = crate::find_window_like(&[WINDOW_TITLE]) {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SetForegroundWindow, ShowWindowAsync, SW_RESTORE, SW_SHOW,
            };
            // 必须异步：同步 ShowWindow 跨线程调用会被阻塞并返回失败
            let _ = ShowWindowAsync(hwnd, SW_SHOW);
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        crate::log_line("从托盘恢复窗口");
    }
    // 窗口重新可见后需要一帧重绘
    ctx.request_repaint();
}

/// 隐藏主窗口（关闭按钮走这里）
pub fn hide_window() {
    if let Some(hwnd) = crate::find_window_like(&[WINDOW_TITLE]) {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

fn toggle_pause(cfg: &SharedConfig) {
    if let Ok(mut c) = cfg.write() {
        c.paused = !c.paused;
        let _ = c.save();
        crate::log_line(if c.paused { "托盘：已暂停" } else { "托盘：已恢复" });
    }
}

fn quit_now() {
    // 先 drop 托盘图标，避免通知区域残留
    let _ = TRAY.try_with(|slot| {
        let _ = slot.borrow_mut().take();
    });
    crate::log_line("用户从托盘退出程序");
    std::process::exit(0);
}

/// 32x32 图标：蓝底圆角 + 白色 P
fn make_icon() -> Result<Icon, String> {
    const S: u32 = 32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];

    for y in 0..S {
        for x in 0..S {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            if in_rounded_rect(fx, fy, 1.5, 1.5, S as f32 - 1.5, S as f32 - 1.5, 7.0) {
                let i = ((y * S + x) * 4) as usize;
                rgba[i] = 37;
                rgba[i + 1] = 99;
                rgba[i + 2] = 235;
                rgba[i + 3] = 255;
            }
        }
    }

    const GLYPH: [[u8; 5]; 7] = [
        [1, 1, 1, 1, 0],
        [1, 0, 0, 1, 0],
        [1, 0, 0, 1, 0],
        [1, 1, 1, 1, 0],
        [1, 0, 0, 0, 0],
        [1, 0, 0, 0, 0],
        [1, 0, 0, 0, 0],
    ];
    let scale = 3u32;
    let gw = 5 * scale;
    let gh = 7 * scale;
    let ox = (S - gw) / 2;
    let oy = (S - gh) / 2;

    for (gy, row) in GLYPH.iter().enumerate() {
        for (gx, &v) in row.iter().enumerate() {
            if v == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = ox + gx as u32 * scale + dx;
                        let y = oy + gy as u32 * scale + dy;
                        if x < S && y < S {
                            let i = ((y * S + x) * 4) as usize;
                            rgba[i] = 255;
                            rgba[i + 1] = 255;
                            rgba[i + 2] = 255;
                            rgba[i + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    Icon::from_rgba(rgba, S, S).map_err(|e| e.to_string())
}

fn in_rounded_rect(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, r: f32) -> bool {
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    let cx = x.clamp(left + r, right - r);
    let cy = y.clamp(top + r, bottom - r);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= r * r
}